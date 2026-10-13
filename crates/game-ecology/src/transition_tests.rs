use super::*;
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_land::register_components(&mut schema).unwrap();
    schema
}
fn runtime() -> Runtime {
    Runtime::new(EcologyRules::default(), 0).unwrap()
}
fn deposit(id: u64, area: u64) -> Resource {
    Resource {
        id: ResourceId(id),
        area: AreaId(area),
        kind: ResourceKind::Berries,
        remaining: Quantity(5_000),
        capacity: Quantity(20_000),
        daily_growth: Quantity(250),
        growth_remainder: 0,
        decay_permille: 0,
        soil_water: 75,
        water_capacity: 120,
        optimal_water: 50,
        minimum_temperature: 0,
        optimum_temperature: 20,
        contamination: 10,
        runoff_contamination: 0,
        labor_per_unit: MINUTE,
        workers: 2,
        access: BTreeSet::from([PersonId(1)]),
        custodian: Some(PersonId(1)),
    }
}
fn setup() -> (Runtime, Store) {
    let runtime = runtime();
    let mut store = Store::with_schema(schema());
    for actor in 1..=3 {
        store.ensure(StableId(actor)).unwrap();
    }
    runtime
        .insert_resource(&mut store, deposit(100, 1))
        .unwrap();
    (runtime, store)
}
fn access(actor: u64) -> AccessFacts {
    AccessFacts {
        actor: PersonId(actor),
        area: AreaId(1),
        capable: true,
    }
}
fn local(_: PersonId) -> Option<AreaId> {
    Some(AreaId(1))
}
fn job() -> Gathering {
    Gathering {
        resource: ResourceId(100),
        destination: InventoryId(400),
        quantity: Quantity(1_000),
        tool: None,
        completes: SimTime(500),
        unauthorized: false,
    }
}
fn collection() -> CollectionFacts {
    CollectionFacts {
        now: SimTime(500),
        area: AreaId(1),
        capable: true,
        cargo_available: true,
        tool_available: true,
    }
}
#[test]
fn request_cooldown_is_exact_and_preserves_other_requests_and_stock() {
    let (runtime, mut store) = setup();
    runtime
        .insert_requests(
            &mut store,
            PersonId(2),
            Requests {
                next_request: BTreeMap::from([(ResourceId(101), SimTime(77))]),
            },
        )
        .unwrap();
    let action = EcologyAction::RequestAccess {
        resource: ResourceId(100),
    };
    let before = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_access(&store, SimTime(100), &action, access(2), local)
        .unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(
        prepared.effect(),
        &AccessEffect::Requested {
            resource: ResourceId(100),
            actor: PersonId(2),
            custodian: PersonId(1)
        }
    );
    let stock = runtime.resource(&store, ResourceId(100)).unwrap().clone();
    prepared.commit(&runtime, &mut store).unwrap();
    let requests = runtime.requests(&store, PersonId(2)).unwrap();
    assert_eq!(requests.next_request[&ResourceId(100)], SimTime(180_100));
    assert_eq!(requests.next_request[&ResourceId(101)], SimTime(77));
    assert_eq!(runtime.resource(&store, ResourceId(100)), Some(&stock));
    assert!(
        runtime
            .prepare_access(&store, SimTime(180_099), &action, access(2), local)
            .is_err()
    );
    assert!(
        runtime
            .prepare_access(&store, SimTime(180_100), &action, access(2), local)
            .is_ok()
    );
    let before = store.snapshot().unwrap();
    assert!(matches!(
        runtime.prepare_access(&store, SimTime(u64::MAX), &action, access(3), local),
        Err(Error::Overflow)
    ));
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn grant_requires_actual_local_authority_and_guarded_resource() {
    let (runtime, mut store) = setup();
    let action = EcologyAction::GrantAccess {
        resource: ResourceId(100),
        recipient: PersonId(2),
        allowed: true,
    };
    assert!(
        runtime
            .prepare_access(&store, SimTime(100), &action, access(3), local)
            .is_err()
    );
    assert!(
        runtime
            .prepare_access(&store, SimTime(100), &action, access(1), |_| Some(AreaId(
                2
            )))
            .is_err()
    );
    let prepared = runtime
        .prepare_access(&store, SimTime(100), &action, access(1), local)
        .unwrap();
    runtime
        .resource_mut(&mut store, ResourceId(100))
        .unwrap()
        .remaining = Quantity(4_000);
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    runtime
        .prepare_access(&store, SimTime(100), &action, access(1), local)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(
        runtime
            .resource(&store, ResourceId(100))
            .unwrap()
            .access
            .contains(&PersonId(2))
    );
    let revoke = EcologyAction::GrantAccess {
        resource: ResourceId(100),
        recipient: PersonId(2),
        allowed: false,
    };
    runtime
        .prepare_access(&store, SimTime(101), &revoke, access(1), local)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(
        !runtime
            .resource(&store, ResourceId(100))
            .unwrap()
            .access
            .contains(&PersonId(2))
    );
    let request = EcologyAction::RequestAccess {
        resource: ResourceId(100),
    };
    let prepared = runtime
        .prepare_access(&store, SimTime(100), &request, access(2), local)
        .unwrap();
    runtime
        .insert_requests(
            &mut store,
            PersonId(2),
            Requests {
                next_request: BTreeMap::from([(ResourceId(101), SimTime(55))]),
            },
        )
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn collection_uses_committed_renewal_quality_and_leaves_job_until_admitted_commit() {
    let (runtime, mut store) = setup();
    let source = runtime.resource_mut(&mut store, ResourceId(100)).unwrap();
    source.kind = ResourceKind::Spring;
    source.contamination = 345;
    source.remaining = Quantity(7_000);
    runtime
        .insert_gathering(&mut store, PersonId(1), job())
        .unwrap();
    let opening = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_collection_finish(&store, PersonId(1), collection())
        .unwrap()
        .unwrap();
    let expected = prepare_collection(
        runtime.resource(&store, ResourceId(100)).unwrap(),
        PersonId(1),
        &job(),
        collection(),
    )
    .unwrap();
    assert_eq!(
        prepared.outcome(),
        &CollectionOutcome::Collected(expected.clone())
    );
    assert_eq!(expected.contamination, Some(345));
    assert_eq!(expected.remaining, Quantity(6_000));
    assert_eq!(store.snapshot().unwrap(), opening); // A refused cargo admission can discard it safely.
    let restored = Store::restore(
        schema(),
        serde_json::from_str(&serde_json::to_string(&opening).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        runtime
            .prepare_collection_finish(&restored, PersonId(1), collection())
            .unwrap()
            .unwrap()
            .outcome(),
        prepared.outcome()
    );
    let outcome = prepared.commit(&runtime, &mut store).unwrap();
    assert_eq!(outcome, CollectionOutcome::Collected(expected));
    assert!(runtime.gathering(&store, PersonId(1)).is_none());
    assert_eq!(
        runtime.resource(&store, ResourceId(100)).unwrap().remaining,
        Quantity(6_000)
    );
    assert!(
        runtime
            .prepare_collection_finish(&store, PersonId(1), collection())
            .unwrap()
            .is_none()
    );
}
#[test]
fn normal_collection_failure_removes_only_the_job_after_guarded_finish() {
    for change in 0..6 {
        let (runtime, mut store) = setup();
        runtime
            .insert_gathering(&mut store, PersonId(1), job())
            .unwrap();
        let mut facts = collection();
        match change {
            0 => facts.now = SimTime(499),
            1 => facts.capable = false,
            2 => facts.cargo_available = false,
            3 => facts.area = AreaId(2),
            4 => {
                runtime
                    .resource_mut(&mut store, ResourceId(100))
                    .unwrap()
                    .remaining = Quantity(1)
            }
            _ => {
                runtime
                    .resource_mut(&mut store, ResourceId(100))
                    .unwrap()
                    .access
                    .clear();
            }
        }
        let source = runtime.resource(&store, ResourceId(100)).unwrap().clone();
        let prepared = runtime
            .prepare_collection_finish(&store, PersonId(1), facts)
            .unwrap()
            .unwrap();
        assert_eq!(
            prepared.outcome(),
            &CollectionOutcome::Interrupted { actor: PersonId(1) }
        );
        prepared.commit(&runtime, &mut store).unwrap();
        assert!(runtime.gathering(&store, PersonId(1)).is_none());
        assert_eq!(runtime.resource(&store, ResourceId(100)), Some(&source));
    }
}
#[test]
fn changed_collection_source_or_job_cannot_remove_a_replacement_job() {
    for source_change in [false, true] {
        let (runtime, mut store) = setup();
        runtime
            .insert_gathering(&mut store, PersonId(1), job())
            .unwrap();
        let prepared = runtime
            .prepare_collection_finish(&store, PersonId(1), collection())
            .unwrap()
            .unwrap();
        if source_change {
            runtime
                .resource_mut(&mut store, ResourceId(100))
                .unwrap()
                .contamination = 11;
        } else {
            let mut replacement = job();
            replacement.quantity = Quantity(2_000);
            runtime
                .insert_gathering(&mut store, PersonId(1), replacement)
                .unwrap();
        }
        let before = store.snapshot().unwrap();
        assert!(prepared.commit(&runtime, &mut store).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
#[test]
fn rejecting_collection_batch_preserves_source_and_job() {
    let runtime = runtime();
    let mut strict = game_ecs::Schema::new();
    strict
        .register::<Resource>("ecology/resource", |row| {
            row.validate()?;
            if row.remaining.0 < 5_000 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    strict
        .register::<Gathering>("ecology/gathering", Gathering::validate)
        .unwrap();
    let mut store = Store::with_schema(strict);
    runtime
        .insert_resource(&mut store, deposit(100, 1))
        .unwrap();
    runtime
        .insert_gathering(&mut store, PersonId(1), job())
        .unwrap();
    let prepared = runtime
        .prepare_collection_finish(&store, PersonId(1), collection())
        .unwrap()
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn accepted_access_response_is_one_atomic_guarded_ecology_exchange() {
    let (runtime, mut store) = setup();
    let request = runtime
        .prepare_access(
            &store,
            SimTime(100),
            &EcologyAction::RequestAccess {
                resource: ResourceId(100),
            },
            access(2),
            local,
        )
        .unwrap();
    let grant = runtime
        .prepare_access(
            &store,
            SimTime(100),
            &EcologyAction::GrantAccess {
                resource: ResourceId(100),
                recipient: PersonId(2),
                allowed: true,
            },
            access(1),
            local,
        )
        .unwrap();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let (effects, guard) = request
        .stage_with_response(Some(grant), &runtime, &store, &mut batch)
        .unwrap();
    assert_eq!(effects.len(), 2);
    assert!(matches!(effects[0], AccessEffect::Requested { .. }));
    assert!(matches!(
        effects[1],
        AccessEffect::Changed { allowed: true, .. }
    ));
    guard.validate(&store, &batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    assert!(
        runtime
            .resource(&store, ResourceId(100))
            .unwrap()
            .access
            .contains(&PersonId(2))
    );
    assert_eq!(
        runtime.requests(&store, PersonId(2)).unwrap().next_request[&ResourceId(100)],
        SimTime(180_100)
    );
}

#[test]
fn wrong_recipient_response_cannot_publish_request_cooldown() {
    let (runtime, store) = setup();
    let request = runtime
        .prepare_access(
            &store,
            SimTime(100),
            &EcologyAction::RequestAccess {
                resource: ResourceId(100),
            },
            access(2),
            local,
        )
        .unwrap();
    let grant = runtime
        .prepare_access(
            &store,
            SimTime(100),
            &EcologyAction::GrantAccess {
                resource: ResourceId(100),
                recipient: PersonId(3),
                allowed: true,
            },
            access(1),
            local,
        )
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(
        request
            .stage_with_response(Some(grant), &runtime, &store, &mut Batch::new())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(runtime.requests(&store, PersonId(2)).is_none());
}
