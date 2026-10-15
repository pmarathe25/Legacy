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
