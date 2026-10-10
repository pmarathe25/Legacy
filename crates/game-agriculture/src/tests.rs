use super::*;

/// Standalone tests assemble a host store explicitly. This wrapper is test-only;
/// production runtime metadata never contains or copies component storage.
struct Fixture {
    runtime: Runtime,
    store: game_ecs::Store,
    queries: Queries,
}
impl std::ops::Deref for Fixture {
    type Target = Runtime;
    fn deref(&self) -> &Runtime {
        &self.runtime
    }
}
impl std::ops::DerefMut for Fixture {
    fn deref_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }
}
impl Fixture {
    fn with_runtime(runtime: Runtime) -> Result<Self, Error> {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema)?;
        let mut store = game_ecs::Store::with_schema(schema);
        let queries = Runtime::bind(&mut store);
        Ok(Self {
            runtime,
            store,
            queries,
        })
    }
    fn new(catalog: Catalog, day: u64) -> Result<Self, Error> {
        Self::with_runtime(Runtime::new(catalog, day)?)
    }
    #[cfg(feature = "content")]
    fn from_content(document: game_content::Document<Catalog>, day: u64) -> Result<Self, Error> {
        Self::with_runtime(Runtime::from_content(document, day)?)
    }
    fn fields(&self) -> impl Iterator<Item = (SiteId, &Field)> {
        self.runtime.fields(&self.store)
    }
    fn soils(&self) -> impl Iterator<Item = (SiteId, &Soil)> {
        self.runtime.soils(&self.store)
    }
    fn field(&self, id: SiteId) -> Option<&Field> {
        self.runtime.field(&self.store, id)
    }
    fn crop(&self, id: SiteId) -> Option<&Crop> {
        self.runtime.crop(&self.store, id)
    }
    fn soil(&self, id: SiteId) -> Option<&Soil> {
        self.runtime.soil(&self.store, id)
    }
    fn field_mut(&mut self, id: SiteId) -> Option<&mut Field> {
        self.runtime.field_mut(&mut self.store, id)
    }
    fn crop_mut(&mut self, id: SiteId) -> Option<&mut Crop> {
        self.runtime.crop_mut(&mut self.store, id)
    }
    fn soil_mut(&mut self, id: SiteId) -> Option<&mut Soil> {
        self.runtime.soil_mut(&mut self.store, id)
    }
    fn insert_field(&mut self, field: Field) -> Result<(), Error> {
        self.runtime.insert_field(&mut self.store, field)?;
        self.queries.refresh(&self.store)
    }
    fn insert_crop(&mut self, id: SiteId, crop: Crop) -> Result<(), Error> {
        self.runtime.insert_crop(&mut self.store, id, crop)?;
        self.queries.refresh(&self.store)
    }
    fn insert_soil(&mut self, id: SiteId, soil: Soil) -> Result<(), Error> {
        self.runtime.insert_soil(&mut self.store, id, soil)?;
        self.queries.refresh(&self.store)
    }
    fn remove_soil(&mut self, id: SiteId) -> Result<Option<Soil>, Error> {
        let previous = self.runtime.remove_soil(&mut self.store, id)?;
        self.queries.refresh(&self.store)?;
        Ok(previous)
    }
    fn rules_for(&self, id: SiteId) -> Result<&CropRules, Error> {
        self.runtime.rules_for(&self.store, id)
    }
    fn field_bundle(&self, id: SiteId) -> Result<(&Field, &Crop, &Soil, &CropRules), Error> {
        self.runtime.field_bundle(&self.store, id)
    }
    fn validate(&self, now: SimTime, year_days: Option<u64>) -> Result<(), Error> {
        self.runtime.validate(&self.store, now, year_days)
    }
    fn prepare_advance(
        &self,
        from: SimTime,
        next: SimTime,
        day: Option<&LocalWeatherDay>,
        evaluate: impl FnOnce(
            &[(SiteId, GrowthInput<'_>)],
        ) -> Result<Vec<(SiteId, GrowthOutput)>, Error>,
    ) -> Result<PreparedAdvance, Error> {
        self.runtime
            .prepare_advance(&self.store, &self.queries, from, next, day, evaluate)
    }
    fn prepare_advance_serial(
        &self,
        from: SimTime,
        next: SimTime,
        day: Option<&LocalWeatherDay>,
    ) -> Result<PreparedAdvance, Error> {
        self.runtime
            .prepare_advance_serial(&self.store, &self.queries, from, next, day)
    }
    fn commit_advance(&mut self, prepared: PreparedAdvance) -> Result<(), Error> {
        self.runtime.commit_advance(&mut self.store, prepared)
    }
    fn advance(
        &self,
        from: SimTime,
        next: SimTime,
        day: Option<&LocalWeatherDay>,
    ) -> Result<Self, Error> {
        let prepared = self.prepare_advance_serial(from, next, day)?;
        let mut next = self.clone();
        next.commit_advance(prepared)?;
        Ok(next)
    }
}
impl Clone for Fixture {
    fn clone(&self) -> Self {
        let mut store = self.store.clone();
        let queries = Runtime::bind(&mut store);
        Self {
            runtime: self.runtime.clone(),
            store,
            queries,
        }
    }
}
impl PartialEq for Fixture {
    fn eq(&self, other: &Self) -> bool {
        self.runtime == other.runtime && self.store == other.store
    }
}
impl std::fmt::Debug for Fixture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fixture")
            .field("runtime", &self.runtime)
            .field("store", &self.store)
            .finish()
    }
}
#[derive(Serialize)]
struct FixtureRef<'a> {
    runtime: &'a Runtime,
    components: game_ecs::Snapshot,
}
#[derive(Deserialize)]
struct SavedFixture {
    runtime: Runtime,
    components: game_ecs::Snapshot,
}
impl Serialize for Fixture {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FixtureRef {
            runtime: &self.runtime,
            components: self.store.snapshot().map_err(serde::ser::Error::custom)?,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Fixture {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = SavedFixture::deserialize(deserializer)?;
        saved
            .runtime
            .catalog
            .validate(None)
            .map_err(serde::de::Error::custom)?;
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).map_err(serde::de::Error::custom)?;
        let mut store =
            game_ecs::Store::restore(schema, saved.components).map_err(serde::de::Error::custom)?;
        let queries = Runtime::bind(&mut store);
        Ok(Self {
            runtime: saved.runtime,
            store,
            queries,
        })
    }
}

