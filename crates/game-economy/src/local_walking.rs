//! Preserve ordinary activity reservations while allowing a person at a local
//! store to choose a physically local carrying action.
use game_economy_local_transport::{Endpoint, LocalTransport, walking::Walking};
use game_lots::{Place, Transaction};
use rayon::prelude::*;
use std::collections::HashSet;

/// Regional co-location alone is insufficient when local walking is enabled.
pub fn people_together(
    walking: Option<&Walking>,
    a: &game_economy_person::Person,
    b: &game_economy_person::Person,
) -> bool {
    let (Some(walking), Some(region), Some(other_region)) = (walking, a.region, b.region) else {
        return true;
    };
    use game_economy_local_transport::walking::Location;
    if a.id == b.id {
        return matches!(walking.location(a.id, region), Location::At { .. });
    }
    region == other_region
        && matches!((walking.location(a.id, region), walking.location(b.id, region)),
        (Location::At { region: left, endpoint: x }, Location::At { region: right, endpoint: y }) if left == right && x == y)
}

pub struct GoalWalkContext<'a> {
    pub owner: &'a LocalTransport,
    pub walking: &'a Walking,
    pub people: &'a [game_economy_person::Person],
    pub view: &'a game_lots::View,
    pub recipes: &'a [game_production_model::Recipe],
    pub planned: &'a game_autowork::Autowork,
    pub base: &'a game_autowork::Autowork,
    pub busy: &'a [u64],
    pub underage: &'a [u64],
    pub max_hunger: u32,
}

pub struct GoalWalkPlan {
    pub blocked: Vec<u64>,
    pub requests: Vec<game_economy_local_transport::walking::Request>,
    pub needs: Vec<game_economy_local_transport::goal::Need>,
}

pub struct SearchProposal {
    pub person: u64,
    pub request: Option<game_economy_local_transport::walking::Request>,
    pub search: game_economy_local_transport::goal::SearchPreference,
}

pub struct GoalSearchContext<'a> {
    pub owner: &'a LocalTransport,
    pub walking: &'a Walking,
    pub people: &'a [game_economy_person::Person],
    pub view: &'a game_lots::View,
    pub recipes: &'a [game_production_model::Recipe],
    pub needs: &'a [game_economy_local_transport::goal::Need],
    pub busy: &'a [u64],
    pub underage: &'a [u64],
    pub max_hunger: u32,
    pub tick: u64,
}

pub struct MissionReturnContext<'a> {
    pub missions: &'a crate::Missions,
    pub walking: &'a Walking,
    pub people: &'a [game_economy_person::Person],
    pub view: &'a game_lots::View,
    pub opinions: Option<&'a game_opinions::Opinions>,
    pub busy: &'a [u64],
}

pub struct MissionSupplyContext<'a> {
    pub missions: &'a crate::Missions,
    pub owner: &'a LocalTransport,
    pub walking: &'a Walking,
    pub people: &'a [game_economy_person::Person],
    pub view: &'a game_lots::View,
    pub opinions: Option<&'a game_opinions::Opinions>,
    pub claims: Option<&'a game_economy_claims::Index<'a>>,
    pub busy: &'a [u64],
    pub blocked: &'a [u64],
    pub max_hunger: u32,
}

