mod support;
use game_api::*;
use game_assembly::*;

fn child(workers: usize) -> Simulation {
    Simulation::new(
        "childhood-fixture",
        1,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap()
}
fn submit(sim: &mut Simulation, command: Command) {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
    .unwrap();
}

#[test]
fn infancy_signals_have_timed_voluntary_responses_and_no_free_resources() {
    let mut sim = child(1);
    let ruler = sim.observe().ruler;
    assert!(
        sim.observe()
            .available_commands
            .iter()
            .all(|c| matches!(c, Command::Signal { .. } | Command::Cancel))
    );
    let opening = sim.canonical_snapshot().unwrap();
    for command in [
        Command::Move {
            destination: AreaId(2),
        },
        Command::Play {
            interest: ChildhoodInterest::Craft,
        },
        Command::Rest,
    ] {
        assert!(
            sim.submit(Submission {
                caller_sequence: 1,
                command
            })
            .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), opening);
    }
    submit(
        &mut sim,
        Command::Signal {
            preference: ChildhoodInterest::Food,
        },
    );
    sim.advance(SimDuration(5_999)).unwrap();
    assert!(
        game_lifecycle::life(sim.canonical_state().components, ruler)
            .expect("life exists")
            .preference
            .is_none()
    );
    sim.advance(SimDuration(1)).unwrap();
    assert!(
        game_lifecycle::life(sim.canonical_state().components, ruler)
            .expect("life exists")
            .preference
            .is_some()
    );
    sim.advance(SimDuration(174_000)).unwrap();
    assert!(sim.canonical_state().events.iter().any(|e| matches!(e.event, EventKind::Cared { child, quantity: Quantity(1_000), .. } if child == ruler)));
    assert!(
        !game_lifecycle::life(sim.canonical_state().components, ruler)
            .expect("life exists")
            .caregivers
            .is_empty()
    );
    assert!(
        game_lifecycle::life(sim.canonical_state().components, ruler)
            .expect("life exists")
            .preference
            .is_none()
    );
    submit(
        &mut sim,
        Command::Signal {
            preference: ChildhoodInterest::Company,
        },
    );
    sim.advance(SimDuration(120_000)).unwrap();
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| matches!(e.event, EventKind::Comforted { child, .. } if child == ruler))
    );
    assert!(
        !game_characters::decisions::mind::<game_api::Command>(
            sim.canonical_state().components,
            ruler
        )
        .expect("mind exists")
        .relationships
        .is_empty()
    );
}

#[test]
fn growth_unlocks_movement_and_play_without_adult_production() {
    let mut sim = child(1);
    submit(
        &mut sim,
        Command::Signal {
            preference: ChildhoodInterest::Food,
        },
    );
    sim.advance(SimDuration(300_000)).unwrap();
    let ruler = sim.observe().ruler;
    assert_eq!(
        game_lifecycle::life(sim.canonical_state().components, ruler)
            .expect("life exists")
            .stage,
        LifeStage::Child
    );
    assert!(
        !sim.observe()
            .available_commands
            .iter()
            .any(|c| matches!(c, Command::Work { .. }))
    );
    submit(
        &mut sim,
        Command::Play {
            interest: ChildhoodInterest::Craft,
        },
    );
    let opening_stocks = sim.canonical_state().state.inventories.clone();
    let work = sim
        .canonical_state()
        .state
        .people
        .iter()
        .find(|p| p.id == ruler)
        .unwrap()
        .work_units;
    sim.advance(SimDuration(59_999)).unwrap();
    assert!(sim.observe().interests.is_empty());
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(
        sim.observe().interests,
        vec![(ChildhoodInterest::Craft, 100)]
    );
    assert_eq!(
        work,
        sim.canonical_state()
            .state
            .people
            .iter()
            .find(|p| p.id == ruler)
            .unwrap()
            .work_units
    );
    for i in opening_stocks
        .values()
        .filter(|i| i.location == InventoryLocation::Carried(ruler))
    {
        assert_eq!(
            i.quantity,
            sim.canonical_state().state.inventories[&i.id].quantity
        );
    }
    submit(
        &mut sim,
        Command::Move {
            destination: AreaId(2),
        },
    );
    sim.advance(SimDuration(120_000)).unwrap();
    assert_eq!(sim.observe().location, Location::AtArea(AreaId(2)));
}