fn rules() -> CropRules {
    // Test fixture, deliberately distinct from the authored setting.
    CropRules {
        planting_first_day: 0,
        planting_last_day: 9,
        prepare_labor: SimDuration(14_400_000),
        plant_labor: SimDuration(7_200_000),
        tend_labor: SimDuration(3_600_000),
        irrigation_labor: SimDuration(3_600_000),
        harvest_labor: SimDuration(14_400_000),
        fallow_labor: SimDuration(3_600_000),
        seed_input: Quantity(1_000),
        irrigation_input: Quantity(1_000),
        irrigation_water: 30,
        base_temperature: 0,
        ripe_degree_days: 20,
        optimal_water: 50,
        failure_stress_days: 12,
        food_yield: Quantity(6_000_000),
        seed_yield: Quantity(2_000),
        soil_depletion: 20,
        soil_recovery: 1,
    }
}
fn world() -> Fixture {
    let rules = rules();
    let definition = CropId("test/cereal".into());
    let mut world = Fixture::new(
        Catalog {
            default_crop: definition.clone(),
            crops: BTreeMap::from([(definition.clone(), rules.clone())]),
        },
        0,
    )
    .unwrap();
    world
        .insert_field(Field {
            id: SiteId(1),
            area: AreaId(2),
            access: BTreeSet::from([PersonId(3)]),
            custodian: Some(PersonId(3)),
            seed: InventoryId(4),
            tool: InventoryId(5),
            water: InventoryId(6),
            food: InventoryId(7),
        })
        .unwrap();
    world
        .insert_crop(
            SiteId(1),
            Crop {
                definition,
                phase: CropPhase::Growing,
                preparation_ms: rules.prepare_labor.0,
                planting_ms: rules.plant_labor.0,
                harvest_ms: 0,
                degree_days_milli: 0,
                stress_days: 0,
                tending: 1_000,
                contributions: BTreeMap::new(),
            },
        )
        .unwrap();
    world
        .insert_soil(
            SiteId(1),
            Soil {
                fertility: 900,
                water: 75,
                capacity: 120,
            },
        )
        .unwrap();
    world
}
fn day(number: u64, rain: u64) -> LocalWeatherDay {
    LocalWeatherDay {
        day: number,
        day_of_year: number - 1,
        cells: BTreeMap::from([(
            AreaId(2),
            WeatherFacts {
                temperature: 10,
                rain,
                evaporation: 2,
            },
        )]),
    }
}
fn begin() -> BeginFacts {
    BeginFacts {
        actor: PersonId(3),
        now: SimTime(0),
        area: AreaId(2),
        capable: true,
        tool_available: true,
        field_available: true,
        seed_available: Quantity(1_000),
        water_available: Quantity(1_000),
        outputs_accessible: true,
        food_room: Quantity(u64::MAX),
        seed_room: Quantity(u64::MAX),
        day_of_year: Some(0),
        temperature_c: Some(10),
    }
}
fn closing(now: SimTime) -> CompletionFacts {
    CompletionFacts {
        now,
        eligible: true,
        outputs_accessible: true,
        food_room: Quantity(u64::MAX),
        seed_room: Quantity(u64::MAX),
    }
}
fn start(
    world: &Fixture,
    operation: FarmOperation,
    facts: BeginFacts,
) -> Result<BeginEffect, Error> {
    let (field, crop, soil, rules) = world.field_bundle(SiteId(1))?;
    prepare_begin(field, crop, soil, rules, operation, facts)
}
fn finish(
    world: &Fixture,
    job: &Cultivation,
    facts: CompletionFacts,
) -> Result<CropCompletion, Error> {
    let (field, crop, soil, rules) = world.field_bundle(SiteId(1))?;
    prepare_completion(field, crop, soil, job, rules, facts)
}

