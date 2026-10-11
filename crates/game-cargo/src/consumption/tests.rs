use super::*;
use crate::{InventoryLocation, insert_inventory, inventory_mut, register_components};
use game_core::{AreaId, Good, PersonId, SimTime};
use std::collections::BTreeSet;
fn stock() -> Inventory {
    Inventory {
        id: InventoryId(4),
        location: InventoryLocation::Area(AreaId(2)),
        quantity: Quantity(1_000),
        capacity: Quantity(2_000),
        access: BTreeSet::from([PersonId(3)]),
        last_granted: Some(PersonId(3)),
        good: Good::Timber,
        spoils_at: Some(SimTime(900)),
    }
}
fn store() -> Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<String>("test/receipt", |row| {
            if row == "invalid" {
                Err(Error::InvalidSave("invalid receipt".into()))
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    insert_inventory(&mut store, stock()).unwrap();
    store
        .insert_or_spawn(StableId(4), "standing".to_owned())
        .unwrap();
    store
}
#[test]
fn repeated_debits_preserve_stock_fields_and_cannot_overdraw() {
    let mut store = store();
    let before = store.snapshot().unwrap();
    assert!(
        prepare_consumptions(
            &store,
            &[
                (InventoryId(4), Quantity(600)),
                (InventoryId(4), Quantity(401))
            ]
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    prepare_consumptions(
        &store,
        &[
            (InventoryId(4), Quantity(600)),
            (InventoryId(4), Quantity(400)),
        ],
    )
    .unwrap()
    .commit(&mut store)
    .unwrap();
    let mut expected = stock();
    expected.quantity = Quantity(0);
    assert_eq!(inventory(&store, InventoryId(4)), Some(&expected));
    assert_eq!(
        store.get::<String>(StableId(4)).map(String::as_str),
        Some("standing")
    );
}
#[test]
fn stale_stock_rejects_consumption_but_foreign_row_edits_do_not() {
    let mut store = store();
    let prepared = prepare_consumptions(&store, &[(InventoryId(4), Quantity(500))]).unwrap();
    inventory_mut(&mut store, InventoryId(4))
        .unwrap()
        .access
        .clear();
    let before = store.snapshot().unwrap();
    assert_eq!(prepared.commit(&mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    let prepared = prepare_consumptions(&store, &[(InventoryId(4), Quantity(500))]).unwrap();
    *store.get_mut::<String>(StableId(4)).unwrap() = "new observation".into();
    prepared.commit(&mut store).unwrap();
    assert_eq!(
        inventory(&store, InventoryId(4)).unwrap().quantity,
        Quantity(500)
    );
}
#[test]
fn foreign_batch_rejection_and_custom_stock_validation_publish_no_debits() {
    let mut store = store();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    prepare_consumptions(&store, &[(InventoryId(4), Quantity(500))])
        .unwrap()
        .stage_into(&store, &mut batch)
        .unwrap();
    batch.insert(StableId(4), "invalid".to_owned());
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<Inventory>("test/custom-stock", |row| {
            row.validate()?;
            if row.quantity.0 < 800 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut constrained = Store::with_schema(schema);
    insert_inventory(&mut constrained, stock()).unwrap();
    let before = constrained.snapshot().unwrap();
    assert!(
        prepare_consumptions(&constrained, &[(InventoryId(4), Quantity(500))])
            .unwrap()
            .commit(&mut constrained)
            .is_err()
    );
    assert_eq!(constrained.snapshot().unwrap(), before);
}
