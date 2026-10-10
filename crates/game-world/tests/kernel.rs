use game_api::*;
use game_world::*;

fn fixture(workers: usize, reverse_jobs: bool) -> Simulation {
    Simulation::kernel_fixture(
        1,
        1_000,
        Execution {
            workers,
            reverse_jobs,
        },
    )
    .unwrap()
}
fn move_to(sim: &mut Simulation, sequence: u64, area: u64) -> Result<CommandReceipt, Error> {
    sim.submit(Submission {
        caller_sequence: sequence,
        command: Command::Move {
            destination: AreaId(area),
        },
    })
}
fn contested(sim: &mut Simulation) -> InventoryId {
    let state = sim.canonical_state();
    let source = state
        .inventories
        .values()
        .find(|i| matches!(i.location, InventoryLocation::Area(_)))
        .unwrap()
        .id;
    let intentions = state.people[3..5]
        .iter()
        .map(|p| TransferIntent {
            actor: p.id,
            source,
            destination: p.inventory,
            quantity: Quantity(1_000),
        })
        .collect();
    sim.debug_schedule_transfers(SimTime(60_000), intentions)
        .unwrap();
    source
}

#[test]
fn arrival_consumes_elapsed_time_and_cannot_credit_travel_as_work() {
    let mut sim = fixture(4, false);
    move_to(&mut sim, 1, 2).unwrap();
    sim.advance(SimDuration(119_999)).unwrap();
    assert!(matches!(
        sim.observe().location,
        Location::InTransit {
            to: AreaId(2),
            arrives: SimTime(120_000),
            ..
        }
    ));
    assert_eq!(sim.canonical_state().people[0].work_units, 0);
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(sim.observe().location, Location::AtArea(AreaId(2)));
    assert_eq!(sim.canonical_state().people[0].work_units, 0);
    sim.advance(MINUTE).unwrap();
    assert_eq!(sim.canonical_state().people[0].work_units, 1_000);
    assert!(matches!(
        sim.canonical_state().events[1],
        Event {
            time: SimTime(120_000),
            kind: EventKind::Arrived {
                area: AreaId(2),
                ..
            },
            ..
        }
    ));
}

#[test]
fn invalid_commands_preserve_all_logical_state_and_allocator_even_in_transit() {
    let mut sim = fixture(1, false);
    let disconnected = sim.debug_add_disconnected_area("island").unwrap();
    move_to(&mut sim, 1, 2).unwrap();
    sim.advance(SimDuration(17)).unwrap();
    let before = sim.canonical_state().clone();
    assert_eq!(move_to(&mut sim, 2, 999), Err(Error::UnknownArea));
    assert_eq!(move_to(&mut sim, 2, disconnected.0), Err(Error::NoRoute));
    assert_eq!(&before, sim.canonical_state());
}

#[test]
fn competing_transfers_conserve_stock_and_carry_it_once() {
    let mut sim = fixture(4, true);
    let source = contested(&mut sim);
    sim.advance(MINUTE).unwrap();
    let state = sim.canonical_state();
    assert_eq!(state.inventories[&source].quantity, Quantity(0));
    assert_eq!(
        state
            .inventories
            .values()
            .map(|i| i.quantity.0)
            .sum::<u64>(),
        1_000
    );
    assert_eq!(
        state.people[3..5]
            .iter()
            .filter(|p| state.inventories[&p.inventory].quantity == Quantity(1_000))
            .count(),
        1
    );
    assert_eq!(
        state
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Transferred { .. }))
            .count(),
        1
    );
    assert_eq!(
        state.inventories[&source].last_granted,
        Some(state.people[3].id)
    );
}

#[test]
fn chunking_workers_and_perturbed_completion_preserve_state_and_events() {
    let mut reference = fixture(1, false);
    contested(&mut reference);
    move_to(&mut reference, 1, 2).unwrap();
    reference.advance(SimDuration(487_123)).unwrap();
    for workers in [1, 2, 4] {
        for reverse in [false, true] {
            let mut sim = fixture(workers, reverse);
            contested(&mut sim);
            move_to(&mut sim, 1, 2).unwrap();
            for delta in [1, 17, 59_981, 4_000, 123_001, 300_123] {
                sim.advance(SimDuration(delta)).unwrap();
            }
            assert_eq!(reference.canonical_state(), sim.canonical_state());
        }
    }
}

