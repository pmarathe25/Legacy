use crate::*;
use game_ecs::{Batch, StableId, Store};
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<String>("test/foreign", |value| {
            if value == "reject" {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema
}
fn facts() -> BTreeMap<PersonId, Body> {
    (1..=4)
        .map(|id| {
            (
                PersonId(id),
                Body {
                    position: Position::Area(AreaId(10)),
                    mass: Quantity(60_000),
                    cargo: Quantity(0),
                    capacity: Quantity(100_000),
                    can_hold: true,
                    can_walk: true,
                    can_bear: true,
                },
            )
        })
        .collect()
}
fn fixture() -> (Runtime, Store, BTreeMap<PersonId, Body>) {
    let mut store = Store::with_schema(schema());
    store
        .insert_or_spawn(StableId(1), "standing".to_owned())
        .unwrap();
    (Runtime::new(Rules::default()).unwrap(), store, facts())
}
fn support() -> SupportRequest {
    SupportRequest {
        passenger: PersonId(3),
        requester: PersonId(3),
        observed_at: SimTime(1),
        expires: SimTime(100),
    }
}
fn response() -> Response {
    Response {
        passenger: PersonId(3),
        bearer: PersonId(1),
        kind: ResponseKind::WaitingForSupport,
        reason: "needs support".into(),
        observed_at: SimTime(1),
    }
}
#[test]
fn seven_columns_admit_only_intrinsic_rows_without_metadata_entity_maps() {
    let (runtime, mut store, _) = fixture();
    assert_eq!(
        schema()
            .keys()
            .filter(|key| key.starts_with("transport/"))
            .count(),
        7
    );
    let before = store.snapshot().unwrap();
    assert!(insert_offer_inbox(&mut store, PersonId(3), OfferInbox::default()).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let value = serde_json::to_value(&runtime).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 2);
    for key in [
        "groups",
        "offers",
        "lifts",
        "approaches",
        "resume_targets",
        "paused_approaches",
        "known_support",
    ] {
        let mut value = value.clone();
        value[key] = serde_json::json!({});
        assert!(serde_json::from_value::<Runtime>(value).is_err());
    }
    let mut unregistered = Store::with_schema(game_ecs::Schema::new());
    assert!(
        insert_resume_target(
            &mut unregistered,
            PersonId(1),
            ResumeTarget { area: AreaId(10) }
        )
        .is_err()
    );
    assert_eq!(unregistered.count::<ResumeTarget>(), 0);
}
#[test]
fn support_and_global_response_ledger_publish_after_one_admission() {
    let (mut runtime, mut store, _) = fixture();
    let before = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_review(
            &store,
            response(),
            Some(&support()),
            [PersonId(1), PersonId(2)],
        )
        .unwrap();
    let mut batch = Batch::new();
    let finish = prepared
        .stage_into(
            &runtime,
            &store,
            &BTreeMap::<PersonId, Body>::new(),
            SimTime(1),
            &mut batch,
        )
        .unwrap();
    batch.insert(StableId(1), "reject".to_owned());
    assert!(store.apply_batch(batch).is_err());
    drop(finish);
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(runtime.responses.is_empty());
    runtime
        .prepare_review(
            &store,
            response(),
            Some(&support()),
            [PersonId(1), PersonId(2)],
        )
        .unwrap()
        .commit(
            &mut runtime,
            &mut store,
            &BTreeMap::<PersonId, Body>::new(),
            SimTime(1),
        )
        .unwrap();
    assert_eq!(runtime.responses, [response()]);
    assert_eq!(
        support_knowledge(&store, PersonId(2)).unwrap().by_passenger[&PersonId(3)],
        support()
    );
    assert_eq!(
        store.get::<String>(StableId(1)).map(String::as_str),
        Some("standing")
    );
}
#[test]
fn stale_topology_and_bodily_facts_reject_lifts_before_any_write() {
    let (mut runtime, mut store, mut bodies) = fixture();
    runtime
        .offer(&mut store, PersonId(1), PersonId(3), SimTime(1), &bodies)
        .unwrap();
    let make = |runtime: &Runtime, store: &Store, bodies: &BTreeMap<PersonId, Body>| {
        runtime
            .prepare_begin_lift(
                store,
                PersonId(3),
                BTreeSet::from([PersonId(1)]),
                Permission {
                    passenger_accepted: true,
                    ..Permission::default()
                },
                SimTime(1),
                bodies,
            )
            .unwrap()
    };
    let prepared = make(&runtime, &store, &bodies);
    insert_lift(
        &mut store,
        PersonId(4),
        Lift {
            passenger: PersonId(4),
            bearers: BTreeSet::from([PersonId(1)]),
            authority: Authority::Consented,
            started: SimTime(1),
            completes: SimTime(30_001),
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    assert!(
        prepared
            .commit(&mut runtime, &mut store, &bodies, SimTime(1))
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    remove_lift(&mut store, PersonId(4)).unwrap();
    let prepared = make(&runtime, &store, &bodies);
    bodies.get_mut(&PersonId(1)).unwrap().can_bear = false;
    let before = store.snapshot().unwrap();
    assert!(
        prepared
            .commit(&mut runtime, &mut store, &bodies, SimTime(1))
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn preparation_time_and_missing_runtime_rows_are_guarded() {
    let (mut runtime, mut store, bodies) = fixture();
    let prepared = runtime
        .prepare_offer(&store, PersonId(1), PersonId(3), SimTime(1), &bodies)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(
        prepared
            .commit(&mut runtime, &mut store, &bodies, SimTime(2))
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(validate_absent(&store).is_ok());
    insert_resume_target(&mut store, PersonId(1), ResumeTarget { area: AreaId(10) }).unwrap();
    assert!(validate_absent(&store).is_err());
}
#[test]
fn paused_route_and_lift_rows_restore_without_cloning_a_world() {
    let (mut runtime, mut store, bodies) = fixture();
    let target = RoadPosition {
        from: AreaId(10),
        to: AreaId(11),
        progress: SimDuration(5),
        duration: SimDuration(10),
    };
    insert_approach(
        &mut store,
        PersonId(1),
        Approach {
            target,
            started: SimTime(1),
            completes: SimTime(6),
        },
    )
    .unwrap();
    runtime
        .prepare_pause_routes(&store, &[PersonId(1)], AreaId(11))
        .commit(&mut runtime, &mut store, &bodies, SimTime(2))
        .unwrap();
    let snapshot = store.snapshot().unwrap();
    let mut restored = Store::restore(schema(), snapshot.clone()).unwrap();
    assert_eq!(restored.snapshot().unwrap(), snapshot);
    assert!(approach_job(&restored, PersonId(1)).is_none());
    assert_eq!(
        paused_approach(&restored, PersonId(1)).unwrap().target,
        target
    );
    assert_eq!(
        resume_target(&restored, PersonId(1)).unwrap().area,
        AreaId(11)
    );
    runtime
        .prepare_departure(&restored, &[PersonId(1)])
        .commit(&mut runtime, &mut restored, &bodies, SimTime(3))
        .unwrap();
    assert!(paused_approach(&restored, PersonId(1)).is_none());
    assert!(resume_target(&restored, PersonId(1)).is_none());
    assert_eq!(
        restored.get::<String>(StableId(1)).map(String::as_str),
        Some("standing")
    );
}
#[test]
fn configured_and_contextual_bounds_are_validated_by_the_owner() {
    let (runtime, mut store, mut bodies) = fixture();
    insert_offer_inbox(
        &mut store,
        PersonId(3),
        OfferInbox {
            by_bearer: BTreeMap::from([(
                PersonId(1),
                Offer {
                    bearer: PersonId(1),
                    passenger: PersonId(3),
                    offered_at: SimTime(1),
                    expires: SimTime(2),
                },
            )]),
        },
    )
    .unwrap();
    assert!(validate_runtime(&runtime, &store, &bodies, SimTime(1)).is_err());
    remove_offer_inbox(&mut store, PersonId(3)).unwrap();
    insert_group(
        &mut store,
        PersonId(3),
        CarryGroup {
            passenger: PersonId(3),
            anchor: PersonId(1),
            bearers: BTreeSet::from([PersonId(1)]),
            authority: Authority::Consented,
            accepted_at: SimTime(1),
        },
    )
    .unwrap();
    validate_runtime(&runtime, &store, &bodies, SimTime(1)).unwrap();
    bodies.get_mut(&PersonId(1)).unwrap().position = Position::Area(AreaId(11));
    assert!(validate_runtime(&runtime, &store, &bodies, SimTime(1)).is_err());
}

#[test]
fn local_lift_admission_reads_only_the_connected_physical_component() {
    use std::cell::RefCell;
    struct Lookup {
        rows: BTreeMap<PersonId, Body>,
        reads: RefCell<Vec<PersonId>>,
    }
    impl BodyFacts for Lookup {
        fn body(&self, actor: PersonId) -> Option<Body> {
            self.reads.borrow_mut().push(actor);
            self.rows.get(&actor).cloned()
        }
    }
    let (mut runtime, mut store, bodies) = fixture();
    runtime
        .offer(&mut store, PersonId(1), PersonId(3), SimTime(1), &bodies)
        .unwrap();
    insert_group(
        &mut store,
        PersonId(90),
        CarryGroup {
            passenger: PersonId(90),
            anchor: PersonId(91),
            bearers: BTreeSet::from([PersonId(91)]),
            authority: Authority::Consented,
            accepted_at: SimTime(0),
        },
    )
    .unwrap();
    let lookup = Lookup {
        rows: bodies,
        reads: RefCell::new(Vec::new()),
    };
    let prepared = runtime
        .prepare_begin_lift(
            &store,
            PersonId(3),
            BTreeSet::from([PersonId(1)]),
            Permission {
                passenger_accepted: true,
                ..Permission::default()
            },
            SimTime(1),
            &lookup,
        )
        .unwrap();
    prepared
        .commit(&mut runtime, &mut store, &lookup, SimTime(1))
        .unwrap();
    assert!(
        lookup
            .reads
            .borrow()
            .iter()
            .all(|actor| [PersonId(1), PersonId(3)].contains(actor))
    );
    assert!(lift(&store, PersonId(3)).is_some());
}
