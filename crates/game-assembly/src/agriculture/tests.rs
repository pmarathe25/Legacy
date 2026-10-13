use super::*;
use game_environment::{Calendar, Climate};

fn crop_rules(sim: &mut Simulation) -> &mut CropRules {
    let world = sim.host.state.agriculture.as_mut().unwrap();
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
    let ruler = sim.host.state.ruler;
    for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
        sim.interrupt_actor_activity(actor);
    }
    let epoch = sim.time();
    let seed = sim.host.seed();
    let world = sim.host.state.environment.as_mut().unwrap();
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
        .reset_epoch(&mut sim.host.storage, epoch, seed)
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
    let world = sim.host.state.agriculture.as_mut().unwrap();
    let first_id = world.fields(&sim.host.storage).next().unwrap().0;
    let field = world.field_mut(&mut sim.host.storage, first_id).unwrap();
    field.access = BTreeSet::from([ruler]);
    let id = field.id;
    let area = field.area;
    sim.start_movement(ruler, area).unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    (sim, id)
}
fn perform(sim: &mut Simulation, field: SiteId, operation: FarmOperation) {
    sim.submit(Submission {
        caller_sequence: sim.host.state.next_command + 100,
        command: Command::Cultivate { field, operation },
    })
    .unwrap();
    let completes = game_agriculture::cultivation(&sim.host.storage, sim.host.state.ruler)
        .unwrap()
        .completes;
    sim.advance(SimDuration(completes.0 - sim.host.clock().time.0))
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
    let cold_area = sim.host.state.areas[0].id;
    let mut field = sim
        .host
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.host.storage, warm)
        .unwrap()
        .clone();
    let cold = SiteId(sim.host.allocate_id().unwrap().0);
    field.id = cold;
    field.area = cold_area;
    for stock in [
        &mut field.seed,
        &mut field.tool,
        &mut field.water,
        &mut field.food,
    ] {
        let mut inventory = game_cargo::inventory(&sim.host.storage, *stock)
            .expect("inventory exists")
            .clone();
        inventory.id = InventoryId(sim.host.allocate_id().unwrap().0);
        inventory.location = InventoryLocation::Area(cold_area);
        *stock = inventory.id;
        sim.inventory_index
            .entry(inventory.location.clone())
            .or_default()
            .push(inventory.id);
        game_cargo::insert_inventory(&mut sim.host.storage, inventory).unwrap();
    }
    let world = sim.host.state.agriculture.as_mut().unwrap();
    let copied_crop = world.crop(&sim.host.storage, warm).unwrap().clone();
    let copied_soil = game_agriculture::Soil {
        water: 0,
        ..world.soil(&sim.host.storage, warm).unwrap().clone()
    };
    world.insert_field(&mut sim.host.storage, field).unwrap();
    world
        .insert_crop(&mut sim.host.storage, cold, copied_crop)
        .unwrap();
    world
        .insert_soil(&mut sim.host.storage, cold, copied_soil)
        .unwrap();
    sim.agriculture_queries.refresh(&sim.host.storage).unwrap();
    let epoch = sim.time();
    let seed = sim.host.seed();
    let environment = sim.host.state.environment.as_mut().unwrap();
    *sim.host
        .storage
        .get_mut::<game_environment::LocalClimate>(game_ecs::StableId(cold_area.0))
        .unwrap() = game_environment::LocalClimate {
        temperature_offset: -20,
        rain_multiplier_milli: 0,
        evaporation_multiplier_milli: 1_000,
    };
    environment
        .reset_epoch(&mut sim.host.storage, epoch, seed)
        .unwrap();
    sim.validate_environment().unwrap();
    sim.validate_agriculture().unwrap();
    (sim, warm, cold)
}