#[test]
fn queries_are_passive_and_normal_views_omit_private_world_state() {
    let mut sim = fixture(4, false);
    move_to(&mut sim, 1, 2).unwrap();
    let before = sim.canonical_state().clone();
    for _ in 0..100 {
        let view = sim.observe();
        assert_eq!(view.time, SimTime(0));
    }
    assert_eq!(&before, sim.canonical_state());
    let json = serde_json::to_string(&sim.observe()).unwrap();
    assert!(
        !json.contains("work_units") && !json.contains("inventories") && !json.contains("citizen")
    );
}

#[test]
fn duplicate_commands_are_idempotent_and_replacement_finishes_current_leg() {
    let mut sim = fixture(2, false);
    let receipt = move_to(&mut sim, 42, 2).unwrap();
    let before = sim.canonical_state().clone();
    assert_eq!(move_to(&mut sim, 42, 2).unwrap(), receipt);
    assert_eq!(&before, sim.canonical_state());
    assert_eq!(move_to(&mut sim, 42, 3), Err(Error::SequenceConflict));
    sim.advance(SimDuration(10_000)).unwrap();
    move_to(&mut sim, 43, 3).unwrap();
    sim.advance(SimDuration(110_000)).unwrap();
    assert!(matches!(
        sim.observe().location,
        Location::InTransit {
            from: AreaId(2),
            to: AreaId(1),
            arrives: SimTime(240_000),
            ..
        }
    ));
    sim.advance(SimDuration(300_000)).unwrap();
    assert_eq!(sim.observe().location, Location::AtArea(AreaId(3)));
    assert_eq!(sim.canonical_state().people[0].work_units, 0);
}

#[test]
fn save_load_preserves_fractional_progress_pending_transfers_and_idempotency() {
    let mut original = fixture(1, false);
    contested(&mut original);
    move_to(&mut original, 7, 3).unwrap();
    original.advance(SimDuration(12_345)).unwrap();
    let mut loaded = Simulation::load(
        &original.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    assert_eq!(original.canonical_state(), loaded.canonical_state());
    original.advance(SimDuration(987_654)).unwrap();
    loaded.advance(SimDuration(987_654)).unwrap();
    assert_eq!(original.canonical_state(), loaded.canonical_state());
    let before = loaded.canonical_state().clone();
    move_to(&mut loaded, 7, 3).unwrap();
    assert_eq!(&before, loaded.canonical_state());
    let mut json: serde_json::Value = serde_json::from_slice(&loaded.save().unwrap()).unwrap();
    let valid_version = json["version"].clone();
    json["version"] = 999.into();
    assert!(Simulation::load(&serde_json::to_vec(&json).unwrap(), Execution::default()).is_err());
    json["version"] = valid_version;
    json["state"]["next_id"] = 1.into();
    assert!(Simulation::load(&serde_json::to_vec(&json).unwrap(), Execution::default()).is_err());
}

#[test]
fn overflow_and_bad_configuration_leave_committed_state_intact() {
    assert!(
        Simulation::kernel_fixture(
            1,
            15,
            Execution {
                workers: 0,
                reverse_jobs: false
            }
        )
        .is_err()
    );
    let mut sim = fixture(1, false);
    sim.advance(SimDuration(1)).unwrap();
    let before = sim.canonical_state().clone();
    assert_eq!(sim.advance(SimDuration(u64::MAX)), Err(Error::Overflow));
    assert_eq!(&before, sim.canonical_state());
    assert_eq!(
        compute::progress(compute::ProgressInput {
            units: u64::MAX,
            remainder: 0,
            units_per_minute: 1,
            elapsed_ms: 60_000
        }),
        Err(Error::Overflow)
    );
}

#[test]
fn replacing_a_route_with_the_current_leg_endpoint_stops_after_that_leg() {
    let mut sim = fixture(1, false);
    move_to(&mut sim, 1, 3).unwrap();
    sim.advance(SimDuration(50_000)).unwrap();
    move_to(&mut sim, 2, 2).unwrap();
    assert!(sim.observe().available_commands.contains(&Command::Move {
        destination: AreaId(3)
    }));
    move_to(&mut sim, 3, 3).unwrap();
    assert!(matches!(
        sim.observe().location,
        Location::InTransit {
            arrives: SimTime(180_000),
            ..
        }
    ));
    sim.advance(SimDuration(130_000)).unwrap();
    assert_eq!(sim.observe().location, Location::AtArea(AreaId(3)));
    assert_eq!(sim.canonical_state().people[0].work_units, 0);
    assert!(sim.canonical_state().people[0].route.is_empty());
}
