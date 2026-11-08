//! Preserve ordinary activity reservations while allowing a person at a local
//! store to choose a physically local carrying action.
use game_economy_local_transport::{Endpoint, LocalTransport, walking::Walking};
use game_lots::{Place, Transaction};

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
