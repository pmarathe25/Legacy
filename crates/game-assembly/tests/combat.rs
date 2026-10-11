use game_api::*;
use game_assembly::*;

fn sim(workers: usize) -> Simulation {
    Simulation::combat_fixture(
        1,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap()
}
fn engage(sim: &mut Simulation, target: PersonId) -> EncounterId {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::Engage { target },
    })
    .unwrap();
    sim.observe().combat.unwrap().encounter
}
fn turn(
    sim: &mut Simulation,
    encounter: EncounterId,
    movement: Option<Point>,
    action: CombatAction,
) -> Result<CommandReceipt, Error> {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::CombatTurn {
            encounter,
            movement,
            action,
        },
    })
}
fn hp(sim: &Simulation, actor: PersonId) -> u64 {
    game_physiology::body(sim.canonical_state().components, actor)
        .expect("body exists")
        .vitals
        .health
}

#[test]
fn earlier_initiative_can_prevent_a_later_turn_and_each_round_costs_one_interval() {
    let mut sim = sim(4);
    let target = sim.canonical_state().state.people[3].id;
    let ruler = sim.canonical_state().state.ruler;
    let battle = engage(&mut sim, target);
    assert_eq!(
        sim.advance(SimDuration(60_000)).unwrap().reason,
        StopReason::CombatInput { encounter: battle }
    );
    assert_eq!(sim.time(), SimTime(0));
    for round in 0..3 {
        turn(&mut sim, battle, None, CombatAction::Attack { target }).unwrap();
        assert_eq!(sim.time(), SimTime(round * 6_000));
        sim.advance(SimDuration(6_000)).unwrap();
    }
    assert_eq!(sim.time(), SimTime(18_000));
    assert_eq!(hp(&sim, target), 45_000);
    assert_eq!(hp(&sim, ruler), 90_000);
    assert_eq!(
        sim.canonical_state()
            .events
            .iter()
            .filter(|e| matches!(e.event,EventKind::Struck {actor,..} if actor == target))
            .count(),
        2
    );
    assert_eq!(sim.canonical_state().state.people[0].work_units, 0);
    assert_eq!(sim.canonical_state().state.people[3].work_units, 0);
}

#[test]
fn turn_validation_is_atomic_for_movement_action_range_and_capture_prerequisites() {
    let mut sim = sim(1);
    let target = sim.canonical_state().state.people[3].id;
    let battle = engage(&mut sim, target);
    let before = sim.canonical_snapshot().unwrap();
    assert!(
        turn(
            &mut sim,
            battle,
            Some(Point { x: 5_000, y: 8_000 }),
            CombatAction::Capture { target }
        )
        .is_err()
    );
    assert_eq!(before, sim.canonical_snapshot().unwrap());
    assert!(
        turn(
            &mut sim,
            battle,
            Some(Point {
                x: 24_000,
                y: 16_000
            }),
            CombatAction::Attack { target }
        )
        .is_err()
    );
    assert!(
        turn(
            &mut sim,
            battle,
            Some(Point {
                x: 13_000,
                y: 6_000
            }),
            CombatAction::Wait
        )
        .is_err()
    );
    assert_eq!(before, sim.canonical_snapshot().unwrap());
    turn(
        &mut sim,
        battle,
        Some(Point { x: 5_123, y: 8_123 }),
        CombatAction::Defend,
    )
    .unwrap();
    assert_eq!(
        sim.observe()
            .combat
            .unwrap()
            .fighters
            .iter()
            .find(|f| f.actor == sim.canonical_state().state.ruler)
            .unwrap()
            .position,
        Point { x: 5_123, y: 8_123 }
    );
    assert!(hp(&sim, sim.canonical_state().state.ruler) >= 165_000);
    assert_eq!(sim.time(), SimTime(0));
}