#[test]
fn connected_crops_match_local_weather_and_soil_without_changing_field_metadata() {
    let (mut sim, warm, cold) = local_crop_fixture(1);
    let world = sim.host.state.agriculture.as_ref().unwrap();
    let fields: BTreeMap<_, _> = world
        .fields(&sim.host.storage)
        .map(|(id, field)| (id, field.clone()))
        .collect();
    let warm_area = world.field(&sim.host.storage, warm).unwrap().area;
    let cold_area = world.field(&sim.host.storage, cold).unwrap().area;
    let stocks: BTreeMap<_, _> = fields
        .values()
        .flat_map(|field| {
            [field.seed, field.tool, field.water, field.food]
                .into_iter()
                .map(|id| {
                    (
                        id,
                        game_cargo::inventory(&sim.host.storage, id)
                            .expect("inventory exists")
                            .clone(),
                    )
                })
        })
        .collect();
    let environment = sim.host.state.environment.as_ref().unwrap();
    assert_eq!(
        environment
            .local_weather(&sim.host.storage, warm_area)
            .unwrap()
            .temperature,
        10
    );
    assert_eq!(
        environment
            .local_weather(&sim.host.storage, warm_area)
            .unwrap()
            .rain,
        10
    );
    assert_eq!(
        environment
            .local_weather(&sim.host.storage, cold_area)
            .unwrap()
            .temperature,
        -10
    );
    assert_eq!(
        environment
            .local_weather(&sim.host.storage, cold_area)
            .unwrap()
            .rain,
        0
    );
    sim.advance(SimDuration(1_200_000 - sim.host.clock().time.0))
        .unwrap();
    let world = sim.host.state.agriculture.as_ref().unwrap();
    assert_eq!(
        world.crop(&sim.host.storage, warm).unwrap().phase,
        CropPhase::Ripe
    );
    assert_eq!(
        world.crop(&sim.host.storage, cold).unwrap().phase,
        CropPhase::Failed
    );
    assert!(world.soil(&sim.host.storage, warm).unwrap().water > 0);
    assert_eq!(world.soil(&sim.host.storage, cold).unwrap().water, 0);
    assert_eq!(
        world
            .fields(&sim.host.storage)
            .map(|(id, field)| (id, field.clone()))
            .collect::<BTreeMap<_, _>>(),
        fields
    );
    for (id, stock) in stocks {
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, id).expect("inventory exists"),
            &stock
        );
    }
    assert_eq!(
        world.crop(&sim.host.storage, cold).unwrap().contributions,
        world.crop(&sim.host.storage, warm).unwrap().contributions
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
        .advance(SimDuration(1_200_000 - direct.host.clock().time.0))
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
    while chunked.host.clock().time.0 < 1_200_000 {
        let before = chunked.canonical_snapshot().unwrap();
        for _ in 0..3 {
            chunked.observe();
        }
        assert_eq!(chunked.canonical_snapshot().unwrap(), before);
        chunked
            .advance(SimDuration(
                (1_200_000 - chunked.host.clock().time.0).min(53_333),
            ))
            .unwrap();
    }
    assert_eq!(direct.canonical_state(), chunked.canonical_state());
}

#[test]
fn deliberate_fallow_loses_planted_seed_and_warm_wet_soil_recovers_without_harvest() {
    let (mut sim, field) = compact(1);
    crop_rules(&mut sim).fallow_labor = SimDuration(5_000);
    crop_rules(&mut sim).soil_recovery = 5;
    plant(&mut sim, field);
    let land = sim
        .host
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.host.storage, field)
        .unwrap()
        .clone();
    let consumed = game_cargo::inventory(&sim.host.storage, land.seed)
        .expect("inventory exists")
        .quantity;
    let food = game_cargo::inventory(&sim.host.storage, land.food)
        .expect("inventory exists")
        .quantity;
    sim.host
        .state
        .agriculture
        .as_mut()
        .unwrap()
        .soil_mut(&mut sim.host.storage, field)
        .unwrap()
        .fertility = 500;
    perform(&mut sim, field, FarmOperation::Fallow);
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, land.seed)
            .expect("inventory exists")
            .quantity,
        consumed
    );
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, land.food)
            .expect("inventory exists")
            .quantity,
        food
    );
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .phase,
        CropPhase::Fallow
    );
    sim.advance(SimDuration(600_000)).unwrap();
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .soil(&sim.host.storage, field)
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
        .host
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.host.storage, field)
        .unwrap()
        .clone();
    let opening_food = game_cargo::inventory(&sim.host.storage, land.food)
        .expect("inventory exists")
        .quantity
        .0;
    let opening_seed = game_cargo::inventory(&sim.host.storage, land.seed)
        .expect("inventory exists")
        .quantity
        .0;
    plant(&mut sim, field);
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, land.seed)
            .expect("inventory exists")
            .quantity
            .0,
        opening_seed - 1_000
    );
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, land.food)
            .expect("inventory exists")
            .quantity
            .0,
        opening_food
    );
    for _ in 0..4 {
        perform(&mut sim, field, FarmOperation::Tend);
    }
    sim.advance(SimDuration(1_200_000 - sim.host.clock().time.0))
        .unwrap();
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .phase,
        CropPhase::Ripe
    );
    perform(&mut sim, field, FarmOperation::Harvest);
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, land.food)
            .expect("inventory exists")
            .quantity
            .0,
        opening_food + 27_000
    );
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, land.seed)
            .expect("inventory exists")
            .quantity
            .0,
        opening_seed - 1_000 + 1_800
    );
    let world = sim.host.state.agriculture.as_ref().unwrap();
    assert_eq!(
        world.crop(&sim.host.storage, field).unwrap().phase,
        CropPhase::Fallow
    );
    assert_eq!(world.soil(&sim.host.storage, field).unwrap().fertility, 880);
    assert_eq!(
        world.crop(&sim.host.storage, field).unwrap().contributions[&sim.host.state.ruler],
        70_000
    );
    assert!(sim.host.events().iter().any(|event| matches!(event.event, EventKind::CropHarvested { field: id, food: Quantity(27_000), seed: Quantity(1_800), .. } if id == field)));
}

