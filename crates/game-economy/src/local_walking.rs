//! Physical reach and personally chosen trips to known stores.
use game_economy_local_transport::{Endpoint, LocalTransport, walking::Walking};
use game_lots::Place;
use rayon::prelude::*;

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
    region == other_region && walking.location(a.id, region) == walking.location(b.id, region)
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
    pub now: u128,
    pub honors_work_inclination: bool,
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
                && (search.expired(context.tick, context.now)
                    || (!context.honors_work_inclination && selected.is_none())
                    || selected.is_some_and(|recipe| Some(recipe) != search.active_recipe))
            {
                search.stop();
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
                || (context.honors_work_inclination && !person.willing_to_work)
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
                    search.stop();
                    return Some(Ok(SearchProposal {
                        person: person.id,
                        request: None,
                        search,
                    }));
                }
                if matches!(location, Location::At { endpoint, .. } if endpoint == destination) {
                    if search.max_visits == 1 {
                        search.stop();
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
                    search.stop();
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
            if !search.can_start(context.tick, context.now)
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
            if let Err(error) = search.begin(recipe_id, context.tick, context.now) {
                return Some(Err(error.into()));
            }
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