#[test]
fn pending_play_and_care_survive_saves_queries_workers_and_time_slices() {
    let mut a = child(1);
    submit(
        &mut a,
        Command::Signal {
            preference: ChildhoodInterest::Food,
        },
    );
    a.advance(SimDuration(300_000)).unwrap();
    submit(
        &mut a,
        Command::Play {
            interest: ChildhoodInterest::Movement,
        },
    );
    a.advance(SimDuration(30_000)).unwrap();
    let mut b = Simulation::load(
        &a.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let before = a.canonical_snapshot().unwrap();
    for _ in 0..100 {
        a.observe();
    }
    assert_eq!(a.canonical_snapshot().unwrap(), before);
    a.advance(SimDuration(90_000)).unwrap();
    for dt in [1, 29_999, 20_000, 40_000] {
        b.advance(SimDuration(dt)).unwrap();
    }
    assert_eq!(a.canonical_state(), b.canonical_state());
    assert_eq!(
        a.observe().interests,
        vec![(ChildhoodInterest::Movement, 100)]
    );
}

#[test]
fn play_interrupts_at_fatigue_limit_and_pending_child_actions_can_cross_maturity() {
    let mut sim = child(1);
    submit(
        &mut sim,
        Command::Signal {
            preference: ChildhoodInterest::Food,
        },
    );
    sim.advance(SimDuration(300_000)).unwrap();
    let ruler = sim.observe().ruler;
    let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    support::component(&mut saved, "physiology/body", ruler)["vitals"]["fatigue"] =
        serde_json::json!(299_999);
    let mut sim =
        Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()).unwrap();
    submit(
        &mut sim,
        Command::Play {
            interest: ChildhoodInterest::Craft,
        },
    );
    sim.advance(SimDuration(60_000)).unwrap();
    assert!(sim.observe().interests.is_empty());
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| e.at == SimTime(600_001)
                && matches!(e.event, EventKind::Interrupted { actor } if actor == ruler))
    );
    submit(&mut sim, Command::Rest);
    sim.advance(SimDuration(239_999)).unwrap();
    submit(
        &mut sim,
        Command::Signal {
            preference: ChildhoodInterest::Company,
        },
    );
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(sim.observe().life_stage, Some(LifeStage::Adult));
    let mut loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    loaded.advance(SimDuration(6_000)).unwrap();
    assert_eq!(sim.canonical_state(), loaded.canonical_state());
}

#[test]
fn a_signal_does_not_compel_care_and_unknown_places_stay_out_of_normal_views() {
    let mut sim = child(1);
    let unknown = sim.debug_add_disconnected_area("unseen island").unwrap();
    assert!(sim.observe().known_areas.iter().all(|a| a.id != unknown));
    let citizens: Vec<_> = sim
        .canonical_state()
        .state
        .people
        .iter()
        .filter(|p| p.id != sim.observe().ruler)
        .map(|p| p.id)
        .collect();
    for id in citizens {
        let mut personality = game_characters::decisions::mind::<game_api::Command>(
            sim.canonical_state().components,
            id,
        )
        .expect("mind exists")
        .personality
        .clone();
        personality.empathy = 0;
        let dream = game_characters::decisions::mind::<game_api::Command>(
            sim.canonical_state().components,
            id,
        )
        .expect("mind exists")
        .dream;
        sim.debug_set_personality(id, personality, dream).unwrap();
        let ruler = sim.observe().ruler;
        sim.debug_set_relationship(
            id,
            ruler,
            game_assembly::decisions::Relationship {
                trust: 0,
                affection: 0,
            },
        )
        .unwrap();
    }
    submit(
        &mut sim,
        Command::Signal {
            preference: ChildhoodInterest::Food,
        },
    );
    sim.advance(SimDuration(180_000)).unwrap();
    let ruler = sim.observe().ruler;
    assert!(
        !sim.canonical_state()
            .events
            .iter()
            .any(|e| matches!(e.event, EventKind::Cared { child, .. } if child == ruler))
    );
    assert!(sim.observe().preference.is_some());
}
