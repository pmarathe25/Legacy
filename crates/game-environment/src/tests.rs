use super::*;

// Standalone host composition for owner tests; only Store owns cell data.
#[derive(Debug)]
struct World {
    runtime: Runtime,
    store: Store,
    queries: std::cell::RefCell<Queries>,
}
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<Foreign>("fixture/foreign", |_| Ok(()))
        .unwrap();
    schema
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Foreign(u64);

#[test]
fn local_weather_uses_shared_components_and_preserves_foreign_data() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    world
        .store
        .insert_or_spawn(StableId(2), Foreign(5))
        .unwrap();
    let prepared = world.prepare(SimTime(10)).unwrap();
    world
        .store
        .insert_or_spawn(StableId(99), Foreign(7))
        .unwrap();
    world.commit(prepared).unwrap();
    assert_eq!(world.store.get::<Foreign>(StableId(2)), Some(&Foreign(5)));
    assert_eq!(world.store.get::<Foreign>(StableId(99)), Some(&Foreign(7)));
    let metadata = serde_json::to_value(&world.runtime).unwrap();
    assert!(!metadata.as_object().unwrap().contains_key("cells"));
    let saved = serde_json::to_vec(&world).unwrap();
    let mut restored: World = serde_json::from_slice(&saved).unwrap();
    restored
        .commit(restored.prepare(SimTime(20)).unwrap())
        .unwrap();
    world.commit(world.prepare(SimTime(20)).unwrap()).unwrap();
    assert_eq!(world, restored);
}

#[test]
fn local_cell_registration_preflights_both_component_validators() {
    let runtime = Runtime::new(config(), SimTime(0), 19).unwrap();
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<LocalClimate>("environment/local-climate", LocalClimate::validate)
        .unwrap();
    schema
        .register::<Weather>("environment/weather", |_| Err(Error::Unavailable))
        .unwrap();
    let mut store = Store::with_schema(schema);
    let before = store.snapshot().unwrap();
    assert_eq!(
        runtime.register_cell(&mut store, AreaId(2), upland()),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn registers_missing_area_cells_and_preserves_existing_cells() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    let existing = world.local_weather(AreaId(2)).unwrap().clone();

    world
        .runtime
        .register_missing_cells(&mut world.store, [AreaId(1), AreaId(2)])
        .unwrap();

    assert!(world.local_weather(AreaId(1)).is_some());
    assert_eq!(world.local_weather(AreaId(2)), Some(&existing));
    world
        .runtime
        .register_missing_cells(&mut world.store, [AreaId(1), AreaId(2)])
        .unwrap();
}

#[test]
fn host_validation_rejects_unknown_cells_and_unpaired_component_columns() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world
        .register_cell(AreaId(2), LocalClimate::default())
        .unwrap();
    assert!(matches!(
        world.runtime.validate_host(
            &world.store,
            SimTime(0),
            19,
            [AreaId(1)]
        ),
        Err(Error::InvalidSave(message)) if message == "unknown weather cell area"
    ));

    let mut unpaired = Store::with_schema(schema());
    unpaired
        .insert_or_spawn(StableId(2), LocalClimate::default())
        .unwrap();
    assert!(matches!(
        world.runtime.validate_host(&unpaired, SimTime(0), 19, [AreaId(2)]),
        Err(Error::InvalidSave(message)) if message == "unpaired local weather components"
    ));
}

#[test]
fn absent_runtime_rejects_orphan_environment_columns() {
    let mut store = Store::with_schema(schema());
    store
        .insert_or_spawn(StableId(2), LocalClimate::default())
        .unwrap();
    assert!(matches!(
        validate_host_presence(&store, None, SimTime(0), 19, [AreaId(2)]),
        Err(Error::InvalidSave(message)) if message == "weather components have no environment runtime"
    ));
}

