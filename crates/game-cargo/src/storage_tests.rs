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
