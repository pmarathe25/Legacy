use game_api::*;
use game_assembly::*;
use std::collections::BTreeSet;

fn sim(workers: usize) -> Simulation {
    Simulation::survival_fixture(
        1,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap()
}
fn command(sim: &mut Simulation, command: Command) -> Result<CommandReceipt, Error> {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
}
fn food(sim: &Simulation) -> InventoryId {
    sim.canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Food && i.location == InventoryLocation::Area(AreaId(1)))
        .unwrap()
        .id
}
fn actor(sim: &Simulation) -> PersonId {
    sim.canonical_state().ruler
}
fn food_total(sim: &Simulation) -> u64 {
    sim.canonical_state()
        .inventories
        .values()
        .filter(|i| i.good == Good::Food)
        .map(|i| i.quantity.0)
        .sum()
}

#[test]
fn reachable_food_requires_consumption_while_guardians_have_no_hunger_need() {
    let mut sim = sim(4);
    let source = food(&sim);
    let before = food_total(&sim);
    for _ in 0..20 {
        command(&mut sim, Command::Eat { source }).unwrap();
        sim.advance(SimDuration(59_999)).unwrap();
        assert!(matches!(
            sim.observe().physical.unwrap().activity,
            Activity::Eat { .. }
        ));
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(sim.observe().physical.unwrap().vitals.hunger, 0);
    }
    assert_eq!(food_total(&sim), before - 20_000);
    assert!(sim.observe().physical.unwrap().vitals.alive);
    let guardian = sim.canonical_state().people[1].id;
    sim.debug_actor_command(guardian, Command::Eat { source })
        .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(food_total(&sim), before - 21_000);
    let state = sim.canonical_state();
    assert_eq!(
        game_physiology::body(&state.components, guardian)
            .expect("body exists")
            .vitals
            .hunger,
        0
    );
    assert!(
        game_physiology::body(&state.components, guardian)
            .expect("body exists")
            .vitals
            .alive
    );
}

#[test]
fn inaccessible_abundance_does_not_prevent_mortal_death_and_goods_remain() {
    let mut sim = sim(1);
    let ruler = actor(&sim);
    let locked = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(1)),
            Quantity(1_000_000),
            Quantity(1_000_000),
            BTreeSet::new(),
        )
        .unwrap();
    let remote = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(3)),
            Quantity(1_000_000),
            Quantity(1_000_000),
            BTreeSet::from([ruler]),
        )
        .unwrap();
    let before = sim.canonical_state().clone();
    assert_eq!(
        command(&mut sim, Command::Eat { source: locked }),
        Err(Error::Unavailable)
    );
    assert_eq!(
        command(&mut sim, Command::Eat { source: remote }),
        Err(Error::Unavailable)
    );
    assert_eq!(&before, sim.canonical_state());
    sim.advance(SimDuration(779_999)).unwrap();
    assert!(sim.observe().physical.unwrap().vitals.alive);
    sim.advance(SimDuration(1)).unwrap();
    assert!(!sim.observe().physical.unwrap().vitals.alive);
    assert_eq!(
        sim.canonical_state().inventories[&locked].quantity,
        Quantity(1_000_000)
    );
    assert_eq!(
        sim.canonical_state().inventories[&remote].quantity,
        Quantity(1_000_000)
    );
    assert!(sim.observe().available_commands.is_empty());
    let before = sim.canonical_state().clone();
    assert_eq!(
        command(
            &mut sim,
            Command::Move {
                destination: AreaId(2)
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(&before, sim.canonical_state());
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| e.time == SimTime(780_000) && e.kind == EventKind::Died { actor: ruler })
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn load_travel_unload_is_physical_and_cancellation_keeps_cargo() {
    let mut sim = sim(4);
    let ruler = actor(&sim);
    let source = food(&sim);
    let carried = sim.canonical_state().people[0].inventory;
    let destination = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(2)),
            Quantity(0),
            Quantity(10_000),
            BTreeSet::from([ruler]),
        )
        .unwrap();
    let before = food_total(&sim);
    command(
        &mut sim,
        Command::Load {
            source,
            destination: carried,
            quantity: Quantity(2_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&carried].quantity,
        Quantity(2_000)
    );
    command(
        &mut sim,
        Command::Move {
            destination: AreaId(2),
        },
    )
    .unwrap();
    sim.advance(SimDuration(119_999)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.canonical_state().inventories[&carried].location,
        InventoryLocation::Carried(ruler)
    );
    sim.advance(SimDuration(1)).unwrap();
    command(
        &mut sim,
        Command::Load {
            source: carried,
            destination,
            quantity: Quantity(2_000),
        },
    )
    .unwrap();
    sim.advance(SimDuration(30_000)).unwrap();
    command(&mut sim, Command::Cancel).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&carried].quantity,
        Quantity(2_000)
    );
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(0)
    );
    command(&mut sim, Command::Rest).unwrap();
    sim.advance(MINUTE).unwrap();
    command(
        &mut sim,
        Command::Load {
            source: carried,
            destination,
            quantity: Quantity(2_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(2_000)
    );
    assert_eq!(food_total(&sim), before);
    assert_eq!(sim.canonical_state().people[0].work_units, 0);
}

#[test]
fn production_consumes_inputs_once_and_interruption_preserves_progress() {
    let mut sim = sim(2);
    let site = game_workshop::sites(&sim.canonical_state().components)
        .map(|(_, site)| site)
        .find(|s| s.area == AreaId(1))
        .unwrap()
        .clone();
    let input = sim.canonical_state().inventories[&site.input].quantity.0;
    let output = sim.canonical_state().inventories[&site.output].quantity.0;
    command(&mut sim, Command::Work { site: site.id }).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&site.input].quantity.0,
        input - 1_000
    );
    sim.advance(SimDuration(23_456)).unwrap();
    command(&mut sim, Command::Rest).unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&site.output].quantity.0,
        output
    );
    command(&mut sim, Command::Work { site: site.id }).unwrap();
    sim.advance(SimDuration(36_544)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&site.input].quantity.0,
        input - 1_000
    );
    assert_eq!(
        sim.canonical_state().inventories[&site.output].quantity.0,
        output + 1_000
    );
    assert_eq!(sim.canonical_state().people[0].work_units, 1_000);
    command(&mut sim, Command::Work { site: site.id }).unwrap();
    command(&mut sim, Command::Cancel).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&site.input].quantity.0,
        input - 2_000
    );
    assert_eq!(
        sim.canonical_state().inventories[&site.output].quantity.0,
        output + 1_000
    );
}