/// Seek a known store only when locally observed supplies do not meet the need.
pub fn mission_supply(
    context: MissionSupplyContext<'_>,
) -> Vec<game_economy_local_transport::walking::Request> {
    use game_economy_expeditions::{MissionPhase, Objective};
    use game_economy_local_transport::walking::{Location, Request};
    let opinions = context
        .missions
        .entries
        .par_iter()
        .any(|mission| {
            mission
                .delivery_preference
                .and_then(|p| p.acquisition)
                .is_some_and(|p| p.known_store.is_some() && p.pressure_threshold.is_some())
        })
        .then(|| context.opinions.map(game_opinions::Opinions::index))
        .flatten();
    let mut requests: Vec<_> = context
        .missions
        .entries
        .par_iter()
        .filter_map(|mission| {
            if mission.objective_met
                || matches!(
                    mission.phase,
                    MissionPhase::Completed | MissionPhase::Failed
                )
                || context.busy.binary_search(&mission.person).is_ok()
                || context.blocked.binary_search(&mission.person).is_ok()
            {
                return None;
            }
            let preference = mission.delivery_preference?;
            let choice = preference.acquisition?;
            let destination = Endpoint::Store {
                id: choice.known_store?,
            };
            let Objective::Deliver {
                person: recipient,
                good,
                quantity,
                delivered,
            } = mission.objective?
            else {
                return None;
            };
            let actor = &context.people[context
                .people
                .binary_search_by_key(&mission.person, |p| p.id)
                .ok()?];
            let region = actor.region?;
            if actor.controlled
                || actor.health == 0
                || actor.hunger > context.max_hunger
                || (mission.phase == MissionPhase::Ready && region != mission.origin)
                || (mission.phase == MissionPhase::Returning && region == mission.origin)
                || context
                    .owner
                    .stores
                    .binary_search_by_key(&choice.known_store?, |store| store.id)
                    .ok()
                    .is_none_or(|i| context.owner.stores[i].region != region)
            {
                return None;
            }
            let pressured = choice.pressure_threshold.is_some_and(|threshold| {
                opinions.as_ref().is_some_and(|index| {
                    let (fear, obligation) = index.feelings(actor.id, recipient);
                    fear.saturating_add(obligation) >= threshold
                })
            });
            if !choice.willing && !pressured {
                return None;
            }
            let holder = context.view.container(Place::Person(actor.id))?;
            if holder.used >= holder.capacity
                || holder.quantity(good)
                    >= quantity
                        .saturating_sub(delivered)
                        .saturating_add(preference.reserve)
            {
                return None;
            }
            let location = context.walking.location(actor.id, region);
            if mission.phase == MissionPhase::Returning
                && !matches!(location, Location::Walking { to, .. } if to == destination)
            {
                return None;
            }
            let visible = match location {
                Location::At {
                    endpoint: Endpoint::Stock,
                    ..
                } if choice.disregard_claims
                    || context
                        .claims
                        .is_none_or(|claims| claims.allows(actor.id, region, good)) =>
                {
                    context.view.container(Place::Stock { region, good })
                }
                Location::At {
                    endpoint: Endpoint::Store { id },
                    ..
                } => context.view.container(Place::Inventory(id)),
                _ => None,
            };
            if visible.is_some_and(|source| source.quantity(good) > 0)
                || !context.walking.can_advance(actor.id, region, destination)
            {
                return None;
            }
            Some(Request {
                person: actor.id,
                region,
                destination,
            })
        })
        .collect();
    requests.par_sort_unstable_by_key(|request| (request.person, request.destination));
    requests.dedup_by_key(|request| request.person);
    requests
}

/// A traveler who still wants to deliver can carry the actual goods to the
/// regional exit, without learning anything about distant inventory.
pub fn mission_return(
    context: MissionReturnContext<'_>,
) -> Vec<game_economy_local_transport::walking::Request> {
    use game_economy_expeditions::{MissionPhase, Objective};
    use game_economy_local_transport::walking::{Location, Request};
    let opinions = context
        .missions
        .entries
        .par_iter()
        .any(|mission| {
            mission
                .delivery_preference
                .is_some_and(|p| p.pressure_threshold.is_some())
        })
        .then(|| context.opinions.map(game_opinions::Opinions::index))
        .flatten();
    let mut choices: Vec<_> = context
        .missions
        .entries
        .par_iter()
        .filter_map(|mission| {
            if matches!(
                mission.phase,
                MissionPhase::Completed | MissionPhase::Failed
            ) || mission.objective_met
                || context.busy.binary_search(&mission.person).is_ok()
            {
                return None;
            }
            let Objective::Deliver {
                person: recipient,
                good,
                quantity,
                delivered,
            } = mission.objective?
            else {
                return None;
            };
            let preference = mission.delivery_preference?;
            let actor = &context.people[context
                .people
                .binary_search_by_key(&mission.person, |person| person.id)
                .ok()?];
            if actor.controlled || actor.health == 0 {
                return None;
            }
            let region = actor.region?;
            if (mission.phase == MissionPhase::Ready && region != mission.origin)
                || (mission.phase == MissionPhase::Returning && region == mission.origin)
            {
                return None;
            }
            let pressured = preference.pressure_threshold.is_some_and(|threshold| {
                opinions.as_ref().is_some_and(|index| {
                    let (fear, obligation) = index.feelings(actor.id, recipient);
                    fear.saturating_add(obligation) >= threshold
                })
            });
            if !preference.willing && !pressured {
                return None;
            }
            let needed = quantity
                .saturating_sub(delivered)
                .saturating_add(preference.reserve);
            let held = context.view.container(Place::Person(actor.id))?.quantity(good);
            if mission.phase != MissionPhase::Returning && held < needed {
                return None;
            }
            if held >= needed && region == mission.destination
                && context.people.binary_search_by_key(&recipient, |person| person.id).ok().is_some_and(|index| {
                    let target = &context.people[index];
                    target.region == Some(region)
                        && matches!((context.walking.location(actor.id, region), context.walking.location(target.id, region)),
                            (Location::At { endpoint: a, .. }, Location::At { endpoint: b, .. }) if a == b)
                })
            {
                return None;
            }
            match context.walking.location(actor.id, region) {
                Location::At {
                    endpoint: Endpoint::Stock,
                    ..
                } => None,
                _ if context
                    .walking
                    .can_advance(actor.id, region, Endpoint::Stock) =>
                {
                    Some((
                        actor.id,
                        Some(Request {
                            person: actor.id,
                            region,
                            destination: Endpoint::Stock,
                        }),
                    ))
                }
                _ => None,
            }
        })
        .collect();
    if choices.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        choices.par_sort_unstable_by_key(|choice| choice.0);
    }
    choices.par_iter().filter_map(|choice| choice.1).collect()
}