#[test]
fn growth_and_harvest_describe_component_changes_without_stock_storage() {
    let world = world();
    let before = world.clone();
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(world, before);
    assert_eq!(first.crop(SiteId(1)).unwrap().phase, CropPhase::Growing);
    let ripe = first
        .advance(SimTime(60_000), SimTime(120_000), Some(&day(2, 10)))
        .unwrap();
    assert_eq!(ripe.crop(SiteId(1)).unwrap().phase, CropPhase::Ripe);
    assert_eq!(
        ripe.fields().collect::<Vec<_>>(),
        world.fields().collect::<Vec<_>>()
    );
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Harvest,
        labor_ms: ripe.rules_for(SiteId(1)).unwrap().harvest_labor.0,
        completes: SimTime(120_000),
    };
    let effect = finish(&ripe, &job, closing(job.completes)).unwrap();
    assert_eq!(effect.harvest.unwrap().food, Quantity(5_400_000));
    assert_eq!(effect.harvest.unwrap().seed, Quantity(1_800));
    assert_eq!(effect.crop.phase, CropPhase::Fallow);
    assert_eq!(effect.soil.fertility, 880);
    let blocked = finish(
        &ripe,
        &job,
        CompletionFacts {
            seed_room: Quantity(0),
            ..closing(job.completes)
        },
    )
    .unwrap();
    assert!(blocked.harvest.is_none());
    assert_eq!(blocked.crop.phase, CropPhase::Ripe);
    assert_eq!(
        blocked.crop.harvest_ms,
        ripe.rules_for(SiteId(1)).unwrap().harvest_labor.0 - 1
    );
}

#[test]
fn failure_and_fallow_recovery_consume_supplied_local_weather_only() {
    let mut world = world();
    world
        .catalog
        .crops
        .values_mut()
        .next()
        .unwrap()
        .failure_stress_days = 2;
    world.soil_mut(SiteId(1)).unwrap().water = 0;
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 0)))
        .unwrap();
    let failed = first
        .advance(SimTime(60_000), SimTime(120_000), Some(&day(2, 0)))
        .unwrap();
    assert_eq!(failed.crop(SiteId(1)).unwrap().phase, CropPhase::Failed);
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Fallow,
        labor_ms: failed.rules_for(SiteId(1)).unwrap().fallow_labor.0,
        completes: SimTime(120_000),
    };
    let mut effect = finish(&failed, &job, closing(job.completes)).unwrap();
    effect.soil.water = 50;
    let before = effect.soil.fertility;
    let (_, soil) = grow(
        &effect.crop,
        &effect.soil,
        failed.rules_for(SiteId(1)).unwrap(),
        &day(3, 0).cells[&AreaId(2)],
    )
    .unwrap();
    assert_eq!(soil.fertility, before + 1);
}

#[test]
fn planting_consumes_seed_once_and_resuming_requires_remaining_work() {
    let mut world = world();
    world.crop_mut(SiteId(1)).unwrap().phase = CropPhase::Prepared;
    world.crop_mut(SiteId(1)).unwrap().planting_ms = 0;
    assert_eq!(
        start(
            &world,
            FarmOperation::Plant,
            BeginFacts {
                seed_available: Quantity(0),
                ..begin()
            }
        ),
        Err(Error::Unavailable)
    );
    let effect = start(&world, FarmOperation::Plant, begin()).unwrap();
    assert_eq!(
        effect.consumed,
        Some(InputConsumption {
            inventory: InventoryId(4),
            quantity: Quantity(1_000)
        })
    );
    assert_eq!(effect.crop.phase, CropPhase::Planting);
    world.insert_crop(SiteId(1), effect.crop).unwrap();
    world.jobs.insert(PersonId(3), effect.job);
    let half = world.advance(SimTime(0), SimTime(3_600_000), None).unwrap();
    let resumed = start(
        &half,
        FarmOperation::Plant,
        BeginFacts {
            now: SimTime(3_600_000),
            seed_available: Quantity(0),
            day_of_year: None,
            temperature_c: None,
            ..begin()
        },
    )
    .unwrap();
    assert!(resumed.consumed.is_none());
    assert_eq!(resumed.job.completes, SimTime(7_200_000));
    assert_eq!(resumed.crop.contributions[&PersonId(3)], 3_600_000);
}