#[test]
fn spoilage_invalidates_loading_without_duplicating_reserved_food() {
    let mut sim = sim(1);
    let source = food(&sim);
    let carried = sim.canonical_state().people[0].inventory;
    command(
        &mut sim,
        Command::Load {
            source,
            destination: carried,
            quantity: Quantity(1_000),
        },
    )
    .unwrap();
    sim.debug_spoil_at(source, SimTime(30_000)).unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.canonical_state().inventories[&carried].quantity,
        Quantity(0)
    );
    assert_eq!(sim.observe().physical.unwrap().activity, Activity::Idle);
    assert!(sim.canonical_state().events.iter().any(|e| matches!(
        e.kind,
        EventKind::Spoiled {
            quantity: Quantity(100_000),
            ..
        }
    )));
}

#[test]
fn guardian_injury_recovers_and_mortal_rest_preserves_chunking() {
    let mut reference = sim(1);
    let guardian = reference.canonical_state().people[1].id;
    let ruler = actor(&reference);
    reference.debug_injure(guardian, 180_000).unwrap();
    reference.debug_injure(ruler, 100_000).unwrap();
    command(&mut reference, Command::Rest).unwrap();
    let mut split = Simulation::load(
        &reference.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    reference.advance(SimDuration(620_001)).unwrap();
    for dt in [1, 59_999, 23_456, 17, 400_003, 136_525] {
        split.advance(SimDuration(dt)).unwrap();
    }
    assert_eq!(reference.canonical_state(), split.canonical_state());
    let vitals = &game_physiology::body(&reference.canonical_state().components, guardian)
        .expect("body exists")
        .vitals;
    assert_eq!(vitals.health, 180_000);
    assert_eq!(vitals.hunger, 0);
    assert!(vitals.alive);
    assert_eq!(reference.observe().physical.unwrap().vitals.health, 159_999);
}

#[test]
fn timed_actions_reservations_and_partial_work_survive_saving() {
    let mut sim = sim(1);
    let source = food(&sim);
    let carried = sim.canonical_state().people[0].inventory;
    command(
        &mut sim,
        Command::Load {
            source,
            destination: carried,
            quantity: Quantity(1_000),
        },
    )
    .unwrap();
    sim.advance(SimDuration(17_123)).unwrap();
    let mut loaded = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    sim.advance(SimDuration(42_877)).unwrap();
    loaded.advance(SimDuration(42_877)).unwrap();
    assert_eq!(sim.canonical_state(), loaded.canonical_state());
    let site = game_workshop::sites(&sim.canonical_state().components)
        .map(|(_, site)| site)
        .find(|s| s.area == AreaId(1))
        .unwrap()
        .id;
    command(&mut sim, Command::Work { site }).unwrap();
    sim.advance(SimDuration(13_003)).unwrap();
    let mut loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    sim.advance(SimDuration(100_000)).unwrap();
    loaded.advance(SimDuration(100_000)).unwrap();
    assert_eq!(sim.canonical_state(), loaded.canonical_state());
}

#[test]
fn all_carried_stacks_share_one_capacity_during_contested_commit() {
    let mut sim = sim(4);
    let ruler = actor(&sim);
    let source = food(&sim);
    let carried = sim.canonical_state().people[0].inventory;
    let timber_source = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Timber && i.location == InventoryLocation::Area(AreaId(1)))
        .unwrap()
        .id;
    let timber_carried = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Timber && i.location == InventoryLocation::Carried(ruler))
        .unwrap()
        .id;
    sim.debug_schedule_transfers(
        SimTime(60_000),
        vec![
            TransferIntent {
                actor: ruler,
                source,
                destination: carried,
                quantity: Quantity(6_000),
            },
            TransferIntent {
                actor: ruler,
                source: timber_source,
                destination: timber_carried,
                quantity: Quantity(6_000),
            },
        ],
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state()
            .inventories
            .values()
            .filter(|i| i.location == InventoryLocation::Carried(ruler))
            .map(|i| i.quantity.0)
            .sum::<u64>(),
        6_000
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn recovery_events_and_fatigue_interruptions_have_exact_chunk_independent_times() {
    let mut sim = sim(1);
    let guardian = sim.canonical_state().people[1].id;
    sim.debug_injure(guardian, 123_456).unwrap();
    let mut split = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    for dt in [100_000, 23_456, 56_544] {
        split.advance(SimDuration(dt)).unwrap();
    }
    assert_eq!(sim.canonical_state(), split.canonical_state());
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| e.kind == EventKind::Recovered { actor: guardian }
                && e.time == SimTime(123_456))
    );
}