#[test]
fn capture_uses_actual_bindings_and_release_takes_time() {
    let mut sim = sim(1);
    let target = sim.canonical_state().state.people[3].id;
    let ruler = sim.canonical_state().state.ruler;
    let battle = engage(&mut sim, target);
    let bindings: u64 = sim
        .canonical_state()
        .state
        .inventories
        .values()
        .filter(|i| i.good == Good::Binding)
        .map(|i| i.quantity.0)
        .sum();
    for _ in 0..3 {
        turn(&mut sim, battle, None, CombatAction::Attack { target }).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
    }
    turn(&mut sim, battle, None, CombatAction::Capture { target }).unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    assert!(
        sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
    );
    assert_eq!(
        sim.canonical_state()
            .state
            .inventories
            .values()
            .filter(|i| i.good == Good::Binding)
            .map(|i| i.quantity.0)
            .sum::<u64>(),
        bindings
    );
    assert_eq!(
        sim.debug_move_actor(target, AreaId(2)),
        Err(Error::Unavailable)
    );
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::FreeCaptive { target },
    })
    .unwrap();
    sim.advance(SimDuration(59_999)).unwrap();
    assert!(
        sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
    );
    sim.advance(SimDuration(1)).unwrap();
    assert!(
        !sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
    );
    assert!(sim.canonical_state().events.iter().any(|e| e.event
        == EventKind::Released {
            actor: ruler,
            target
        }));
}

#[test]
fn a_surrendered_person_can_be_captured_without_injury() {
    let mut sim = sim(4);
    let target = sim.canonical_state().state.people[3].id;
    let battle = engage(&mut sim, target);
    turn(&mut sim, battle, None, CombatAction::Surrender).unwrap();
    assert!(
        sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&sim.canonical_state().state.ruler)
    );
    assert_eq!(hp(&sim, sim.canonical_state().state.ruler), 180_000);
    sim.advance(SimDuration(6_000)).unwrap();
    assert!(!sim.observe().combat.unwrap().active);
}