#[test]
fn labor_and_restore_are_identical_across_slices() {
    let mut world = world();
    world.crop_mut(SiteId(1)).unwrap().phase = CropPhase::Planting;
    world.crop_mut(SiteId(1)).unwrap().planting_ms = 0;
    world.catalog.crops.values_mut().next().unwrap().plant_labor = SimDuration(120_000);
    world.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Plant,
            labor_ms: 0,
            completes: SimTime(120_000),
        },
    );
    let whole = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    let early = world.advance(SimTime(0), SimTime(17_001), None).unwrap();
    let restored: Fixture = serde_json::from_slice(&serde_json::to_vec(&early).unwrap()).unwrap();
    let split = restored
        .advance(SimTime(17_001), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(whole, split);
    assert_eq!(
        whole.crop(SiteId(1)).unwrap().contributions[&PersonId(3)],
        60_000
    );
    assert_eq!(whole.crop(SiteId(1)).unwrap().phase, CropPhase::Planting);
}

#[test]
fn physical_ineligibility_and_early_completion_are_inert() {
    let mut world = world();
    world.crop_mut(SiteId(1)).unwrap().phase = CropPhase::Ripe;
    let before = world.clone();
    assert_eq!(
        start(
            &world,
            FarmOperation::Harvest,
            BeginFacts {
                capable: false,
                ..begin()
            }
        ),
        Err(Error::Unavailable)
    );
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Harvest,
        labor_ms: 0,
        completes: SimTime(10),
    };
    assert_eq!(
        finish(&world, &job, closing(SimTime(9))),
        Err(Error::Unavailable)
    );
    assert_eq!(
        finish(
            &world,
            &job,
            CompletionFacts {
                eligible: false,
                ..closing(SimTime(10))
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(world, before);
}

#[test]
fn duplicate_labor_days_and_overflow_cannot_partially_change_components() {
    let mut world = world();
    let job = Cultivation {
        field: SiteId(1),
        operation: FarmOperation::Tend,
        labor_ms: 0,
        completes: SimTime(60_000),
    };
    world.jobs.insert(PersonId(3), job.clone());
    world.jobs.insert(PersonId(8), job);
    let before = world.clone();
    assert!(world.advance(SimTime(0), SimTime(1), None).is_err());
    assert_eq!(world, before);
    world.jobs.clear();
    let first = world
        .advance(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert!(
        first
            .advance(SimTime(60_000), SimTime(61_000), Some(&day(1, 10)))
            .is_err()
    );
    let mut crop = world.crop(SiteId(1)).unwrap().clone();
    crop.degree_days_milli = u64::MAX;
    let before = crop.clone();
    let soil = world.soil(SiteId(1)).unwrap().clone();
    assert_eq!(
        grow(
            &crop,
            &soil,
            world.rules_for(SiteId(1)).unwrap(),
            &day(1, 10).cells[&AreaId(2)]
        ),
        Err(Error::Overflow)
    );
    assert_eq!(crop, before);
    assert_eq!(world.soil(SiteId(1)).unwrap(), &soil);
}

#[test]
fn calendar_validation_is_optional_and_has_no_climate_state_dependency() {
    let world = world();
    world.catalog.validate(None).unwrap();
    world.catalog.validate(Some(365)).unwrap();
    assert!(world.catalog.validate(Some(2)).is_err());
    let mut bad = world.catalog.clone();
    bad.crops.values_mut().next().unwrap().prepare_labor = SimDuration(0);
    assert!(bad.validate(None).is_err());
}

#[test]
fn owner_validation_rejects_missing_components_and_corrupt_jobs() {
    let mut valid = world();
    valid.crop_mut(SiteId(1)).unwrap().tending = 0;
    let completes = SimTime(valid.rules_for(SiteId(1)).unwrap().tend_labor.0);
    valid.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes,
        },
    );
    valid.validate(SimTime(0), Some(365)).unwrap();
    let mut bad = valid.clone();
    bad.crop_mut(SiteId(1)).unwrap().phase = CropPhase::Prepared;
    assert!(bad.validate(SimTime(0), Some(365)).is_err());
    let mut bad = valid.clone();
    bad.jobs.get_mut(&PersonId(3)).unwrap().completes.0 += 1;
    assert!(bad.validate(SimTime(0), Some(365)).is_err());
    let mut bad = valid.clone();
    let duplicate = bad.jobs[&PersonId(3)].clone();
    bad.jobs.insert(PersonId(8), duplicate);
    assert!(bad.validate(SimTime(0), None).is_err());
    let mut bad = valid.clone();
    bad.crop_mut(SiteId(1))
        .unwrap()
        .contributions
        .insert(PersonId(3), 1);
    assert!(bad.validate(SimTime(0), None).is_err());
    let mut bad = valid.clone();
    for id in bad.soils().map(|(id, _)| id).collect::<Vec<_>>() {
        bad.remove_soil(id).unwrap();
    }
    assert!(bad.validate(SimTime(0), None).is_err());
}

#[test]
fn different_crop_definitions_join_their_own_soil_and_weather_cells() {
    let mut world = world();
    let mut crop = world.crop(SiteId(1)).unwrap().clone();
    crop.definition = CropId("test/slow".into());
    let mut rules = rules();
    rules.base_temperature = 9;
    world.catalog.crops.insert(crop.definition.clone(), rules);
    world.insert_crop(SiteId(8), crop).unwrap();
    let mut field = world.field(SiteId(1)).unwrap().clone();
    field.id = SiteId(8);
    field.area = AreaId(9);
    world.insert_field(field).unwrap();
    world
        .insert_soil(
            SiteId(8),
            Soil {
                fertility: 400,
                water: 25,
                capacity: 30,
            },
        )
        .unwrap();
    let mut weather = day(1, 10);
    weather.cells.insert(
        AreaId(9),
        WeatherFacts {
            temperature: 10,
            rain: 0,
            evaporation: 0,
        },
    );
    let next = world
        .advance(SimTime(0), SimTime(60_000), Some(&weather))
        .unwrap();
    assert_eq!(next.crop(SiteId(1)).unwrap().degree_days_milli, 10_000);
    assert_eq!(next.crop(SiteId(8)).unwrap().degree_days_milli, 500);
    assert_eq!(next.soil(SiteId(1)).unwrap().water, 83);
    assert_eq!(next.soil(SiteId(8)).unwrap().water, 25);
    assert_eq!(
        next.fields().collect::<Vec<_>>(),
        world.fields().collect::<Vec<_>>()
    );
}

#[test]
fn unmatched_growth_entities_are_skipped_and_missing_weather_preserves_labor() {
    let mut world = world();
    let mut spare = world.crop(SiteId(1)).unwrap().clone();
    spare.tending = 0;
    world.insert_crop(SiteId(8), spare).unwrap();
    world.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes: SimTime(60_000),
        },
    );
    let mut weather = day(1, 10);
    weather.cells.clear();
    let next = world
        .advance(SimTime(0), SimTime(1), Some(&weather))
        .unwrap();
    assert_eq!(next.crop(SiteId(1)).unwrap().degree_days_milli, 0);
    assert_eq!(next.crop(SiteId(1)).unwrap().contributions[&PersonId(3)], 1);
    assert_eq!(
        next.crop(SiteId(8)).unwrap(),
        world.crop(SiteId(8)).unwrap()
    );
    assert_eq!(
        next.soils().collect::<Vec<_>>(),
        world.soils().collect::<Vec<_>>()
    );
    let mut partial = world.clone();
    for id in partial.soils().map(|(id, _)| id).collect::<Vec<_>>() {
        partial.remove_soil(id).unwrap();
    }
    let next = partial
        .advance(SimTime(0), SimTime(1), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(next.crop(SiteId(1)).unwrap().degree_days_milli, 0);
}

