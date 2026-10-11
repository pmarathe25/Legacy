use game_api::*;
use game_world::*;
use std::collections::BTreeSet;

fn add(
    sim: &mut Simulation,
    good: Good,
    place: InventoryLocation,
    quantity: u64,
    capacity: u64,
) -> InventoryId {
    let actor = sim.canonical_state().ruler;
    sim.debug_add_inventory(
        good,
        place,
        Quantity(quantity),
        Quantity(capacity),
        BTreeSet::from([actor]),
    )
    .unwrap()
}
fn submit_choice(sim: &mut Simulation, action: QuantityAction, quantity: Quantity) {
    let view = sim.observe();
    let opportunity = view
        .quantity_opportunities
        .iter()
        .find(|opportunity| opportunity.action == action)
        .expect("native quantity opportunity");
    assert!(opportunity.suggested.contains(&quantity), "{opportunity:?}");
    let command = opportunity.action.command(quantity);
    assert!(view.available_commands.contains(&command));
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
    .unwrap();
}
fn health_fixture() -> Simulation {
    use game_world::decisions::{Aspiration, Personality};
    let mut sim = Simulation::ecology_fixture(67, Execution::default()).unwrap();
    let mut rules = HealthRules::fixture();
    rules.drink_quantity = Quantity(250);
    sim.initialize_health(rules).unwrap();
    for person in sim.canonical_state().people.clone() {
        sim.debug_actor_command(person.id, Command::Cancel).unwrap();
        sim.debug_set_personality(person.id, Personality::restful(), Aspiration::Comfort)
            .unwrap();
    }
    sim
}

#[test]
fn small_food_and_timber_loads_are_public_and_oversized_examples_are_absent() {
    for good in [Good::Food, Good::Timber] {
        let mut sim = Simulation::survival_fixture(1, Execution::default()).unwrap();
        let actor = sim.canonical_state().ruler;
        let source = add(&mut sim, good, InventoryLocation::Area(AreaId(1)), 250, 250);
        let destination = add(&mut sim, good, InventoryLocation::Carried(actor), 0, 250);
        submit_choice(
            &mut sim,
            QuantityAction::Load {
                source,
                destination,
            },
            Quantity(250),
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.canonical_state().inventories[&source].quantity,
            Quantity(0)
        );
        assert_eq!(
            sim.canonical_state().inventories[&destination].quantity,
            Quantity(250)
        );
    }
    let mut sim = Simulation::survival_fixture(1, Execution::default()).unwrap();
    let actor = sim.canonical_state().ruler;
    let source = add(
        &mut sim,
        Good::Timber,
        InventoryLocation::Area(AreaId(1)),
        1000,
        1000,
    );
    let destination = add(
        &mut sim,
        Good::Timber,
        InventoryLocation::Carried(actor),
        0,
        500,
    );
    let action = QuantityAction::Load {
        source,
        destination,
    };
    let view = sim.observe();
    let opportunity = view
        .quantity_opportunities
        .iter()
        .find(|opportunity| opportunity.action == action)
        .unwrap();
    assert_eq!(opportunity.upper_bound, Quantity(500));
    assert!(
        opportunity
            .suggested
            .iter()
            .all(|quantity| quantity.0 <= 500)
    );
    assert!(
        !view
            .available_commands
            .contains(&action.command(Quantity(1000)))
    );
    submit_choice(&mut sim, action, Quantity(500));
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(500)
    );
}

#[test]
fn public_small_pickups_work_without_an_existing_compartment() {
    let mut sim = Simulation::survival_fixture(1, Execution::default()).unwrap();
    let source = add(
        &mut sim,
        Good::Herb,
        InventoryLocation::Area(AreaId(1)),
        250,
        250,
    );
    let actor = sim.canonical_state().ruler;
    submit_choice(&mut sim, QuantityAction::Carry { source }, Quantity(250));
    sim.advance(MINUTE).unwrap();
    let carried = sim
        .canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Herb && stock.location == InventoryLocation::Carried(actor)
        })
        .unwrap();
    assert_eq!(carried.quantity, Quantity(250));
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
}

#[test]
fn public_split_tool_consolidation_enables_a_native_small_gather() {
    let mut sim = Simulation::ecology_fixture(67, Execution::default()).unwrap();
    let actor = sim.canonical_state().ruler;
    let destination = add(
        &mut sim,
        Good::Tool,
        InventoryLocation::Carried(actor),
        400,
        1000,
    );
    let source = add(
        &mut sim,
        Good::Tool,
        InventoryLocation::Carried(actor),
        600,
        1000,
    );
    let resource = sim
        .debug_add_resource(
            ResourceKind::Coppice,
            AreaId(1),
            Quantity(250),
            Quantity(250),
            Quantity(0),
            BTreeSet::from([actor]),
            None,
            0,
        )
        .unwrap();
    assert!(!sim.observe().quantity_opportunities.iter().any(|opportunity| matches!(opportunity.action, QuantityAction::Gather { resource: id, .. } if id == resource)));
    submit_choice(
        &mut sim,
        QuantityAction::Load {
            source,
            destination,
        },
        Quantity(600),
    );
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity,
        Quantity(1000)
    );
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
    submit_choice(
        &mut sim,
        QuantityAction::Gather {
            resource,
            unauthorized: false,
        },
        Quantity(250),
    );
    let job = sim.canonical_state().components.gathering(actor).unwrap();
    assert_eq!(job.quantity, Quantity(250));
    assert_eq!(job.tool, Some(destination));
}

