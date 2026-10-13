//! Empty compartments and newborn personal defaults enter their owners' shared batch.
use crate::*;
use game_core::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

#[derive(Clone, Debug)]
pub struct PreparedPropertyInitialization {
    child: Option<PersonId>,
    inventory: InventoryId,
    reads: ReadSet,
}
fn prepare_initial(
    store: &Store,
    child: Option<PersonId>,
    inventory: InventoryId,
) -> Result<PreparedPropertyInitialization, Error> {
    if inventory.0 == 0 || child.is_some_and(|child| child.0 == 0) {
        return Err(Error::Unavailable);
    }
    let mut reads = ReadSet::new();
    if reads
        .read::<Provenance>(store, StableId(inventory.0))
        .is_some()
    {
        return Err(Error::Unavailable);
    }
    reads.entity(store, StableId(inventory.0));
    if let Some(stock) = game_cargo::inventory(store, inventory) {
        if stock.quantity.0 != 0 || stock.id != inventory {
            return Err(Error::Unavailable);
        }
        reads.expect_row(StableId(inventory.0), Some(stock.clone()));
    }
    if let Some(child) = child {
        let id = StableId(child.0);
        reads.entity(store, id);
        if reads.read::<Attitude>(store, id).is_some()
            || reads.read::<Knowledge>(store, id).is_some()
            || reads.read::<Requests>(store, id).is_some()
        {
            return Err(Error::Unavailable);
        }
    }
    Ok(PreparedPropertyInitialization {
        child,
        inventory,
        reads,
    })
}
pub fn prepare_newborn(
    store: &Store,
    child: PersonId,
    inventory: InventoryId,
) -> Result<PreparedPropertyInitialization, Error> {
    prepare_initial(store, Some(child), inventory)
}
/// Cargo supplies the actual prepared empty compartment, without publishing it.
pub fn prepare_empty_inventory(
    store: &Store,
    stock: &game_cargo::Inventory,
) -> Result<PreparedPropertyInitialization, Error> {
    stock.validate()?;
    if stock.quantity.0 != 0 {
        return Err(Error::Unavailable);
    }
    prepare_initial(store, None, stock.id)
}
impl PreparedPropertyInitialization {
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        let mut admitted = ReadFootprint::new();
        let inventory = StableId(self.inventory.0);
        if batch.touches::<Provenance>(inventory) {
            return Err(Error::Unavailable);
        }
        admitted.protect::<Provenance>(inventory);
        batch.insert(inventory, Provenance::default());
        if let Some(child) = self.child {
            let child = StableId(child.0);
            if batch.touches::<Attitude>(child)
                || batch.touches::<Knowledge>(child)
                || batch.touches::<Requests>(child)
            {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Attitude>(child);
            admitted.protect::<Knowledge>(child);
            admitted.protect::<Requests>(child);
            batch.insert(
                child,
                Attitude {
                    exclusive_claims: 0,
                    sharing: 0,
                    willingness_to_defend: 0,
                },
            );
            batch.insert(child, Knowledge::default());
            batch.insert(
                child,
                Requests {
                    next_search: SimTime(0),
                },
            );
        }
        self.reads.bind(store, batch, &admitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newborn_defaults_and_cargo_slot_publish_together_and_born_replay_retains_learning() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let before = store.snapshot().unwrap();
        let cargo =
            game_cargo::prepare_newborn_inventory(&store, InventoryId(2), PersonId(1)).unwrap();
        let property = prepare_newborn(&store, PersonId(1), InventoryId(2)).unwrap();
        let mut batch = Batch::new();
        let cargo = cargo.stage_into(&store, &mut batch).unwrap();
        let property = property.stage_into(&store, &mut batch).unwrap();
        cargo.validate(&store, &batch).unwrap();
        property.validate(&store, &batch).unwrap();
        assert_eq!(store.snapshot().unwrap(), before);
        store.apply_batch(batch).unwrap();
        assert!(portions(&store, InventoryId(2)).unwrap().is_empty());
        assert!(known_claims(&store, PersonId(1)).unwrap().is_empty());
        assert_eq!(
            requests(&store, PersonId(1)).unwrap().next_search,
            SimTime(0)
        );
        attitude_mut(&mut store, PersonId(1)).unwrap().sharing = 700;
        let mut runtime = Runtime::default();
        runtime
            .prepare_receipts(
                &store,
                &[PropertyReceipt {
                    event: 1,
                    change: ReceiptChange::Born {
                        person: PersonId(1),
                    },
                }],
                &std::collections::BTreeMap::new(),
                1,
            )
            .unwrap()
            .commit(&mut runtime, &mut store)
            .unwrap();
        assert_eq!(attitude(&store, PersonId(1)).unwrap().sharing, 700);
        assert!(prepare_newborn(&store, PersonId(1), InventoryId(2)).is_err());
    }
    #[test]
    fn empty_inventory_and_newborn_initialization_reject_late_owned_row_overwrites() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        let store = Store::with_schema(schema);
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let guard = prepare_newborn(&store, PersonId(1), InventoryId(2))
            .unwrap()
            .stage_into(&store, &mut batch)
            .unwrap();
        batch.remove::<Knowledge>(StableId(1));
        assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
        let stock =
            game_cargo::prepare_newborn_inventory(&store, InventoryId(2), PersonId(1)).unwrap();
        let mut batch = Batch::new();
        let guard = prepare_empty_inventory(&store, stock.receiving())
            .unwrap()
            .stage_into(&store, &mut batch)
            .unwrap();
        batch.remove::<Provenance>(StableId(2));
        assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
