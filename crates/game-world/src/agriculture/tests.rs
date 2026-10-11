use super::*;
use game_environment::{Calendar, Climate};

fn crop_rules(sim: &mut Simulation) -> &mut CropRules {
    let world = sim.state.agriculture.as_mut().unwrap();
    world.content = None;
    world
        .catalog
        .crops
        .get_mut(&world.catalog.default_crop)
        .unwrap()
}

fn compact(workers: usize) -> (Simulation, SiteId) {
    let mut sim = Simulation::farming_fixture(
        91,
        Execution {
            workers,
            reverse_jobs: workers > 1,
        },
    )
    .unwrap();
    let ruler = sim.state.ruler;
    for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
        sim.interrupt_actor_activity(actor);
    }
    let world = sim.state.environment.as_mut().unwrap();
    world.calendar = Calendar {
        day: SimDuration(600_000),
        season_days: [10; 4],
        starting_day: 0,
    };
    world.climate = Climate {
        seasonal_temperature: [10; 4],
        seasonal_rain: [10; 4],
        seasonal_evaporation: [2; 4],
        temperature_variation: 0,
        rainfall_variation: 0,
    };
    world
        .reset_epoch(&mut sim.state.components, sim.state.time, sim.state.seed)
        .unwrap();
    let rules = crop_rules(&mut sim);
    rules.planting_last_day = 9;
    rules.food_yield = Quantity(30_000);
    rules.prepare_labor = SimDuration(20_000);
    rules.plant_labor = SimDuration(10_000);
    rules.tend_labor = SimDuration(5_000);
    rules.irrigation_labor = SimDuration(5_000);
    rules.harvest_labor = SimDuration(20_000);
    rules.base_temperature = 0;
    rules.ripe_degree_days = 20;
    let world = sim.state.agriculture.as_mut().unwrap();
    let first_id = world.fields(&sim.state.components).next().unwrap().0;
    let field = world
        .field_mut(&mut sim.state.components, first_id)
        .unwrap();
    field.access = BTreeSet::from([ruler]);
    let id = field.id;
    let area = field.area;
    sim.start_movement(ruler, area).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    (sim, id)
}
fn perform(sim: &mut Simulation, field: SiteId, operation: FarmOperation) {
    sim.submit(Submission {
        caller_sequence: sim.state.next_command + 100,
        command: Command::Cultivate { field, operation },
    })
    .unwrap();
    let completes = sim.state.agriculture.as_ref().unwrap().jobs[&sim.state.ruler].completes;
    sim.advance(SimDuration(completes.0 - sim.state.time.0))
        .unwrap();
}
fn plant(sim: &mut Simulation, field: SiteId) {
    perform(sim, field, FarmOperation::Prepare);
    perform(sim, field, FarmOperation::Plant);
}

fn local_crop_fixture(workers: usize) -> (Simulation, SiteId, SiteId) {
    let (mut sim, warm) = compact(workers);
    crop_rules(&mut sim).failure_stress_days = 2;
    plant(&mut sim, warm);
    let cold_area = sim.state.areas[0].id;
    let mut field = sim
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.state.components, warm)
        .unwrap()
        .clone();
    let cold = SiteId(allocate(&mut sim.state).unwrap());
    field.id = cold;
    field.area = cold_area;
    for stock in [
        &mut field.seed,
        &mut field.tool,
        &mut field.water,
        &mut field.food,
    ] {
        let mut inventory = sim.state.inventories[&*stock].clone();
        inventory.id = InventoryId(allocate(&mut sim.state).unwrap());
        inventory.location = InventoryLocation::Area(cold_area);
        *stock = inventory.id;
        sim.inventory_index
            .entry(inventory.location.clone())
            .or_default()
            .push(inventory.id);
        sim.state.inventories.insert(inventory.id, inventory);
    }
    let world = sim.state.agriculture.as_mut().unwrap();
    let copied_crop = world.crop(&sim.state.components, warm).unwrap().clone();
    let copied_soil = game_agriculture::Soil {
        water: 0,
        ..world.soil(&sim.state.components, warm).unwrap().clone()
    };
    world
        .insert_field(&mut sim.state.components, field)
        .unwrap();
    world
        .insert_crop(&mut sim.state.components, cold, copied_crop)
        .unwrap();
    world
        .insert_soil(&mut sim.state.components, cold, copied_soil)
        .unwrap();
    sim.agriculture_queries
        .refresh(&sim.state.components)
        .unwrap();
    let environment = sim.state.environment.as_mut().unwrap();
    *sim.state
        .components
        .get_mut::<game_environment::LocalClimate>(game_ecs::StableId(cold_area.0))
        .unwrap() = game_environment::LocalClimate {
        temperature_offset: -20,
        rain_multiplier_milli: 0,
        evaporation_multiplier_milli: 1_000,
    };
    environment
        .reset_epoch(&mut sim.state.components, sim.state.time, sim.state.seed)
        .unwrap();
    sim.validate_environment().unwrap();
    sim.validate_agriculture().unwrap();
    (sim, warm, cold)
}