#[test]
fn actor_observation_requires_life_stationary_location_and_registered_cell() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world
        .register_cell(AreaId(2), LocalClimate::default())
        .unwrap();
    let at_area = game_core::Location::AtArea(AreaId(2));
    let observation = world
        .runtime
        .observe_for_actor(&world.store, true, &at_area)
        .unwrap();
    assert_eq!(observation, world.observe_cell(AreaId(2)).unwrap());
    assert!(
        world
            .runtime
            .observe_for_actor(&world.store, false, &at_area)
            .is_none()
    );
    let in_transit = game_core::Location::InTransit {
        from: AreaId(2),
        to: AreaId(3),
        departed: SimTime(0),
        arrives: SimTime(10),
        road_start: None,
    };
    assert!(
        world
            .runtime
            .observe_for_actor(&world.store, true, &in_transit)
            .is_none()
    );
    assert!(
        world
            .runtime
            .observe_cell(&world.store, AreaId(3))
            .is_none()
    );
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct OwnerState {
    environment: Option<Runtime>,
}

fn owner_host() -> (
    game_world::GenericHost<OwnerState, ()>,
    game_world::Registry<OwnerState, ()>,
) {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let mut host = game_world::GenericHost::new(schema, 19, OwnerState::default());
    // Stable area IDs are allocated by the host before the owner attaches rows.
    host.allocate_ids(2).unwrap();
    // Weather components attach to the area's already-existing entity.
    host.storage.spawn(StableId(2)).unwrap();
    let mut registry = game_world::Registry::new();
    register_system(
        &mut registry,
        game_world::RuntimeSlot::new(
            |state: &OwnerState| &state.environment,
            |state: &mut OwnerState| &mut state.environment,
        ),
    )
    .unwrap();
    (host, registry)
}

fn initialize_owner(
    host: &mut game_world::GenericHost<OwnerState, ()>,
    registry: &game_world::Registry<OwnerState, ()>,
    config: Config,
) {
    let prepared = registry
        .prepare(host, SimTime(0), None, |tx| {
            tx.call::<EnvironmentSystem>(SystemInput::Initialize {
                config,
                areas: vec![AreaId(2)],
            })
        })
        .unwrap();
    assert_eq!(
        registry.commit(host, prepared).unwrap(),
        SystemOutput::Initialized
    );
}

#[test]
fn owner_weather_advance_commits_rows_and_runtime_together() {
    let (mut host, registry) = owner_host();
    let mut config = config();
    config.calendar.season_days = [1; 4];
    initialize_owner(&mut host, &registry, config.clone());
    let before = host.storage.get::<Weather>(StableId(2)).unwrap().clone();
    let next = SimTime(config.calendar.day.0);
    let prepared = registry
        .prepare(&host, next, None, |tx| {
            tx.call::<EnvironmentSystem>(SystemInput::Advance { next })
        })
        .unwrap();
    assert!(matches!(
        registry.commit(&mut host, prepared).unwrap(),
        SystemOutput::Advanced(Some(step)) if step.elapsed_local_day.is_some()
    ));
    assert_eq!(host.state.environment.as_ref().unwrap().epoch(), next);
    assert_eq!(host.state.environment.as_ref().unwrap().weather_day, 1);
    assert_ne!(host.storage.get::<Weather>(StableId(2)).unwrap(), &before);
}

#[test]
fn calendar_and_local_weather_requests_read_owner_metadata_and_typed_rows() {
    let (mut host, registry) = owner_host();
    let config = config();
    initialize_owner(&mut host, &registry, config.clone());
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            Ok((
                tx.request(ReadCalendar { area: AreaId(2) })?,
                tx.request(ReadLocalWeather { area: AreaId(2) })?,
                tx.request(ReadCalendar { area: AreaId(3) })?,
                tx.request(ReadLocalWeather { area: AreaId(3) })?,
            ))
        })
        .unwrap();
    let (calendar, weather, missing_calendar, missing_weather) =
        registry.commit(&mut host, prepared).unwrap();
    let calendar = calendar.unwrap();
    assert_eq!(calendar.day, 0);
    assert_eq!(calendar.year_days, config.calendar.year_days());
    assert_eq!(
        calendar.day_of_year,
        u64::from(config.calendar.starting_day)
    );
    assert_eq!(
        calendar.temperature_c,
        weather.as_ref().map(|row| row.temperature)
    );
    assert!(missing_calendar.unwrap().temperature_c.is_none());
    assert!(missing_weather.is_none());
}

