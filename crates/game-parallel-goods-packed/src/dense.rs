//! Uniform all-person batches reserve a shared stock without per-actor effect records.
use super::{Direction, Inventory, PackedCargo, PeopleView, Transfer};
use game_parallel_production_model::Amount;
use rayon::prelude::*;
pub(super) fn transfer(
    cargo: &mut PackedCargo,
    inventories: &mut [Inventory],
    people: &impl PeopleView,
    requests: &[Transfer],
) -> Option<Result<(), String>> {
    if requests.len() < 4096 || requests.len() != people.len() {
        return None;
    }
    let first = &requests[0];
    if first.quantity == 0
        || requests
            .par_windows(2)
            .any(|pair| pair[0].person.checked_add(1) != Some(pair[1].person))
        || requests.par_iter().any(|request| {
            request.good != first.good
                || request.quantity != first.quantity
                || request.direction != first.direction
        })
    {
        return None;
    }
    let (index, region, _, _) = people.actor(first.person)?;
    if index != 0 {
        return None;
    }
    let region = region?;
    let invalid = cargo
        .slots
        .par_iter()
        .zip(requests.par_iter())
        .enumerate()
        .any(|(index, (slot, request))| {
            let Some((actual, local, health, busy)) = people.actor(request.person) else {
                return true;
            };
            if actual != index || local != Some(region) || health == 0 || busy {
                return true;
            }
            let carried = slot
                .items
                .iter()
                .find(|item| item.good == first.good)
                .map_or(0, |item| item.quantity);
            let total: u128 = slot
                .items
                .iter()
                .map(|item| u128::from(item.quantity))
                .sum();
            match first.direction {
                Direction::Pickup => {
                    total + u128::from(first.quantity) > u128::from(cargo.capacity)
                }
                Direction::Deposit => carried < first.quantity,
            }
        });
    if invalid {
        return Some(Err("invalid actor or cargo in uniform transfer".into()));
    }
    let inventory = match inventories
        .iter_mut()
        .find(|inventory| inventory.region == region)
    {
        Some(inventory) => inventory,
        None => return Some(Err("missing inventory".into())),
    };
    let stock = match inventory
        .stocks
        .iter_mut()
        .find(|stock| stock.good == first.good)
    {
        Some(stock) => stock,
        None => return Some(Err("missing stock".into())),
    };
    let amount = u128::from(first.quantity) * requests.len() as u128;
    match first.direction {
        Direction::Pickup if amount > u128::from(stock.quantity) => {
            return Some(Err("shared stock or storage unavailable".into()));
        }
        Direction::Deposit if amount > u128::from(stock.capacity - stock.quantity) => {
            return Some(Err("shared stock or storage unavailable".into()));
        }
        Direction::Pickup => stock.quantity -= amount as u64,
        Direction::Deposit => stock.quantity += amount as u64,
    }
    cargo.slots.par_iter_mut().for_each(|slot| {
        if let Some(item) = slot.items.iter_mut().find(|item| item.good == first.good) {
            match first.direction {
                Direction::Pickup => item.quantity += first.quantity,
                Direction::Deposit => item.quantity -= first.quantity,
            }
            slot.items.retain(|item| item.quantity > 0);
        } else {
            slot.items.push(Amount {
                good: first.good,
                quantity: first.quantity,
            });
            slot.items.sort_unstable_by_key(|item| item.good);
        }
    });
    Some(Ok(()))
}