#[test]
fn connected_crops_match_local_weather_and_soil_without_changing_field_metadata() {
    let (mut sim, warm, cold) = local_crop_fixture(1);
    let world = sim.state.agriculture.as_ref().unwrap();
    let fields: BTreeMap<_, _> = world
        .fields(&sim.state.components)
        .map(|(id, field)| (id, field.clone()))
        .collect();
    let warm_area = world.field(&sim.state.components, warm).unwrap().area;
    let cold_area = world.field(&sim.state.components, cold).unwrap().area;
    let stocks: BTreeMap<_, _> = fields
        .values()
        .flat_map(|field| {
            [field.seed, field.tool, field.water, field.food]
                .into_iter()
                .map(|id| (id, sim.state.inventories[&id].clone()))
        })
        .collect();
    let environment = sim.state.environment.as_ref().unwrap();
    assert_eq!(
        environment
            .local_weather(&sim.state.components, warm_area)
            .unwrap()
            .temperature,
        10
    );
    assert_eq!(
        environment
            .local_weather(&sim.state.components, warm_area)
            .unwrap()
            .rain,
        10
    );
    assert_eq!(
        environment
            .local_weather(&sim.state.components, cold_area)
            .unwrap()
            .temperature,
        -10
    );
    assert_eq!(
        environment
            .local_weather(&sim.state.components, cold_area)
            .unwrap()
            .rain,
        0
    );
    sim.advance(SimDuration(1_200_000 - sim.state.time.0))
        .unwrap();
    let world = sim.state.agriculture.as_ref().unwrap();
    assert_eq!(
        world.crop(&sim.state.components, warm).unwrap().phase,
        CropPhase::Ripe
    );
    assert_eq!(
        world.crop(&sim.state.components, cold).unwrap().phase,
        CropPhase::Failed
    );
    assert!(world.soil(&sim.state.components, warm).unwrap().water > 0);
    assert_eq!(world.soil(&sim.state.components, cold).unwrap().water, 0);
    assert_eq!(
        world
            .fields(&sim.state.components)
            .map(|(id, field)| (id, field.clone()))
            .collect::<BTreeMap<_, _>>(),
        fields
    );
    for (id, stock) in stocks {
        assert_eq!(sim.state.inventories[&id], stock);
    }
    assert_eq!(
        world
            .crop(&sim.state.components, cold)
            .unwrap()
            .contributions,
        world
            .crop(&sim.state.components, warm)
            .unwrap()
            .contributions
    );
    sim.validate_environment().unwrap();
    sim.validate_agriculture().unwrap();
}