/// Visit a bounded sequence of known stores, inspecting each only on arrival.
pub fn goal_search(context: GoalSearchContext<'_>) -> Result<Vec<SearchProposal>, String> {
    use game_economy_local_transport::walking::{Location, Request};
    context
        .owner
        .goal_fetchers
        .par_iter()
        .filter_map(|preference| {
            let mut search = preference.search.clone()?;
            let selected = context
                .needs
                .binary_search_by_key(&preference.person, |need| need.person)
                .ok()
                .map(|index| context.needs[index].recipe);
            if search.active_recipe.is_some()
                && (context.tick >= search.expires_at
                    || selected.is_some_and(|recipe| Some(recipe) != search.active_recipe))
            {
                search.active_recipe = None;
                search.target = None;
                return Some(Ok(SearchProposal {
                    person: preference.person,
                    request: None,
                    search,
                }));
            }
            let person = &context.people[context
                .people
                .binary_search_by_key(&preference.person, |person| person.id)
                .ok()?];
            let region = person.region?;
            let destination = Endpoint::Store {
                id: search.target.unwrap_or(search.store),
            };
            let location = context.walking.location(person.id, region);
            if !preference.enabled
                || !search.willing
                || person.controlled
                || person.health == 0
                || !person.willing_to_work
                || person.hunger > context.max_hunger
                || context.busy.binary_search(&person.id).is_ok()
                || context.underage.binary_search(&person.id).is_ok()
                || context
                    .owner
                    .stores
                    .binary_search_by_key(&search.store, |store| store.id)
                    .ok()
                    .is_none_or(|index| context.owner.stores[index].region != region)
            {
                return None;
            }
            let recipe_id = selected?;
            if search.active_recipe.is_some() {
                let recipe = context
                    .recipes
                    .iter()
                    .find(|recipe| recipe.id == recipe_id)?;
                let holder = context.view.container(Place::Person(person.id))?;
                if holder.used >= holder.capacity
                    || recipe
                        .inputs
                        .iter()
                        .any(|input| holder.quantity(input.good) > 0)
                {
                    search.active_recipe = None;
                    search.target = None;
                    return Some(Ok(SearchProposal {
                        person: person.id,
                        request: None,
                        search,
                    }));
                }
                if matches!(location, Location::At { endpoint, .. } if endpoint == destination) {
                    if search.max_visits == 1 {
                        search.active_recipe = None;
                        search.target = None;
                        return Some(Ok(SearchProposal {
                            person: person.id,
                            request: None,
                            search,
                        }));
                    }
                    let source = context
                        .view
                        .container(Place::Inventory(search.target.unwrap_or(search.store)))?;
                    if recipe
                        .inputs
                        .iter()
                        .any(|input| source.quantity(input.good) > 0)
                    {
                        return None;
                    }
                    search.examined = search.examined.saturating_add(1);
                    let current = search.target.unwrap_or(search.store);
                    let mut candidates =
                        std::iter::once(search.store).chain(search.alternatives.iter().copied());
                    let next = candidates
                        .by_ref()
                        .skip_while(|id| *id != current)
                        .skip(1)
                        .find_map(|id| {
                            if search.examined >= search.max_visits {
                                return None;
                            }
                            let next = Endpoint::Store { id };
                            if context.walking.has_route(region, destination, next) {
                                Some((id, next))
                            } else if context.walking.has_route(
                                region,
                                destination,
                                Endpoint::Stock,
                            ) && context.walking.has_route(region, Endpoint::Stock, next)
                            {
                                Some((id, Endpoint::Stock))
                            } else {
                                None
                            }
                        });
                    if let Some((target, step)) = next {
                        search.target = Some(target);
                        return Some(Ok(SearchProposal {
                            person: person.id,
                            request: Some(Request {
                                person: person.id,
                                region,
                                destination: step,
                            }),
                            search,
                        }));
                    }
                    search.active_recipe = None;
                    search.target = None;
                    return Some(Ok(SearchProposal {
                        person: person.id,
                        request: None,
                        search,
                    }));
                }
                let step = match location {
                    Location::Walking { to, .. } if to == destination || to == Endpoint::Stock => {
                        Some(to)
                    }
                    Location::At {
                        endpoint: Endpoint::Stock,
                        ..
                    } if context
                        .walking
                        .has_route(region, Endpoint::Stock, destination) =>
                    {
                        Some(destination)
                    }
                    Location::At { endpoint, .. }
                        if context.walking.has_route(region, endpoint, destination) =>
                    {
                        Some(destination)
                    }
                    _ => None,
                };
                return step.map(|destination| {
                    Ok(SearchProposal {
                        person: person.id,
                        request: Some(Request {
                            person: person.id,
                            region,
                            destination,
                        }),
                        search,
                    })
                });
            }
            if context.tick < search.next_review
                || !matches!(
                    location,
                    Location::At {
                        endpoint: Endpoint::Stock,
                        ..
                    }
                )
                || !context.walking.can_advance(person.id, region, destination)
            {
                return None;
            }
            let recipe = context
                .recipes
                .iter()
                .find(|recipe| recipe.id == recipe_id)?;
            let holder = context.view.container(Place::Person(person.id))?;
            if holder.used >= holder.capacity {
                return None;
            }
            let shortage = recipe.inputs.iter().any(|input| {
                holder.quantity(input.good) == 0
                    && context
                        .view
                        .container(Place::Stock {
                            region,
                            good: input.good,
                        })
                        .is_some_and(|stock| stock.quantity(input.good) < input.quantity)
            });
            if !shortage {
                return None;
            }
            let Some(next_review) = context.tick.checked_add(search.retry_ticks) else {
                return Some(Err("local goal search retry overflow".into()));
            };
            let Some(expires_at) = context.tick.checked_add(search.max_ticks) else {
                return Some(Err("local goal search expiry overflow".into()));
            };
            search.next_review = next_review;
            search.expires_at = expires_at;
            search.active_recipe = Some(recipe_id);
            search.examined = 0;
            search.target = Some(search.store);
            Some(Ok(SearchProposal {
                person: person.id,
                request: Some(Request {
                    person: person.id,
                    region,
                    destination,
                }),
                search,
            }))
        })
        .collect()
}

