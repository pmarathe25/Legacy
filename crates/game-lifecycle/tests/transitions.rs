use game_core::{AreaId, Error, LifeStage, Location, PersonId, PregnancyId, SimDuration, SimTime};
use game_ecs::{Schema, StableId, Store};
use game_lifecycle::*;
use std::collections::{BTreeMap, BTreeSet};
fn world() -> (Runtime, Store) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    world_with_schema(schema)
}
fn world_with_schema(schema: Schema) -> (Runtime, Store) {
    let rules = Rules {
        conception_time: SimDuration(10),
        gestation: SimDuration(20),
        infancy: SimDuration(40),
        adulthood: SimDuration(100),
        elderhood: SimDuration(200),
        lifespan: SimDuration(300),
    };
    let mut store = Store::with_schema(schema);
    insert_life(&mut store, life_row(1, Reproduction::Gestates)).unwrap();
    insert_life(&mut store, life_row(2, Reproduction::SuppliesSeed)).unwrap();
    (Runtime { rules }, store)
}
fn life_row(id: u64, reproduction: Reproduction) -> Life {
    Life {
        id: PersonId(id),
        born: SimTime(0),
        initial_age: SimDuration(100),
        stage: LifeStage::Adult,
        reproduction,
        parents: Vec::new(),
        caregivers: BTreeSet::new(),
        preference: None,
    }
}
fn body(id: u64) -> BodyFacts {
    BodyFacts {
        actor: PersonId(id),
        alive: true,
        health: 100,
        position: Location::AtArea(AreaId(7)),
        action_restricted: false,
        attempt: None,
    }
}
fn accept() -> Consent {
    Consent {
        initiator: true,
        partner: true,
    }
}
fn started() -> (Runtime, Store, [BodyFacts; 2]) {
    let (world, mut store) = world();
    let mut bodies = [body(1), body(2)];
    let prepared = world
        .prepare_start_conception(
            &store,
            SimTime(0),
            PregnancyId(3),
            [&bodies[0], &bodies[1]],
            accept(),
        )
        .unwrap();
    assert_eq!(all_pregnancies(&store).count(), 0, "preparation is inert");
    prepared.commit(&world, &mut store).unwrap();
    for body in &mut bodies {
        body.attempt = Some(Attempt {
            pregnancy: PregnancyId(3),
            completes: SimTime(10),
        });
    }
    (world, store, bodies)
}
fn gestating() -> (Runtime, Store, [BodyFacts; 2]) {
    let (world, mut store, bodies) = started();
    world
        .prepare_complete_conception(
            &store,
            PregnancyId(3),
            SimTime(10),
            [&bodies[0], &bodies[1]],
        )
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    (world, store, bodies)
}
#[test]
fn both_current_choices_are_required_and_refusal_uses_no_id_or_mutation() {
    for consent in [
        Consent {
            initiator: false,
            partner: true,
        },
        Consent {
            initiator: true,
            partner: false,
        },
    ] {
        let (world, mut store) = world();
        let before = (world.clone(), store.clone());
        let result = world
            .prepare_start_conception(
                &store,
                SimTime(0),
                PregnancyId(3),
                [&body(1), &body(2)],
                consent,
            )
            .unwrap();
        assert!(matches!(result.effects(), [Effect::Refused { .. }]));
        result.commit(&world, &mut store).unwrap();
        assert_eq!((world.clone(), store.clone()), before);
    }
}
#[test]
fn adulthood_bodies_contact_and_existing_gestation_are_owned_guards() {
    let (world, mut store) = world();
    let mut b = body(2);
    life_mut(&mut store, PersonId(2)).unwrap().initial_age = SimDuration(99);
    assert!(
        world
            .prepare_start_conception(&store, SimTime(0), PregnancyId(3), [&body(1), &b], accept())
            .is_err()
    );
    // Actual age, not an externally supplied adult verdict or stale stage.
    world
        .prepare_start_conception(&store, SimTime(1), PregnancyId(3), [&body(1), &b], accept())
        .unwrap();
    life_mut(&mut store, PersonId(2)).unwrap().initial_age = SimDuration(100);
    for changed in [
        BodyFacts {
            alive: false,
            ..body(2)
        },
        BodyFacts {
            health: 0,
            ..body(2)
        },
        BodyFacts {
            action_restricted: true,
            ..body(2)
        },
        BodyFacts {
            position: Location::AtArea(AreaId(8)),
            ..body(2)
        },
    ] {
        assert!(
            world
                .prepare_start_conception(
                    &store,
                    SimTime(0),
                    PregnancyId(3),
                    [&body(1), &changed],
                    accept()
                )
                .is_err()
        );
    }
    b.position = Location::StoppedOnRoad {
        from: AreaId(7),
        to: AreaId(8),
        progress: SimDuration(1),
        duration: SimDuration(2),
    };
    assert!(
        world
            .prepare_start_conception(&store, SimTime(0), PregnancyId(3), [&body(1), &b], accept())
            .is_err()
    );
    let (world, store, bodies) = gestating();
    assert!(
        world
            .prepare_start_conception(
                &store,
                SimTime(20),
                PregnancyId(4),
                [&bodies[0], &bodies[1]],
                accept()
            )
            .is_err()
    );
    let world = world.clone();
    let mut store = store.clone();
    life_mut(&mut store, PersonId(2)).unwrap().reproduction = Reproduction::Gestates;
    assert!(
        world
            .prepare_start_conception(
                &store,
                SimTime(20),
                PregnancyId(4),
                [&bodies[0], &bodies[1]],
                accept()
            )
            .is_err()
    );
}
#[test]
fn conception_completion_checks_exact_time_coupled_attempts_and_actual_contact() {
    let (world, store, bodies) = started();
    let before = (world.clone(), store.clone());
    assert!(
        world
            .prepare_complete_conception(
                &store,
                PregnancyId(3),
                SimTime(9),
                [&bodies[0], &bodies[1]]
            )
            .is_err()
    );
    assert_eq!((world.clone(), store.clone()), before);
    let mut changed = bodies[1].clone();
    changed.attempt = None;
    let world = world.clone();
    let mut store = store.clone();
    let effects = world
        .prepare_complete_conception(&store, PregnancyId(3), SimTime(10), [&bodies[0], &changed])
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert!(matches!(effects.as_slice(), [Effect::PregnancyEnded(_)]));
    assert_eq!(
        pregnancy(&store, PregnancyId(3)).unwrap().state,
        PregnancyState::Ended
    );
    let (world, mut store, bodies) = started();
    let changed = BodyFacts {
        position: Location::AtArea(AreaId(99)),
        ..bodies[1].clone()
    };
    world
        .prepare_complete_conception(&store, PregnancyId(3), SimTime(10), [&bodies[0], &changed])
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert_eq!(
        pregnancy(&store, PregnancyId(3)).unwrap().state,
        PregnancyState::Ended
    );
}
#[test]
fn later_partner_death_or_separation_does_not_end_an_actual_gestation() {
    let (world, mut store, _) = gestating();
    let before = (world.clone(), store.clone());
    let prepared = world.prepare_interruption(&store, PersonId(2), true, [PregnancyId(3)]);
    assert!(prepared.effects().is_empty());
    prepared.commit(&world, &mut store).unwrap();
    assert_eq!((world.clone(), store.clone()), before);
    let prepared = world.prepare_interruption(&store, PersonId(1), false, [PregnancyId(3)]);
    assert!(prepared.effects().is_empty());
    prepared.commit(&world, &mut store).unwrap();
    assert_eq!((world.clone(), store.clone()), before);
    let effects = world
        .prepare_interruption(&store, PersonId(1), true, [PregnancyId(3)])
        .commit(&world, &mut store)
        .unwrap();
    assert!(matches!(
        effects.as_slice(),
        [Effect::PregnancyEnded(PregnancyEndedEffect {
            was_conceiving: false,
            ..
        })]
    ));
    assert_eq!(
        pregnancy(&store, PregnancyId(3)).unwrap().state,
        PregnancyState::Ended
    );
}
#[test]
fn birth_is_due_once_uses_biological_parent_identity_and_transit_position() {
    let (world, mut store, _) = gestating();
    let mut gestator = body(1);
    gestator.position = Location::InTransit {
        from: AreaId(7),
        to: AreaId(8),
        departed: SimTime(25),
        arrives: SimTime(50),
        road_start: None,
    };
    life_mut(&mut store, PersonId(1))
        .unwrap()
        .caregivers
        .insert(PersonId(2));
    assert!(
        world
            .prepare_birth(
                &store,
                PregnancyId(3),
                SimTime(29),
                PersonId(4),
                &gestator,
                7
            )
            .is_err()
    );
    assert!(
        world
            .prepare_birth(
                &store,
                PregnancyId(3),
                SimTime(30),
                PersonId(4),
                &body(2),
                7
            )
            .is_err()
    );
    assert!(
        world
            .prepare_birth(
                &store,
                PregnancyId(3),
                SimTime(30),
                PersonId(1),
                &gestator,
                7
            )
            .is_err()
    );
    let prepared = world
        .prepare_birth(
            &store,
            PregnancyId(3),
            SimTime(30),
            PersonId(4),
            &gestator,
            7,
        )
        .unwrap();
    assert!(!life(&store, PersonId(4)).is_some());
    assert!(
        matches!(prepared.effects(),[Effect::Born {position, next_age:SimTime(70),..}] if *position==gestator.position)
    );
    prepared.commit(&world, &mut store).unwrap();
    let child = life(&store, PersonId(4)).unwrap();
    assert_eq!(child.parents, [PersonId(1), PersonId(2)]);
    assert!(child.caregivers.is_empty());
    assert_eq!(child.stage, LifeStage::Infant);
    assert_eq!(child.initial_age, SimDuration(0));
    assert!(
        world
            .prepare_birth(
                &store,
                PregnancyId(3),
                SimTime(30),
                PersonId(5),
                &gestator,
                7
            )
            .unwrap()
            .effects()
            .is_empty()
    );
    validate_runtime(&world, &store, SimTime(30)).unwrap();
    let saved = serde_json::to_vec(&world).unwrap();
    assert_eq!(world, serde_json::from_slice::<Runtime>(&saved).unwrap());
    let restored = Store::restore(
        store.schema().clone(),
        serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(store, restored);
    validate_runtime(&world, &restored, SimTime(30)).unwrap();
}
#[test]
fn maternal_death_ends_due_pregnancy_without_manufacturing_a_child() {
    let (world, mut store, _) = gestating();
    let gestator = BodyFacts {
        alive: false,
        ..body(1)
    };
    let effects = world
        .prepare_birth(
            &store,
            PregnancyId(3),
            SimTime(30),
            PersonId(4),
            &gestator,
            7,
        )
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    assert!(matches!(effects.as_slice(), [Effect::PregnancyEnded(_)]));
    assert!(!life(&store, PersonId(4)).is_some());
}
#[test]
fn aging_and_reduced_lifespan_are_prepared_without_mutating_unrelated_personal_state() {
    let (world, mut store) = world();
    life_mut(&mut store, PersonId(1)).unwrap().initial_age = SimDuration(0);
    let prepared = world
        .prepare_age(&store, PersonId(1), SimTime(40), true, 0, true)
        .unwrap();
    assert_eq!(life(&store, PersonId(1)).unwrap().stage, LifeStage::Adult);
    let effects = prepared.commit(&world, &mut store).unwrap();
    assert!(matches!(
        effects.as_slice(),
        [Effect::Developed(AgeEffect {
            stage: LifeStage::Child,
            next_deadline: Some(SimTime(100)),
            ..
        })]
    ));
    assert_eq!(life(&store, PersonId(1)).unwrap().stage, LifeStage::Child);
    assert!(
        world
            .prepare_age(&store, PersonId(1), SimTime(300), false, 0, true)
            .unwrap()
            .effects()
            .is_empty()
    );
    assert!(
        world
            .prepare_age(&store, PersonId(1), SimTime(300), true, 0, false)
            .unwrap()
            .effects()
            .is_empty()
    );
    assert!(matches!(
        world
            .prepare_age(&store, PersonId(1), SimTime(250), true, 50, true)
            .unwrap()
            .effects(),
        [Effect::Death { actor: PersonId(1) }]
    ));
}
#[test]
fn overflowing_start_completion_or_birth_deadlines_leave_records_untouched() {
    let (world, mut store) = world();
    let now = SimTime(u64::MAX - 1);
    let actors: Vec<_> = all_lives(&store).map(|(actor, _)| actor).collect();
    for actor in actors {
        life_mut(&mut store, actor).unwrap().born = now;
    }
    let before = (world.clone(), store.clone());
    assert!(matches!(
        world.prepare_start_conception(&store, now, PregnancyId(3), [&body(1), &body(2)], accept()),
        Err(Error::Overflow)
    ));
    assert_eq!((world.clone(), store.clone()), before);
    insert_pregnancy(
        &mut store,
        Pregnancy {
            id: PregnancyId(3),
            gestator: PersonId(1),
            partner: PersonId(2),
            state: PregnancyState::Conceiving { completes: now },
        },
    )
    .unwrap();
    let mut bodies = [body(1), body(2)];
    for body in &mut bodies {
        body.attempt = Some(Attempt {
            pregnancy: PregnancyId(3),
            completes: now,
        });
    }
    let before = (world.clone(), store.clone());
    assert!(matches!(
        world.prepare_complete_conception(&store, PregnancyId(3), now, [&bodies[0], &bodies[1]]),
        Err(Error::Overflow)
    ));
    assert_eq!((world.clone(), store.clone()), before);
    pregnancy_mut(&mut store, PregnancyId(3)).unwrap().state =
        PregnancyState::Gestating { due: now };
    let before = (world.clone(), store.clone());
    assert!(matches!(
        world.prepare_birth(&store, PregnancyId(3), now, PersonId(4), &body(1), 7),
        Err(Error::Overflow)
    ));
    assert_eq!((world.clone(), store.clone()), before);
}

#[test]
fn stale_completion_or_reused_child_identity_fails_before_any_domain_write() {
    let (world, mut store, bodies) = started();
    let completion = world
        .prepare_complete_conception(
            &store,
            PregnancyId(3),
            SimTime(10),
            [&bodies[0], &bodies[1]],
        )
        .unwrap();
    world
        .prepare_interruption(&store, PersonId(1), false, [PregnancyId(3)])
        .commit(&world, &mut store)
        .unwrap();
    let before = (world.clone(), store.clone());
    assert_eq!(
        completion.commit(&world, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!((world.clone(), store.clone()), before);
    let (world, mut store, _) = gestating();
    let birth = world
        .prepare_birth(
            &store,
            PregnancyId(3),
            SimTime(30),
            PersonId(4),
            &body(1),
            7,
        )
        .unwrap();
    insert_life(&mut store, life_row(4, Reproduction::Neither)).unwrap();
    let before = (world.clone(), store.clone());
    assert_eq!(birth.commit(&world, &mut store), Err(Error::Unavailable));
    assert_eq!(
        (world.clone(), store.clone()),
        before,
        "pregnancy must remain gestating when the later child edit is stale"
    );
}

#[test]
fn typed_deadline_preflight_validates_actual_fsm_state_and_future_time_inertly() {
    let (world, store, _) = started();
    let before = (world.clone(), store.clone());
    world
        .preflight_deadlines(&store, SimTime(10), [Deadline::Conception(PregnancyId(3))])
        .unwrap();
    assert_eq!(
        world.preflight_deadlines(&store, SimTime(9), [Deadline::Conception(PregnancyId(3))]),
        Err(Error::Unavailable)
    );
    assert_eq!(
        world.preflight_deadlines(&store, SimTime(10), [Deadline::Birth(PregnancyId(3))]),
        Err(Error::Unavailable)
    );
    assert_eq!((world.clone(), store.clone()), before);
    let (world, store, _) = gestating();
    world
        .preflight_deadlines(
            &store,
            SimTime(30),
            [Deadline::Birth(PregnancyId(3)), Deadline::Age(PersonId(1))],
        )
        .unwrap();
    assert_eq!(
        world.preflight_deadlines(&store, SimTime(u64::MAX), []),
        Err(Error::Overflow)
    );
}

#[test]
fn domain_validation_checks_live_stages_attempts_and_scheduling_ports() {
    let (world, store) = world();
    let bodies = BTreeMap::from([
        (
            PersonId(1),
            ValidationBody {
                body: body(1),
                aging_enabled: true,
                lifespan_loss: 0,
            },
        ),
        (
            PersonId(2),
            ValidationBody {
                body: body(2),
                aging_enabled: true,
                lifespan_loss: 0,
            },
        ),
    ]);
    let deadlines = [
        (SimTime(100), Deadline::Age(PersonId(1))),
        (SimTime(100), Deadline::Age(PersonId(2))),
    ];
    world
        .validate_with_facts(&store, SimTime(0), &bodies, &deadlines)
        .unwrap();
    assert!(
        world
            .validate_with_facts(&store, SimTime(0), &bodies, &deadlines[..1])
            .is_err()
    );
    let wrong = [
        (SimTime(99), Deadline::Age(PersonId(1))),
        (SimTime(100), Deadline::Age(PersonId(2))),
    ];
    assert!(
        world
            .validate_with_facts(&store, SimTime(0), &bodies, &wrong)
            .is_err()
    );
    let (world, store, attempts) = started();
    let mut bodies = bodies;
    for attempt in attempts {
        let actor = attempt.actor;
        bodies.get_mut(&actor).unwrap().body = attempt;
    }
    world
        .validate_with_facts(&store, SimTime(1), &bodies, &deadlines)
        .unwrap();
    bodies.get_mut(&PersonId(2)).unwrap().body.attempt = None;
    assert!(
        world
            .validate_with_facts(&store, SimTime(1), &bodies, &deadlines)
            .is_err()
    );
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Foreign(u64);

#[test]
fn actual_birth_and_owner_removal_preserve_foreign_rows_and_roundtrip() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    assert_eq!(
        schema.keys().collect::<Vec<_>>(),
        vec!["lifecycle/life", "lifecycle/pregnancy"]
    );
    assert!(register_components(&mut schema).is_err());
    schema
        .register::<Foreign>("other/value", |_| Ok(()))
        .unwrap();
    let (runtime, mut store) = world_with_schema(schema.clone());
    store.insert(StableId(1), Foreign(71)).unwrap();
    store.insert_or_spawn(StableId(4), Foreign(74)).unwrap();
    let mut bodies = [body(1), body(2)];
    runtime
        .prepare_start_conception(
            &store,
            SimTime(0),
            PregnancyId(3),
            [&bodies[0], &bodies[1]],
            accept(),
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    for row in &mut bodies {
        row.attempt = Some(Attempt {
            pregnancy: PregnancyId(3),
            completes: SimTime(10),
        });
    }
    runtime
        .prepare_complete_conception(
            &store,
            PregnancyId(3),
            SimTime(10),
            [&bodies[0], &bodies[1]],
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    let effects = runtime
        .prepare_birth(
            &store,
            PregnancyId(3),
            SimTime(30),
            PersonId(4),
            &body(1),
            7,
        )
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert!(matches!(
        effects.as_slice(),
        [Effect::Born {
            child: PersonId(4),
            ..
        }]
    ));
    assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(71)));
    assert_eq!(store.get::<Foreign>(StableId(4)), Some(&Foreign(74)));
    assert_eq!(all_lives(&store).count(), 3);
    assert_eq!(all_pregnancies(&store).count(), 1);
    let restored = Store::restore(
        schema,
        serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(store, restored);
    validate_runtime(&runtime, &restored, SimTime(30)).unwrap();
    assert!(remove_life(&mut store, PersonId(4)).unwrap().is_some());
    assert!(store.contains(StableId(4)));
    assert_eq!(store.get::<Foreign>(StableId(4)), Some(&Foreign(74)));
    assert!(
        remove_pregnancy(&mut store, PregnancyId(3))
            .unwrap()
            .is_some()
    );
    assert!(store.contains(StableId(3)));
    assert!(remove_life(&mut store, PersonId(99)).unwrap().is_none());
    assert!(
        remove_pregnancy(&mut store, PregnancyId(99))
            .unwrap()
            .is_none()
    );
}

#[test]
fn intrinsic_rows_and_contextual_lifecycle_validation_remain_separate() {
    let (runtime, mut store) = world();
    let before = store.clone();
    let mut invalid = life_row(4, Reproduction::Neither);
    invalid.parents.push(PersonId(4));
    assert!(insert_life(&mut store, invalid).is_err());
    assert!(
        insert_pregnancy(
            &mut store,
            Pregnancy {
                id: PregnancyId(3),
                gestator: PersonId(1),
                partner: PersonId(2),
                state: PregnancyState::Born(PersonId(3))
            }
        )
        .is_err()
    );
    assert_eq!(store, before);
    let mut orphan = life_row(4, Reproduction::Neither);
    orphan.parents.push(PersonId(99));
    insert_life(&mut store, orphan).unwrap();
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert!(matches!(
        validate_runtime(&runtime, &restored, SimTime(0)),
        Err(Error::InvalidSave(_))
    ));
    remove_life(&mut store, PersonId(4)).unwrap();
    store
        .insert_or_spawn(StableId(9), life_row(4, Reproduction::Neither))
        .unwrap();
    assert!(matches!(
        validate_runtime(&runtime, &store, SimTime(0)),
        Err(Error::InvalidSave(_))
    ));
    life_mut(&mut store, PersonId(9)).unwrap().id = PersonId(0);
    assert!(Store::restore(store.schema().clone(), store.snapshot().unwrap()).is_err());
    let mut json = serde_json::to_value(runtime).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("lives".into(), serde_json::json!({}));
    assert!(serde_json::from_value::<Runtime>(json).is_err());
}

#[test]
fn missing_or_rejecting_pregnancy_schema_leaves_no_partial_attempt_entity() {
    for missing in [false, true] {
        let mut schema = Schema::new();
        schema
            .register::<Life>("lifecycle/life", Life::validate)
            .unwrap();
        if !missing {
            schema
                .register::<Pregnancy>("lifecycle/pregnancy", |_| Err(Error::Unavailable))
                .unwrap();
        }
        let (runtime, mut store) = world_with_schema(schema);
        let prepared = runtime
            .prepare_start_conception(
                &store,
                SimTime(0),
                PregnancyId(3),
                [&body(1), &body(2)],
                accept(),
            )
            .unwrap();
        let before = (runtime.clone(), store.clone());
        assert!(prepared.commit(&runtime, &mut store).is_err());
        assert_eq!((runtime, store), before);
    }
}

#[test]
fn prepared_rules_and_whole_life_rows_are_exact_read_guards() {
    let (mut runtime, mut store) = world();
    let prepared = runtime
        .prepare_age(&store, PersonId(1), SimTime(100), true, 0, true)
        .unwrap();
    runtime.rules.infancy = SimDuration(41);
    let before = (runtime.clone(), store.clone());
    assert_eq!(
        prepared.commit(&runtime, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!((runtime, store), before);

    let (runtime, mut store) = world();
    let prepared = runtime
        .prepare_age(&store, PersonId(1), SimTime(100), true, 0, true)
        .unwrap();
    life_mut(&mut store, PersonId(1))
        .unwrap()
        .caregivers
        .insert(PersonId(2));
    let before = (runtime.clone(), store.clone());
    assert_eq!(
        prepared.commit(&runtime, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!((runtime, store), before);
}

#[test]
fn competing_conception_membership_is_guarded_without_observing_other_gestators() {
    let (runtime, mut store) = world();
    let first = runtime
        .prepare_start_conception(
            &store,
            SimTime(0),
            PregnancyId(3),
            [&body(1), &body(2)],
            accept(),
        )
        .unwrap();
    let second = runtime
        .prepare_start_conception(
            &store,
            SimTime(0),
            PregnancyId(4),
            [&body(1), &body(2)],
            accept(),
        )
        .unwrap();
    first.commit(&runtime, &mut store).unwrap();
    let before = (runtime.clone(), store.clone());
    assert_eq!(second.commit(&runtime, &mut store), Err(Error::Unavailable));
    assert_eq!((runtime, store), before);

    let (runtime, mut store) = world();
    let prepared = runtime
        .prepare_start_conception(
            &store,
            SimTime(0),
            PregnancyId(3),
            [&body(1), &body(2)],
            accept(),
        )
        .unwrap();
    insert_life(&mut store, life_row(10, Reproduction::Gestates)).unwrap();
    insert_life(&mut store, life_row(11, Reproduction::SuppliesSeed)).unwrap();
    insert_pregnancy(
        &mut store,
        Pregnancy {
            id: PregnancyId(12),
            gestator: PersonId(10),
            partner: PersonId(11),
            state: PregnancyState::Gestating { due: SimTime(50) },
        },
    )
    .unwrap();
    prepared.commit(&runtime, &mut store).unwrap();
    assert_eq!(
        pregnancy(&store, PregnancyId(12)).unwrap().state,
        PregnancyState::Gestating { due: SimTime(50) }
    );
}

#[test]
fn schema_rejection_of_birth_never_publishes_only_the_child_or_pregnancy() {
    for reject_life in [false, true] {
        let mut schema = Schema::new();
        if reject_life {
            schema
                .register::<Life>("lifecycle/life", |row| {
                    row.validate()?;
                    if row.initial_age.0 == 0 {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
            schema
                .register::<Pregnancy>("lifecycle/pregnancy", Pregnancy::validate)
                .unwrap();
        } else {
            schema
                .register::<Life>("lifecycle/life", Life::validate)
                .unwrap();
            schema
                .register::<Pregnancy>("lifecycle/pregnancy", |row| {
                    row.validate()?;
                    if matches!(row.state, PregnancyState::Born(_)) {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
        }
        let (runtime, mut store) = world_with_schema(schema);
        insert_pregnancy(
            &mut store,
            Pregnancy {
                id: PregnancyId(3),
                gestator: PersonId(1),
                partner: PersonId(2),
                state: PregnancyState::Gestating { due: SimTime(30) },
            },
        )
        .unwrap();
        let before = (runtime.clone(), store.clone());
        let prepared = runtime
            .prepare_birth(
                &store,
                PregnancyId(3),
                SimTime(30),
                PersonId(4),
                &body(1),
                7,
            )
            .unwrap();
        assert_eq!(
            prepared.commit(&runtime, &mut store),
            Err(Error::Unavailable)
        );
        assert_eq!((runtime, store), before);
    }
}

#[test]
fn birth_rejects_reusing_the_pregnancy_entity_as_the_child_before_any_write() {
    let (runtime, store, _) = gestating();
    let before = (runtime.clone(), store.clone());
    assert!(matches!(
        runtime.prepare_birth(
            &store,
            PregnancyId(3),
            SimTime(30),
            PersonId(3),
            &body(1),
            7
        ),
        Err(Error::Unavailable)
    ));
    assert_eq!((runtime, store), before);
}