#[test]
fn regeneration_in_place_does_not_remove_restraints() {
    let mut sim = sim(1);
    let guardian = sim.canonical_state().state.people[1].id;
    let battle = engage(&mut sim, guardian);
    for _ in 0..4 {
        turn(
            &mut sim,
            battle,
            None,
            CombatAction::Attack { target: guardian },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
    }
    turn(
        &mut sim,
        battle,
        None,
        CombatAction::Capture { target: guardian },
    )
    .unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    let custody = sim
        .canonical_state()
        .state
        .combat
        .as_ref()
        .unwrap()
        .captivity[&guardian]
        .clone();
    sim.advance(SimDuration(300_000)).unwrap();
    assert_eq!(hp(&sim, guardian), 180_000);
    assert_eq!(
        sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity[&guardian],
        custody
    );
    assert_eq!(
        sim.debug_move_actor(guardian, AreaId(2)),
        Err(Error::Unavailable)
    );
}

#[test]
fn saved_resolved_turns_are_not_repeated_and_observations_are_passive() {
    let mut original = sim(1);
    let target = original.canonical_state().state.people[3].id;
    let battle = engage(&mut original, target);
    turn(&mut original, battle, None, CombatAction::Attack { target }).unwrap();
    original.advance(SimDuration(1_234)).unwrap();
    let before = original.canonical_snapshot().unwrap();
    for _ in 0..100 {
        original.observe();
    }
    assert_eq!(before, original.canonical_snapshot().unwrap());
    let mut loaded = Simulation::load(
        &original.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    original.advance(SimDuration(4_766)).unwrap();
    loaded.advance(SimDuration(4_766)).unwrap();
    assert_eq!(original.canonical_state(), loaded.canonical_state());
    turn(&mut original, battle, None, CombatAction::Defend).unwrap();
    turn(&mut loaded, battle, None, CombatAction::Defend).unwrap();
    original.advance(SimDuration(6_000)).unwrap();
    for dt in [1, 333, 5_666] {
        loaded.advance(SimDuration(dt)).unwrap();
    }
    assert_eq!(original.canonical_state(), loaded.canonical_state());
}

#[test]
fn independent_encounters_share_the_clock_and_participant_count_does_not_multiply_time() {
    let mut sim = sim(4);
    let first = sim.canonical_state().state.people[5].id;
    let second = sim.canonical_state().state.people[6].id;
    sim.debug_move_actor(first, AreaId(3)).unwrap();
    sim.debug_move_actor(second, AreaId(3)).unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    let other = sim
        .debug_start_encounter(vec![(first, 0), (second, 1)])
        .unwrap();
    let enemy = sim.canonical_state().state.people[3].id;
    let friend = sim.canonical_state().state.people[7].id;
    let ruler = sim.canonical_state().state.ruler;
    let battle = sim
        .debug_start_encounter(vec![(ruler, 0), (enemy, 1), (friend, 0)])
        .unwrap();
    turn(&mut sim, battle, None, CombatAction::Defend).unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    assert_eq!(sim.time(), SimTime(186_000));
    assert_eq!(
        sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .encounters[&battle]
            .round,
        1
    );
    assert_eq!(
        sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .encounters[&other]
            .round,
        1
    );
}

#[test]
fn retreat_rejoins_actual_travel_without_crediting_the_round_as_travel() {
    let mut sim = sim(1);
    let target = sim.canonical_state().state.people[3].id;
    let battle = engage(&mut sim, target);
    turn(
        &mut sim,
        battle,
        Some(Point { x: 1_000, y: 8_000 }),
        CombatAction::Retreat {
            destination: AreaId(2),
        },
    )
    .unwrap();
    assert_eq!(sim.observe().location, Location::AtArea(AreaId(1)));
    sim.advance(SimDuration(6_000)).unwrap();
    assert!(matches!(
        sim.observe().location,
        Location::InTransit {
            departed: SimTime(6_000),
            arrives: SimTime(126_000),
            ..
        }
    ));
    sim.advance(SimDuration(119_999)).unwrap();
    assert!(matches!(sim.observe().location, Location::InTransit { .. }));
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(sim.observe().location, Location::AtArea(AreaId(2)));
    assert_eq!(sim.canonical_state().state.people[0].work_units, 0);
    assert!(sim.observe().combat.is_none());
}

#[test]
fn generated_geometry_and_damage_persist_across_loading_and_reentry() {
    let mut sim = sim(1);
    let guardian = sim.canonical_state().state.people[1].id;
    let battle = engage(&mut sim, guardian);
    let view = sim.observe().combat.unwrap();
    let map_id = view.map;
    let left = view.obstacles[0].minimum.x;
    turn(
        &mut sim,
        battle,
        Some(Point { x: 9_000, y: 8_000 }),
        CombatAction::Wait,
    )
    .unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    let spot = Point {
        x: left - 1_000,
        y: 8_000,
    };
    for i in 0..4 {
        turn(
            &mut sim,
            battle,
            if i == 0 { Some(spot) } else { None },
            CombatAction::Breach { obstacle: 0 },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
    }
    assert_eq!(sim.observe().combat.unwrap().obstacles[0].integrity, 0);
    let loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    assert_eq!(loaded.observe().combat, sim.observe().combat);
    let bounds = sim.canonical_state().state.combat.as_ref().unwrap().maps[&AreaId(1)].clone();
    // End this encounter by retreating, then return through the area graph.
    turn(
        &mut sim,
        battle,
        Some(Point {
            x: spot.x - 5_000,
            y: 8_000,
        }),
        CombatAction::Wait,
    )
    .unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    turn(
        &mut sim,
        battle,
        Some(Point { x: 1_000, y: 8_000 }),
        CombatAction::Retreat {
            destination: AreaId(2),
        },
    )
    .unwrap();
    sim.advance(SimDuration(126_000)).unwrap();
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::Move {
            destination: AreaId(1),
        },
    })
    .unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    engage(&mut sim, guardian);
    assert_eq!(sim.observe().combat.unwrap().map, map_id);
    assert_eq!(
        sim.canonical_state().state.combat.as_ref().unwrap().maps[&AreaId(1)],
        bounds
    );
}

#[test]
fn births_outside_an_encounter_continue_on_the_same_clock() {
    let mut sim = sim(1);
    let guardian = sim.canonical_state().state.people[1].id;
    let battle = engage(&mut sim, guardian);
    for _ in 0..50 {
        turn(&mut sim, battle, None, CombatAction::Defend).unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
    }
    assert_eq!(sim.time(), SimTime(300_000));
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| matches!(e.event, EventKind::Born { .. }) && e.at == SimTime(300_000))
    );
    assert_eq!(sim.canonical_state().state.people[0].work_units, 0);
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}
