use super::*;
use game_ecs::{Schema, StableId, Store};
fn schema() -> Schema {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<String>("test/foreign", |_| Ok(()))
        .unwrap();
    schema
}
fn stock(id: u64) -> Inventory {
    Inventory {
        id: InventoryId(id),
        location: InventoryLocation::Area(AreaId(1)),
        quantity: Quantity(100),
        capacity: Quantity(1000),
        access: BTreeSet::from([PersonId(2)]),
        last_granted: None,
        good: Good::Water,
        spoils_at: None,
    }
}
#[test]
fn inventories_roundtrip_in_id_order_and_preserve_foreign_rows() {
    let mut store = Store::with_schema(schema());
    for id in [8, 3, 5] {
        insert_inventory(&mut store, stock(id)).unwrap();
    }
    store.insert(StableId(3), "marker".to_string()).unwrap();
    assert_eq!(
        inventories(&store).map(|(id, _)| id.0).collect::<Vec<_>>(),
        vec![3, 5, 8]
    );
    let restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
    assert_eq!(store, restored);
    assert_eq!(
        remove_inventory(&mut store, InventoryId(3)).unwrap(),
        Some(stock(3))
    );
    assert_eq!(
        store.get::<String>(StableId(3)).map(String::as_str),
        Some("marker")
    );
}
#[test]
fn invalid_inventory_admission_is_inert_and_schema_rejects_bad_snapshots() {
    let mut store = Store::with_schema(schema());
    insert_inventory(&mut store, stock(3)).unwrap();
    let before = store.snapshot().unwrap();
    for row in [
        Inventory {
            id: InventoryId(0),
            ..stock(9)
        },
        Inventory {
            quantity: Quantity(1001),
            ..stock(9)
        },
        Inventory {
            location: InventoryLocation::Carried(PersonId(0)),
            ..stock(9)
        },
        Inventory {
            last_granted: Some(PersonId(0)),
            ..stock(9)
        },
    ] {
        assert!(insert_inventory(&mut store, row).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
    let mut snapshot = serde_json::to_value(before).unwrap();
    snapshot["columns"]["cargo/inventory"][0]["value"]["quantity"] = serde_json::json!(1001);
    assert!(Store::restore(schema(), serde_json::from_value(snapshot).unwrap()).is_err());
}

#[test]
fn intrinsic_restore_does_not_replace_owner_identity_validation() {
    let mut store = Store::with_schema(schema());
    store.insert_or_spawn(StableId(9), stock(3)).unwrap();
    let restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
    assert!(validate_storage(&restored).is_err());
    assert_eq!(store, restored);
}

#[test]
fn fresh_receiving_split_and_creation_are_guarded_atomic_cargo_effects() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let source = Inventory {
        id: InventoryId(10),
        good: Good::Record,
        location: InventoryLocation::Area(AreaId(1)),
        quantity: Quantity(2000),
        capacity: Quantity(5000),
        access: BTreeSet::new(),
        last_granted: Some(PersonId(1)),
        spoils_at: None,
    };
    insert_inventory(&mut store, source.clone()).unwrap();
    let mut receiving = source.clone();
    receiving.id = InventoryId(11);
    receiving.quantity = Quantity(0);
    receiving.capacity = Quantity(1000);
    receiving.last_granted = None;
    for amount in [0, 1001, 2001] {
        assert!(
            prepare_inventory_split(&store, source.id, receiving.clone(), Quantity(amount))
                .is_err()
        );
    }
    let plan =
        prepare_inventory_split(&store, source.id, receiving.clone(), Quantity(1000)).unwrap();
    inventory_mut(&mut store, source.id)
        .unwrap()
        .access
        .insert(PersonId(2));
    let stale = store.clone();
    assert!(
        plan.stage_into(&store, &mut game_ecs::Batch::new())
            .is_err()
    );
    assert_eq!(store, stale);
    let plan =
        prepare_inventory_split(&store, source.id, receiving.clone(), Quantity(1000)).unwrap();
    let mut batch = game_ecs::Batch::new();
    plan.stage_into(&store, &mut batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        inventory(&store, source.id).unwrap().quantity,
        Quantity(1000)
    );
    assert_eq!(
        inventory(&store, source.id).unwrap().last_granted,
        Some(PersonId(1))
    );
    assert_eq!(
        inventory(&store, receiving.id).unwrap().quantity,
        Quantity(1000)
    );
    assert!(prepare_inventory_creation(&store, receiving.clone()).is_err());
    receiving.id = InventoryId(12);
    receiving.quantity = Quantity(1000);
    let mut batch = game_ecs::Batch::new();
    prepare_inventory_creation(&store, receiving)
        .unwrap()
        .stage_into(&store, &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        inventory(&store, InventoryId(12)).unwrap().quantity,
        Quantity(1000)
    );
}

#[test]
fn split_receipts_reject_foreign_writes_and_preserve_unrelated_components() {
    let mut store = Store::with_schema(schema());
    insert_inventory(&mut store, stock(3)).unwrap();
    let mut receiving = stock(4);
    receiving.quantity = Quantity(0);
    let before = store.snapshot().unwrap();
    for conflicting in [3, 4] {
        let plan = prepare_inventory_split(&store, InventoryId(3), receiving.clone(), Quantity(40))
            .unwrap();
        let mut batch = game_ecs::Batch::new();
        let guard = plan.stage_into(&store, &mut batch).unwrap();
        assert!(guard.validate_batch(&game_ecs::Batch::new()).is_err());
        batch.remove::<Inventory>(StableId(conflicting));
        assert!(guard.validate_batch(&batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
    let plan =
        prepare_inventory_split(&store, InventoryId(3), receiving.clone(), Quantity(40)).unwrap();
    let mut batch = game_ecs::Batch::new();
    let guard = plan.stage_into(&store, &mut batch).unwrap();
    batch.insert(StableId(3), "independent".to_string());
    guard.validate_batch(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        inventory(&store, InventoryId(3)).unwrap().quantity,
        Quantity(60)
    );
    assert_eq!(
        inventory(&store, receiving.id).unwrap().quantity,
        Quantity(40)
    );
    assert_eq!(store.get::<String>(StableId(3)).unwrap(), "independent");
}

fn forced_source() -> Inventory {
    Inventory {
        id: InventoryId(3),
        location: InventoryLocation::Carried(PersonId(2)),
        good: Good::Binding,
        quantity: Quantity(1500),
        capacity: Quantity(2000),
        access: BTreeSet::new(),
        last_granted: Some(PersonId(2)),
        spoils_at: Some(SimTime(900)),
    }
}
fn forced(store: &Store) -> PreparedForcedSplit {
    prepare_forced_split(
        store,
        InventoryId(3),
        Quantity(1000),
        InventoryId(4),
        InventoryLocation::Carried(PersonId(9)),
        BTreeSet::from([PersonId(2)]),
    )
    .unwrap()
}
#[test]
fn forced_split_owns_balance_and_new_compartment_metadata_without_receiving_consent() {
    let mut store = Store::with_schema(schema());
    insert_inventory(&mut store, forced_source()).unwrap();
    let before = store.snapshot().unwrap();
    let prepared = forced(&store);
    assert_eq!(prepared.receiving().good, Good::Binding);
    assert_eq!(prepared.receiving().capacity, Quantity(1000));
    assert_eq!(prepared.receiving().last_granted, None);
    assert_eq!(prepared.receiving().spoils_at, None);
    let mut batch = game_ecs::Batch::new();
    let guard = prepared.stage_into(&store, &mut batch).unwrap();
    batch.insert(StableId(3), "owner receipt".to_owned());
    guard.validate(&store, &batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    let source = inventory(&store, InventoryId(3)).unwrap();
    let applied = inventory(&store, InventoryId(4)).unwrap();
    assert_eq!(source.quantity.0 + applied.quantity.0, 1500);
    assert_eq!(source.last_granted, Some(PersonId(2)));
    assert_eq!(source.spoils_at, Some(SimTime(900)));
    assert_eq!(applied.location, InventoryLocation::Carried(PersonId(9)));
    assert_eq!(applied.access, BTreeSet::from([PersonId(2)]));
}
#[test]
fn forced_split_rejects_overdraw_stale_source_and_taken_identity_without_publication() {
    let mut store = Store::with_schema(schema());
    insert_inventory(&mut store, forced_source()).unwrap();
    let before = store.snapshot().unwrap();
    for quantity in [0, 1501] {
        assert!(
            prepare_forced_split(
                &store,
                InventoryId(3),
                Quantity(quantity),
                InventoryId(4),
                InventoryLocation::Carried(PersonId(9)),
                BTreeSet::new()
            )
            .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    let prepared = forced(&store);
    inventory_mut(&mut store, InventoryId(3)).unwrap().quantity = Quantity(1499);
    let changed = store.snapshot().unwrap();
    assert!(
        prepared
            .stage_into(&store, &mut game_ecs::Batch::new())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), changed);
    let prepared = forced(&store);
    store.spawn(StableId(4)).unwrap();
    let changed = store.snapshot().unwrap();
    assert!(prepared.commit(&mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), changed);
}
#[test]
fn forced_split_guard_protects_stock_rows_and_the_exclusive_new_entity_shape() {
    let mut store = Store::with_schema(schema());
    insert_inventory(&mut store, forced_source()).unwrap();
    let before = store.snapshot().unwrap();
    for conflict in [0, 1, 2] {
        let mut batch = game_ecs::Batch::new();
        let guard = forced(&store).stage_into(&store, &mut batch).unwrap();
        match conflict {
            0 => batch.remove::<Inventory>(StableId(3)),
            1 => batch.remove::<Inventory>(StableId(4)),
            _ => batch.insert(StableId(4), "foreign minted component".to_owned()),
        };
        assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
    }
    let mut batch = game_ecs::Batch::new();
    batch.insert(StableId(4), "preexisting minted component".to_owned());
    assert!(forced(&store).stage_into(&store, &mut batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
