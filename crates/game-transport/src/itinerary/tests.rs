use super::*;
use game_ecs::Schema;
fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    store.spawn(StableId(1)).unwrap();
    store
}
#[test]
fn route_replacement_is_native_empty_absent_and_rejects_stale_before() {
    let mut store = store();
    prepare_route_replacement(&store, PersonId(1), vec![AreaId(2), AreaId(3)])
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let plan = prepare_route_replacement(&store, PersonId(1), vec![AreaId(4)]).unwrap();
    prepare_route_replacement(&store, PersonId(1), Vec::new())
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(plan.commit(&mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(route(&store, PersonId(1)).is_none());
    assert!(prepare_route_replacement(&store, PersonId(1), vec![AreaId(0)]).is_err());
}
#[test]
fn admitted_replacement_does_not_treat_foreign_removal_as_authorization() {
    let mut store = store();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    batch.remove::<Route>(StableId(1));
    assert!(matches!(
        prepare_route_replacement(&store, PersonId(1), vec![AreaId(2)])
            .unwrap()
            .stage_into(&store, &mut batch),
        Err(Error::Unavailable)
    ));
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = Batch::new();
    prepare_route_replacement(&store, PersonId(1), Vec::new())
        .unwrap()
        .stage_into(&store, &mut batch)
        .unwrap();
    batch.insert(
        StableId(1),
        Route {
            steps: vec![AreaId(3)],
        },
    );
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn route_schema_roundtrip_rejects_empty_and_zero_steps() {
    let mut store = store();
    prepare_route_replacement(&store, PersonId(1), vec![AreaId(2)])
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.columns["transport/route"].len(), 1);
    let restored = Store::restore(store.schema().clone(), snapshot.clone()).unwrap();
    assert_eq!(route_steps(&restored, PersonId(1)), &[AreaId(2)]);
    for invalid in [
        Route { steps: Vec::new() },
        Route {
            steps: vec![AreaId(0)],
        },
    ] {
        let mut malformed = snapshot.clone();
        malformed.columns.get_mut("transport/route").unwrap()[0].value =
            serde_json::value::to_raw_value(&invalid).unwrap();
        assert!(Store::restore(store.schema().clone(), malformed).is_err());
    }
}
#[test]
fn deterministic_shortest_path_keeps_lexical_ties_and_overflow_errors() {
    let roads = KnownRoutes::from([
        (
            AreaId(1),
            BTreeMap::from([(AreaId(2), SimDuration(10)), (AreaId(3), SimDuration(10))]),
        ),
        (AreaId(2), BTreeMap::from([(AreaId(4), SimDuration(10))])),
        (AreaId(3), BTreeMap::from([(AreaId(4), SimDuration(10))])),
    ]);
    assert_eq!(
        shortest_path(&roads, AreaId(1), AreaId(4)).unwrap(),
        vec![AreaId(2), AreaId(4)]
    );
    let overflow = KnownRoutes::from([
        (
            AreaId(1),
            BTreeMap::from([(AreaId(2), SimDuration(u64::MAX))]),
        ),
        (AreaId(2), BTreeMap::from([(AreaId(3), SimDuration(1))])),
    ]);
    assert_eq!(
        shortest_path(&overflow, AreaId(1), AreaId(3)),
        Err(Error::Overflow)
    );
}
struct References {
    location: Option<Location>,
}
impl RouteReferences for References {
    fn location(&self, _: PersonId) -> Option<&Location> {
        self.location.as_ref()
    }
    fn link(&self, from: AreaId, to: AreaId) -> bool {
        (from, to) == (AreaId(1), AreaId(2))
    }
}
#[test]
fn contextual_route_validation_rejects_orphans_carried_and_nonlinks() {
    let mut store = store();
    prepare_route_replacement(&store, PersonId(1), vec![AreaId(2)])
        .unwrap()
        .commit(&mut store)
        .unwrap();
    validate_routes(
        &store,
        &References {
            location: Some(Location::InTransit {
                from: AreaId(10),
                to: AreaId(1),
                departed: SimTime(0),
                arrives: SimTime(100),
                road_start: None,
            }),
        },
    )
    .unwrap();
    for location in [
        None,
        Some(Location::AtArea(AreaId(3))),
        Some(Location::Carried {
            carrier: PersonId(2),
        }),
    ] {
        assert!(validate_routes(&store, &References { location }).is_err());
    }
}

#[test]
fn route_admission_proves_actual_staging_in_the_originating_batch() {
    let store = store();
    let mut original = Batch::new();
    let admission = prepare_route_replacement(&store, PersonId(1), vec![AreaId(2)])
        .unwrap()
        .stage_into(&store, &mut original)
        .unwrap();
    let mut identical_foreign = Batch::new();
    prepare_route_replacement(&store, PersonId(1), vec![AreaId(2)])
        .unwrap()
        .stage_into(&store, &mut identical_foreign)
        .unwrap();
    assert_eq!(
        admission.validate_footprint(&store, &identical_foreign),
        Err(Error::Unavailable)
    );
    original.remove::<OfferInbox>(StableId(1));
    admission.validate_footprint(&store, &original).unwrap();
    original.remove::<Route>(StableId(1));
    assert_eq!(
        admission.validate_footprint(&store, &original),
        Err(Error::Unavailable)
    );
}