#[test]
fn local_crop_components_survive_restore_and_worker_and_chunk_order_changes() {
    let (mut direct, warm, cold) = local_crop_fixture(1);
    let (mut chunked, other_warm, other_cold) = local_crop_fixture(4);
    assert_eq!((warm, cold), (other_warm, other_cold));
    direct
        .advance(SimDuration(1_200_000 - direct.state.time.0))
        .unwrap();
    chunked.advance(SimDuration(333_331)).unwrap();
    let bytes = chunked.save().unwrap();
    chunked = Simulation::load(
        &bytes,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    while chunked.state.time.0 < 1_200_000 {
        let before = chunked.state.clone();
        for _ in 0..3 {
            chunked.observe();
        }
        assert_eq!(chunked.state, before);
        chunked
            .advance(SimDuration((1_200_000 - chunked.state.time.0).min(53_333)))
            .unwrap();
    }
    assert_eq!(direct.state, chunked.state);
}

#[test]
fn deliberate_fallow_loses_planted_seed_and_warm_wet_soil_recovers_without_harvest() {
    let (mut sim, field) = compact(1);
    crop_rules(&mut sim).fallow_labor = SimDuration(5_000);
    crop_rules(&mut sim).soil_recovery = 5;
    plant(&mut sim, field);
    let land = sim
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.state.components, field)
        .unwrap()
        .clone();
    let consumed = sim.state.inventories[&land.seed].quantity;
    let food = sim.state.inventories[&land.food].quantity;
    sim.state
        .agriculture
        .as_mut()
        .unwrap()
        .soil_mut(&mut sim.state.components, field)
        .unwrap()
        .fertility = 500;
    perform(&mut sim, field, FarmOperation::Fallow);
    assert_eq!(sim.state.inventories[&land.seed].quantity, consumed);
    assert_eq!(sim.state.inventories[&land.food].quantity, food);
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .phase,
        CropPhase::Fallow
    );
    sim.advance(SimDuration(600_000)).unwrap();
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .soil(&sim.state.components, field)
            .unwrap()
            .fertility,
        505
    );
    sim.validate_environment().unwrap();
}
#[test]
fn full_cycle_consumes_seed_and_credits_only_real_labor_and_harvest() {
    let (mut sim, field) = compact(1);
    let land = sim
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.state.components, field)
        .unwrap()
        .clone();
    let opening_food = sim.state.inventories[&land.food].quantity.0;
    let opening_seed = sim.state.inventories[&land.seed].quantity.0;
    plant(&mut sim, field);
    assert_eq!(
        sim.state.inventories[&land.seed].quantity.0,
        opening_seed - 1_000
    );
    assert_eq!(sim.state.inventories[&land.food].quantity.0, opening_food);
    for _ in 0..4 {
        perform(&mut sim, field, FarmOperation::Tend);
    }
    sim.advance(SimDuration(1_200_000 - sim.state.time.0))
        .unwrap();
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .phase,
        CropPhase::Ripe
    );
    perform(&mut sim, field, FarmOperation::Harvest);
    assert_eq!(
        sim.state.inventories[&land.food].quantity.0,
        opening_food + 27_000
    );
    assert_eq!(
        sim.state.inventories[&land.seed].quantity.0,
        opening_seed - 1_000 + 1_800
    );
    let world = sim.state.agriculture.as_ref().unwrap();
    assert_eq!(
        world.crop(&sim.state.components, field).unwrap().phase,
        CropPhase::Fallow
    );
    assert_eq!(
        world.soil(&sim.state.components, field).unwrap().fertility,
        880
    );
    assert_eq!(
        world
            .crop(&sim.state.components, field)
            .unwrap()
            .contributions[&sim.state.ruler],
        70_000
    );
    assert!(sim.state.events.iter().any(|event| matches!(event.kind, EventKind::CropHarvested { field: id, food: Quantity(27_000), seed: Quantity(1_800), .. } if id == field)));
}

#[test]
fn interrupted_planting_preserves_labor_and_consumed_seed() {
    let (mut sim, field) = compact(1);
    perform(&mut sim, field, FarmOperation::Prepare);
    let seed = sim
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.state.components, field)
        .unwrap()
        .seed;
    let opening = sim.state.inventories[&seed].quantity;
    let ruler = sim.state.ruler;
    sim.debug_actor_command(
        ruler,
        Command::Cultivate {
            field,
            operation: FarmOperation::Plant,
        },
    )
    .unwrap();
    sim.advance(SimDuration(4_999)).unwrap();
    sim.debug_actor_command(ruler, Command::Cancel).unwrap();
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .planting_ms,
        4_999
    );
    assert_eq!(sim.state.inventories[&seed].quantity.0, opening.0 - 1_000);
    perform(&mut sim, field, FarmOperation::Plant);
    assert_eq!(sim.state.inventories[&seed].quantity.0, opening.0 - 1_000);
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .phase,
        CropPhase::Growing
    );
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .contributions[&ruler],
        30_000
    );
}