#[test]
fn registered_scheduler_advances_environment_at_its_native_day_boundary() {
    let (mut host, mut registry) = owner_host();
    let mut config = config();
    config.calendar.season_days = [1; 4];
    initialize_owner(&mut host, &registry, config.clone());
    let mut scheduler = game_world::scheduling::Scheduler::<()>::new();
    register_schedule(
        &mut scheduler,
        &mut registry,
        game_world::scheduling::Order {
            priority: 10,
            module: game_sim::ModuleId(1),
        },
    )
    .unwrap();

    let day = config.calendar.day.0;
    let span = scheduler
        .step(&registry, &mut host, SimTime(day * 3), None)
        .unwrap();

    assert_eq!(
        span,
        game_world::scheduling::ClockSpan {
            from: SimTime(0),
            to: SimTime(day),
        }
    );
    assert_eq!(
        host.state.environment.as_ref().unwrap().epoch(),
        SimTime(day)
    );
}

#[test]
fn shared_elapsed_day_and_readonly_observation_preserve_opening_weather() {
    let (mut host, registry) = owner_host();
    let mut config = config();
    config.calendar.season_days = [1; 4];
    initialize_owner(&mut host, &registry, config.clone());
    let opening_weather = host.storage.get::<Weather>(StableId(2)).unwrap().clone();
    let next = SimTime(config.calendar.day.0);
    let prepared = registry
        .prepare(&host, next, None, |tx| {
            tx.call::<EnvironmentSystem>(SystemInput::Advance { next })?;
            let elapsed = tx.request(ReadElapsedDay)?;
            let observation = tx.request(ObserveEnvironment {
                alive: true,
                location: Some(game_core::Location::AtArea(AreaId(2))),
            })?;
            Ok((elapsed, observation))
        })
        .unwrap();
    let (elapsed, observation) = registry.commit(&mut host, prepared).unwrap();

    let elapsed = elapsed.unwrap();
    assert_eq!(elapsed.day, 1);
    assert_eq!(elapsed.day_of_year, u64::from(config.calendar.starting_day));
    assert_eq!(elapsed.cells.get(&AreaId(2)), Some(&opening_weather));
    assert_eq!(
        observation.unwrap().weather,
        host.storage.get::<Weather>(StableId(2)).unwrap().clone()
    );
}

#[test]
fn late_environment_validation_failure_rejects_weather_and_runtime_together() {
    let (mut host, registry) = owner_host();
    initialize_owner(&mut host, &registry, config());
    let before = host.clone();
    let next = SimTime(config().calendar.day.0);
    let result = registry.prepare(&host, next, None, |tx| {
        tx.call::<EnvironmentSystem>(SystemInput::Advance { next })?;
        tx.call::<EnvironmentSystem>(SystemInput::Validate { areas: Vec::new() })
    });
    assert!(matches!(
        result,
        Err(Error::InvalidSave(message)) if message == "unknown weather cell area"
    ));
    assert_eq!(host, before);
}

