use game_api::*;
use game_world::*;
use std::collections::BTreeSet;

fn fixture() -> (Simulation, InventoryId) {
    let mut sim = Simulation::new("health-fixture", 91, Execution::default()).unwrap();
    let actor = sim.canonical_state().ruler;
    let area = match *sim
        .canonical_state()
        .components
        .location(actor)
        .expect("location exists")
    {
        Location::AtArea(area) => area,
        _ => unreachable!(),
    };
    let water = sim
        .debug_add_inventory(
            Good::Water,
            InventoryLocation::Area(area),
            Quantity(2_000),
            Quantity(2_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
    (sim, water)
}
fn drink(sim: &mut Simulation, source: InventoryId) {
    sim.submit(Submission {
        caller_sequence: 1,
        command: Command::Health {
            action: HealthAction::Drink { source },
        },
    })
    .unwrap();
}

#[test]
fn drinking_requires_time_and_real_water_and_does_not_feed_the_body() {
    let (mut sim, source) = fixture();
    sim.advance(SimDuration(50_000)).unwrap();
    let before = sim.observe();
    drink(&mut sim, source);
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(2_000)
    );
    sim.advance(SimDuration(9_999)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(2_000)
    );
    sim.advance(SimDuration(1)).unwrap();
    let after = sim.observe();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(1_000)
    );
    assert_eq!(after.health.as_ref().unwrap().thirst, 0);
    assert_eq!(
        after.physical.as_ref().unwrap().vitals.hunger,
        before.physical.as_ref().unwrap().vitals.hunger + 10_000
    );
}

