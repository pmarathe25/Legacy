use super::*;

// Tests assemble an explicit host; the production runtime owns no store.
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
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_land::register_components(&mut schema).unwrap();
    schema
}
impl Fixture {
    fn job(&self, actor: PersonId) -> Option<&Gathering> {
        self.runtime.gathering(&self.store, actor)
    }
    fn insert_job(&mut self, actor: PersonId, job: Gathering) {
        self.runtime
            .insert_gathering(&mut self.store, actor, job)
            .unwrap();
        self.queries.refresh(&self.store).unwrap();
    }
    fn job_rows(&self) -> Vec<(PersonId, Gathering)> {
        let mut rows: Vec<_> = self
            .runtime
            .gatherings(&self.store)
            .map(|(actor, row)| (actor, row.clone()))
            .collect();
        rows.sort_by_key(|(actor, _)| *actor);
        rows
    }
    fn knowledge_rows(&self) -> Vec<(PersonId, Knowledge)> {
        let mut rows: Vec<_> = self
            .runtime
            .all_knowledge(&self.store)
            .map(|(actor, row)| (actor, row.clone()))
            .collect();
        rows.sort_by_key(|(actor, _)| *actor);
        rows
    }
    fn known_mut(&mut self, actor: PersonId) -> &mut BTreeMap<ResourceId, ResourceView> {
        if self.runtime.knowledge(&self.store, actor).is_none() {
            self.runtime
                .insert_knowledge(&mut self.store, actor, Knowledge::default())
                .unwrap();
            self.queries.refresh(&self.store).unwrap();
        }
        &mut self
            .runtime
            .knowledge_mut(&mut self.store, actor)
            .unwrap()
            .known_resources
    }
    fn insert_known(&mut self, actor: PersonId, known: BTreeMap<ResourceId, ResourceView>) {
        let mut row = self
            .runtime
            .knowledge(&self.store, actor)
            .cloned()
            .unwrap_or_default();
        row.known_resources = known;
        self.runtime
            .insert_knowledge(&mut self.store, actor, row)
            .unwrap();
        self.queries.refresh(&self.store).unwrap();
    }
    fn requests_mut(&mut self, actor: PersonId) -> &mut BTreeMap<ResourceId, SimTime> {
        if self.runtime.requests(&self.store, actor).is_none() {
            self.runtime
                .insert_requests(&mut self.store, actor, Requests::default())
                .unwrap();
            self.queries.refresh(&self.store).unwrap();
        }
        &mut self
            .runtime
            .requests_mut(&mut self.store, actor)
            .unwrap()
            .next_request
    }
    fn observe_area(&mut self, actor: PersonId, area: AreaId, now: SimTime) {
        self.runtime
            .observe_area(&mut self.store, actor, area, now)
            .unwrap();
        self.queries.refresh(&self.store).unwrap();
    }
    fn acquisition_prospects(
        &self,
        need: AcquisitionNeed,
        local: &[LocalAcquisition],
    ) -> Vec<AcquisitionProspect> {
        self.runtime.acquisition_prospects(&self.store, need, local)
    }
    fn exploration_areas(
        &self,
        actor: PersonId,
        area: AreaId,
        now: SimTime,
        known_areas: &[AreaId],
    ) -> Vec<(AreaId, SimTime)> {
        self.runtime
            .exploration_areas(&self.store, actor, area, now, known_areas)
    }
    fn new(rules: EcologyRules, day: u64) -> Result<Self, Error> {
        let runtime = Runtime::new(rules, day)?;
        let mut store = game_ecs::Store::with_schema(schema());
        let queries = Runtime::bind(&mut store);
        Ok(Self {
            runtime,
            store,
            queries,
        })
    }
    fn resources(&self) -> impl Iterator<Item = (ResourceId, &Resource)> {
        self.runtime.resources(&self.store)
    }
    fn plots(&self) -> impl Iterator<Item = (SiteId, &Plot)> {
        self.runtime.plots(&self.store)
    }
    fn resource(&self, id: ResourceId) -> Option<&Resource> {
        self.runtime.resource(&self.store, id)
    }
    fn plot(&self, id: SiteId) -> Option<&Plot> {
        self.runtime.plot(&self.store, id)
    }
    fn soil(&self, id: SiteId) -> Option<&Soil> {
        self.runtime.soil(&self.store, id)
    }
    fn resource_mut(&mut self, id: ResourceId) -> Option<&mut Resource> {
        self.runtime.resource_mut(&mut self.store, id)
    }
    fn insert_resource(&mut self, resource: Resource) -> Result<(), Error> {
        self.runtime.insert_resource(&mut self.store, resource)?;
        self.queries.refresh(&self.store)
    }
    fn insert_plot(&mut self, plot: Plot, soil: Soil) -> Result<(), Error> {
        self.runtime.insert_plot_soil(&mut self.store, plot, soil)?;
        self.queries.refresh(&self.store)
    }
    fn remove_resource(&mut self, id: ResourceId) -> Result<Option<Resource>, Error> {
        let row = self.runtime.remove_resource(&mut self.store, id)?;
        self.queries.refresh(&self.store)?;
        Ok(row)
    }
    fn remove_plot(&mut self, id: SiteId) -> Result<Option<Plot>, Error> {
        let row = self.runtime.remove_plot(&mut self.store, id)?;
        self.queries.refresh(&self.store)?;
        Ok(row)
    }
    fn gather_upper_bound(
        &self,
        actor: PersonId,
        resource: ResourceId,
        facts: GatherFacts,
    ) -> Result<Quantity, Error> {
        self.runtime
            .gather_upper_bound(&self.store, actor, resource, facts)
    }
    fn gathering_conditions(
        &self,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
        facts: GatherFacts,
    ) -> Result<GatheringConditions, Error> {
        self.runtime.gathering_conditions(
            &self.store,
            actor,
            resource,
            quantity,
            unauthorized,
            facts,
        )
    }
    fn prepare_gather(
        &self,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
        facts: GatherFacts,
    ) -> Result<GatheringPlan, Error> {
        self.runtime
            .prepare_gather(&self.store, actor, resource, quantity, unauthorized, facts)
    }
    fn prepare_advance_serial(
        &self,
        day: Option<&LocalWeatherDay>,
    ) -> Result<PreparedAdvance, Error> {
        self.runtime
            .prepare_advance_serial(&self.store, &self.queries, day)
    }
    fn commit_advance(&mut self, prepared: PreparedAdvance) -> Result<(), Error> {
        self.runtime.commit_advance(&mut self.store, prepared)
    }
    fn advance(&self, day: Option<&LocalWeatherDay>) -> Result<Self, Error> {
        let prepared = self.prepare_advance_serial(day)?;
        let mut future = self.clone();
        future.commit_advance(prepared)?;
        Ok(future)
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
struct SavedRef<'a> {
    runtime: &'a Runtime,
    components: game_ecs::Snapshot,
}
#[derive(Deserialize)]
struct Saved {
    runtime: Runtime,
    components: game_ecs::Snapshot,
}
impl Serialize for Fixture {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SavedRef {
            runtime: &self.runtime,
            components: self.store.snapshot().map_err(serde::ser::Error::custom)?,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Fixture {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = Saved::deserialize(deserializer)?;
        let mut store = game_ecs::Store::restore(schema(), saved.components)
            .map_err(serde::de::Error::custom)?;
        let queries = Runtime::bind(&mut store);
        Ok(Self {
            runtime: saved.runtime,
            store,
            queries,
        })
    }
}

fn world(kind: ResourceKind, remaining: u64) -> Fixture {
    let source = Resource {
        id: ResourceId(1),
        area: AreaId(2),
        kind,
        remaining: Quantity(remaining),
        capacity: Quantity(20_000),
        daily_growth: Quantity(250),
        growth_remainder: 0,
        decay_permille: 0,
        soil_water: 75,
        water_capacity: 120,
        optimal_water: 50,
        minimum_temperature: 0,
        optimum_temperature: 20,
        contamination: 0,
        runoff_contamination: 0,
        labor_per_unit: SimDuration(60_000),
        workers: 2,
        access: BTreeSet::from([PersonId(3), PersonId(4), PersonId(5)]),
        custodian: Some(PersonId(3)),
    };
    let plot = Plot {
        id: SiteId(9),
        area: AreaId(2),
        access: BTreeSet::from([PersonId(3)]),
        custodian: Some(PersonId(3)),
    };
    let mut world = Fixture::new(EcologyRules::default(), 0).unwrap();
    world.insert_resource(source).unwrap();
    world
        .insert_plot(
            plot,
            Soil {
                fertility: 900,
                water: 75,
                capacity: 120,
            },
        )
        .unwrap();
    world
}
fn day(number: u64, temp: i32) -> LocalWeatherDay {
    LocalWeatherDay {
        day: number,
        day_of_year: number - 1,
        cells: BTreeMap::from([(
            AreaId(2),
            WeatherFacts {
                temperature: temp,
                rain: 30,
                evaporation: 0,
            },
        )]),
    }
}
fn facts() -> GatherFacts {
    GatherFacts {
        now: SimTime(10),
        area: AreaId(2),
        capable: true,
        adult: true,
        carrying_room: Quantity(5_000),
        tool: Some(InventoryId(6)),
    }
}
fn closing(now: SimTime) -> CollectionFacts {
    CollectionFacts {
        now,
        area: AreaId(2),
        capable: true,
        cargo_available: true,
        tool_available: true,
    }
}
#[test]
fn renewal_is_finite_and_changes_no_custody_permissions_or_personal_knowledge() {
    let world = world(ResourceKind::Coppice, 0);
    let before = world.clone();
    let next = world.advance(Some(&day(1, 20))).unwrap();
    assert_eq!(
        next.resource(ResourceId(1)).unwrap().remaining,
        Quantity(250)
    );
    assert_eq!(next.soil(SiteId(9)).unwrap().fertility, 901);
    assert_eq!(
        next.resource(ResourceId(1)).unwrap().access,
        world.resource(ResourceId(1)).unwrap().access
    );
    assert!(next.knowledge_rows().is_empty());
    assert_eq!(world, before);
    let cold = world.advance(Some(&day(1, -5))).unwrap();
    assert_eq!(cold.resource(ResourceId(1)).unwrap().remaining, Quantity(0));
    assert_eq!(cold.soil(SiteId(9)).unwrap().fertility, 900);
    assert_eq!(next.advance(Some(&day(1, 20))).unwrap(), next);
}
#[test]
fn land_recovery_configuration_is_independent_of_crop_rules() {
    let mut world = world(ResourceKind::Herbs, 0);
    world.rules = EcologyRules {
        base_temperature: 30,
        soil_recovery: 7,
        ..EcologyRules::default()
    };
    let cool = world.advance(Some(&day(1, 20))).unwrap();
    assert_eq!(cool.soil(SiteId(9)).unwrap().fertility, 900);
    let warm = world.advance(Some(&day(1, 40))).unwrap();
    assert_eq!(warm.soil(SiteId(9)).unwrap().fertility, 907);
}

#[test]
fn renewal_and_soil_match_local_weather_without_a_global_fallback() {
    let mut world = world(ResourceKind::Berries, 1_000);
    let mut cold_source = world.resource(ResourceId(1)).unwrap().clone();
    cold_source.id = ResourceId(2);
    cold_source.area = AreaId(3);
    let mut missing_source = cold_source.clone();
    missing_source.id = ResourceId(3);
    missing_source.area = AreaId(4);
    missing_source.decay_permille = 500;
    missing_source.growth_remainder = 125_000;
    world.insert_resource(cold_source).unwrap();
    world.insert_resource(missing_source.clone()).unwrap();
    let mut cold_plot = world.plot(SiteId(9)).unwrap().clone();
    cold_plot.id = SiteId(10);
    cold_plot.area = AreaId(3);
    let mut missing_plot = cold_plot.clone();
    missing_plot.id = SiteId(11);
    missing_plot.area = AreaId(4);
    world
        .insert_plot(cold_plot, world.soil(SiteId(9)).unwrap().clone())
        .unwrap();
    world
        .insert_plot(missing_plot.clone(), world.soil(SiteId(9)).unwrap().clone())
        .unwrap();

    let mut local = day(1, 20);
    local.cells.insert(
        AreaId(3),
        WeatherFacts {
            temperature: -10,
            rain: 0,
            evaporation: 5,
        },
    );
    let next = world.advance(Some(&local)).unwrap();
    assert_eq!(
        next.resource(ResourceId(1)).unwrap().remaining,
        Quantity(1_250)
    );
    assert_eq!(
        next.resource(ResourceId(2)).unwrap().remaining,
        Quantity(1_000)
    );
    assert_eq!(next.resource(ResourceId(2)).unwrap().soil_water, 70);
    assert_eq!(next.soil(SiteId(9)).unwrap().fertility, 901);
    assert_eq!(next.soil(SiteId(10)).unwrap().fertility, 900);
    assert_eq!(next.soil(SiteId(10)).unwrap().water, 70);
    assert_eq!(next.resource(ResourceId(3)).unwrap(), &missing_source);
    assert_eq!(next.plot(SiteId(11)).unwrap(), &missing_plot);
    assert_eq!(next.soil(SiteId(11)), world.soil(SiteId(11)));
    assert_eq!(next.day, 1);
}

#[test]
fn missing_weather_preserves_land_and_finite_gathering_still_completes() {
    let mut world = world(ResourceKind::Spring, 2_000);
    world.resource_mut(ResourceId(1)).unwrap().contamination = 250;
    let plan = world
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(500), false, facts())
        .unwrap();
    world.insert_job(PersonId(3), plan.job(InventoryId(7)));
    assert_eq!(world.advance(None).unwrap(), world);
    let no_cells = LocalWeatherDay {
        cells: BTreeMap::new(),
        ..day(1, 20)
    };
    let next = world.advance(Some(&no_cells)).unwrap();
    assert_eq!(next.day, 1);
    assert_eq!(
        next.resources().collect::<Vec<_>>(),
        world.resources().collect::<Vec<_>>()
    );
    assert_eq!(
        next.plots().collect::<Vec<_>>(),
        world.plots().collect::<Vec<_>>()
    );
    assert_eq!(next.job_rows(), world.job_rows());
    for (id, _) in world.plots() {
        assert_eq!(next.soil(id), world.soil(id));
    }
    let effect = prepare_collection(
        next.resource(ResourceId(1)).unwrap(),
        PersonId(3),
        next.job(PersonId(3)).unwrap(),
        closing(plan.completes),
    )
    .unwrap();
    assert_eq!(effect.quantity, Quantity(500));
    assert_eq!(effect.remaining, Quantity(1_500));
    assert_eq!(effect.contamination, Some(250));
    assert_eq!(next.advance(Some(&day(2, 20))).unwrap().day, 2);
}

#[test]
fn invalid_local_cell_rejects_the_whole_day_without_stock_or_soil_changes() {
    let world = world(ResourceKind::Berries, 1_000);
    let before = world.clone();
    let mut local = day(1, 20);
    local.cells.insert(
        AreaId(999),
        WeatherFacts {
            temperature: i32::MAX,
            rain: 0,
            evaporation: 0,
        },
    );
    assert!(world.advance(Some(&local)).is_err());
    assert_eq!(world, before);
    local.cells.get_mut(&AreaId(999)).unwrap().temperature = 20;
    local.cells.get_mut(&AreaId(999)).unwrap().rain = u64::MAX;
    assert!(world.advance(Some(&local)).is_err());
    assert_eq!(world, before);
}

#[test]
fn independent_entity_updates_equal_a_batch_regardless_of_insertion_order() {
    let first = world(ResourceKind::Berries, 1_000);
    let mut second = world(ResourceKind::Herbs, 500);
    let mut source = second.remove_resource(ResourceId(1)).unwrap().unwrap();
    source.id = ResourceId(2);
    source.area = AreaId(3);
    second.insert_resource(source).unwrap();
    let mut plot = second.remove_plot(SiteId(9)).unwrap().unwrap();
    plot.id = SiteId(10);
    plot.area = AreaId(3);
    let soil = second.soil(SiteId(9)).unwrap().clone();
    game_land::remove_soil(&mut second.store, SiteId(9)).unwrap();
    second.insert_plot(plot, soil).unwrap();
    let mut local = day(1, 20);
    local.cells.insert(
        AreaId(3),
        WeatherFacts {
            temperature: 10,
            rain: 5,
            evaporation: 12,
        },
    );
    let mut combined = first.clone();
    for (_, resource) in second.resources() {
        combined.insert_resource(resource.clone()).unwrap();
    }
    for (id, plot) in second.plots() {
        combined
            .insert_plot(plot.clone(), second.soil(id).unwrap().clone())
            .unwrap();
    }
    let mut reversed = Fixture::new(combined.rules.clone(), 0).unwrap();
    for (_, resource) in combined.resources().collect::<Vec<_>>().into_iter().rev() {
        reversed.insert_resource(resource.clone()).unwrap();
    }
    for (id, plot) in combined.plots().collect::<Vec<_>>().into_iter().rev() {
        reversed
            .insert_plot(plot.clone(), combined.soil(id).unwrap().clone())
            .unwrap();
    }
    let mut reversed_local = local.clone();
    reversed_local.cells = local.cells.clone().into_iter().rev().collect();
    let batch = combined.advance(Some(&local)).unwrap();
    assert_eq!(reversed.advance(Some(&reversed_local)).unwrap(), batch);
    let first_result = first.advance(Some(&local)).unwrap();
    let second_result = second.advance(Some(&local)).unwrap();
    assert_eq!(batch.soil(SiteId(9)), first_result.soil(SiteId(9)));
    assert_eq!(batch.soil(SiteId(10)), second_result.soil(SiteId(10)));
    assert_eq!(
        batch.resource(ResourceId(1)).unwrap(),
        first_result.resource(ResourceId(1)).unwrap()
    );
    assert_eq!(
        batch.resource(ResourceId(2)).unwrap(),
        second_result.resource(ResourceId(2)).unwrap()
    );
    assert_eq!(
        batch.plot(SiteId(9)).unwrap(),
        first_result.plot(SiteId(9)).unwrap()
    );
    assert_eq!(
        batch.plot(SiteId(10)).unwrap(),
        second_result.plot(SiteId(10)).unwrap()
    );
}
#[test]
fn actual_reservations_worker_slots_and_cargo_limits_bound_collection_plans() {
    let mut world = world(ResourceKind::Spring, 2_000);
    let first = world
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    assert_eq!(first.good, Good::Water);
    assert!(first.tool.is_none());
    assert_eq!(first.completes, SimTime(60_010));
    world.insert_job(PersonId(3), first.job(InventoryId(7)));
    let second = world
        .prepare_gather(PersonId(4), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    assert_eq!(
        world.prepare_gather(PersonId(4), ResourceId(1), Quantity(2_000), false, facts()),
        Err(Error::Unavailable)
    );
    world.insert_job(PersonId(4), second.job(InventoryId(8)));
    let before = world.clone();
    assert_eq!(
        world.prepare_gather(PersonId(5), ResourceId(1), Quantity(1_000), false, facts()),
        Err(Error::Unavailable)
    );
    assert_eq!(world, before);
    assert_eq!(
        world.prepare_gather(
            PersonId(3),
            ResourceId(1),
            Quantity(1_000),
            false,
            GatherFacts {
                carrying_room: Quantity(999),
                ..facts()
            }
        ),
        Err(Error::Unavailable)
    );
}
#[test]
fn capable_child_can_collect_small_herbs_but_needs_maturity_and_tool_for_timber() {
    let child = GatherFacts {
        adult: false,
        tool: None,
        ..facts()
    };
    let herbs = world(ResourceKind::Herbs, 4_000);
    herbs
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, child)
        .unwrap();
    assert_eq!(
        herbs.prepare_gather(PersonId(3), ResourceId(1), Quantity(2_000), false, child),
        Err(Error::Unavailable)
    );
    let forest = world(ResourceKind::Coppice, 4_000);
    assert_eq!(
        forest.prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, child),
        Err(Error::Unavailable)
    );
    assert_eq!(
        forest.prepare_gather(
            PersonId(3),
            ResourceId(1),
            Quantity(1_000),
            false,
            GatherFacts {
                tool: None,
                ..facts()
            }
        ),
        Err(Error::Unavailable)
    );
}
#[test]
fn permission_is_distinct_from_physical_possibility_and_closing_refusal_moves_no_goods() {
    let mut world = world(ResourceKind::Coppice, 2_000);
    world
        .resource_mut(ResourceId(1))
        .unwrap()
        .access
        .remove(&PersonId(4));
    assert_eq!(
        world.prepare_gather(PersonId(4), ResourceId(1), Quantity(1_000), false, facts()),
        Err(Error::Unavailable)
    );
    let attempt = world
        .prepare_gather(PersonId(4), ResourceId(1), Quantity(1_000), true, facts())
        .unwrap();
    let job = attempt.job(InventoryId(7));
    let before = world.clone();
    assert_eq!(
        prepare_collection(
            world.resource(ResourceId(1)).unwrap(),
            PersonId(4),
            &job,
            CollectionFacts {
                tool_available: false,
                ..closing(job.completes)
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(
        prepare_collection(
            world.resource(ResourceId(1)).unwrap(),
            PersonId(4),
            &job,
            CollectionFacts {
                capable: false,
                ..closing(job.completes)
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(
        prepare_collection(
            world.resource(ResourceId(1)).unwrap(),
            PersonId(4),
            &job,
            closing(SimTime(job.completes.0 - 1))
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(world, before);
    let effect = prepare_collection(
        world.resource(ResourceId(1)).unwrap(),
        PersonId(4),
        &job,
        closing(job.completes),
    )
    .unwrap();
    assert_eq!(
        effect.remaining.0 + effect.quantity.0,
        world.resource(ResourceId(1)).unwrap().remaining.0
    );
    assert_eq!(effect.destination, InventoryId(7));
    assert!(effect.contamination.is_none());
}
#[test]
fn water_quality_effect_uses_committed_renewed_source_without_owning_health_lots() {
    let mut world = world(ResourceKind::Spring, 1_000);
    let source = world.resource_mut(ResourceId(1)).unwrap();
    source.contamination = 1_000;
    source.daily_growth = Quantity(1_000);
    let renewed = world.advance(Some(&day(1, 20))).unwrap();
    assert_eq!(
        renewed.resource(ResourceId(1)).unwrap().remaining,
        Quantity(2_000)
    );
    assert_eq!(renewed.resource(ResourceId(1)).unwrap().contamination, 500);
    let plan = renewed
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    let effect = prepare_collection(
        renewed.resource(ResourceId(1)).unwrap(),
        PersonId(3),
        &plan.job(InventoryId(7)),
        closing(plan.completes),
    )
    .unwrap();
    assert_eq!(effect.contamination, Some(500));
    assert_eq!(effect.remaining, Quantity(1_000));
}
#[test]
fn current_restore_pending_jobs_and_fractional_renewal_continue_without_rerolling() {
    let mut world = world(ResourceKind::Berries, 1_000);
    let source = world.resource_mut(ResourceId(1)).unwrap();
    source.daily_growth = Quantity(1);
    source.soil_water = 1;
    let plan = world
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, facts())
        .unwrap();
    world.insert_job(PersonId(3), plan.job(InventoryId(7)));
    let first = world
        .advance(Some(&LocalWeatherDay {
            cells: BTreeMap::from([(
                AreaId(2),
                WeatherFacts {
                    temperature: 10,
                    rain: 0,
                    evaporation: 0,
                },
            )]),
            ..day(1, 10)
        }))
        .unwrap();
    assert!(first.resource(ResourceId(1)).unwrap().growth_remainder > 0);
    let loaded: Fixture = serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
    assert_eq!(
        first.advance(Some(&day(2, 20))).unwrap(),
        loaded.advance(Some(&day(2, 20))).unwrap()
    );
    assert_eq!(
        prepare_collection(
            first.resource(ResourceId(1)).unwrap(),
            PersonId(3),
            first.job(PersonId(3)).unwrap(),
            closing(plan.completes)
        )
        .unwrap(),
        prepare_collection(
            loaded.resource(ResourceId(1)).unwrap(),
            PersonId(3),
            loaded.job(PersonId(3)).unwrap(),
            closing(plan.completes)
        )
        .unwrap()
    );
}
#[test]
fn malformed_or_overflowing_inputs_fail_without_partial_renewal_or_reservation() {
    let mut world = world(ResourceKind::Spring, 2_000);
    let before = world.clone();
    assert!(world.advance(Some(&day(2, 20))).is_err());
    assert_eq!(world, before);
    world.resource_mut(ResourceId(1)).unwrap().optimal_water = 0;
    assert!(world.advance(Some(&day(1, 20))).is_err());
    let source = world.resource_mut(ResourceId(1)).unwrap();
    source.optimal_water = 50;
    source.capacity = Quantity(u64::MAX);
    source.remaining = Quantity(u64::MAX);
    source.labor_per_unit = SimDuration(u64::MAX);
    assert_eq!(
        world.prepare_gather(
            PersonId(3),
            ResourceId(1),
            Quantity(u64::MAX),
            false,
            GatherFacts {
                carrying_room: Quantity(u64::MAX),
                ..facts()
            }
        ),
        Err(Error::Overflow)
    );
    assert!(world.job_rows().is_empty());
}

#[test]
fn exploration_requires_actual_inspection_and_retries_a_stale_negative_observation() {
    let mut world = world(ResourceKind::Spring, 1_000);
    let areas = [AreaId(2), AreaId(3)];
    assert!(
        world
            .exploration_areas(PersonId(3), AreaId(2), SimTime(0), &areas)
            .is_empty()
    );
    // A dated received resource view does not constitute physical inspection.
    world.insert_known(
        PersonId(3),
        BTreeMap::from([(
            ResourceId(1),
            ResourceView {
                id: ResourceId(1),
                area: AreaId(2),
                kind: ResourceKind::Spring,
                available: Quantity(1_000),
                accessible: true,
                custodian: None,
                contamination: 0,
                observed_at: SimTime(0),
            },
        )]),
    );
    assert!(
        world
            .exploration_areas(PersonId(3), AreaId(2), SimTime(0), &areas)
            .is_empty()
    );
    world.observe_area(PersonId(3), AreaId(2), SimTime(0));
    assert_eq!(
        world.exploration_areas(PersonId(3), AreaId(2), SimTime(0), &areas),
        vec![(AreaId(3), SimTime(0))]
    );
    world.observe_area(PersonId(3), AreaId(3), SimTime(10));
    assert!(
        world
            .exploration_areas(PersonId(3), AreaId(2), SimTime(11), &areas)
            .is_empty()
    );
    assert_eq!(
        world.exploration_areas(
            PersonId(3),
            AreaId(2),
            SimTime(10 + world.rules.search_retry.0),
            &areas
        ),
        vec![(AreaId(3), SimTime(0))]
    );
    for id in 4..25 {
        world.observe_area(PersonId(3), AreaId(id), SimTime(id));
    }
    assert_eq!(
        world
            .runtime
            .knowledge(&world.store, PersonId(3))
            .unwrap()
            .observed_areas
            .len(),
        16
    );
}
#[test]
fn acquisition_prospects_use_only_personal_views_and_local_admission_for_every_resource_good() {
    for kind in [
        ResourceKind::Coppice,
        ResourceKind::Berries,
        ResourceKind::Herbs,
        ResourceKind::Spring,
    ] {
        let mut world = world(kind, 1_000);
        let need = AcquisitionNeed {
            actor: PersonId(3),
            area: AreaId(2),
            now: SimTime(0),
            good: kind.good(),
            quantity: Quantity(500),
            room: Quantity(1_000),
        };
        assert!(world.acquisition_prospects(need, &[]).is_empty());
        world.insert_known(
            PersonId(3),
            BTreeMap::from([(
                ResourceId(1),
                ResourceView {
                    id: ResourceId(1),
                    area: AreaId(2),
                    kind,
                    available: Quantity(1_000),
                    accessible: true,
                    custodian: Some(PersonId(4)),
                    contamination: 0,
                    observed_at: SimTime(0),
                },
            )]),
        );
        assert!(world.acquisition_prospects(need, &[]).is_empty());
        let local = [LocalAcquisition {
            resource: ResourceId(1),
            can_prepare: true,
            can_request: false,
        }];
        let offers = world.acquisition_prospects(need, &local);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].quantity, Quantity(500));
        assert_eq!(offers[0].needs_tool, kind == ResourceKind::Coppice);
        world
            .known_mut(PersonId(3))
            .get_mut(&ResourceId(1))
            .unwrap()
            .area = AreaId(3);
        assert_eq!(world.acquisition_prospects(need, &[]).len(), 1);
        assert!(
            world
                .acquisition_prospects(
                    AcquisitionNeed {
                        actor: PersonId(5),
                        ..need
                    },
                    &[]
                )
                .is_empty()
        );
    }
}

#[test]
fn conditional_operator_retains_missing_carried_equipment_but_actual_gather_revalidates_it() {
    let world = world(ResourceKind::Coppice, 2_000);
    let no_tool = GatherFacts {
        tool: None,
        ..facts()
    };
    let conditions = world
        .gathering_conditions(PersonId(3), ResourceId(1), Quantity(1_000), false, no_tool)
        .unwrap();
    assert!(conditions.tool_required);
    assert!(!conditions.ready);
    assert_eq!(
        world.prepare_gather(PersonId(3), ResourceId(1), Quantity(1_000), false, no_tool),
        Err(Error::Unavailable)
    );
    assert!(
        world
            .gathering_conditions(
                PersonId(3),
                ResourceId(1),
                Quantity(1_000),
                false,
                GatherFacts {
                    capable: false,
                    ..no_tool
                }
            )
            .is_err()
    );
    assert!(
        world
            .gathering_conditions(
                PersonId(3),
                ResourceId(1),
                Quantity(1_000),
                false,
                GatherFacts {
                    adult: false,
                    ..no_tool
                }
            )
            .is_err()
    );
    assert!(
        world
            .gathering_conditions(
                PersonId(3),
                ResourceId(1),
                Quantity(1_000),
                false,
                GatherFacts {
                    carrying_room: Quantity(999),
                    ..no_tool
                }
            )
            .is_err()
    );
    assert!(
        world
            .gathering_conditions(PersonId(9), ResourceId(1), Quantity(1_000), false, no_tool)
            .is_err()
    );
}

#[test]
fn requesting_known_resource_access_needs_no_material_carrying_room() {
    let mut world = world(ResourceKind::Spring, 1_000);
    world.insert_known(
        PersonId(3),
        BTreeMap::from([(
            ResourceId(1),
            ResourceView {
                id: ResourceId(1),
                area: AreaId(2),
                kind: ResourceKind::Spring,
                available: Quantity(1_000),
                accessible: false,
                custodian: Some(PersonId(4)),
                contamination: 0,
                observed_at: SimTime(0),
            },
        )]),
    );
    let need = AcquisitionNeed {
        actor: PersonId(3),
        area: AreaId(2),
        now: SimTime(0),
        good: Good::Water,
        quantity: Quantity(0),
        room: Quantity(0),
    };
    let prospects = world.acquisition_prospects(
        need,
        &[LocalAcquisition {
            resource: ResourceId(1),
            can_prepare: false,
            can_request: true,
        }],
    );
    assert_eq!(prospects.len(), 1);
    assert_eq!(prospects[0].quantity, Quantity(0));
    assert_eq!(
        prospects[0].action,
        EcologyAction::RequestAccess {
            resource: ResourceId(1)
        }
    );
    world
        .known_mut(PersonId(3))
        .get_mut(&ResourceId(1))
        .unwrap()
        .accessible = true;
    assert!(
        world
            .acquisition_prospects(
                need,
                &[LocalAcquisition {
                    resource: ResourceId(1),
                    can_prepare: true,
                    can_request: false
                }]
            )
            .is_empty()
    );
}

#[test]
fn source_offers_preserve_permission_and_unauthorized_alternatives_for_personal_judgment() {
    let mut world = world(ResourceKind::Spring, 1_000);
    world.insert_known(
        PersonId(3),
        BTreeMap::from([(
            ResourceId(1),
            ResourceView {
                id: ResourceId(1),
                area: AreaId(2),
                kind: ResourceKind::Spring,
                available: Quantity(1_000),
                accessible: false,
                custodian: Some(PersonId(4)),
                contamination: 0,
                observed_at: SimTime(0),
            },
        )]),
    );
    let offers = world.acquisition_prospects(
        AcquisitionNeed {
            actor: PersonId(3),
            area: AreaId(2),
            now: SimTime(0),
            good: Good::Water,
            quantity: Quantity(500),
            room: Quantity(500),
        },
        &[LocalAcquisition {
            resource: ResourceId(1),
            can_prepare: true,
            can_request: true,
        }],
    );
    assert_eq!(offers.len(), 2);
    assert_eq!(
        offers[0].action,
        EcologyAction::RequestAccess {
            resource: ResourceId(1)
        }
    );
    assert_eq!(
        offers[1].action,
        EcologyAction::Gather {
            resource: ResourceId(1),
            quantity: Quantity(500),
            unauthorized: true,
        }
    );
}

#[test]
fn gathering_ceiling_uses_actual_other_reservations_body_room_and_child_limit() {
    let mut world = world(ResourceKind::Spring, 750);
    let job = world
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(250), false, facts())
        .unwrap()
        .job(InventoryId(10));
    world.insert_job(PersonId(3), job);
    assert_eq!(
        world
            .gather_upper_bound(PersonId(4), ResourceId(1), facts())
            .unwrap(),
        Quantity(500)
    );
    assert_eq!(
        world
            .gather_upper_bound(PersonId(3), ResourceId(1), facts())
            .unwrap(),
        Quantity(750)
    );
    let small = GatherFacts {
        carrying_room: Quantity(100),
        ..facts()
    };
    assert_eq!(
        world
            .gather_upper_bound(PersonId(4), ResourceId(1), small)
            .unwrap(),
        Quantity(100)
    );
    let world = self::world(ResourceKind::Spring, 5000);
    let child = GatherFacts {
        adult: false,
        ..facts()
    };
    assert_eq!(
        world
            .gather_upper_bound(PersonId(4), ResourceId(1), child)
            .unwrap(),
        Quantity(1000)
    );
}

#[test]
fn gathering_physical_ceiling_is_not_gear_or_deadline_admission() {
    let world = world(ResourceKind::Coppice, 250);
    let no_tool = GatherFacts {
        tool: None,
        ..facts()
    };
    let limit = world
        .gather_upper_bound(PersonId(4), ResourceId(1), no_tool)
        .unwrap();
    assert_eq!(limit, Quantity(250));
    assert_eq!(
        world.prepare_gather(PersonId(4), ResourceId(1), limit, false, no_tool),
        Err(Error::Unavailable)
    );
    let too_late = GatherFacts {
        now: SimTime(u64::MAX - 100),
        ..facts()
    };
    assert_eq!(
        world.prepare_gather(PersonId(4), ResourceId(1), limit, false, too_late),
        Err(Error::Overflow)
    );
}

#[test]
fn no_day_prepares_no_rows_and_preserves_concurrent_runtime_metadata() {
    let mut world = world(ResourceKind::Herbs, 0);
    let batch = world
        .runtime
        .prepare_advance(
            &world.store,
            &world.queries,
            None,
            |_| panic!("resource renewal is not due"),
            |_| panic!("land recovery is not due"),
        )
        .unwrap();
    assert_eq!(batch.changed_resource_rows(), 0);
    assert_eq!(batch.changed_soil_rows(), 0);
    world.observe_area(PersonId(3), AreaId(2), SimTime(19));
    world
        .requests_mut(PersonId(3))
        .insert(ResourceId(1), SimTime(99));
    let before = world.clone();
    world.commit_advance(batch).unwrap();
    assert_eq!(world, before);
}

#[test]
fn sparse_daily_commit_keeps_gathering_knowledge_and_foreign_components() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ForeignTag(u64);
    let mut world = world(ResourceKind::Spring, 2_000);
    let mut schema = schema();
    schema
        .register::<ForeignTag>("fixture/tag", |_| Ok(()))
        .unwrap();
    let mut saved = world.store.snapshot().unwrap();
    saved.columns.insert("fixture/tag".into(), Vec::new());
    world.store = game_ecs::Store::restore(schema, saved).unwrap();
    world
        .store
        .insert(game_ecs::StableId(1), ForeignTag(5))
        .unwrap();
    world.queries = Runtime::bind(&mut world.store);
    let batch = world.prepare_advance_serial(Some(&day(1, 20))).unwrap();
    let job = world
        .prepare_gather(PersonId(3), ResourceId(1), Quantity(500), false, facts())
        .unwrap()
        .job(InventoryId(7));
    world.insert_job(PersonId(3), job.clone());
    world.observe_area(PersonId(3), AreaId(2), SimTime(19));
    world.known_mut(PersonId(3)).insert(
        ResourceId(1),
        ResourceView {
            id: ResourceId(1),
            area: AreaId(2),
            kind: ResourceKind::Spring,
            available: Quantity(2_000),
            accessible: true,
            custodian: Some(PersonId(3)),
            contamination: 0,
            observed_at: SimTime(19),
        },
    );
    world
        .store
        .get_mut::<ForeignTag>(game_ecs::StableId(1))
        .unwrap()
        .0 = 9;
    let known = world.knowledge_rows();
    world.commit_advance(batch).unwrap();
    assert_eq!(world.job(PersonId(3)), Some(&job));
    assert_eq!(world.knowledge_rows(), known);
    assert_eq!(
        world.resource(ResourceId(1)).unwrap().remaining,
        Quantity(2_250)
    );
    assert_eq!(
        world.store.get::<ForeignTag>(game_ecs::StableId(1)),
        Some(&ForeignTag(9))
    );
}

#[test]
fn personal_views_requests_and_gathering_are_independent_sparse_components() {
    let mut world = world(ResourceKind::Spring, 1_000);
    let known = ResourceView {
        id: ResourceId(1),
        area: AreaId(3),
        kind: ResourceKind::Spring,
        available: Quantity(1_000),
        accessible: false,
        custodian: Some(PersonId(4)),
        contamination: 0,
        observed_at: SimTime(0),
    };
    for actor in [PersonId(3), PersonId(4)] {
        world.insert_known(actor, BTreeMap::from([(known.id, known.clone())]));
    }
    world
        .runtime
        .insert_requests(
            &mut world.store,
            PersonId(3),
            Requests {
                next_request: BTreeMap::from([(ResourceId(1), SimTime(99))]),
            },
        )
        .unwrap();
    world.resource_mut(ResourceId(1)).unwrap().remaining = Quantity(0);
    let need = AcquisitionNeed {
        actor: PersonId(3),
        area: AreaId(2),
        now: SimTime(0),
        good: Good::Water,
        quantity: Quantity(500),
        room: Quantity(1_000),
    };
    let offers = world.acquisition_prospects(need, &[]);
    assert_eq!(offers.len(), 1);
    assert!(matches!(offers[0].action, EcologyAction::Gather { .. }));
    assert_eq!(
        offers[0].quantity,
        Quantity(500),
        "dated remote knowledge is not replaced by current resource state"
    );
    assert!(
        world
            .acquisition_prospects(
                AcquisitionNeed {
                    actor: PersonId(4),
                    ..need
                },
                &[]
            )
            .iter()
            .any(|offer| matches!(offer.action, EcologyAction::RequestAccess { .. }))
    );
    assert!(world.runtime.gathering(&world.store, PersonId(3)).is_none());
    let restored = game_ecs::Store::restore(schema(), world.store.snapshot().unwrap()).unwrap();
    assert_eq!(restored, world.store);
    let before = world.clone();
    assert!(
        world
            .runtime
            .insert_gathering(
                &mut world.store,
                PersonId(99),
                Gathering {
                    resource: ResourceId(1),
                    destination: InventoryId(7),
                    quantity: Quantity(0),
                    tool: None,
                    completes: SimTime(20),
                    unauthorized: false
                }
            )
            .is_err()
    );
    assert!(
        world
            .runtime
            .insert_requests(
                &mut world.store,
                PersonId(99),
                Requests {
                    next_request: BTreeMap::from([(ResourceId(0), SimTime(99))])
                }
            )
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn observation_schema_rejection_preserves_existing_personal_views_and_entity_membership() {
    let mut world = world(ResourceKind::Spring, 1_000);
    world.insert_known(PersonId(3), BTreeMap::new());
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<Resource>("ecology/resource", Resource::validate)
        .unwrap();
    schema
        .register::<Plot>("ecology/plot", Plot::validate)
        .unwrap();
    schema
        .register::<Gathering>("ecology/gathering", Gathering::validate)
        .unwrap();
    schema
        .register::<Requests>("ecology/requests", Requests::validate)
        .unwrap();
    schema
        .register::<Knowledge>("ecology/knowledge", |known| {
            known.validate()?;
            if !known.observed_areas.is_empty() {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    game_land::register_components(&mut schema).unwrap();
    world.store = game_ecs::Store::restore(schema, world.store.snapshot().unwrap()).unwrap();
    let before = world.clone();
    for actor in [PersonId(3), PersonId(99)] {
        assert_eq!(
            world
                .runtime
                .observe_area(&mut world.store, actor, AreaId(2), SimTime(10)),
            Err(Error::Unavailable)
        );
        assert_eq!(world, before);
    }
}

#[test]
fn rejection_of_daily_resource_or_land_rows_never_partially_publishes_the_other_rows_or_day() {
    for reject_resource in [false, true] {
        let mut world = world(ResourceKind::Spring, 2_000);
        world
            .insert_plot(
                Plot {
                    id: SiteId(9),
                    area: AreaId(2),
                    access: BTreeSet::new(),
                    custodian: None,
                },
                Soil {
                    fertility: 0,
                    water: 1_000,
                    capacity: 2_000,
                },
            )
            .unwrap();
        let mut schema = game_ecs::Schema::new();
        if reject_resource {
            schema
                .register::<Resource>("ecology/resource", |source| {
                    source.validate()?;
                    if source.remaining.0 > 2_000 {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
            game_land::register_components(&mut schema).unwrap();
        } else {
            schema
                .register::<Resource>("ecology/resource", Resource::validate)
                .unwrap();
            schema
                .register::<Soil>("land/soil", |soil| {
                    soil.validate()?;
                    if soil.fertility > 0 {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
        }
        schema
            .register::<Plot>("ecology/plot", Plot::validate)
            .unwrap();
        schema
            .register::<Gathering>("ecology/gathering", Gathering::validate)
            .unwrap();
        schema
            .register::<Knowledge>("ecology/knowledge", Knowledge::validate)
            .unwrap();
        schema
            .register::<Requests>("ecology/requests", Requests::validate)
            .unwrap();
        world.store = game_ecs::Store::restore(schema, world.store.snapshot().unwrap()).unwrap();
        world.queries = Runtime::bind(&mut world.store);
        let batch = world.prepare_advance_serial(Some(&day(1, 20))).unwrap();
        let before = world.clone();
        assert_eq!(world.commit_advance(batch), Err(Error::Unavailable));
        assert_eq!(world, before);
    }
}

#[test]
fn late_row_rules_day_and_membership_changes_reject_the_entire_batch() {
    let world = world(ResourceKind::Herbs, 0);
    let batch = world.prepare_advance_serial(Some(&day(1, 20))).unwrap();
    for mutation in 0..6 {
        let mut changed = world.clone();
        match mutation {
            0 => {
                changed
                    .runtime
                    .soil_mut(&mut changed.store, SiteId(9))
                    .unwrap()
                    .water = 0
            }
            1 => changed.rules.soil_recovery += 1,
            2 => changed.day += 1,
            3 => {
                changed.remove_plot(SiteId(9)).unwrap();
            }
            5 => changed
                .runtime
                .plot_mut(&mut changed.store, SiteId(9))
                .unwrap()
                .access
                .clear(),
            4 => {
                let mut added = changed.resource(ResourceId(1)).unwrap().clone();
                added.id = ResourceId(99);
                changed.insert_resource(added).unwrap();
            }
            _ => unreachable!(),
        }
        let before = changed.clone();
        assert!(changed.commit_advance(batch.clone()).is_err());
        assert_eq!(changed, before);
    }
}

#[test]
fn stable_native_inputs_support_parallel_pure_evaluation_after_save_restore() {
    let mut world = world(ResourceKind::Herbs, 0);
    for id in (2..18).rev() {
        let mut source = world.resource(ResourceId(1)).unwrap().clone();
        source.id = ResourceId(id);
        world.insert_resource(source).unwrap();
    }
    for id in (20..28).rev() {
        let mut plot = world.plot(SiteId(9)).unwrap().clone();
        plot.id = SiteId(id);
        world
            .insert_plot(plot, world.soil(SiteId(9)).unwrap().clone())
            .unwrap();
    }
    let restored: Fixture = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    let forcing = day(1, 20);
    let batch = restored
        .runtime
        .prepare_advance(
            &restored.store,
            &restored.queries,
            Some(&forcing),
            |rows| {
                std::thread::scope(|scope| {
                    assert!(rows.windows(2).all(|pair| pair[0].0 < pair[1].0));
                    let jobs: Vec<_> = rows
                        .iter()
                        .map(|(id, input)| scope.spawn(move || Ok((*id, input.evaluate()?))))
                        .collect();
                    jobs.into_iter().map(|job| job.join().unwrap()).collect()
                })
            },
            |rows| {
                std::thread::scope(|scope| {
                    assert!(rows.windows(2).all(|pair| pair[0].0 < pair[1].0));
                    let jobs: Vec<_> = rows
                        .iter()
                        .map(|(id, input)| scope.spawn(move || Ok((*id, input.evaluate()?))))
                        .collect();
                    jobs.into_iter().map(|job| job.join().unwrap()).collect()
                })
            },
        )
        .unwrap();
    let expected = world.advance(Some(&forcing)).unwrap();
    let mut parallel = restored;
    parallel.commit_advance(batch).unwrap();
    assert_eq!(parallel, expected);
}

#[test]
fn missing_schema_stricter_validation_and_foreign_executor_writes_are_inert() {
    let fixture = world(ResourceKind::Herbs, 0);
    let mut store = game_ecs::Store::with_schema(game_ecs::Schema::new());
    let before = store.snapshot().unwrap();
    assert!(
        fixture
            .runtime
            .insert_resource(&mut store, fixture.resource(ResourceId(1)).unwrap().clone())
            .is_err()
    );
    assert!(
        fixture
            .runtime
            .insert_plot_soil(
                &mut store,
                fixture.plot(SiteId(9)).unwrap().clone(),
                fixture.soil(SiteId(9)).unwrap().clone()
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<Resource>("ecology/resource", |_| Err(Error::Unavailable))
        .unwrap();
    schema
        .register::<Plot>("ecology/plot", |_| Err(Error::Unavailable))
        .unwrap();
    game_land::register_components(&mut schema).unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    let before = store.snapshot().unwrap();
    assert_eq!(
        fixture
            .runtime
            .insert_resource(&mut store, fixture.resource(ResourceId(1)).unwrap().clone()),
        Err(Error::Unavailable)
    );
    assert_eq!(
        fixture.runtime.insert_plot_soil(
            &mut store,
            fixture.plot(SiteId(9)).unwrap().clone(),
            fixture.soil(SiteId(9)).unwrap().clone()
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let before = fixture.clone();
    assert!(
        fixture
            .runtime
            .prepare_advance(
                &fixture.store,
                &fixture.queries,
                Some(&day(1, 20)),
                |rows| {
                    let mut output = rows[0].1.evaluate()?;
                    output.access.clear();
                    Ok(vec![(rows[0].0, output)])
                },
                |rows| rows
                    .iter()
                    .map(|(id, input)| Ok((*id, input.evaluate()?)))
                    .collect()
            )
            .is_err()
    );
    assert!(
        fixture
            .runtime
            .prepare_advance(
                &fixture.store,
                &fixture.queries,
                Some(&day(1, 20)),
                |rows| Ok(vec![(ResourceId(99), rows[0].1.evaluate()?)]),
                |rows| rows
                    .iter()
                    .map(|(id, input)| Ok((*id, input.evaluate()?)))
                    .collect()
            )
            .is_err()
    );
    assert_eq!(fixture, before);
}

#[test]
fn owned_validation_checks_component_identity_and_intrinsics_without_foreign_entities() {
    let world = world(ResourceKind::Herbs, 0);
    world.runtime.validate(&world.store).unwrap();
    let mut invalid = world.clone();
    invalid.resource_mut(ResourceId(1)).unwrap().id = ResourceId(99);
    assert!(invalid.runtime.validate(&invalid.store).is_err());
    let mut invalid = world.clone();
    invalid
        .runtime
        .plot_mut(&mut invalid.store, SiteId(9))
        .unwrap()
        .id = SiteId(99);
    assert!(invalid.runtime.validate(&invalid.store).is_err());
    let mut invalid = world.clone();
    invalid
        .runtime
        .soil_mut(&mut invalid.store, SiteId(9))
        .unwrap()
        .water = 121;
    assert!(invalid.runtime.validate(&invalid.store).is_err());
}

#[test]
fn plot_soil_is_registered_by_land_and_retained_when_land_use_changes() {
    let mut world = world(ResourceKind::Herbs, 0);
    let before = world.soil(SiteId(9)).unwrap().clone();
    let metadata = world.plot(SiteId(9)).unwrap().clone();
    assert!(!{
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        schema.is_registered::<Soil>()
    });
    world.remove_plot(SiteId(9)).unwrap();
    assert_eq!(world.soil(SiteId(9)), Some(&before));
    world
        .runtime
        .insert_plot(&mut world.store, metadata)
        .unwrap();
    world.queries.refresh(&world.store).unwrap();
    let batch = world.prepare_advance_serial(Some(&day(1, 20))).unwrap();
    assert_eq!(batch.changed_soil_rows(), 1);
    world.commit_advance(batch).unwrap();
    assert_eq!(world.soil(SiteId(9)).unwrap().fertility, 901);
    assert_eq!(world.soil(SiteId(9)).unwrap().water, 105);
}

#[test]
fn plot_insertion_requires_real_soil_and_pair_rejection_is_atomic() {
    let world = world(ResourceKind::Herbs, 0);
    let mut store = game_ecs::Store::with_schema(schema());
    let metadata = world.plot(SiteId(9)).unwrap().clone();
    let before = store.snapshot().unwrap();
    assert_eq!(
        world.runtime.insert_plot(&mut store, metadata.clone()),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(
        world
            .runtime
            .insert_plot_soil(
                &mut store,
                metadata.clone(),
                Soil {
                    capacity: 0,
                    ..world.soil(SiteId(9)).unwrap().clone()
                }
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let mut strict = game_ecs::Schema::new();
    register_components(&mut strict).unwrap();
    strict
        .register::<Soil>("land/soil", |_| Err(Error::Unavailable))
        .unwrap();
    let mut store = game_ecs::Store::with_schema(strict);
    let before = store.snapshot().unwrap();
    assert_eq!(
        world.runtime.insert_plot_soil(
            &mut store,
            metadata,
            world.soil(SiteId(9)).unwrap().clone()
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn missing_shared_soil_is_rejected_before_renewal() {
    let mut world = world(ResourceKind::Herbs, 0);
    game_land::remove_soil(&mut world.store, SiteId(9)).unwrap();
    world.queries.refresh(&world.store).unwrap();
    assert!(world.runtime.validate(&world.store).is_err());
    assert!(world.prepare_advance_serial(Some(&day(1, 20))).is_err());
}
