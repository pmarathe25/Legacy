use super::*;
use game_core::{PersonId, SimDuration};
fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    Store::with_schema(schema)
}
#[test]
fn raw_stationary_visibility_is_distinct_from_carried_area_and_coarse_road_cells() {
    let mut store = store();
    set_location(&mut store, StableId(1), Location::AtArea(AreaId(10))).unwrap();
    set_location(
        &mut store,
        StableId(2),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    set_location(
        &mut store,
        StableId(3),
        Location::StoppedOnRoad {
            from: AreaId(10),
            to: AreaId(11),
            progress: SimDuration(40),
            duration: SimDuration(100),
        },
    )
    .unwrap();
    set_location(
        &mut store,
        StableId(4),
        Location::StoppedOnRoad {
            from: AreaId(11),
            to: AreaId(10),
            progress: SimDuration(180),
            duration: SimDuration(300),
        },
    )
    .unwrap();
    for actor in 5..=100 {
        set_location(&mut store, StableId(actor), Location::AtArea(AreaId(20))).unwrap();
    }
    let roster = LocalRoster::build(&store).unwrap();
    assert_eq!(
        roster.raw_at_area(AreaId(10)).collect::<Vec<_>>(),
        [StableId(1)]
    );
    assert_eq!(
        roster.resolved_at_area(AreaId(10)).collect::<Vec<_>>(),
        [StableId(1), StableId(2)]
    );
    assert_eq!(
        roster.cell(AreaId(10)).collect::<Vec<_>>(),
        [StableId(1), StableId(2), StableId(3)]
    );
    assert_eq!(
        roster.on_road(AreaId(10), AreaId(11)).collect::<Vec<_>>(),
        [StableId(3), StableId(4)]
    );
    assert_eq!(
        roster.position_candidates(StableId(3)).unwrap(),
        [StableId(1), StableId(2), StableId(3), StableId(4)]
    );
    assert_eq!(roster.indexed(), 100);
}
#[test]
fn value_change_birth_carrier_reassignment_and_restoration_rebuild_from_canonical_rows() {
    let mut store = store();
    set_location(&mut store, StableId(1), Location::AtArea(AreaId(10))).unwrap();
    set_location(
        &mut store,
        StableId(2),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    {
        let roster = LocalRoster::build(&store).unwrap();
        assert_eq!(roster.resolved_at_area(AreaId(10)).count(), 2);
    }
    *location_mut(&mut store, StableId(1)).unwrap() = Location::AtArea(AreaId(11));
    set_location(&mut store, StableId(3), Location::AtArea(AreaId(12))).unwrap();
    set_location(
        &mut store,
        StableId(4),
        Location::Carried {
            carrier: PersonId(2),
        },
    )
    .unwrap();
    *location_mut(&mut store, StableId(2)).unwrap() = Location::Carried {
        carrier: PersonId(3),
    };
    let expected = LocalRoster::build(&store).unwrap();
    assert_eq!(
        expected.resolved_at_area(AreaId(12)).collect::<Vec<_>>(),
        [StableId(2), StableId(3), StableId(4)]
    );
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    let actual = LocalRoster::build(&restored).unwrap();
    for area in [AreaId(10), AreaId(11), AreaId(12)] {
        assert_eq!(
            expected.raw_at_area(area).collect::<Vec<_>>(),
            actual.raw_at_area(area).collect::<Vec<_>>()
        );
        assert_eq!(
            expected.resolved_at_area(area).collect::<Vec<_>>(),
            actual.resolved_at_area(area).collect::<Vec<_>>()
        );
    }
}
#[test]
fn phase_index_rejects_missing_anchors_and_cycles_instead_of_guessing_cells() {
    let mut store = store();
    set_location(
        &mut store,
        StableId(1),
        Location::Carried {
            carrier: PersonId(2),
        },
    )
    .unwrap();
    assert!(LocalRoster::build(&store).is_err());
    set_location(
        &mut store,
        StableId(2),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    assert!(LocalRoster::build(&store).is_err());
}