#[test]
fn existing_physical_completion_and_debug_transfer_cannot_consume_the_same_units() {
    let mut sim = sim(1);
    let ruler = actor(&sim);
    let source = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(1)),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([ruler]),
        )
        .unwrap();
    let carried = sim.canonical_state().people[0].inventory;
    command(&mut sim, Command::Eat { source }).unwrap();
    sim.debug_schedule_transfers(
        SimTime(60_000),
        vec![TransferIntent {
            actor: ruler,
            source,
            destination: carried,
            quantity: Quantity(1_000),
        }],
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.canonical_state().inventories[&carried].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.canonical_state()
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Ate { .. }))
            .count(),
        1
    );
    assert_eq!(
        sim.canonical_state()
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Transferred { .. }))
            .count(),
        0
    );
}

fn arriving_at_a_body_limit(exhaustion: bool) -> (Simulation, PersonId) {
    let mut sim = sim(1);
    command(
        &mut sim,
        Command::Move {
            destination: AreaId(2),
        },
    )
    .unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    let ruler = actor(&sim);
    let mut envelope: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let mut state: CanonicalState = serde_json::from_value(envelope["state"].take()).unwrap();
    let physical = state.physical.as_mut().unwrap();
    let body = game_physiology::body_mut(&mut state.components, ruler).unwrap();
    body.vitals.fatigue = if exhaustion {
        physical.rules.fatigue_limit - 120_000
    } else {
        0
    };
    if !exhaustion {
        body.vitals.hunger = physical.rules.hunger_harm;
        body.vitals.health = 120_000;
    }
    envelope["state"] = serde_json::to_value(state).unwrap();
    sim = Simulation::load(
        &serde_json::to_vec(&envelope).unwrap(),
        Execution::default(),
    )
    .unwrap();
    command(
        &mut sim,
        Command::Move {
            destination: AreaId(3),
        },
    )
    .unwrap();
    (sim, ruler)
}

#[test]
fn death_at_an_intermediate_arrival_does_not_publish_a_phantom_next_leg() {
    let (mut sim, ruler) = arriving_at_a_body_limit(false);
    sim.advance(SimDuration(120_000)).unwrap();
    let state = sim.canonical_state();
    assert!(
        !game_physiology::body(&state.components, ruler)
            .expect("body exists")
            .vitals
            .alive
    );
    assert_eq!(
        *game_spatial::location(
            &state.components,
            game_ecs::StableId((state.people[0].id).0)
        )
        .expect("location exists"),
        Location::AtArea(AreaId(1))
    );
    assert!(state.people[0].route.is_empty());
    assert!(
        !state
            .events
            .iter()
            .any(|event| event.time == SimTime(240_000)
                && matches!(event.kind, EventKind::Departed { actor, .. } if actor == ruler))
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn exhaustion_at_an_intermediate_arrival_requires_rest_before_another_leg() {
    let (mut sim, ruler) = arriving_at_a_body_limit(true);
    sim.advance(SimDuration(120_000)).unwrap();
    assert_eq!(
        *game_spatial::location(
            &sim.canonical_state().components,
            game_ecs::StableId((sim.canonical_state().people[0].id).0)
        )
        .expect("location exists"),
        Location::AtArea(AreaId(1))
    );
    assert!(sim.canonical_state().people[0].route.is_empty());
    assert_eq!(
        command(
            &mut sim,
            Command::Move {
                destination: AreaId(3)
            }
        ),
        Err(Error::Unavailable)
    );
    assert!(
        game_physiology::body(&sim.canonical_state().components, ruler)
            .expect("body exists")
            .vitals
            .alive
    );
    command(&mut sim, Command::Rest).unwrap();
    sim.advance(MINUTE).unwrap();
    command(
        &mut sim,
        Command::Move {
            destination: AreaId(3),
        },
    )
    .unwrap();
}
