//! Factual carried-stock selection uses the canonical Cargo column.
use crate::{InventoryLocation, inventories};
use game_core::{Error, Good, InventoryId, PersonId, Quantity};
use game_ecs::Store;

pub fn carried_quantity(store: &Store, holder: PersonId) -> Result<Quantity, Error> {
    carried_quantity_rows(
        inventories(store)
            .filter(|(_, stock)| stock.location == InventoryLocation::Carried(holder))
            .map(|(_, stock)| stock),
    )
}
/// Sum already indexed borrowed compartments without scanning unrelated stocks.
pub fn carried_quantity_rows<'a>(
    stocks: impl IntoIterator<Item = &'a crate::Inventory>,
) -> Result<Quantity, Error> {
    stocks
        .into_iter()
        .try_fold(0u64, |total, stock| {
            total.checked_add(stock.quantity.0).ok_or(Error::Overflow)
        })
        .map(Quantity)
}
/// Bulk carried totals for save validation and other full-column passes.
pub fn carried_quantities(
    store: &Store,
) -> Result<std::collections::BTreeMap<PersonId, Quantity>, Error> {
    let mut totals = std::collections::BTreeMap::<PersonId, Quantity>::new();
    for (_, stock) in inventories(store) {
        if let InventoryLocation::Carried(holder) = stock.location {
            let total = totals.entry(holder).or_insert(Quantity(0));
            total.0 = total
                .0
                .checked_add(stock.quantity.0)
                .ok_or(Error::Overflow)?;
        }
    }
    Ok(totals)
}

/// Select the smallest stable ID whose actual compartment admits this good and
/// amount. Contact, aggregate carrying room, and consent are separate facts.
pub fn compatible_carried_destination(
    store: &Store,
    holder: PersonId,
    good: Good,
    quantity: Quantity,
) -> Option<InventoryId> {
    inventories(store)
        .filter(|(_, stock)| {
            stock.location == InventoryLocation::Carried(holder)
                && stock.good == good
                && stock.room().is_ok_and(|room| room >= quantity)
        })
        .map(|(id, _)| id)
        .min()
}

/// A borrowed actual row plus access and reservation facts from their owners.
pub struct CarriedDeliveryFacts<'a> {
    pub stock: &'a crate::Inventory,
    pub accessible: bool,
    pub unreserved: Quantity,
}
pub fn delivery_cargo(
    holder: PersonId,
    good: Good,
    source: Option<InventoryId>,
    attributed: &std::collections::BTreeMap<InventoryId, Quantity>,
    stocks: &[CarriedDeliveryFacts<'_>],
) -> Vec<game_logistics::Cargo> {
    let mut rows: Vec<_> = stocks
        .iter()
        .filter(|facts| {
            facts.accessible && facts.stock.location == InventoryLocation::Carried(holder)
        })
        .collect();
    rows.sort_by_key(|facts| facts.stock.id);
    rows.into_iter()
        .map(|facts| {
            let stock = facts.stock;
            let quantity = Quantity(facts.unreserved.0.min(stock.quantity.0));
            let origin_quantity = source
                .filter(|source| *source != stock.id && stock.good == good)
                .map_or(Quantity(0), |_| {
                    Quantity(
                        quantity
                            .0
                            .min(attributed.get(&stock.id).copied().unwrap_or(Quantity(0)).0),
                    )
                });
            game_logistics::Cargo {
                inventory: stock.id,
                good: stock.good,
                quantity,
                origin: source.filter(|_| origin_quantity.0 > 0),
                origin_quantity,
                room: Quantity(stock.capacity.0.saturating_sub(stock.quantity.0)),
            }
        })
        .collect()
}

/// First actual carried compartment for each good, in stable identity order.
/// Capacity and admission remain separate owner checks at the chosen amount.
pub fn carried_destinations<'a>(
    stocks: impl IntoIterator<Item = &'a crate::Inventory>,
) -> std::collections::BTreeMap<Good, InventoryId> {
    let mut destinations = std::collections::BTreeMap::new();
    for stock in stocks {
        destinations
            .entry(stock.good)
            .and_modify(|id: &mut InventoryId| *id = (*id).min(stock.id))
            .or_insert(stock.id);
    }
    destinations
}

/// Select actual binding material in carried scope. Forced restraint does not
/// require stock access; callers retain encounter and actor authorization.
pub fn binding_stock<'a>(
    actor: PersonId,
    stocks: impl IntoIterator<Item = &'a crate::Inventory>,
) -> Option<InventoryId> {
    stocks
        .into_iter()
        .filter(|stock| {
            stock.location == InventoryLocation::Carried(actor)
                && stock.good == Good::Binding
                && stock.quantity >= Quantity(1_000)
        })
        .map(|stock| stock.id)
        .min()
}

/// Indexed canonical carrying totals avoid an inventory-column scan per actor.
pub fn carried_quantity_in(
    store: &Store,
    holder: PersonId,
    ids: impl IntoIterator<Item = InventoryId>,
) -> Result<Quantity, Error> {
    ids.into_iter().try_fold(Quantity(0), |total, id| {
        let stock = crate::inventory(store, id).ok_or(Error::Unavailable)?;
        if stock.id != id {
            return Err(Error::Unavailable);
        }
        if stock.location != InventoryLocation::Carried(holder) {
            return Ok(total);
        }
        total
            .0
            .checked_add(stock.quantity.0)
            .map(Quantity)
            .ok_or(Error::Overflow)
    })
}
pub fn compatible_carried_destination_in(
    store: &Store,
    holder: PersonId,
    good: Good,
    quantity: Quantity,
    ids: impl IntoIterator<Item = InventoryId>,
) -> Option<InventoryId> {
    ids.into_iter()
        .filter(|id| {
            crate::inventory(store, *id).is_some_and(|stock| {
                stock.id == *id
                    && stock.location == InventoryLocation::Carried(holder)
                    && stock.good == good
                    && stock.room().is_ok_and(|room| room >= quantity)
            })
        })
        .min()
}

#[cfg(test)]
mod binding_tests {
    use super::*;
    #[test]
    fn binding_selection_is_stable_and_requires_actual_material_but_no_access_grant() {
        let stock = |id, holder, amount, good| crate::Inventory {
            id: InventoryId(id),
            location: InventoryLocation::Carried(PersonId(holder)),
            good,
            quantity: Quantity(amount),
            capacity: Quantity(2000),
            access: std::collections::BTreeSet::new(),
            last_granted: None,
            spoils_at: None,
        };
        let mut rows = vec![
            stock(9, 1, 1000, Good::Binding),
            stock(3, 2, 1000, Good::Binding),
            stock(4, 1, 999, Good::Binding),
            stock(5, 1, 1000, Good::Binding),
            stock(2, 1, 1000, Good::Timber),
        ];
        assert_eq!(
            binding_stock(PersonId(1), rows.iter()),
            Some(InventoryId(5))
        );
        rows.reverse();
        assert_eq!(
            binding_stock(PersonId(1), rows.iter()),
            Some(InventoryId(5))
        );
        rows.retain(|row| row.id != InventoryId(5) && row.id != InventoryId(9));
        assert_eq!(binding_stock(PersonId(1), rows.iter()), None);
    }
}
