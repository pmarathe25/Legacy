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
fn row_consumption_returns_conserved_clone_and_rejects_overdraw() {
    let before = stock();
    let mut expected = before.clone();
    expected.quantity = Quantity(750);
    assert_eq!(
        consume_inventory_row(InventoryId(4), &before, Quantity(250)),
        Ok(expected)
    );
    assert_eq!(before, stock());
    assert_eq!(
        consume_inventory_row(InventoryId(4), &before, Quantity(1_001)),
        Err(Error::Unavailable)
    );
    assert_eq!(before, stock());
    assert_eq!(
        consume_inventory_row(InventoryId(5), &before, Quantity(1)),
        Err(Error::Unavailable)
    );
    assert_eq!(before, stock());
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

fn portion_facts(good: Good, budget: u64) -> ConsumptionFacts {
    ConsumptionFacts {
        good,
        accessible: true,
        contact: true,
        unreserved: Quantity(budget),
    }
}
#[test]
fn portion_admission_uses_shared_budget_contact_access_and_requested_good_without_spending() {
    let mut store = store();
    inventory_mut(&mut store, InventoryId(4)).unwrap().good = Good::Food;
    let stock = inventory(&store, InventoryId(4)).unwrap();
    let facts = portion_facts(Good::Food, 500);
    let before = store.snapshot().unwrap();
    assert!(portion_available(stock, Quantity(500), facts));
    for invalid in [
        ConsumptionFacts {
            unreserved: Quantity(499),
            ..facts
        },
        ConsumptionFacts {
            accessible: false,
            ..facts
        },
        ConsumptionFacts {
            contact: false,
            ..facts
        },
        ConsumptionFacts {
            good: Good::Water,
            ..facts
        },
    ] {
        assert!(!portion_available(stock, Quantity(500), invalid));
        assert!(
            prepare_food_consumption(&store, PersonId(7), InventoryId(4), Quantity(500), invalid)
                .is_err()
        );
    }
    assert!(!portion_available(stock, Quantity(0), facts));
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn consumed_food_grant_metadata_publishes_with_receivers_and_rejects_late_edits() {
    let mut store = store();
    inventory_mut(&mut store, InventoryId(4)).unwrap().good = Good::Food;
    let before = store.snapshot().unwrap();
    let prepare = |store: &Store| {
        prepare_food_consumption(
            store,
            PersonId(7),
            InventoryId(4),
            Quantity(500),
            portion_facts(Good::Food, 1000),
        )
        .unwrap()
    };
    let mut batch = Batch::new();
    let guard = prepare(&store).stage_into(&store, &mut batch).unwrap();
    guard.validate(&store, &batch).unwrap();
    batch.insert(StableId(4), "invalid".to_owned());
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = Batch::new();
    let guard = prepare(&store).stage_into(&store, &mut batch).unwrap();
    batch.remove::<Inventory>(StableId(4));
    assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    prepare(&store).commit(&mut store).unwrap();
    let stock = inventory(&store, InventoryId(4)).unwrap();
    assert_eq!(stock.quantity, Quantity(500));
    assert_eq!(stock.last_granted, Some(PersonId(7)));
    assert_eq!(stock.access, BTreeSet::from([PersonId(3)]));
}
#[test]
fn generic_care_portion_retains_water_material_and_rejects_stale_opening_rows() {
    let mut store = store();
    inventory_mut(&mut store, InventoryId(4)).unwrap().good = Good::Water;
    let facts = portion_facts(Good::Water, 1000);
    assert!(
        prepare_food_consumption(&store, PersonId(7), InventoryId(4), Quantity(500), facts)
            .is_err()
    );
    let prepared =
        prepare_portion_consumption(&store, PersonId(7), InventoryId(4), Quantity(500), facts)
            .unwrap();
    inventory_mut(&mut store, InventoryId(4))
        .unwrap()
        .last_granted = Some(PersonId(8));
    let before = store.snapshot().unwrap();
    assert_eq!(prepared.commit(&mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    prepare_portion_consumption(&store, PersonId(7), InventoryId(4), Quantity(500), facts)
        .unwrap()
        .commit(&mut store)
        .unwrap();
    assert_eq!(inventory(&store, InventoryId(4)).unwrap().good, Good::Water);
    assert_eq!(
        inventory(&store, InventoryId(4)).unwrap().quantity,
        Quantity(500)
    );
}

#[test]
fn delayed_portion_admission_never_debits_and_protects_source_through_activity_publication() {
    let mut store = store();
    inventory_mut(&mut store, InventoryId(4)).unwrap().good = Good::Food;
    let before = store.snapshot().unwrap();
    let prepare = |store: &Store| {
        prepare_food_consumption(
            store,
            PersonId(7),
            InventoryId(4),
            Quantity(500),
            portion_facts(Good::Food, 1000),
        )
        .unwrap()
    };
    let mut batch = Batch::new();
    batch.insert(StableId(4), "activity starts".to_owned());
    let guard = prepare(&store).guard_admission(&store, &batch).unwrap();
    guard.validate(&store, &batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    batch.remove::<Inventory>(StableId(4));
    assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = Batch::new();
    batch.insert(StableId(4), "activity starts".to_owned());
    let prepared = prepare(&store);
    let guard = prepared.guard_admission(&store, &batch).unwrap();
    guard.validate(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        inventory(&store, InventoryId(4)).unwrap().quantity,
        Quantity(1000)
    );
    assert_eq!(
        inventory(&store, InventoryId(4)).unwrap().last_granted,
        Some(PersonId(3))
    );
    let mut preexisting = Batch::new();
    preexisting.insert(
        StableId(4),
        inventory(&store, InventoryId(4)).unwrap().clone(),
    );
    assert!(prepared.guard_admission(&store, &preexisting).is_err());
}

#[test]
fn spoilage_clears_quantity_and_deadline_and_preserves_every_other_stock_field() {
    let mut store = store();
    let before = store.snapshot().unwrap();
    let prepared = prepare_spoilage(&store, InventoryId(4)).unwrap();
    assert_eq!(prepared.inventory(), InventoryId(4));
    assert_eq!(prepared.quantity(), Quantity(1000));
    let mut expected = stock();
    expected.quantity = Quantity(0);
    expected.spoils_at = None;
    assert_eq!(prepared.closing(), &expected);
    assert_eq!(store.snapshot().unwrap(), before);
    prepared.commit(&mut store).unwrap();
    assert_eq!(inventory(&store, InventoryId(4)), Some(&expected));
    let empty = prepare_spoilage(&store, InventoryId(4)).unwrap();
    assert_eq!(empty.quantity(), Quantity(0));
    empty.commit(&mut store).unwrap();
    assert_eq!(inventory(&store, InventoryId(4)), Some(&expected));
}
#[test]
fn spoilage_rejects_stale_deadline_and_late_foreign_inventory_writes_without_publication() {
    let mut store = store();
    let prepared = prepare_spoilage(&store, InventoryId(4)).unwrap();
    inventory_mut(&mut store, InventoryId(4)).unwrap().spoils_at = Some(SimTime(901));
    let changed = store.snapshot().unwrap();
    assert_eq!(prepared.commit(&mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), changed);
    let mut batch = Batch::new();
    let guard = prepare_spoilage(&store, InventoryId(4))
        .unwrap()
        .stage_into(&store, &mut batch)
        .unwrap();
    batch.remove::<Inventory>(StableId(4));
    assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), changed);
}