#[test]
fn absence_of_calendar_and_weather_only_disables_operations_that_need_them() {
    let mut world = world();
    let facts = BeginFacts {
        day_of_year: None,
        temperature_c: None,
        ..begin()
    };
    assert!(start(&world, FarmOperation::Tend, facts).is_err()); // Already fully tended.
    world.crop_mut(SiteId(1)).unwrap().tending = 0;
    start(&world, FarmOperation::Tend, facts).unwrap();
    world.crop_mut(SiteId(1)).unwrap().phase = CropPhase::Fallow;
    assert_eq!(
        start(&world, FarmOperation::Prepare, facts),
        Err(Error::Unavailable)
    );
    world.crop_mut(SiteId(1)).unwrap().phase = CropPhase::Prepared;
    world.crop_mut(SiteId(1)).unwrap().planting_ms = 0;
    assert_eq!(
        start(&world, FarmOperation::Plant, facts),
        Err(Error::Unavailable)
    );
    world.crop_mut(SiteId(1)).unwrap().phase = CropPhase::Ripe;
    start(&world, FarmOperation::Harvest, facts).unwrap();
}

#[cfg(feature = "content")]
#[test]
fn authored_crop_content_round_trips_and_edits_change_behavior_without_code_changes() {
    let document = Catalog::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/agriculture/crops.toml"
    ))
    .unwrap();
    document.data.validate(Some(365)).unwrap();
    let original = Fixture::from_content(document.clone(), 0).unwrap();
    assert!(original.content.is_some());
    let restored: Fixture =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(original, restored);
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/agriculture/crops.toml"
    ))
    .unwrap();
    let edited: game_content::Document<Catalog> =
        game_content::parse(&source.replace("food_yield = 6000000", "food_yield = 250")).unwrap();
    let changed = Fixture::from_content(edited, 0).unwrap();
    assert_ne!(original.content, changed.content);
    let crop = world().crop(SiteId(1)).unwrap().clone();
    let soil = Soil {
        fertility: 1_000,
        water: 50,
        capacity: 100,
    };
    assert_eq!(
        yield_amounts(
            &crop,
            &soil,
            &changed.catalog.crops[&changed.catalog.default_crop]
        )
        .0,
        Quantity(250)
    );
    assert_eq!(
        restored.catalog.crops[&restored.catalog.default_crop].food_yield,
        Quantity(6_000_000)
    );
}

