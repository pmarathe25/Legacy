use game_core::*;
use game_objects::*;
use game_sim::*;
#[test]
fn catalog_validates_ids_and_freezes_resolved_definitions() {
    let catalog = standard().unwrap();
    assert_eq!(catalog.resolve("grain").unwrap(), GoodId(1));
    assert_eq!(catalog.mass(GoodId(1), 10).unwrap(), 10);
    let mut invalid = catalog.clone();
    invalid.items.get_mut("timber").unwrap().id = GoodId(1);
    assert!(invalid.validate().is_err());
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    h.dispatch(Install(catalog)).unwrap();
    let save = h.save().unwrap();
    assert!(h.dispatch(Install(standard().unwrap())).is_err());
    assert_eq!(h.save().unwrap(), save);
}

#[test]
fn extended_catalog_rebuilds_id_lookup_and_rejects_duplicate_saved_ids() {
    let base = standard().unwrap();
    let first = base.items.values().map(|o| o.id.0).max().unwrap() + 1;
    let mut data = serde_json::to_value(base).unwrap();
    for id in first..first + 10_000 {
        data["items"][format!("object-{id}")] = serde_json::json!({
            "id": id, "name": format!("Object {id}"), "mass": id, "tags": [], "food": null
        });
    }
    let catalog: Objects = serde_json::from_value(data.clone()).unwrap();
    catalog.validate().unwrap();
    for id in [first, first + 4900, first + 9999] {
        assert_eq!(catalog.mass(GoodId(id), 2).unwrap(), 2 * id);
    }
    assert!(catalog.by_id(GoodId(99_999)).is_err());
    assert_eq!(
        serde_json::to_value(&catalog).unwrap(),
        data,
        "derived index stays out of content and saves"
    );
    data["items"][format!("object-{first}")]["id"] = (first + 1).into();
    let invalid: Objects = serde_json::from_value(data).unwrap();
    assert!(invalid.validate().is_err());
}
