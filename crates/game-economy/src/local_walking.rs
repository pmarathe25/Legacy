//! Preserve ordinary activity reservations while allowing a person at a local
//! store to choose a physically local carrying action.
use game_economy_local_transport::{Endpoint, LocalTransport, walking::Walking};
use game_lots::{Place, Transaction};
use rayon::prelude::*;
use std::collections::HashSet;

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
    let blocked = if context.walking.preferences.is_empty() {
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
