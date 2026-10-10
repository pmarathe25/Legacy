use game_core::{AreaId, Error, LifeStage, Location, PersonId, PregnancyId, SimDuration, SimTime};
use game_lifecycle::*;
use std::collections::{BTreeMap, BTreeSet};
fn world() -> World {
    let rules = Rules {
        conception_time: SimDuration(10),
        gestation: SimDuration(20),
        infancy: SimDuration(40),
        adulthood: SimDuration(100),
        elderhood: SimDuration(200),
        lifespan: SimDuration(300),
    };
    World {
        rules,
        lives: BTreeMap::from([
            (PersonId(1), life(1, Reproduction::Gestates)),
            (PersonId(2), life(2, Reproduction::SuppliesSeed)),
        ]),
        pregnancies: BTreeMap::new(),
    }
}
fn life(id: u64, reproduction: Reproduction) -> Life {
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
fn started() -> (World, [BodyFacts; 2]) {
    let mut world = world();
    let mut bodies = [body(1), body(2)];
    let prepared = world
        .prepare_start_conception(
            SimTime(0),
            PregnancyId(3),
            [&bodies[0], &bodies[1]],
            accept(),
        )
        .unwrap();
    assert_eq!(world.pregnancies.len(), 0, "preparation is inert");
    prepared.commit(&mut world).unwrap();
    for body in &mut bodies {
        body.attempt = Some(Attempt {
            pregnancy: PregnancyId(3),
            completes: SimTime(10),
        });
    }
    (world, bodies)
}
fn gestating() -> (World, [BodyFacts; 2]) {
    let (mut world, bodies) = started();
    world
        .prepare_complete_conception(PregnancyId(3), SimTime(10), [&bodies[0], &bodies[1]])
        .unwrap()
        .commit(&mut world)
        .unwrap();
    (world, bodies)
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
        let mut world = world();
        let before = world.clone();
        let result = world
            .prepare_start_conception(SimTime(0), PregnancyId(3), [&body(1), &body(2)], consent)
            .unwrap();
        assert!(matches!(result.effects(), [Effect::Refused { .. }]));
        result.commit(&mut world).unwrap();
        assert_eq!(world, before);
    }
}
#[test]
fn adulthood_bodies_contact_and_existing_gestation_are_owned_guards() {
    let mut world = world();
    let mut b = body(2);
    world.lives.get_mut(&PersonId(2)).unwrap().initial_age = SimDuration(99);
    assert!(
        world
            .prepare_start_conception(SimTime(0), PregnancyId(3), [&body(1), &b], accept())
            .is_err()
    );
    // Actual age, not an externally supplied adult verdict or stale stage.
    world
        .prepare_start_conception(SimTime(1), PregnancyId(3), [&body(1), &b], accept())
        .unwrap();
    world.lives.get_mut(&PersonId(2)).unwrap().initial_age = SimDuration(100);
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
            .prepare_start_conception(SimTime(0), PregnancyId(3), [&body(1), &b], accept())
            .is_err()
    );
    let (world, bodies) = gestating();
    assert!(
        world
            .prepare_start_conception(
                SimTime(20),
                PregnancyId(4),
                [&bodies[0], &bodies[1]],
                accept()
            )
            .is_err()
    );
    let mut world = world.clone();
    world.lives.get_mut(&PersonId(2)).unwrap().reproduction = Reproduction::Gestates;
    assert!(
        world
            .prepare_start_conception(
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
    let (world, bodies) = started();
    let before = world.clone();
    assert!(
        world
            .prepare_complete_conception(PregnancyId(3), SimTime(9), [&bodies[0], &bodies[1]])
            .is_err()
    );
    assert_eq!(world, before);
    let mut changed = bodies[1].clone();
    changed.attempt = None;
    let mut world = world.clone();
    let effects = world
        .prepare_complete_conception(PregnancyId(3), SimTime(10), [&bodies[0], &changed])
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(matches!(effects.as_slice(), [Effect::PregnancyEnded(_)]));
    assert_eq!(
        world.pregnancies[&PregnancyId(3)].state,
        PregnancyState::Ended
    );
    let (mut world, bodies) = started();
    let changed = BodyFacts {
        position: Location::AtArea(AreaId(99)),
        ..bodies[1].clone()
    };
    world
        .prepare_complete_conception(PregnancyId(3), SimTime(10), [&bodies[0], &changed])
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert_eq!(
        world.pregnancies[&PregnancyId(3)].state,
        PregnancyState::Ended
    );
}
#[test]
fn later_partner_death_or_separation_does_not_end_an_actual_gestation() {
    let (mut world, _) = gestating();
    let before = world.clone();
    let prepared = world.prepare_interruption(PersonId(2), true, [PregnancyId(3)]);
    assert!(prepared.effects().is_empty());
    prepared.commit(&mut world).unwrap();
    assert_eq!(world, before);
    let prepared = world.prepare_interruption(PersonId(1), false, [PregnancyId(3)]);
    assert!(prepared.effects().is_empty());
    prepared.commit(&mut world).unwrap();
    assert_eq!(world, before);
    let effects = world
        .prepare_interruption(PersonId(1), true, [PregnancyId(3)])
        .commit(&mut world)
        .unwrap();
    assert!(matches!(
        effects.as_slice(),
        [Effect::PregnancyEnded(PregnancyEndedEffect {
            was_conceiving: false,
            ..
        })]
    ));
    assert_eq!(
        world.pregnancies[&PregnancyId(3)].state,
        PregnancyState::Ended
    );
}
#[test]
fn birth_is_due_once_uses_biological_parent_identity_and_transit_position() {
    let (mut world, _) = gestating();
    let mut gestator = body(1);
    gestator.position = Location::InTransit {
        from: AreaId(7),
        to: AreaId(8),
        departed: SimTime(25),
        arrives: SimTime(50),
        road_start: None,
    };
    world
        .lives
        .get_mut(&PersonId(1))
        .unwrap()
        .caregivers
        .insert(PersonId(2));
    assert!(
        world
            .prepare_birth(PregnancyId(3), SimTime(29), PersonId(4), &gestator, 7)
            .is_err()
    );
    assert!(
        world
            .prepare_birth(PregnancyId(3), SimTime(30), PersonId(4), &body(2), 7)
            .is_err()
    );
    assert!(
        world
            .prepare_birth(PregnancyId(3), SimTime(30), PersonId(1), &gestator, 7)
            .is_err()
    );
    let prepared = world
        .prepare_birth(PregnancyId(3), SimTime(30), PersonId(4), &gestator, 7)
        .unwrap();
    assert!(!world.lives.contains_key(&PersonId(4)));
    assert!(
        matches!(prepared.effects(),[Effect::Born {position, next_age:SimTime(70),..}] if *position==gestator.position)
    );
    prepared.commit(&mut world).unwrap();
    let child = &world.lives[&PersonId(4)];
    assert_eq!(child.parents, [PersonId(1), PersonId(2)]);
    assert!(child.caregivers.is_empty());
    assert_eq!(child.stage, LifeStage::Infant);
    assert_eq!(child.initial_age, SimDuration(0));
    assert!(
        world
            .prepare_birth(PregnancyId(3), SimTime(30), PersonId(5), &gestator, 7)
            .unwrap()
            .effects()
            .is_empty()
    );
    validate_local(&world, SimTime(30)).unwrap();
    let saved = serde_json::to_vec(&world).unwrap();
    assert_eq!(world, serde_json::from_slice::<World>(&saved).unwrap());
}
#[test]
fn maternal_death_ends_due_pregnancy_without_manufacturing_a_child() {
    let (mut world, _) = gestating();
    let gestator = BodyFacts {
        alive: false,
        ..body(1)
    };
    let effects = world
        .prepare_birth(PregnancyId(3), SimTime(30), PersonId(4), &gestator, 7)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    assert!(matches!(effects.as_slice(), [Effect::PregnancyEnded(_)]));
    assert!(!world.lives.contains_key(&PersonId(4)));
}
#[test]
fn aging_and_reduced_lifespan_are_prepared_without_mutating_unrelated_personal_state() {
    let mut world = world();
    world.lives.get_mut(&PersonId(1)).unwrap().initial_age = SimDuration(0);
    let prepared = world
        .prepare_age(PersonId(1), SimTime(40), true, 0, true)
        .unwrap();
    assert_eq!(world.lives[&PersonId(1)].stage, LifeStage::Adult);
    let effects = prepared.commit(&mut world).unwrap();
    assert!(matches!(
        effects.as_slice(),
        [Effect::Developed(AgeEffect {
            stage: LifeStage::Child,
            next_deadline: Some(SimTime(100)),
            ..
        })]
    ));
    assert_eq!(world.lives[&PersonId(1)].stage, LifeStage::Child);
    assert!(
        world
            .prepare_age(PersonId(1), SimTime(300), false, 0, true)
            .unwrap()
            .effects()
            .is_empty()
    );
    assert!(
        world
            .prepare_age(PersonId(1), SimTime(300), true, 0, false)
            .unwrap()
            .effects()
            .is_empty()
    );
    assert!(matches!(
        world
            .prepare_age(PersonId(1), SimTime(250), true, 50, true)
            .unwrap()
            .effects(),
        [Effect::Death { actor: PersonId(1) }]
    ));
}
#[test]
fn overflowing_start_completion_or_birth_deadlines_leave_records_untouched() {
    let mut world = world();
    let now = SimTime(u64::MAX - 1);
    for life in world.lives.values_mut() {
        life.born = now;
    }
    let before = world.clone();
    assert!(matches!(
        world.prepare_start_conception(now, PregnancyId(3), [&body(1), &body(2)], accept()),
        Err(Error::Overflow)
    ));
    assert_eq!(world, before);
    world.pregnancies.insert(
        PregnancyId(3),
        Pregnancy {
            id: PregnancyId(3),
            gestator: PersonId(1),
            partner: PersonId(2),
            state: PregnancyState::Conceiving { completes: now },
        },
    );
    let mut bodies = [body(1), body(2)];
    for body in &mut bodies {
        body.attempt = Some(Attempt {
            pregnancy: PregnancyId(3),
            completes: now,
        });
    }
    let before = world.clone();
    assert!(matches!(
        world.prepare_complete_conception(PregnancyId(3), now, [&bodies[0], &bodies[1]]),
        Err(Error::Overflow)
    ));
    assert_eq!(world, before);
    world.pregnancies.get_mut(&PregnancyId(3)).unwrap().state =
        PregnancyState::Gestating { due: now };
    let before = world.clone();
    assert!(matches!(
        world.prepare_birth(PregnancyId(3), now, PersonId(4), &body(1), 7),
        Err(Error::Overflow)
    ));
    assert_eq!(world, before);
}

#[test]
fn stale_completion_or_reused_child_identity_fails_before_any_domain_write() {
    let (mut world, bodies) = started();
    let completion = world
        .prepare_complete_conception(PregnancyId(3), SimTime(10), [&bodies[0], &bodies[1]])
        .unwrap();
    world
        .prepare_interruption(PersonId(1), false, [PregnancyId(3)])
        .commit(&mut world)
        .unwrap();
    let before = world.clone();
    assert_eq!(completion.commit(&mut world), Err(Error::Unavailable));
    assert_eq!(world, before);
    let (mut world, _) = gestating();
    let birth = world
        .prepare_birth(PregnancyId(3), SimTime(30), PersonId(4), &body(1), 7)
        .unwrap();
    world
        .lives
        .insert(PersonId(4), life(4, Reproduction::Neither));
    let before = world.clone();
    assert_eq!(birth.commit(&mut world), Err(Error::Unavailable));
    assert_eq!(
        world, before,
        "pregnancy must remain gestating when the later child edit is stale"
    );
}

#[test]
fn typed_deadline_preflight_validates_actual_fsm_state_and_future_time_inertly() {
    let (world, _) = started();
    let before = world.clone();
    world
        .preflight_deadlines(SimTime(10), [Deadline::Conception(PregnancyId(3))])
        .unwrap();
    assert_eq!(
        world.preflight_deadlines(SimTime(9), [Deadline::Conception(PregnancyId(3))]),
        Err(Error::Unavailable)
    );
    assert_eq!(
        world.preflight_deadlines(SimTime(10), [Deadline::Birth(PregnancyId(3))]),
        Err(Error::Unavailable)
    );
    assert_eq!(world, before);
    let (world, _) = gestating();
    world
        .preflight_deadlines(
            SimTime(30),
            [Deadline::Birth(PregnancyId(3)), Deadline::Age(PersonId(1))],
        )
        .unwrap();
    assert_eq!(
        world.preflight_deadlines(SimTime(u64::MAX), []),
        Err(Error::Overflow)
    );
}

#[test]
fn domain_validation_checks_live_stages_attempts_and_scheduling_ports() {
    let world = world();
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
        .validate_with_facts(SimTime(0), &bodies, &deadlines)
        .unwrap();
    assert!(
        world
            .validate_with_facts(SimTime(0), &bodies, &deadlines[..1])
            .is_err()
    );
    let wrong = [
        (SimTime(99), Deadline::Age(PersonId(1))),
        (SimTime(100), Deadline::Age(PersonId(2))),
    ];
    assert!(
        world
            .validate_with_facts(SimTime(0), &bodies, &wrong)
            .is_err()
    );
    let (world, attempts) = started();
    let mut bodies = bodies;
    for attempt in attempts {
        let actor = attempt.actor;
        bodies.get_mut(&actor).unwrap().body = attempt;
    }
    world
        .validate_with_facts(SimTime(1), &bodies, &deadlines)
        .unwrap();
    bodies.get_mut(&PersonId(2)).unwrap().body.attempt = None;
    assert!(
        world
            .validate_with_facts(SimTime(1), &bodies, &deadlines)
            .is_err()
    );
}