impl std::fmt::Debug for Queries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Queries")
    }
}
impl World {
    fn from_runtime(runtime: Runtime) -> Self {
        let mut store = Store::with_schema(schema());
        let queries = std::cell::RefCell::new(Runtime::bind(&mut store));
        Self {
            runtime,
            store,
            queries,
        }
    }
    fn new(config: Config, at: SimTime, seed: u64) -> Result<Self, Error> {
        Ok(Self::from_runtime(Runtime::new(config, at, seed)?))
    }
    #[cfg(feature = "content")]
    fn from_content(
        doc: game_content::Document<Config>,
        at: SimTime,
        seed: u64,
    ) -> Result<Self, Error> {
        Ok(Self::from_runtime(Runtime::from_content(doc, at, seed)?))
    }
    fn register_cell(&mut self, area: AreaId, policy: LocalClimate) -> Result<(), Error> {
        self.runtime.register_cell(&mut self.store, area, policy)
    }
    fn local_weather(&self, area: AreaId) -> Option<&Weather> {
        self.runtime.local_weather(&self.store, area)
    }
    fn observe_cell(&self, area: AreaId) -> Option<Observation> {
        self.runtime.observe_cell(&self.store, area)
    }
    fn validate(&self, at: SimTime, seed: u64) -> Result<(), Error> {
        self.runtime.validate(&self.store, at, seed)
    }
    fn reset_epoch(&mut self, at: SimTime, seed: u64) -> Result<(), Error> {
        self.runtime.reset_epoch(&mut self.store, at, seed)
    }
    #[cfg(feature = "content")]
    fn attach_content(&mut self, doc: game_content::Document<Config>) -> Result<(), Error> {
        self.runtime.attach_content(&self.store, doc)
    }
    fn prepare(&self, next: SimTime) -> Result<Step, Error> {
        self.advance(self.at, next, self.seed)
    }
    fn advance(&self, from: SimTime, next: SimTime, seed: u64) -> Result<Step, Error> {
        let mut queries = self.queries.borrow_mut();
        queries.refresh(&self.store)?;
        self.runtime
            .advance(&self.store, &queries, from, next, seed)
    }
    fn commit(&mut self, step: Step) -> Result<(), Error> {
        self.runtime
            .commit(&mut self.store, self.queries.get_mut(), step)
    }
    fn preview(&self, step: &Step) -> Result<Self, Error> {
        let mut out = self.clone();
        out.commit(step.clone())?;
        Ok(out)
    }
    fn cells(&self) -> BTreeMap<AreaId, WeatherCell> {
        self.runtime
            .cells(&self.store)
            .map(|(id, policy, weather)| {
                (
                    id,
                    WeatherCell {
                        policy: *policy,
                        weather: weather.clone(),
                    },
                )
            })
            .collect()
    }
    fn weather_mut(&mut self, area: AreaId) -> &mut Weather {
        self.store.get_mut(StableId(area.0)).unwrap()
    }
    fn remove_cell(&mut self, area: AreaId) -> WeatherCell {
        WeatherCell {
            policy: self
                .store
                .remove::<LocalClimate>(StableId(area.0))
                .unwrap()
                .unwrap(),
            weather: self
                .store
                .remove::<Weather>(StableId(area.0))
                .unwrap()
                .unwrap(),
        }
    }
}
impl std::ops::Deref for World {
    type Target = Runtime;
    fn deref(&self) -> &Runtime {
        &self.runtime
    }
}
impl std::ops::DerefMut for World {
    fn deref_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }
}
impl Clone for World {
    fn clone(&self) -> Self {
        let mut store = self.store.clone();
        let queries = std::cell::RefCell::new(Runtime::bind(&mut store));
        Self {
            runtime: self.runtime.clone(),
            store,
            queries,
        }
    }
}
impl PartialEq for World {
    fn eq(&self, other: &Self) -> bool {
        self.runtime == other.runtime && self.store == other.store
    }
}
#[derive(Serialize, Deserialize)]
struct Save {
    runtime: Runtime,
    components: game_ecs::Snapshot,
}
impl Serialize for World {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Save {
            runtime: self.runtime.clone(),
            components: self.store.snapshot().map_err(serde::ser::Error::custom)?,
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for World {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let save = Save::deserialize(d)?;
        let mut store =
            Store::restore(schema(), save.components).map_err(serde::de::Error::custom)?;
        let queries = std::cell::RefCell::new(Runtime::bind(&mut store));
        Ok(Self {
            runtime: save.runtime,
            store,
            queries,
        })
    }
}

fn config() -> Config {
    Config {
        calendar: Calendar {
            day: SimDuration(10),
            season_days: [1; 4],
            starting_day: 3,
        },
        climate: Climate {
            seasonal_temperature: [10, 20, 30, 40],
            seasonal_rain: [1, 2, 3, 4],
            seasonal_evaporation: [0; 4],
            temperature_variation: 0,
            rainfall_variation: 0,
        },
    }
}
#[test]
fn day_effect_uses_elapsed_weather_before_publishing_next_season() {
    let world = World::new(config(), SimTime(0), 7).unwrap();
    assert_eq!(world.weather.temperature, 40);
    let step = world.advance(SimTime(0), SimTime(10), 7).unwrap();
    assert_eq!(
        step.elapsed_day,
        Some(DayFacts {
            day: 1,
            day_of_year: 3,
            weather: world.weather.clone()
        })
    );
    let closing = world.preview(&step).unwrap();
    assert_eq!(closing.weather.temperature, 10);
    assert_eq!(closing.calendar.day_of_year(SimTime(10)), 0);
    assert_eq!(world.weather_day, 0);
}
#[test]
fn irregular_slices_and_current_save_produce_identical_daily_facts() {
    let mut configured = config();
    configured.climate.temperature_variation = 3;
    configured.climate.rainfall_variation = 3;
    let world = World::new(configured, SimTime(0), 19).unwrap();
    let whole = world.advance(SimTime(0), SimTime(10), 19).unwrap();
    let early = world.advance(SimTime(0), SimTime(3), 19).unwrap();
    assert!(early.elapsed_day.is_none());
    let encoded = serde_json::to_vec(&world.preview(&early).unwrap()).unwrap();
    let loaded: World = serde_json::from_slice(&encoded).unwrap();
    let split = loaded.advance(SimTime(3), SimTime(10), 19).unwrap();
    let resumed = loaded.preview(&split).unwrap();
    assert_eq!(world.preview(&whole).unwrap(), resumed);
    assert_eq!(whole.elapsed_day, split.elapsed_day);
    resumed.validate(SimTime(10), 19).unwrap();
}

#[test]
fn runtime_owns_deadline_observation_and_publication() {
    let mut runtime = World::new(config(), SimTime(7), 19).unwrap();
    let before = runtime.clone();
    assert_eq!(runtime.deadline(SimTime(100)).unwrap(), SimTime(10));
    let output = runtime.prepare(SimTime(10)).unwrap();
    assert_eq!(runtime, before);
    assert_eq!(runtime.observe().at, SimTime(7));
    assert_eq!(runtime.observe().season, 3);
    assert_eq!(output.elapsed_day.as_ref().unwrap().weather.temperature, 40);
    runtime.commit(output).unwrap();
    assert_eq!(runtime.epoch(), SimTime(10));
    assert_eq!(runtime.observe().season, 0);
    assert_eq!(runtime.observe().weather.temperature, 10);
    assert_eq!(runtime.deadline(SimTime(100)).unwrap(), SimTime(20));
}

#[test]
fn stale_and_changed_prepared_effects_cannot_publish() {
    let mut runtime = World::new(config(), SimTime(0), 19).unwrap();
    let first = runtime.prepare(SimTime(10)).unwrap();
    let stale = first.clone();
    runtime.commit(first).unwrap();
    let closing = runtime.clone();
    assert_eq!(runtime.commit(stale), Err(Error::SequenceConflict));
    assert_eq!(runtime, closing);
    let mut changed = runtime.prepare(SimTime(20)).unwrap();
    changed.elapsed_day.as_mut().unwrap().weather.rain += 1;
    assert_eq!(runtime.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(runtime, closing);
    let mut changed = runtime.prepare(SimTime(20)).unwrap();
    changed.daily.as_mut().unwrap().weather.rain += 1;
    assert_eq!(runtime.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(runtime, closing);
}

#[test]
fn save_restores_feature_epoch_and_rejects_a_host_clock_or_seed_mismatch() {
    let mut runtime = World::new(config(), SimTime(0), 19).unwrap();
    runtime
        .commit(runtime.prepare(SimTime(7)).unwrap())
        .unwrap();
    let encoded = serde_json::to_vec(&runtime).unwrap();
    let restored: World = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored.epoch(), SimTime(7));
    restored.validate(SimTime(7), 19).unwrap();
    assert!(restored.validate(SimTime(8), 19).is_err());
    assert!(restored.validate(SimTime(7), 20).is_err());
    assert!(restored.boundary(SimTime(8), SimTime(10)).is_err());
    let uninterrupted = runtime.prepare(SimTime(10)).unwrap();
    let resumed = restored.prepare(SimTime(10)).unwrap();
    assert_eq!(uninterrupted, resumed);
}

#[test]
fn opening_epoch_reset_validates_before_mutating() {
    let mut runtime = World::new(config(), SimTime(0), 19).unwrap();
    runtime.reset_epoch(SimTime(25), 3).unwrap();
    let configured = World::new(config(), SimTime(25), 3).unwrap();
    assert_eq!(runtime, configured);
    runtime.calendar.day = SimDuration(0);
    let before = runtime.clone();
    assert!(runtime.reset_epoch(SimTime(5), 4).is_err());
    assert_eq!(runtime, before);
}

fn upland() -> LocalClimate {
    LocalClimate {
        temperature_offset: -5,
        rain_multiplier_milli: 2_000,
        evaporation_multiplier_milli: 500,
    }
}

#[test]
fn local_cells_publish_their_own_elapsed_forcing_and_observations() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    assert_eq!(world.local_weather(AreaId(1)), Some(&world.weather));
    assert_eq!(world.local_weather(AreaId(2)).unwrap().temperature, 35);
    assert_eq!(world.local_weather(AreaId(2)).unwrap().rain, 8);
    assert!(world.local_weather(AreaId(3)).is_none());
    assert!(world.observe_cell(AreaId(3)).is_none());
    let local_before = world.observe_cell(AreaId(2)).unwrap();
    let step = world.prepare(SimTime(10)).unwrap();
    let elapsed = step.elapsed_local_day.as_ref().unwrap();
    assert_eq!(elapsed.day, 1);
    assert_eq!(elapsed.day_of_year, 3);
    assert_eq!(elapsed.cells[&AreaId(2)], local_before.weather);
    let closing = world.preview(&step).unwrap();
    assert_eq!(closing.local_weather(AreaId(2)).unwrap().temperature, 5);
    assert_eq!(closing.observe_cell(AreaId(2)).unwrap().season, 0);
    assert_eq!(world.observe_cell(AreaId(2)), Some(local_before));
    world.commit(step).unwrap();
    assert_eq!(world.local_weather(AreaId(2)).unwrap().rain, 2);
}

#[test]
fn local_cell_registration_order_slices_and_restores_do_not_change_forcing() {
    let mut first = World::new(config(), SimTime(0), 19).unwrap();
    let mut second = first.clone();
    for (area, policy) in [(AreaId(1), LocalClimate::default()), (AreaId(2), upland())] {
        first.register_cell(area, policy).unwrap();
    }
    for (area, policy) in [(AreaId(2), upland()), (AreaId(1), LocalClimate::default())] {
        second.register_cell(area, policy).unwrap();
    }
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    let whole = first.prepare(SimTime(10)).unwrap();
    second.commit(second.prepare(SimTime(3)).unwrap()).unwrap();
    let restored: World = serde_json::from_slice(&serde_json::to_vec(&second).unwrap()).unwrap();
    let split = restored.prepare(SimTime(10)).unwrap();
    assert_eq!(
        first.preview(&whole).unwrap(),
        restored.preview(&split).unwrap()
    );
    assert_eq!(whole.elapsed_local_day, split.elapsed_local_day);
    assert_eq!(whole.elapsed_day, split.elapsed_day);
}

#[test]
fn short_intervals_prepare_no_weather_work_and_leave_cell_storage_in_place() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    let cells = world.cells();
    let address = std::ptr::from_ref(world.local_weather(AreaId(2)).unwrap());
    for next in 1..10 {
        let step = world.prepare(SimTime(next)).unwrap();
        assert!(step.daily.is_none());
        assert!(step.elapsed_day.is_none());
        assert!(step.elapsed_local_day.is_none());
        world.commit(step).unwrap();
        assert_eq!(world.epoch(), SimTime(next));
        assert_eq!(world.cells(), cells);
        assert_eq!(
            std::ptr::from_ref(world.local_weather(AreaId(2)).unwrap()),
            address
        );
    }
    let step = world.prepare(SimTime(10)).unwrap();
    assert_eq!(step.daily.as_ref().unwrap().cells.len(), 2);
    assert_eq!(step.elapsed_local_day.as_ref().unwrap().cells.len(), 2);
    world.commit(step).unwrap();
    assert_eq!(
        std::ptr::from_ref(world.local_weather(AreaId(2)).unwrap()),
        address
    );
    assert_ne!(world.cells(), cells);
}

