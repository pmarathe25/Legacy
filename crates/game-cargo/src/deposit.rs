//! Material supplied by another owner enters a guarded cargo compartment.
use crate::*;
use game_ecs::{Batch, StableId, Store};

#[derive(Clone, Debug)]
pub struct PreparedDeposit {
    before: Inventory,
    after: Inventory,
}
impl PreparedDeposit {
    pub fn quantity_after(&self) -> Quantity {
        self.after.quantity
    }
    pub fn location(&self) -> &InventoryLocation {
        &self.after.location
    }
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if inventory(store, self.before.id) != Some(&self.before) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// Validate the opening row before composing with other owners' writes.
    pub fn append_writes(self, batch: &mut Batch) {
        batch.insert(StableId(self.after.id.0), self.after);
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        self.validate(store)?;
        let mut batch = Batch::new();
        self.append_writes(&mut batch);
        store.apply_batch(batch)
    }
}
fn admitted_deposit(
    store: &Store,
    destination: InventoryId,
    good: Good,
    amount: Quantity,
) -> Result<&Inventory, Error> {
    let before = inventory(store, destination).ok_or(Error::Unavailable)?;
    before.validate()?;
    if before.id != destination || before.good != good || amount.0 == 0 || before.room()? < amount {
        return Err(Error::Unavailable);
    }
    Ok(before)
}
pub fn deposit_available(
    store: &Store,
    destination: InventoryId,
    good: Good,
    amount: Quantity,
) -> bool {
    admitted_deposit(store, destination, good, amount).is_ok()
}
pub fn prepare_deposit(
    store: &Store,
    destination: InventoryId,
    good: Good,
    amount: Quantity,
) -> Result<PreparedDeposit, Error> {
    let before = admitted_deposit(store, destination, good, amount)?;
    let mut after = before.clone();
    after.quantity.0 = after
        .quantity
        .0
        .checked_add(amount.0)
        .ok_or(Error::Overflow)?;
    Ok(PreparedDeposit {
        before: before.clone(),
        after,
    })
}

#[derive(Clone, Debug)]
pub struct DepositBounds {
    pub capacity: Quantity,
    pub location: InventoryLocation,
}
/// Admit the maximum quantity shape for the built-in inventory validator.
/// Its quantity constraint is only quantity<=capacity; identity and receiving
/// metadata remain fixed during a closing interval. This is a bounds proof for
/// that validator, not an admission guarantee for arbitrary custom predicates.
pub fn append_deposit_schema_bounds(
    store: &Store,
    destination: InventoryId,
    good: Good,
    batch: &mut Batch,
) -> Result<DepositBounds, Error> {
    let before = inventory(store, destination).ok_or(Error::Unavailable)?;
    before.validate()?;
    if before.id != destination || before.good != good {
        return Err(Error::Unavailable);
    }
    let bounds = DepositBounds {
        capacity: before.capacity,
        location: before.location.clone(),
    };
    let mut after = before.clone();
    after.quantity = after.capacity;
    batch.insert(StableId(after.id.0), after);
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Inventory {
        Inventory {
            id: InventoryId(10),
            location: InventoryLocation::Area(AreaId(1)),
            quantity: Quantity(100),
            capacity: Quantity(1000),
            access: BTreeSet::new(),
            last_granted: None,
            good: Good::Food,
            spoils_at: None,
        }
    }
    fn setup() -> Store {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        insert_inventory(&mut store, row()).unwrap();
        store
    }
    #[test]
    fn material_deposit_uses_actual_good_room_and_exact_opening_guard() {
        let mut store = setup();
        let before = store.clone();
        for (good, amount) in [
            (Good::Water, Quantity(100)),
            (Good::Food, Quantity(0)),
            (Good::Food, Quantity(901)),
        ] {
            assert!(prepare_deposit(&store, InventoryId(10), good, amount).is_err());
            assert_eq!(store, before);
        }
        let prepared = prepare_deposit(&store, InventoryId(10), Good::Food, Quantity(200)).unwrap();
        assert_eq!(prepared.quantity_after(), Quantity(300));
        assert_eq!(store, before);
        inventory_mut(&mut store, InventoryId(10))
            .unwrap()
            .access
            .insert(PersonId(1));
        let stale = store.clone();
        assert_eq!(prepared.commit(&mut store), Err(Error::Unavailable));
        assert_eq!(store, stale);
        prepare_deposit(&store, InventoryId(10), Good::Food, Quantity(200))
            .unwrap()
            .commit(&mut store)
            .unwrap();
        assert_eq!(
            inventory(&store, InventoryId(10)).unwrap().quantity,
            Quantity(300)
        );
    }
    #[test]
    fn schema_rejects_deposit_without_changing_opening_stock() {
        let mut schema = game_ecs::Schema::new();
        schema
            .register::<Inventory>("cargo/inventory", |row| {
                row.validate()?;
                if row.quantity.0 > 100 {
                    return Err(Error::Unavailable);
                }
                Ok(())
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        insert_inventory(&mut store, row()).unwrap();
        let before = store.clone();
        assert_eq!(
            prepare_deposit(&store, InventoryId(10), Good::Food, Quantity(1))
                .unwrap()
                .commit(&mut store),
            Err(Error::Unavailable)
        );
        assert_eq!(store, before);
    }
    #[test]
    fn schema_bounds_admit_the_largest_compartment_without_publishing() {
        let mut store = setup();
        let row = inventory_mut(&mut store, InventoryId(10)).unwrap();
        row.capacity = Quantity(u64::MAX);
        row.quantity = Quantity(0);
        let before = store.clone();
        let mut batch = Batch::new();
        let bounds =
            append_deposit_schema_bounds(&store, InventoryId(10), Good::Food, &mut batch).unwrap();
        assert_eq!(bounds.capacity, Quantity(u64::MAX));
        store.validate_batch(&batch).unwrap();
        assert_eq!(store, before);
        assert!(deposit_available(
            &store,
            InventoryId(10),
            Good::Food,
            Quantity(u64::MAX)
        ));
        prepare_deposit(&store, InventoryId(10), Good::Food, Quantity(u64::MAX))
            .unwrap()
            .commit(&mut store)
            .unwrap();
        assert_eq!(
            inventory(&store, InventoryId(10)).unwrap().quantity,
            Quantity(u64::MAX)
        );
    }
}
