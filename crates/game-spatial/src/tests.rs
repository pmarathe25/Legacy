use super::*;
use game_core::{PersonId, SimDuration};
use serde::{Deserialize, Serialize};

fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    Store::with_schema(schema)
}
fn road() -> Location {
    Location::InTransit {
        from: AreaId(1),
        to: AreaId(2),
        departed: SimTime(10),
        arrives: SimTime(110),
        road_start: None,
    }
}
#[test]
fn missing_locations_stay_absent_and_plain_rows_share_entity_identity() {
    let mut store = store();
    assert!(location(&store, StableId(1)).is_none());
    assert!(location_mut(&mut store, StableId(1)).is_none());
    assert!(remove_location(&mut store, StableId(1)).unwrap().is_none());
    set_location(&mut store, StableId(1), Location::AtArea(AreaId(2))).unwrap();
    assert_eq!(
        locations(&store).map(|(id, _)| id).collect::<Vec<_>>(),
        vec![StableId(1)]
    );
    *location_mut(&mut store, StableId(1)).unwrap() = Location::AtArea(AreaId(3));
    assert_eq!(
        location(&store, StableId(1)),
        Some(&Location::AtArea(AreaId(3)))
    );
    assert_eq!(
        remove_location(&mut store, StableId(1)).unwrap(),
        Some(Location::AtArea(AreaId(3)))
    );
}
#[test]
fn carrier_chains_borrow_the_actual_terminal_location_without_changing_any_rows() {
    let mut store = store();
    set_location(
        &mut store,
        StableId(1),
        Location::Carried {
            carrier: PersonId(2),
        },
    )
    .unwrap();
    set_location(
        &mut store,
        StableId(2),
        Location::Carried {
            carrier: PersonId(3),
        },
    )
    .unwrap();
    set_location(&mut store, StableId(3), road()).unwrap();
    let before = store.snapshot().unwrap();
    let (anchor, row) = resolved_anchor(&store, StableId(1)).unwrap();
    assert_eq!(anchor, StableId(3));
    assert!(std::ptr::eq(row, location(&store, StableId(3)).unwrap()));
    assert_eq!(resolve(&store, StableId(1)).unwrap(), road());
    assert_eq!(cell(&store, StableId(1)).unwrap(), AreaId(1));
    assert_eq!(store.snapshot().unwrap(), before);
    set_location(
        &mut store,
        StableId(3),
        Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(2),
            progress: SimDuration(70),
            duration: SimDuration(100),
        },
    )
    .unwrap();
    assert_eq!(
        resolve(&store, StableId(1)).unwrap(),
        location(&store, StableId(3)).unwrap().clone()
    );
    assert_eq!(cell(&store, StableId(1)).unwrap(), AreaId(1));
    set_location(&mut store, StableId(3), Location::AtArea(AreaId(2))).unwrap();
    assert_eq!(cell(&store, StableId(1)).unwrap(), AreaId(2));
}
#[test]
fn missing_anchors_and_cycles_are_explicit_errors_without_fallback_positions() {
    let mut store = store();
    set_location(
        &mut store,
        StableId(1),
        Location::Carried {
            carrier: PersonId(2),
        },
    )
    .unwrap();
    assert_eq!(
        resolve(&store, StableId(1)),
        Err(Error::InvalidSave("missing spatial anchor".into()))
    );
    assert!(resolve_locations(locations(&store)).is_err());
    set_location(
        &mut store,
        StableId(2),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        resolved_anchor(&store, StableId(1)),
        Err(Error::InvalidSave("cyclic spatial carrying".into()))
    );
    assert!(resolve_locations(locations(&store)).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    set_location(
        &mut store,
        StableId(1),
        Location::Carried {
            carrier: PersonId(1),
        },
    )
    .unwrap();
    assert!(resolve(&store, StableId(1)).is_err());
}
#[test]
fn borrowed_batch_resolution_is_independent_of_row_order_and_memoizes_shared_paths() {
    let rows = [
        (
            StableId(1),
            Location::Carried {
                carrier: PersonId(3),
            },
        ),
        (
            StableId(2),
            Location::Carried {
                carrier: PersonId(1),
            },
        ),
        (StableId(3), Location::AtArea(AreaId(9))),
        (
            StableId(4),
            Location::Carried {
                carrier: PersonId(1),
            },
        ),
    ];
    let expected = resolve_locations(rows.iter().map(|(id, row)| (*id, row))).unwrap();
    let reverse = resolve_locations(rows.iter().rev().map(|(id, row)| (*id, row))).unwrap();
    assert_eq!(expected, reverse);
    assert_eq!(
        expected.keys().copied().collect::<Vec<_>>(),
        vec![StableId(1), StableId(2), StableId(3), StableId(4)]
    );
    assert!(
        expected
            .values()
            .all(|row| *row == Location::AtArea(AreaId(9)))
    );
    assert!(resolve_locations([(StableId(1), &rows[0].1), (StableId(1), &rows[1].1)]).is_err());
    assert!(resolve_locations([(StableId(0), &rows[2].1)]).is_err());
}
#[test]
fn malformed_geometry_or_identifiers_cannot_create_or_replace_locations() {
    let mut store = store();
    set_location(&mut store, StableId(1), road()).unwrap();
    let before = store.snapshot().unwrap();
    let malformed = [
        Location::AtArea(AreaId(0)),
        Location::Carried {
            carrier: PersonId(0),
        },
        Location::InTransit {
            from: AreaId(1),
            to: AreaId(1),
            departed: SimTime(10),
            arrives: SimTime(20),
            road_start: None,
        },
        Location::InTransit {
            from: AreaId(1),
            to: AreaId(2),
            departed: SimTime(20),
            arrives: SimTime(20),
            road_start: None,
        },
        Location::InTransit {
            from: AreaId(1),
            to: AreaId(2),
            departed: SimTime(10),
            arrives: SimTime(20),
            road_start: Some(RoadPosition {
                from: AreaId(2),
                to: AreaId(3),
                progress: SimDuration(5),
                duration: SimDuration(10),
            }),
        },
        Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(2),
            progress: SimDuration(11),
            duration: SimDuration(10),
        },
        Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(2),
            progress: SimDuration(0),
            duration: SimDuration(0),
        },
    ];
    for invalid in malformed {
        for id in [StableId(1), StableId(99)] {
            assert!(set_location(&mut store, id, invalid.clone()).is_err());
        }
        assert_eq!(store.snapshot().unwrap(), before);
        let mut corrupted = before.clone();
        corrupted.columns.get_mut("spatial/location").unwrap()[0].value =
            serde_json::value::to_raw_value(&invalid).unwrap();
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        assert!(Store::restore(schema, corrupted).is_err());
    }
    assert!(set_location(&mut store, StableId(0), Location::AtArea(AreaId(1))).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn schema_absence_and_stricter_registered_validation_are_atomic() {
    let mut store = Store::with_schema(Schema::new());
    let before = store.snapshot().unwrap();
    assert!(set_location(&mut store, StableId(1), Location::AtArea(AreaId(1))).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut schema = Schema::new();
    schema
        .register::<Location>("spatial/location", |_| Err(Error::Unavailable))
        .unwrap();
    let mut store = Store::with_schema(schema);
    let before = store.snapshot().unwrap();
    assert_eq!(
        set_location(&mut store, StableId(1), road()),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn save_clone_and_restore_preserve_foreign_components_and_resolve_the_same_graph() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ForeignTag(u64);
    let schema = || {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        schema
            .register::<ForeignTag>("fixture/tag", |_| Ok(()))
            .unwrap();
        schema
    };
    let mut store = Store::with_schema(schema());
    set_location(
        &mut store,
        StableId(1),
        Location::Carried {
            carrier: PersonId(2),
        },
    )
    .unwrap();
    set_location(&mut store, StableId(2), road()).unwrap();
    store.insert(StableId(1), ForeignTag(u64::MAX)).unwrap();
    let clone = store.clone();
    let bytes = serde_json::to_vec(&store.snapshot().unwrap()).unwrap();
    let restored = Store::restore(schema(), serde_json::from_slice(&bytes).unwrap()).unwrap();
    assert_eq!(clone, store);
    assert_eq!(restored, store);
    assert_eq!(
        resolve_locations(locations(&restored)).unwrap(),
        resolve_locations(locations(&clone)).unwrap()
    );
    set_location(&mut store, StableId(1), Location::AtArea(AreaId(4))).unwrap();
    assert_eq!(
        store.get::<ForeignTag>(StableId(1)),
        Some(&ForeignTag(u64::MAX))
    );
    remove_location(&mut store, StableId(1)).unwrap();
    assert_eq!(
        store.get::<ForeignTag>(StableId(1)),
        Some(&ForeignTag(u64::MAX))
    );
}
#[test]
fn context_checks_topology_and_opening_time_without_owning_transport_rules() {
    let areas = BTreeSet::from([AreaId(1), AreaId(2)]);
    let links = BTreeSet::from([(AreaId(1), AreaId(2))]);
    let facts = Context {
        now: SimTime(10),
        areas: &areas,
        links: &links,
    };
    validate_context(&road(), &facts).unwrap();
    assert!(validate_context(&Location::AtArea(AreaId(3)), &facts).is_err());
    assert!(
        validate_context(
            &road(),
            &Context {
                now: SimTime(9),
                ..facts
            }
        )
        .is_err()
    );
    let reversed = Location::InTransit {
        from: AreaId(2),
        to: AreaId(1),
        departed: SimTime(10),
        arrives: SimTime(60),
        road_start: Some(RoadPosition {
            from: AreaId(1),
            to: AreaId(2),
            progress: SimDuration(50),
            duration: SimDuration(100),
        }),
    };
    validate(&reversed).unwrap();
    assert!(validate_context(&reversed, &facts).is_err());
    let links = BTreeSet::from([(AreaId(1), AreaId(2)), (AreaId(2), AreaId(1))]);
    validate_context(
        &reversed,
        &Context {
            now: SimTime(10),
            areas: &areas,
            links: &links,
        },
    )
    .unwrap();
    // Arrival deadlines and frozen movement rates are transport's facts. A dead
    // body or a closing boundary may retain its leg until the host reconciles it.
    validate_context(
        &road(),
        &Context {
            now: SimTime(200),
            ..facts
        },
    )
    .unwrap();
}