#[test]
fn drought_can_kill_a_crop_and_irrigation_uses_actual_water() {
    let (mut sim, field) = compact(1);
    plant(&mut sim, field);
    let world = sim.state.environment.as_mut().unwrap();
    world.climate.seasonal_rain = [0; 4];
    world.climate.seasonal_evaporation = [0; 4];
    world
        .reset_epoch(&mut sim.state.components, sim.state.time, sim.state.seed)
        .unwrap();
    crop_rules(&mut sim).failure_stress_days = 2;
    let world = sim.state.agriculture.as_mut().unwrap();
    world
        .soil_mut(&mut sim.state.components, field)
        .unwrap()
        .water = 0;
    let water = world.field(&sim.state.components, field).unwrap().water;
    let opening = sim.state.inventories[&water].quantity.0;
    perform(&mut sim, field, FarmOperation::Irrigate);
    assert_eq!(sim.state.inventories[&water].quantity.0, opening - 1_000);
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .soil(&sim.state.components, field)
            .unwrap()
            .water,
        30
    );
    sim.state
        .agriculture
        .as_mut()
        .unwrap()
        .soil_mut(&mut sim.state.components, field)
        .unwrap()
        .water = 0;
    sim.advance(SimDuration(1_200_000 - sim.state.time.0))
        .unwrap();
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .phase,
        CropPhase::Failed
    );
    let before = sim.state.time;
    perform(&mut sim, field, FarmOperation::Prepare);
    assert_eq!(sim.state.time.0 - before.0, 20_000);
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .phase,
        CropPhase::Prepared
    );
    assert_eq!(
        sim.state
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::CropFailed { field: id } if id == field))
            .count(),
        1
    );
}

