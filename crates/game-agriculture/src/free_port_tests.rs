use super::*;
use game_ecs::{Schema, Store};
#[test]
fn independent_cultivation_ports_validate_and_preserve_other_owner_rows() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_land::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let soil = Soil {
        fertility: 800,
        water: 75,
        capacity: 120,
    };
    game_land::insert_soil(&mut store, SiteId(50), soil.clone()).unwrap();
    let row = Cultivation {
        field: SiteId(50),
        operation: FarmOperation::Prepare,
        labor_ms: 0,
        completes: SimTime(500),
    };
    insert_cultivation(&mut store, PersonId(2), row.clone()).unwrap();
    assert_eq!(cultivation(&store, PersonId(2)), Some(&row));
    cultivation_mut(&mut store, PersonId(2)).unwrap().labor_ms = 100;
    assert_eq!(cultivations(&store).next().unwrap().0, PersonId(2));
    assert_eq!(
        remove_cultivation(&mut store, PersonId(2))
            .unwrap()
            .unwrap()
            .labor_ms,
        100
    );
    assert!(
        remove_cultivation(&mut store, PersonId(999))
            .unwrap()
            .is_none()
    );
    let before = store.snapshot().unwrap();
    let mut invalid = row;
    invalid.field = SiteId(0);
    assert!(insert_cultivation(&mut store, PersonId(99), invalid).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(game_land::soil(&store, SiteId(50)), Some(&soil));
}