#[test]
fn interrupted_planting_preserves_labor_and_consumed_seed() {
    let (mut sim, field) = compact(1);
    perform(&mut sim, field, FarmOperation::Prepare);
    let seed = sim
        .host
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.host.storage, field)
        .unwrap()
        .seed;
    let opening = game_cargo::inventory(&sim.host.storage, seed)
        .expect("inventory exists")
        .quantity;
    let ruler = sim.host.state.ruler;
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
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .planting_ms,
        4_999
    );
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, seed)
            .expect("inventory exists")
            .quantity
            .0,
        opening.0 - 1_000
    );
    perform(&mut sim, field, FarmOperation::Plant);
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, seed)
            .expect("inventory exists")
            .quantity
            .0,
        opening.0 - 1_000
    );
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .phase,
        CropPhase::Growing
    );
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .contributions[&ruler],
        30_000
    );
}

#[test]
fn drought_can_kill_a_crop_and_irrigation_uses_actual_water() {
    let (mut sim, field) = compact(1);
    plant(&mut sim, field);
    let epoch = sim.time();
    let seed = sim.host.seed();
    let world = sim.host.state.environment.as_mut().unwrap();
    world.climate.seasonal_rain = [0; 4];
    world.climate.seasonal_evaporation = [0; 4];
    world
        .reset_epoch(&mut sim.host.storage, epoch, seed)
        .unwrap();
    crop_rules(&mut sim).failure_stress_days = 2;
    let world = sim.host.state.agriculture.as_mut().unwrap();
    world.soil_mut(&mut sim.host.storage, field).unwrap().water = 0;
    let water = world.field(&sim.host.storage, field).unwrap().water;
    let opening = game_cargo::inventory(&sim.host.storage, water)
        .expect("inventory exists")
        .quantity
        .0;
    perform(&mut sim, field, FarmOperation::Irrigate);
    assert_eq!(
        game_cargo::inventory(&sim.host.storage, water)
            .expect("inventory exists")
            .quantity
            .0,
        opening - 1_000
    );
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .soil(&sim.host.storage, field)
            .unwrap()
            .water,
        30
    );
    sim.host
        .state
        .agriculture
        .as_mut()
        .unwrap()
        .soil_mut(&mut sim.host.storage, field)
        .unwrap()
        .water = 0;
    sim.advance(SimDuration(1_200_000 - sim.host.clock().time.0))
        .unwrap();
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .phase,
        CropPhase::Failed
    );
    let before = sim.host.clock().time;
    perform(&mut sim, field, FarmOperation::Prepare);
    assert_eq!(sim.host.clock().time.0 - before.0, 20_000);
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .phase,
        CropPhase::Prepared
    );
    assert_eq!(
        sim.host
            .events()
            .iter()
            .filter(|e| matches!(e.event, EventKind::CropFailed { field: id } if id == field))
            .count(),
        1
    );
}