#[test]
fn offered_water_portions_compose_with_actual_pickup_boiling_and_drinking_and_replay() {
    let mut sim = health_fixture();
    let actor = sim.canonical_state().ruler;
    let spring = sim
        .debug_add_resource(
            ResourceKind::Spring,
            AreaId(1),
            Quantity(500),
            Quantity(500),
            Quantity(0),
            BTreeSet::from([actor]),
            None,
            300,
        )
        .unwrap();
    submit_choice(
        &mut sim,
        QuantityAction::Gather {
            resource: spring,
            unauthorized: false,
        },
        Quantity(500),
    );
    let completes = sim
        .canonical_state()
        .components
        .gathering(actor)
        .unwrap()
        .completes;
    sim.advance(SimDuration(completes.0 - sim.canonical_state().time.0))
        .unwrap();
    let water = sim
        .canonical_state()
        .events
        .iter()
        .find_map(|event| match event.kind {
            EventKind::ResourceCollected {
                resource,
                destination,
                ..
            } if resource == spring => Some(destination),
            _ => None,
        })
        .unwrap();
    let bulk = add(
        &mut sim,
        Good::Water,
        InventoryLocation::Area(AreaId(1)),
        0,
        500,
    );
    submit_choice(
        &mut sim,
        QuantityAction::Load {
            source: water,
            destination: bulk,
        },
        Quantity(500),
    );
    sim.advance(MINUTE).unwrap();
    assert!(
        sim.canonical_state()
            .components
            .water_lot(bulk)
            .expect("water lot exists")
            .load()
            > 0
    );
    add(
        &mut sim,
        Good::Timber,
        InventoryLocation::Carried(actor),
        9750,
        9750,
    );
    let before = sim.canonical_state().clone();
    for _ in 0..20 {
        sim.observe();
    }
    assert_eq!(sim.canonical_state(), &before);
    let choice = QuantityAction::Carry { source: bulk };
    let opportunity = sim
        .observe()
        .quantity_opportunities
        .into_iter()
        .find(|opportunity| opportunity.action == choice)
        .unwrap();
    assert_eq!(opportunity.upper_bound, Quantity(250));
    submit_choice(&mut sim, choice, Quantity(250));
    sim.advance(SimDuration(30_000)).unwrap();
    let mut replay = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    sim.advance(SimDuration(30_000)).unwrap();
    replay.advance(SimDuration(30_000)).unwrap();
    assert_eq!(sim.canonical_state(), replay.canonical_state());
    assert_eq!(
        sim.canonical_state().inventories[&water].quantity,
        Quantity(250)
    );
    assert_eq!(
        sim.canonical_state().inventories[&bulk].quantity,
        Quantity(250)
    );
    let boil = sim.observe().available_commands.into_iter().find(|command| matches!(command, Command::Health { action: HealthAction::Boil { source, .. } } if *source == water)).unwrap();
    let Command::Health {
        action: HealthAction::Boil { fuel, .. },
    } = boil
    else {
        unreachable!()
    };
    let before_fuel = sim.canonical_state().inventories[&fuel].quantity.0;
    for run in [&mut sim, &mut replay] {
        run.submit(Submission {
            caller_sequence: run.next_caller_sequence().unwrap(),
            command: boil.clone(),
        })
        .unwrap();
        run.advance(SimDuration(30_000)).unwrap();
    }
    assert_eq!(sim.canonical_state(), replay.canonical_state());
    assert_eq!(
        sim.canonical_state()
            .components
            .water_lot(water)
            .expect("water lot exists")
            .load(),
        0
    );
    assert!(sim.canonical_state().inventories[&fuel].quantity.0 <= before_fuel - 1000);
    let drink = Command::Health {
        action: HealthAction::Drink { source: water },
    };
    assert!(sim.observe().available_commands.contains(&drink));
    for run in [&mut sim, &mut replay] {
        run.submit(Submission {
            caller_sequence: run.next_caller_sequence().unwrap(),
            command: drink.clone(),
        })
        .unwrap();
        run.advance(SimDuration(10_000)).unwrap();
    }
    assert_eq!(sim.canonical_state(), replay.canonical_state());
    assert_eq!(
        sim.canonical_state().inventories[&water].quantity,
        Quantity(0)
    );
    assert_eq!(sim.observe().health.as_ref().unwrap().thirst, 0);
}

#[test]
fn quantity_choices_do_not_read_remote_or_inaccessible_stock_or_create_records() {
    let mut sim = Simulation::survival_fixture(1, Execution::default()).unwrap();
    let actor = sim.canonical_state().ruler;
    let before = sim.observe().quantity_opportunities;
    sim.debug_add_inventory(
        Good::Timber,
        InventoryLocation::Area(AreaId(1)),
        Quantity(250),
        Quantity(250),
        BTreeSet::new(),
    )
    .unwrap();
    sim.debug_add_inventory(
        Good::Timber,
        InventoryLocation::Area(AreaId(3)),
        Quantity(250),
        Quantity(250),
        BTreeSet::from([actor]),
    )
    .unwrap();
    let record = add(
        &mut sim,
        Good::Record,
        InventoryLocation::Area(AreaId(1)),
        1000,
        1000,
    );
    let state = sim.canonical_state().clone();
    assert_eq!(sim.observe().quantity_opportunities, before);
    assert_eq!(sim.canonical_state(), &state);
    assert!(
        !sim.observe()
            .available_commands
            .iter()
            .any(|command| matches!(command, Command::Carry { source, .. } if *source == record))
    );
}