#[test]
fn changed_daily_membership_or_a_late_cell_guard_cannot_partially_publish() {
    let mut opening = World::new(config(), SimTime(0), 19).unwrap();
    opening
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    opening.register_cell(AreaId(2), upland()).unwrap();
    for variation in 0..4 {
        let mut world = opening.clone();
        let step = world.prepare(SimTime(10)).unwrap();
        match variation {
            0 => world
                .register_cell(AreaId(3), LocalClimate::default())
                .unwrap(),
            1 => {
                world.remove_cell(AreaId(2));
            }
            2 => {
                let existing = world.remove_cell(AreaId(2));
                world.register_cell(AreaId(3), existing.policy).unwrap();
            }
            3 => {
                world.weather_mut(AreaId(2)).rain += 1;
            }
            _ => unreachable!(),
        }
        let before = world.clone();
        assert_eq!(world.commit(step), Err(Error::SequenceConflict));
        assert_eq!(world, before);
        assert_eq!(world.epoch(), SimTime(0));
        assert_eq!(
            *world.local_weather(AreaId(1)).unwrap(),
            *opening.local_weather(AreaId(1)).unwrap()
        );
    }
}

#[test]
fn short_interval_header_changes_and_added_forcing_are_rejected_before_publication() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    let mut changed = world.prepare(SimTime(1)).unwrap();
    changed.elapsed_local_day = Some(LocalDayFacts {
        day: 1,
        day_of_year: 3,
        cells: BTreeMap::new(),
    });
    let before = world.clone();
    assert_eq!(world.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(world, before);
    let step = world.prepare(SimTime(1)).unwrap();
    world.climate.seasonal_temperature[0] += 1;
    let before = world.clone();
    assert_eq!(world.commit(step), Err(Error::SequenceConflict));
    assert_eq!(world, before);
}