#[test]
fn rejected_registered_irrigation_water_preserves_the_command_state() {
    let (mut sim, field) = compact(1);
    plant(&mut sim, field);
    sim.initialize_health(game_health::Rules::fixture())
        .unwrap();
    let ruler = sim.host.state.ruler;
    let water = sim
        .host
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.host.storage, field)
        .unwrap()
        .water;
    let opening = game_cargo::inventory(&sim.host.storage, water)
        .unwrap()
        .quantity;
    game_health::insert_water_lot(
        &mut sim.host.storage,
        water,
        game_health::WaterLot::new(opening.0 - 1, 100),
    )
    .unwrap();
    sim.host
        .state
        .agriculture
        .as_mut()
        .unwrap()
        .soil_mut(&mut sim.host.storage, field)
        .unwrap()
        .water = 0;

    let before = sim.canonical_snapshot().unwrap();
    let next_id = sim.host.next_id();
    let next_event = sim.host.next_event();
    assert_eq!(
        sim.debug_actor_command(
            ruler,
            Command::Cultivate {
                field,
                operation: FarmOperation::Irrigate,
            },
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
    assert_eq!(sim.host.next_id(), next_id);
    assert_eq!(sim.host.next_event(), next_event);
}

#[test]
fn inaccessible_or_remote_land_and_out_of_season_planting_are_inert() {
    let (mut sim, field) = compact(1);
    let actor = sim.person_at(3).id;
    let before = sim.canonical_snapshot().unwrap();
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
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
    perform(&mut sim, field, FarmOperation::Prepare);
    crop_rules(&mut sim).planting_first_day = 1;
    let before = sim.canonical_snapshot().unwrap();
    assert_eq!(
        sim.debug_actor_command(
            sim.host.state.ruler,
            Command::Cultivate {
                field,
                operation: FarmOperation::Plant
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
}

#[test]
fn farming_is_equivalent_across_partial_advances_workers_and_restore() {
    let (mut a, field) = compact(1);
    let (mut b, other_field) = compact(4);
    assert_eq!(field, other_field);
    plant(&mut a, field);
    plant(&mut b, field);
    a.advance(SimDuration(1_200_000 - a.host.clock().time.0))
        .unwrap();
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
    while b.host.clock().time.0 < 1_200_000 {
        b.advance(SimDuration((1_200_000 - b.host.clock().time.0).min(17_333)))
            .unwrap();
    }
    assert_eq!(a.canonical_state(), b.canonical_state());
    perform(&mut a, field, FarmOperation::Harvest);
    perform(&mut b, field, FarmOperation::Harvest);
    assert_eq!(a.canonical_state(), b.canonical_state());
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
    let Location::AtArea(ruler_area) = *game_spatial::location(
        &configured.host.storage,
        game_ecs::StableId((configured.host.state.ruler).0),
    )
    .expect("location exists") else {
        panic!("fixture ruler starts in a local area");
    };
    assert_eq!(
        configured.observe().environment.unwrap().temperature_c,
        configured
            .host
            .state
            .environment
            .as_ref()
            .unwrap()
            .local_weather(&configured.host.storage, ruler_area)
            .unwrap()
            .temperature
    );
    assert_eq!(
        configured
            .host
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
    sim.host.state.environment.as_mut().unwrap().calendar.day = SimDuration(0);
    assert!(sim.validate_environment().is_err());
}
#[test]
fn pending_labor_survives_save_and_fatigue_interrupts_without_refunding_inputs() {
    let (mut sim, field) = compact(1);
    perform(&mut sim, field, FarmOperation::Prepare);
    let ruler = sim.host.state.ruler;
    game_physiology::body_mut(&mut sim.host.storage, ruler)
        .unwrap()
        .vitals
        .fatigue = 0;
    sim.host
        .state
        .physical
        .as_mut()
        .unwrap()
        .rules
        .fatigue_limit = 7_000;
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
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .planting_ms,
        7_000
    );
    assert_eq!(*sim.host.storage.activity(ruler).unwrap(), Activity::Idle);
    assert!(
        game_agriculture::cultivations(&sim.host.storage)
            .next()
            .is_none()
    );
    sim.debug_actor_command(ruler, Command::Rest).unwrap();
    sim.advance(SimDuration(3_500)).unwrap();
    perform(&mut sim, field, FarmOperation::Plant);
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .planting_ms,
        10_000
    );
}

#[test]
fn harvest_waits_for_real_output_capacity_and_local_queries_are_passive() {
    let (mut sim, field) = compact(1);
    plant(&mut sim, field);
    sim.advance(SimDuration(1_200_000 - sim.host.clock().time.0))
        .unwrap();
    let food = sim
        .host
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .field(&sim.host.storage, field)
        .unwrap()
        .food;
    game_cargo::inventory_mut(&mut sim.host.storage, food)
        .unwrap()
        .capacity = game_cargo::inventory(&sim.host.storage, food)
        .expect("inventory exists")
        .quantity;
    let before = sim.canonical_snapshot().unwrap();
    for _ in 0..40 {
        let view = sim.observe();
        assert_eq!(view.agriculture.as_ref().unwrap().fields[0].phase, "Ripe");
    }
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
    assert_eq!(
        sim.debug_actor_command(
            sim.host.state.ruler,
            Command::Cultivate {
                field,
                operation: FarmOperation::Harvest
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
    game_cargo::inventory_mut(&mut sim.host.storage, food)
        .unwrap()
        .capacity
        .0 += 30_000;
    perform(&mut sim, field, FarmOperation::Harvest);
    assert_eq!(
        sim.host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .crop(&sim.host.storage, field)
            .unwrap()
            .phase,
        CropPhase::Fallow
    );
    let village = sim.host.state.areas[0].id;
    sim.start_movement(sim.host.state.ruler, village).unwrap();
    assert!(sim.observe().agriculture.is_none());
    sim.advance(SimDuration(120_000)).unwrap();
    assert!(sim.observe().agriculture.unwrap().fields.is_empty());
}