/// Bring personally held inputs home without inspecting home while away.
pub fn goal_walk(context: GoalWalkContext<'_>) -> GoalWalkPlan {
    use game_economy_local_transport::walking::{Location, Request};
    let explicit: HashSet<_> = context
        .base
        .policies
        .par_iter()
        .map(|policy| policy.person)
        .collect();
    let mut needs: Vec<_> = context
        .planned
        .policies
        .par_iter()
        .filter(|policy| policy.enabled && !explicit.contains(&policy.person))
        .map(|policy| game_economy_local_transport::goal::Need {
            person: policy.person,
            recipe: policy.recipe,
        })
        .collect();
    if needs
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        needs.par_sort_unstable_by_key(|need| need.person);
    }
    let mut recipes: Vec<_> = context.recipes.par_iter().collect();
    if recipes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        recipes.par_sort_unstable_by_key(|recipe| recipe.id);
    }
    let choices: Vec<_> = context
        .owner
        .goal_fetchers
        .par_iter()
        .filter_map(|preference| {
            if !preference.enabled
                || context.busy.binary_search(&preference.person).is_ok()
                || context.underage.binary_search(&preference.person).is_ok()
            {
                return None;
            }
            let person = &context.people[context
                .people
                .binary_search_by_key(&preference.person, |person| person.id)
                .ok()?];
            if person.controlled
                || person.health == 0
                || !person.willing_to_work
                || person.hunger > context.max_hunger
            {
                return None;
            }
            let region = person.region?;
            let recipe_id = needs
                .binary_search_by_key(&person.id, |need| need.person)
                .ok()
                .map(|i| needs[i].recipe)?;
            let recipe = recipes
                .binary_search_by_key(&recipe_id, |recipe| recipe.id)
                .ok()
                .map(|i| recipes[i])?;
            let holder = context.view.container(Place::Person(person.id))?;
            let location = context.walking.location(person.id, region);
            if matches!(
                location,
                Location::At {
                    endpoint: Endpoint::Stock,
                    ..
                }
            ) && !recipe.inputs.is_empty()
                && recipe.inputs.iter().all(|input| {
                    context
                        .view
                        .container(Place::Stock {
                            region,
                            good: input.good,
                        })
                        .is_some_and(|stock| stock.quantity(input.good) >= input.quantity)
                })
            {
                return Some((person.id, None));
            }
            let useful = recipe.inputs.iter().any(|input| {
                if holder.quantity(input.good) == 0 {
                    return false;
                }
                if matches!(
                    location,
                    Location::At {
                        endpoint: Endpoint::Stock,
                        ..
                    }
                ) {
                    context
                        .view
                        .container(Place::Stock {
                            region,
                            good: input.good,
                        })
                        .is_some_and(|stock| {
                            stock.quantity(input.good) < input.quantity
                                && stock.used < stock.capacity
                        })
                } else {
                    true
                }
            });
            if !useful {
                return None;
            }
            if matches!(
                location,
                Location::At {
                    endpoint: Endpoint::Stock,
                    ..
                }
            ) {
                Some((person.id, None))
            } else if context
                .walking
                .can_advance(person.id, region, Endpoint::Stock)
            {
                Some((
                    person.id,
                    Some(Request {
                        person: person.id,
                        region,
                        destination: Endpoint::Stock,
                    }),
                ))
            } else {
                None
            }
        })
        .collect();
    let blocked = if context.walking.preferences.is_empty()
        && !context
            .owner
            .goal_fetchers
            .par_iter()
            .any(|preference| preference.search.is_some())
    {
        Vec::new()
    } else {
        choices.par_iter().map(|choice| choice.0).collect()
    };
    let requests = choices.par_iter().filter_map(|choice| choice.1).collect();
    GoalWalkPlan {
        blocked,
        requests,
        needs,
    }
}