#[test]
fn idle_intervals_prepare_no_rows_and_never_run_daily_growth() {
    let world = world();
    let batch = world
        .prepare_advance(SimTime(0), SimTime(1_000), None, |_| {
            panic!("daily growth must be due")
        })
        .unwrap();
    assert_eq!(batch.changed_crop_rows(), 0);
    assert_eq!(batch.changed_job_rows(), 0);
    assert_eq!(batch.growth_rows(), 0);
    let mut next = world.clone();
    next.commit_advance(batch).unwrap();
    assert_eq!(next, world);
}

#[test]
fn sparse_growth_commits_preserve_unrelated_rows_and_metadata() {
    let mut world = world();
    world
        .insert_crop(SiteId(8), Crop::new(world.catalog.default_crop.clone()))
        .unwrap();
    let batch = world
        .prepare_advance_serial(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(batch.changed_crop_rows(), 1);
    assert_eq!(batch.growth_rows(), 1);
    world
        .field_mut(SiteId(1))
        .unwrap()
        .access
        .insert(PersonId(99));
    world.field_mut(SiteId(1)).unwrap().food = InventoryId(99);
    world.crop_mut(SiteId(8)).unwrap().tending = 500;
    world.catalog.default_crop = CropId("unrelated/default-selection".into());
    let metadata = world
        .fields()
        .map(|(id, value)| (id, value.clone()))
        .collect::<BTreeMap<_, _>>();
    world.commit_advance(batch).unwrap();
    assert_eq!(world.crop(SiteId(1)).unwrap().degree_days_milli, 10_000);
    assert_eq!(world.crop(SiteId(8)).unwrap().tending, 500);
    assert_eq!(
        world
            .fields()
            .map(|(id, value)| (id, value.clone()))
            .collect::<BTreeMap<_, _>>(),
        metadata
    );
}

#[test]
fn stale_read_rows_reject_the_entire_sparse_commit_before_publication() {
    let world = world();
    let batch = world
        .prepare_advance_serial(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    for mutation in 0..6 {
        let mut changed = world.clone();
        match mutation {
            0 => changed.crop_mut(SiteId(1)).unwrap().tending = 0,
            1 => changed.soil_mut(SiteId(1)).unwrap().water = 0,
            2 => changed.field_mut(SiteId(1)).unwrap().area = AreaId(99),
            3 => {
                changed
                    .catalog
                    .crops
                    .values_mut()
                    .next()
                    .unwrap()
                    .base_temperature += 1
            }
            4 => changed.day += 1,
            5 => {
                changed.jobs.insert(
                    PersonId(99),
                    Cultivation {
                        field: SiteId(1),
                        operation: FarmOperation::Tend,
                        labor_ms: 0,
                        completes: SimTime(120_000),
                    },
                );
            }
            _ => unreachable!(),
        }
        let before = changed.clone();
        assert!(changed.commit_advance(batch.clone()).is_err());
        assert_eq!(changed, before);
    }
}

#[test]
fn executor_errors_wrong_keys_and_foreign_writes_cannot_prepare_changes() {
    let world = world();
    let before = world.clone();
    let forcing = day(1, 10);
    assert!(
        world
            .prepare_advance(SimTime(0), SimTime(1), Some(&forcing), |_| Err(
                Error::Overflow
            ))
            .is_err()
    );
    assert!(
        world
            .prepare_advance(SimTime(0), SimTime(1), Some(&forcing), |inputs| {
                Ok(vec![(SiteId(99), inputs[0].1.evaluate()?)])
            })
            .is_err()
    );
    assert!(
        world
            .prepare_advance(SimTime(0), SimTime(1), Some(&forcing), |inputs| {
                let mut output = inputs[0].1.evaluate()?;
                output.crop.contributions.insert(PersonId(99), 1);
                Ok(vec![(inputs[0].0, output)])
            })
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn pure_growth_batches_are_identical_under_threaded_evaluation_and_zero_duration() {
    let mut world = world();
    for id in 8..24 {
        let mut field = world.field(SiteId(1)).unwrap().clone();
        field.id = SiteId(id);
        world.insert_field(field).unwrap();
        world
            .insert_crop(SiteId(id), world.crop(SiteId(1)).unwrap().clone())
            .unwrap();
        world
            .insert_soil(SiteId(id), world.soil(SiteId(1)).unwrap().clone())
            .unwrap();
    }
    world.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes: SimTime(60_000),
        },
    );
    let forcing = day(1, 10);
    let serial = world
        .prepare_advance_serial(SimTime(0), SimTime(0), Some(&forcing))
        .unwrap();
    let parallel = world
        .prepare_advance(SimTime(0), SimTime(0), Some(&forcing), |inputs| {
            std::thread::scope(|scope| {
                let handles: Vec<_> = inputs
                    .iter()
                    .map(|(id, input)| scope.spawn(move || Ok((*id, input.evaluate()?))))
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().unwrap())
                    .collect()
            })
        })
        .unwrap();
    let mut serial_world = world.clone();
    let mut parallel_world = world.clone();
    serial_world.commit_advance(serial).unwrap();
    parallel_world.commit_advance(parallel).unwrap();
    assert_eq!(serial_world, parallel_world);
    assert_eq!(
        parallel_world.crop(SiteId(1)).unwrap().contributions,
        BTreeMap::new()
    );
    assert_eq!(
        world.next_boundary(SimTime(0), SimTime(120_000)),
        SimTime(60_000)
    );
}

#[test]
fn native_component_allocation_order_clone_and_restore_do_not_change_growth_or_save() {
    let sample = world();
    let make = |reverse: bool| {
        let mut world = Fixture::new(sample.catalog.clone(), 0).unwrap();
        let ids = if reverse {
            [SiteId(8), SiteId(1)]
        } else {
            [SiteId(1), SiteId(8)]
        };
        for id in ids {
            let mut field = sample.field(SiteId(1)).unwrap().clone();
            field.id = id;
            world.insert_field(field).unwrap();
            world
                .insert_crop(id, sample.crop(SiteId(1)).unwrap().clone())
                .unwrap();
            world
                .insert_soil(id, sample.soil(SiteId(1)).unwrap().clone())
                .unwrap();
        }
        world
    };
    let first = make(false);
    let second = make(true);
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    let restored: Fixture = serde_json::from_slice(&serde_json::to_vec(&second).unwrap()).unwrap();
    assert_eq!(restored, first);
    let forcing = day(1, 10);
    let outcomes: Vec<_> = [first.clone(), second, restored]
        .into_iter()
        .map(|mut world| {
            let batch = world
                .prepare_advance(SimTime(0), SimTime(60_000), Some(&forcing), |inputs| {
                    assert_eq!(
                        inputs.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                        vec![SiteId(1), SiteId(8)]
                    );
                    inputs
                        .iter()
                        .map(|(id, input)| Ok((*id, input.evaluate()?)))
                        .collect()
                })
                .unwrap();
            world.commit_advance(batch).unwrap();
            world
        })
        .collect();
    assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn structural_edits_refresh_native_matches_and_invalid_insertions_are_inert() {
    let mut world = world();
    let original = world.clone();
    let soil = world.remove_soil(SiteId(1)).unwrap().unwrap();
    let missing = world
        .prepare_advance_serial(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(missing.growth_rows(), 0);
    world.insert_soil(SiteId(1), soil).unwrap();
    let complete = world
        .prepare_advance_serial(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(complete.growth_rows(), 1);
    assert_eq!(world, original);
    let before = serde_json::to_vec(&world).unwrap();
    assert!(
        world
            .insert_soil(
                SiteId(99),
                Soil {
                    fertility: 1_001,
                    water: 0,
                    capacity: 1
                }
            )
            .is_err()
    );
    assert!(
        world
            .insert_crop(SiteId(99), Crop::new(CropId("missing".into())))
            .is_err()
    );
    assert!(
        world
            .insert_field(Field {
                id: SiteId(0),
                ..world.field(SiteId(1)).unwrap().clone()
            })
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
}

#[test]
fn dense_growth_and_sparse_labor_merge_once_in_stable_order() {
    let mut world = world();
    world.crop_mut(SiteId(1)).unwrap().tending = 0;
    for id in [SiteId(8), SiteId(3)] {
        let mut field = world.field(SiteId(1)).unwrap().clone();
        field.id = id;
        if id == SiteId(8) {
            field.area = AreaId(9);
        }
        world.insert_field(field).unwrap();
        let mut crop = world.crop(SiteId(1)).unwrap().clone();
        if id == SiteId(8) {
            crop.phase = CropPhase::Planting;
            crop.planting_ms = 0;
        }
        world.insert_crop(id, crop).unwrap();
        world
            .insert_soil(id, world.soil(SiteId(1)).unwrap().clone())
            .unwrap();
    }
    world.jobs.insert(
        PersonId(3),
        Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Tend,
            labor_ms: 0,
            completes: SimTime(3_600_000),
        },
    );
    world.jobs.insert(
        PersonId(99),
        Cultivation {
            field: SiteId(8),
            operation: FarmOperation::Plant,
            labor_ms: 0,
            completes: SimTime(7_200_000),
        },
    );
    world.validate(SimTime(0), None).unwrap();
    let batch = world
        .prepare_advance_serial(SimTime(0), SimTime(60_000), Some(&day(1, 10)))
        .unwrap();
    assert_eq!(
        batch.changed_crops().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![SiteId(1), SiteId(3), SiteId(8)]
    );
    assert_eq!(batch.changed_crop_rows(), 3);
    assert_eq!(batch.changed_job_rows(), 2);
    assert_eq!(batch.growth_rows(), 2);
    let mut stale = world.clone();
    stale.soil_mut(SiteId(3)).unwrap().water = 0;
    let before = stale.clone();
    assert!(stale.commit_advance(batch.clone()).is_err());
    assert_eq!(stale, before);
    world.commit_advance(batch).unwrap();
    assert_eq!(
        world.crop(SiteId(1)).unwrap().contributions[&PersonId(3)],
        60_000
    );
    assert_eq!(
        world.crop(SiteId(8)).unwrap().contributions[&PersonId(99)],
        60_000
    );
    assert_eq!(world.crop(SiteId(8)).unwrap().planting_ms, 60_000);
    assert_eq!(world.crop(SiteId(8)).unwrap().degree_days_milli, 0);
    assert_eq!(world.crop(SiteId(3)).unwrap().degree_days_milli, 10_000);
    world.validate(SimTime(60_000), None).unwrap();
}

#[test]
fn shared_host_foreign_components_survive_agricultural_commits_and_bind_after_structure_changes() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ForeignTag {
        value: u64,
    }
    let fixture = world();
    let mut runtime = fixture.runtime.clone();
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<ForeignTag>("fixture/foreign-tag", |_| Ok(()))
        .unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    runtime
        .insert_field(&mut store, fixture.field(SiteId(1)).unwrap().clone())
        .unwrap();
    runtime
        .insert_crop(
            &mut store,
            SiteId(1),
            fixture.crop(SiteId(1)).unwrap().clone(),
        )
        .unwrap();
    runtime
        .insert_soil(
            &mut store,
            SiteId(1),
            fixture.soil(SiteId(1)).unwrap().clone(),
        )
        .unwrap();
    let mut queries = Runtime::bind(&mut store);
    store
        .insert(game_ecs::StableId(1), ForeignTag { value: 4 })
        .unwrap();
    assert!(
        runtime
            .prepare_advance_serial(
                &store,
                &queries,
                SimTime(0),
                SimTime(60_000),
                Some(&day(1, 10))
            )
            .is_err()
    );
    queries.refresh(&store).unwrap();
    let batch = runtime
        .prepare_advance_serial(
            &store,
            &queries,
            SimTime(0),
            SimTime(60_000),
            Some(&day(1, 10)),
        )
        .unwrap();
    store
        .get_mut::<ForeignTag>(game_ecs::StableId(1))
        .unwrap()
        .value = 8;
    runtime.validate_advance(&store, &batch).unwrap();
    runtime.commit_advance(&mut store, batch).unwrap();
    assert_eq!(
        store.get::<ForeignTag>(game_ecs::StableId(1)),
        Some(&ForeignTag { value: 8 })
    );
    assert_eq!(
        runtime.crop(&store, SiteId(1)).unwrap().degree_days_milli,
        10_000
    );
    let snapshot = store.snapshot().unwrap();
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<ForeignTag>("fixture/foreign-tag", |_| Ok(()))
        .unwrap();
    let mut restored = game_ecs::Store::restore(schema, snapshot).unwrap();
    let bound = Runtime::bind(&mut restored);
    let batch = runtime
        .prepare_advance_serial(
            &restored,
            &bound,
            SimTime(60_000),
            SimTime(120_000),
            Some(&day(2, 10)),
        )
        .unwrap();
    runtime.commit_advance(&mut restored, batch).unwrap();
    assert_eq!(
        runtime.crop(&restored, SiteId(1)).unwrap().phase,
        CropPhase::Ripe
    );
    assert_eq!(
        restored.get::<ForeignTag>(game_ecs::StableId(1)),
        Some(&ForeignTag { value: 8 })
    );
}

#[test]
fn missing_component_registration_cannot_leave_partial_entities() {
    let fixture = world();
    let runtime = &fixture.runtime;
    let mut store = game_ecs::Store::with_schema(game_ecs::Schema::new());
    let before = store.snapshot().unwrap();
    assert!(
        runtime
            .insert_field(&mut store, fixture.field(SiteId(1)).unwrap().clone())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(
        runtime
            .insert_crop(
                &mut store,
                SiteId(1),
                fixture.crop(SiteId(1)).unwrap().clone()
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(
        runtime
            .insert_soil(
                &mut store,
                SiteId(1),
                fixture.soil(SiteId(1)).unwrap().clone()
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn stricter_registered_agricultural_validators_cannot_leave_partial_entities() {
    let fixture = world();
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<Field>("agriculture/field", |_| Err(Error::Unavailable))
        .unwrap();
    schema
        .register::<Crop>("agriculture/crop", |_| Err(Error::Unavailable))
        .unwrap();
    schema
        .register::<Soil>("agriculture/soil", |_| Err(Error::Unavailable))
        .unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    let before = store.snapshot().unwrap();
    assert_eq!(
        fixture
            .runtime
            .insert_field(&mut store, fixture.field(SiteId(1)).unwrap().clone()),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(
        fixture.runtime.insert_crop(
            &mut store,
            SiteId(1),
            fixture.crop(SiteId(1)).unwrap().clone()
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(
        fixture.runtime.insert_soil(
            &mut store,
            SiteId(1),
            fixture.soil(SiteId(1)).unwrap().clone()
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
