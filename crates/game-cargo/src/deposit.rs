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
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.validate(store)?;
        let id = StableId(self.before.id.0);
        let mut reads = game_ecs::ReadSet::new();
        reads.expect_row(id, Some(self.before.clone()));
        if batch.touches::<Inventory>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Inventory>(id);
        self.append_writes(batch);
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
/// Return a checked credit to one already-selected inventory row.
///
/// Cargo owns receiving-good and compartment-capacity invariants here. Source
/// selection, access, reservations, and the reason for receipt belong to the
/// requesting owner; this transform does not read or mutate a Store.
pub fn deposit_inventory_row(
    destination: InventoryId,
    row: &Inventory,
    good: Good,
    amount: Quantity,
) -> Result<Inventory, Error> {
    row.validate()?;
    if row.id != destination || row.good != good || amount.0 == 0 || row.room()? < amount {
        return Err(Error::Unavailable);
    }
    let mut after = row.clone();
    after.quantity.0 = after
        .quantity
        .0
        .checked_add(amount.0)
        .ok_or(Error::Overflow)?;
    Ok(after)
}
pub fn deposit_available(
    store: &Store,
    destination: InventoryId,
    good: Good,
    amount: Quantity,
) -> bool {
    inventory(store, destination)
        .is_some_and(|row| deposit_inventory_row(destination, row, good, amount).is_ok())
}
pub fn prepare_deposit(
    store: &Store,
    destination: InventoryId,
    good: Good,
    amount: Quantity,
) -> Result<PreparedDeposit, Error> {
    let before = inventory(store, destination)
        .cloned()
        .ok_or(Error::Unavailable)?;
    let after = deposit_inventory_row(destination, &before, good, amount)?;
    Ok(PreparedDeposit { before, after })
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
    fn row_deposit_returns_conserved_clone_and_enforces_good_and_capacity() {
        let before = row();
        let mut expected = before.clone();
        expected.quantity = Quantity(350);
        assert_eq!(
            deposit_inventory_row(InventoryId(10), &before, Good::Food, Quantity(250)),
            Ok(expected)
        );
        assert_eq!(before, row());
        assert_eq!(
            deposit_inventory_row(InventoryId(10), &before, Good::Water, Quantity(1)),
            Err(Error::Unavailable)
        );
        assert_eq!(
            deposit_inventory_row(InventoryId(10), &before, Good::Food, Quantity(901)),
            Err(Error::Unavailable)
        );

        // A valid row at the u64 ceiling cannot reach checked_add overflow:
        // the capacity admission rejects any amount that would overflow.
        let mut at_ceiling = before.clone();
        at_ceiling.quantity = Quantity(u64::MAX - 1);
        at_ceiling.capacity = Quantity(u64::MAX);
        assert_eq!(
            deposit_inventory_row(InventoryId(10), &at_ceiling, Good::Food, Quantity(2)),
            Err(Error::Unavailable)
        );
        assert_eq!(at_ceiling.quantity, Quantity(u64::MAX - 1));
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