pub fn carrying_busy(
    busy: &[u64],
    nonstock: &[u64],
    prior: &[u64],
    walking: Option<&Walking>,
    local_actors: &[u64],
) -> Vec<u64> {
    let mut result = game_economy_work::exclude_sorted(busy, nonstock);
    game_economy_work::reserve(&mut result, prior.iter().copied());
    if let Some(walking) = walking {
        game_economy_work::reserve(&mut result, walking.events.iter().map(|event| event.person));
    }
    game_economy_work::reserve(&mut result, local_actors.iter().copied());
    result
}

/// Guard every direct person/store and person/regional-stock transfer, including
/// transfers planned by features outside the local carrying owner.
pub fn allows_transaction(
    owner: &LocalTransport,
    walking: &Walking,
    people: &[game_economy_person::Person],
    transaction: &Transaction,
) -> bool {
    transaction.movements.iter().all(|movement| {
        let (person, place) = match (movement.from, movement.to) {
            (Place::Person(person), Some(place)) | (place, Some(Place::Person(person))) => {
                (person, place)
            }
            _ => return true,
        };
        let present = |region, endpoint| {
            people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_some_and(|index| {
                    people[index].region == Some(region)
                        && walking.present(person, region, endpoint)
                })
        };
        match place {
            Place::Stock { region, .. } => present(region, Endpoint::Stock),
            Place::Inventory(id) => owner
                .stores
                .binary_search_by_key(&id, |store| store.id)
                .ok()
                .is_some_and(|index| present(owner.stores[index].region, Endpoint::Store { id })),
            _ => true,
        }
    })
}