#[test]
fn invalid_local_policies_and_future_extremes_are_rejected_before_registration() {
    let configured = Config {
        climate: Climate {
            seasonal_rain: [1, 1, 1, 1_000_000],
            ..config().climate
        },
        ..config()
    };
    let mut world = World::new(configured, SimTime(10), 19).unwrap();
    let before = world.clone();
    assert!(world.register_cell(AreaId(1), upland()).is_err());
    assert_eq!(world, before);
    assert!(
        world
            .register_cell(AreaId(0), LocalClimate::default())
            .is_err()
    );
    assert!(
        world
            .register_cell(
                AreaId(1),
                LocalClimate {
                    temperature_offset: 101,
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(world, before);
    world
        .register_cell(AreaId(1), LocalClimate::default())
        .unwrap();
    let before = world.clone();
    assert!(
        world
            .register_cell(AreaId(1), LocalClimate::default())
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn local_cell_corruption_and_changed_elapsed_facts_cannot_be_committed() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    let mut corrupt = world.clone();
    corrupt.weather_mut(AreaId(2)).rain += 1;
    assert!(corrupt.validate(SimTime(0), 19).is_err());
    let before = world.clone();
    let mut changed = world.prepare(SimTime(10)).unwrap();
    changed
        .elapsed_local_day
        .as_mut()
        .unwrap()
        .cells
        .get_mut(&AreaId(2))
        .unwrap()
        .rain += 1;
    assert_eq!(world.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(world, before);
    let mut changed = world.prepare(SimTime(10)).unwrap();
    changed.daily.as_mut().unwrap().cells.clear();
    assert_eq!(world.commit(changed), Err(Error::SequenceConflict));
    assert_eq!(world, before);
}

#[test]
fn resetting_an_opening_epoch_preserves_local_policies_and_samples_new_weather() {
    let mut world = World::new(config(), SimTime(0), 19).unwrap();
    world.register_cell(AreaId(2), upland()).unwrap();
    world.reset_epoch(SimTime(25), 3).unwrap();
    assert_eq!(world.cells()[&AreaId(2)].policy, upland());
    assert_eq!(
        world.local_weather(AreaId(2)),
        Some(&upland().apply(&world.weather).unwrap())
    );
    assert_eq!(world.local_weather(AreaId(2)).unwrap().temperature, 15);
    world.validate(SimTime(25), 3).unwrap();
}

#[cfg(feature = "content")]
#[test]
fn runtime_loads_edited_toml_without_changing_an_existing_save() {
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/environment/temperate.toml");
    let source = std::fs::read_to_string(&bundled).unwrap();
    let original = Config::read(&bundled).unwrap();
    assert_eq!(original.data, Config::default());
    let runtime = World::from_content(original, SimTime(0), 19).unwrap();
    let before = serde_json::to_vec(&runtime).unwrap();
    let path = std::env::temp_dir().join(format!(
        "legacy-environment-content-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, source.replace("[12, 20, 10, 0]", "[30, 30, 30, 30]")).unwrap();
    let edited = World::from_content(Config::read(&path).unwrap(), SimTime(0), 19).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_ne!(runtime.content_stamp(), edited.content_stamp());
    assert_ne!(runtime.observe().weather, edited.observe().weather);
    let restored: World = serde_json::from_slice(&before).unwrap();
    assert_eq!(restored, runtime);
    restored.validate(SimTime(0), 19).unwrap();
    assert_eq!(
        restored.prepare(SimTime(86_400_000)),
        runtime.prepare(SimTime(86_400_000))
    );
}

#[cfg(feature = "content")]
#[test]
fn invalid_or_changed_authored_definitions_are_rejected() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/environment/temperate.toml"),
    )
    .unwrap();
    let invalid =
        game_content::parse::<Config>(&source.replace("day = 86400000", "day = 0")).unwrap();
    assert!(World::from_content(invalid, SimTime(0), 19).is_err());
    assert!(
        game_content::parse::<Config>(
            &source.replace("temperature_variation = 3", "temperature_varation = 3")
        )
        .is_err()
    );
    let document = game_content::parse::<Config>(&source).unwrap();
    let mut world = World::from_content(document, SimTime(0), 19).unwrap();
    world.climate.seasonal_temperature[0] += 1;
    world.weather = world.climate.weather(19, 0, 0);
    assert!(world.validate(SimTime(0), 19).is_err());
    let before = world.clone();
    assert!(world.reset_epoch(SimTime(10), 19).is_err());
    assert_eq!(world, before);
}

#[cfg(feature = "content")]
#[test]
fn attaching_content_preserves_local_components_and_epoch_and_rejects_mismatches() {
    let bundled = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/environment/temperate.toml");
    let document = Config::read(&bundled).unwrap();
    let expected_stamp = document.stamp().unwrap();
    let mut runtime = World::new(document.data.clone(), SimTime(7), 19).unwrap();
    runtime.register_cell(AreaId(2), upland()).unwrap();
    let components = runtime.cells();
    runtime.attach_content(document.clone()).unwrap();
    assert_eq!(runtime.epoch(), SimTime(7));
    assert_eq!(runtime.cells(), components);
    assert_eq!(runtime.content_stamp(), Some(&expected_stamp));
    let before = runtime.clone();
    let mut changed = document.clone();
    changed.data.climate.seasonal_rain[0] += 1;
    assert!(runtime.attach_content(changed).is_err());
    assert_eq!(runtime, before);
    let mut invalid = document;
    invalid.version = 0;
    assert!(runtime.attach_content(invalid).is_err());
    assert_eq!(runtime, before);
}
#[test]
fn invalid_calendar_is_rejected_before_division_and_weather_sampling() {
    for bad in [
        Config {
            calendar: Calendar {
                day: SimDuration(0),
                ..config().calendar
            },
            ..config()
        },
        Config {
            calendar: Calendar {
                season_days: [0, 1, 1, 1],
                ..config().calendar
            },
            ..config()
        },
        Config {
            calendar: Calendar {
                starting_day: 4,
                ..config().calendar
            },
            ..config()
        },
    ] {
        assert!(World::new(bad, SimTime(0), 0).is_err());
    }
    let bad = Config {
        climate: Climate {
            temperature_variation: 101,
            ..config().climate
        },
        ..config()
    };
    assert!(World::new(bad, SimTime(0), 0).is_err());
}
#[test]
fn skipped_reversed_and_corrupted_intervals_leave_the_snapshot_unchanged() {
    let world = World::new(config(), SimTime(0), 1).unwrap();
    let before = world.clone();
    assert!(world.advance(SimTime(0), SimTime(20), 1).is_err());
    assert!(world.advance(SimTime(10), SimTime(9), 1).is_err());
    assert_eq!(world, before);
    let mut corrupt = world;
    corrupt.weather.rain += 1;
    assert!(corrupt.advance(SimTime(0), SimTime(1), 1).is_err());
}
#[test]
fn extreme_time_and_day_boundary_do_not_overflow() {
    let configured = Config {
        calendar: Calendar {
            day: SimDuration(1),
            ..config().calendar
        },
        ..config()
    };
    let world = World::new(configured, SimTime(u64::MAX - 1), 2).unwrap();
    let step = world
        .advance(SimTime(u64::MAX - 1), SimTime(u64::MAX), 2)
        .unwrap();
    let closing = world.preview(&step).unwrap();
    assert_eq!(closing.weather_day, u64::MAX);
    assert_eq!(step.elapsed_day.unwrap().day, u64::MAX);
    assert_eq!(
        closing
            .boundary(SimTime(u64::MAX), SimTime(u64::MAX))
            .unwrap(),
        SimTime(u64::MAX)
    );
    assert_eq!(closing.calendar.day_of_year(SimTime(u64::MAX)), 2);
}