#[test]
fn inaccessible_or_remote_land_and_out_of_season_planting_are_inert() {
    let (mut sim, field) = compact(1);
    let actor = sim.state.people[3].id;
    let before = sim.state.clone();
    assert_eq!(
        sim.debug_actor_command(
            actor,
            Command::Cultivate {
                field,
                operation: FarmOperation::Prepare
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.state, before);
    perform(&mut sim, field, FarmOperation::Prepare);
    crop_rules(&mut sim).planting_first_day = 1;
    let before = sim.state.clone();
    assert_eq!(
        sim.debug_actor_command(
            sim.state.ruler,
            Command::Cultivate {
                field,
                operation: FarmOperation::Plant
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.state, before);
}

#[test]
fn farming_is_equivalent_across_partial_advances_workers_and_restore() {
    let (mut a, field) = compact(1);
    let (mut b, other_field) = compact(4);
    assert_eq!(field, other_field);
    plant(&mut a, field);
    plant(&mut b, field);
    a.advance(SimDuration(1_200_000 - a.state.time.0)).unwrap();
    b.advance(SimDuration(191_333)).unwrap();
    for _ in 0..20 {
        b.observe();
    }
    let bytes = b.save().unwrap();
    b = Simulation::load(
        &bytes,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    while b.state.time.0 < 1_200_000 {
        b.advance(SimDuration((1_200_000 - b.state.time.0).min(17_333)))
            .unwrap();
    }
    assert_eq!(a.state, b.state);
    perform(&mut a, field, FarmOperation::Harvest);
    perform(&mut b, field, FarmOperation::Harvest);
    assert_eq!(a.state, b.state);
}

#[test]
fn calendar_and_weather_are_configurable_and_validation_rejects_corruption() {
    let calendar = Calendar {
        day: SimDuration(10),
        season_days: [2, 3, 4, 5],
        starting_day: 1,
    };
    assert_eq!(calendar.season(SimTime(0)), 0);
    assert_eq!(calendar.season(SimTime(10)), 1);
    assert_eq!(calendar.season(SimTime(40)), 2);
    assert_eq!(calendar.day_of_year(SimTime(140)), 1);
    let mut config = FarmingConfig::default();
    config.environment.calendar.starting_day = 100;
    config.environment.climate.seasonal_temperature = [30; 4];
    let configured =
        Simulation::farming_with_config(4, Execution::default(), config.clone()).unwrap();
    let Location::AtArea(ruler_area) = *configured
        .state
        .components
        .location(configured.state.ruler)
        .expect("location exists")
    else {
        panic!("fixture ruler starts in a local area");
    };
    assert_eq!(
        configured.observe().environment.unwrap().temperature_c,
        configured
            .state
            .environment
            .as_ref()
            .unwrap()
            .local_weather(&configured.state.components, ruler_area)
            .unwrap()
            .temperature
    );
    assert_eq!(
        configured
            .state
            .environment
            .as_ref()
            .unwrap()
            .calendar
            .day_of_year(SimTime(0)),
        100
    );
    config.environment.calendar.day = SimDuration(0);
    assert!(matches!(
        Simulation::farming_with_config(4, Execution::default(), config),
        Err(Error::InvalidConfiguration(_))
    ));
    let (mut sim, _) = compact(1);
    sim.validate_environment().unwrap();
    sim.state.environment.as_mut().unwrap().calendar.day = SimDuration(0);
    assert!(sim.validate_environment().is_err());
}
#[test]
fn pending_labor_survives_save_and_fatigue_interrupts_without_refunding_inputs() {
    let (mut sim, field) = compact(1);
    perform(&mut sim, field, FarmOperation::Prepare);
    let ruler = sim.state.ruler;
    sim.state.components.body_mut(ruler).unwrap().vitals.fatigue = 0;
    sim.state.physical.as_mut().unwrap().rules.fatigue_limit = 7_000;
    sim.debug_actor_command(
        ruler,
        Command::Cultivate {
            field,
            operation: FarmOperation::Plant,
        },
    )
    .unwrap();
    sim.advance(SimDuration(4_000)).unwrap();
    let bytes = sim.save().unwrap();
    let mut restored = Simulation::load(&bytes, Execution::default()).unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    restored.advance(SimDuration(6_000)).unwrap();
    assert_eq!(sim.state, restored.state);
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .planting_ms,
        7_000
    );
    assert_eq!(
        *sim.state.components.activity(ruler).unwrap(),
        Activity::Idle
    );
    assert!(sim.state.agriculture.as_ref().unwrap().jobs.is_empty());
    sim.debug_actor_command(ruler, Command::Rest).unwrap();
    sim.advance(SimDuration(3_500)).unwrap();
    perform(&mut sim, field, FarmOperation::Plant);
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .planting_ms,
        10_000
    );
}

#[test]
fn harvest_waits_for_real_output_capacity_and_local_queries_are_passive() {
    let (mut sim, field) = compact(1);
    plant(&mut sim, field);
    sim.advance(SimDuration(1_200_000 - sim.state.time.0))
        .unwrap();
    let food = sim
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.state.components, field)
        .unwrap()
        .food;
    sim.state.inventories.get_mut(&food).unwrap().capacity = sim.state.inventories[&food].quantity;
    let before = sim.state.clone();
    for _ in 0..40 {
        let view = sim.observe();
        assert_eq!(view.agriculture.as_ref().unwrap().fields[0].phase, "Ripe");
    }
    assert_eq!(sim.state, before);
    assert_eq!(
        sim.debug_actor_command(
            sim.state.ruler,
            Command::Cultivate {
                field,
                operation: FarmOperation::Harvest
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.state, before);
    sim.state.inventories.get_mut(&food).unwrap().capacity.0 += 30_000;
    perform(&mut sim, field, FarmOperation::Harvest);
    assert_eq!(
        sim.state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.state.components, field)
            .unwrap()
            .phase,
        CropPhase::Fallow
    );
    let village = sim.state.areas[0].id;
    sim.start_movement(sim.state.ruler, village).unwrap();
    assert!(sim.observe().agriculture.is_none());
    sim.advance(SimDuration(120_000)).unwrap();
    assert!(sim.observe().agriculture.unwrap().fields.is_empty());
}