#[test]
fn an_interrupted_or_removed_drink_cannot_deliver_water() {
    let (mut sim, source) = fixture();
    drink(&mut sim, source);
    sim.advance(SimDuration(5_000)).unwrap();
    sim.submit(Submission {
        caller_sequence: 2,
        command: Command::Cancel,
    })
    .unwrap();
    sim.advance(SimDuration(5_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(2_000)
    );
    assert_eq!(sim.observe().health.unwrap().thirst, 10_000);
    let (mut sim, source) = fixture();
    let actor = sim.canonical_state().ruler;
    let destination = sim
        .debug_add_inventory(
            Good::Water,
            InventoryLocation::Carried(actor),
            Quantity(0),
            Quantity(2_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
    sim.debug_schedule_transfers(
        SimTime(5_000),
        vec![TransferIntent {
            actor,
            source,
            destination,
            quantity: Quantity(2_000),
        }],
    )
    .unwrap();
    drink(&mut sim, source);
    sim.advance(SimDuration(10_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
    assert_eq!(sim.observe().health.unwrap().thirst, 10_000);
}

#[test]
fn pending_drinking_and_clinical_state_survive_current_save_restore_and_slice_changes() {
    let (mut whole, source) = fixture();
    drink(&mut whole, source);
    whole.advance(SimDuration(4_003)).unwrap();
    let mut restored = Simulation::load(
        &whole.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    whole.advance(SimDuration(5_997)).unwrap();
    for elapsed in [1, 996, 5_000] {
        restored.advance(SimDuration(elapsed)).unwrap();
    }
    assert_eq!(whole.canonical_state(), restored.canonical_state());
}

#[test]
fn malformed_clinical_actor_is_rejected_by_the_save_boundary() {
    let (sim, _) = fixture();
    let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    // Attach clinical data to an existing non-person entity, so the snapshot
    // remains structurally valid and the health actor boundary rejects it.
    let foreign = sim
        .canonical_state()
        .components
        .sites()
        .next()
        .expect("fixture workshop")
        .0
        .0;
    let rows = saved["state"]["components"]["columns"]["health/clinical"]
        .as_array_mut()
        .expect("actual clinical component column");
    rows.push(serde_json::json!({
        "entity": foreign,
        "value": game_world::health::Clinical::default(),
    }));
    rows.sort_by_key(|row| row["entity"].as_u64().unwrap());
    assert!(matches!(
        Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()),
        Err(Error::InvalidSave(_))
    ));
}

#[test]
fn public_boiling_option_purifies_real_collected_water_then_drinking_consumes_it() {
    use game_world::decisions::{Aspiration, Personality};
    let mut whole = Simulation::ecology_fixture(67, Execution::default()).unwrap();
    whole.initialize_health(HealthRules::fixture()).unwrap();
    let actor = whole.canonical_state().ruler;
    for person in whole.canonical_state().people.clone() {
        whole
            .debug_actor_command(person.id, Command::Cancel)
            .unwrap();
        whole
            .debug_set_personality(person.id, Personality::restful(), Aspiration::Comfort)
            .unwrap();
    }
    let resource = whole
        .debug_add_resource(
            ResourceKind::Spring,
            AreaId(1),
            Quantity(1_000),
            Quantity(1_000),
            Quantity(0),
            BTreeSet::from([actor]),
            None,
            300,
        )
        .unwrap();
    whole
        .submit(Submission {
            caller_sequence: whole.next_caller_sequence().unwrap(),
            command: Command::Ecology {
                action: EcologyAction::Gather {
                    resource,
                    quantity: Quantity(1_000),
                    unauthorized: false,
                },
            },
        })
        .unwrap();
    whole.advance(MINUTE).unwrap();
    let water = whole
        .canonical_state()
        .events
        .iter()
        .find_map(|event| match event.kind {
            EventKind::ResourceCollected {
                resource: from,
                destination,
                ..
            } if from == resource => Some(destination),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        whole.canonical_state().inventories[&water].quantity,
        Quantity(1_000)
    );
    assert!(whole.canonical_state().health.as_ref().unwrap().water[&water].load() > 0);
    let before = whole.canonical_state().clone();
    let command=whole.observe().available_commands.into_iter().find(|command|matches!(command,Command::Health {action:HealthAction::Boil {source,..}} if *source==water)).expect("known collected serving must expose native boiling");
    for _ in 0..20 {
        assert!(whole.observe().available_commands.contains(&command));
    }
    assert_eq!(whole.canonical_state(), &before);
    let Command::Health {
        action: HealthAction::Boil { fuel, tool, .. },
    } = command
    else {
        unreachable!()
    };
    let original_fuel = whole.canonical_state().inventories[&fuel].quantity;
    let original_tool = whole.canonical_state().inventories[&tool].quantity;
    let fuel_cost = whole
        .canonical_state()
        .health
        .as_ref()
        .unwrap()
        .rules
        .boiling_fuel;
    let duration = whole
        .canonical_state()
        .health
        .as_ref()
        .unwrap()
        .rules
        .boiling_time;
    whole
        .submit(Submission {
            caller_sequence: whole.next_caller_sequence().unwrap(),
            command,
        })
        .unwrap();
    assert_eq!(
        whole.canonical_state().inventories[&fuel].quantity,
        Quantity(original_fuel.0 - fuel_cost.0)
    );
    assert_eq!(
        whole.canonical_state().inventories[&water].quantity,
        Quantity(1_000)
    );
    assert!(whole.canonical_state().health.as_ref().unwrap().water[&water].load() > 0);
    let mut restored = Simulation::load(
        &whole.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    whole.advance(duration).unwrap();
    restored.advance(SimDuration(duration.0 / 2)).unwrap();
    restored = Simulation::load(
        &restored.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    restored
        .advance(SimDuration(duration.0 - duration.0 / 2))
        .unwrap();
    assert_eq!(whole.canonical_state(), restored.canonical_state());
    assert_eq!(
        whole.canonical_state().health.as_ref().unwrap().water[&water].load(),
        0
    );
    assert_eq!(
        whole.canonical_state().inventories[&water].quantity,
        Quantity(1_000)
    );
    assert_eq!(
        whole.canonical_state().inventories[&tool].quantity,
        original_tool
    );
    let command = Command::Health {
        action: HealthAction::Drink { source: water },
    };
    assert!(whole.observe().available_commands.contains(&command));
    for world in [&mut whole, &mut restored] {
        world
            .submit(Submission {
                caller_sequence: world.next_caller_sequence().unwrap(),
                command: command.clone(),
            })
            .unwrap();
    }
    let duration = whole
        .canonical_state()
        .health
        .as_ref()
        .unwrap()
        .rules
        .drinking_time;
    whole.advance(duration).unwrap();
    restored.advance(duration).unwrap();
    assert_eq!(whole.canonical_state(), restored.canonical_state());
    assert_eq!(
        whole.canonical_state().inventories[&water].quantity,
        Quantity(0)
    );
    assert_eq!(
        whole.canonical_state().inventories[&fuel].quantity,
        Quantity(original_fuel.0 - fuel_cost.0)
    );
    assert!(
        !whole
            .canonical_state()
            .components
            .clinical(actor)
            .expect("clinical state exists")
            .illnesses
            .contains_key(&IllnessKind::Enteric)
    );
    assert!(whole.canonical_state().events.iter().any(|event|matches!(event.kind,EventKind::Drank {source,actor:drinker,..} if source==water && drinker==actor)));
}
