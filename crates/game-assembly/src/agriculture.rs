//! Agricultural adapter: factual inputs and atomic stock/event application.
use crate::*;
pub use game_agriculture::{
    Catalog, Crop, CropId, CropPhase, CropRules, Cultivation, Field, Runtime, Soil,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FarmingConfig {
    pub environment: game_environment::Config,
    pub agriculture: game_content::DefinitionSource<Catalog>,
}
pub fn default_catalog() -> game_content::DefinitionSource<Catalog> {
    catalog_source(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/agriculture/crops.toml"),
    )
}
pub fn catalog_source(
    path: impl Into<std::path::PathBuf>,
) -> game_content::DefinitionSource<Catalog> {
    game_content::DefinitionSource::File { path: path.into() }
}
impl Default for FarmingConfig {
    fn default() -> Self {
        Self {
            environment: game_environment::Config::default(),
            agriculture: default_catalog(),
        }
    }
}
impl FarmingConfig {
    pub fn validate(&self) -> Result<(), Error> {
        self.environment.validate()?;
        self.agriculture
            .resolve()?
            .data
            .validate(Some(self.environment.calendar.year_days()))
    }
}

impl Simulation {
    pub fn farming_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        Self::farming_with_config(seed, execution, FarmingConfig::default())
    }

    pub fn farming_with_config(
        seed: u64,
        execution: Execution,
        config: FarmingConfig,
    ) -> Result<Self, Error> {
        config.validate()?;
        let crop_document = config.agriculture.resolve()?;
        let mut sim = Self::autonomy_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "farming-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let day = 86_400_000;
        let rules = &mut sim.host.state.physical.as_mut().unwrap().rules;
        rules.hunger_harm = 3 * day;
        rules.meal_relief = day;
        rules.fatigue_limit = 16 * 3_600_000;
        rules.max_health = 2 * day;
        let actors: Vec<_> = game_physiology::bodies(&sim.host.storage)
            .map(|(actor, _)| actor)
            .collect();
        for actor in actors {
            game_physiology::body_mut(&mut sim.host.storage, actor)
                .unwrap()
                .vitals
                .health = 2 * day;
        }
        // Authoring an initial shared field is fixture data, not a government mode.
        // Remove the earlier timber-to-food recipe from this seasonal scenario.
        for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            sim.interrupt_actor_activity(actor);
            game_workshop::remove_job(&mut sim.host.storage, actor).map(|_| ())?;
        }
        let sites: Vec<_> = game_workshop::sites(&sim.host.storage)
            .map(|(id, _)| id)
            .collect();
        for site in sites {
            game_workshop::remove_site(&mut sim.host.storage, site)?;
        }
        let actors: Vec<_> =
            game_characters::decisions::minds::<game_api::Command>(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
        let owner_now = sim.time();
        for actor in actors {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap();
            mind.sites.clear();
            mind.plan = None;
            mind.retry_after = owner_now;
        }
        let development_rules = development::Rules::human();
        let lives = sim
            .people()
            .enumerate()
            .map(|(index, person)| {
                (
                    person.id,
                    development::Life {
                        id: person.id,
                        born: SimTime(0),
                        initial_age: development_rules.adulthood,
                        stage: LifeStage::Adult,
                        reproduction: if person.role == Role::Guardian {
                            development::Reproduction::Neither
                        } else if index % 2 == 0 {
                            development::Reproduction::SuppliesSeed
                        } else {
                            development::Reproduction::Gestates
                        },
                        parents: Vec::new(),
                        caregivers: BTreeSet::new(),
                        preference: None,
                    },
                )
            })
            .collect();
        sim.initialize_life_people(development_rules, lives)?;
        let farm = sim.host.state.areas[1].id;
        // This resident starts physically at the farm; later workers must travel.
        let farmer = sim.person_at(3).id;
        *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((farmer).0))
            .expect("location exists") = Location::AtArea(farm);
        let people = sim.people().map(|p| p.id).collect::<BTreeSet<_>>();
        let seed_stock = sim.debug_add_inventory(
            Good::Seed,
            InventoryLocation::Area(farm),
            Quantity(10_000),
            Quantity(1_000_000),
            people.clone(),
        )?;
        let tool = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(farm),
            Quantity(1_000),
            Quantity(1_000),
            people.clone(),
        )?;
        let water = sim.debug_add_inventory(
            Good::Water,
            InventoryLocation::Area(farm),
            Quantity(30_000),
            Quantity(1_000_000),
            people.clone(),
        )?;
        let food = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(farm),
            Quantity(3_000_000),
            Quantity(10_000_000),
            people.clone(),
        )?;
        for __stock_id in game_cargo::inventories(&sim.host.storage)
            .filter(|(_, i)| {
                i.good == Good::Food && matches!(i.location, InventoryLocation::Area(_))
            })
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let stock = game_cargo::inventory_mut(&mut sim.host.storage, __stock_id)
                .expect("inventory exists");
            stock.capacity = Quantity(10_000_000);
            stock.quantity = Quantity(3_000_000);
        }
        let id = SiteId(sim.host.allocate_id()?.0);
        sim.host.state.environment = Some(game_environment::Runtime::new(
            config.environment,
            sim.host.clock().time,
            seed,
        )?);
        sim.register_weather_cells()?;
        let agriculture = Runtime::from_content(
            crop_document,
            sim.host.state.environment.as_ref().unwrap().weather_day,
        )?;
        let crop_id = agriculture.catalog.default_crop.clone();
        agriculture
            .insert_field(
                &mut sim.host.storage,
                Field {
                    custodian: None,
                    id,
                    area: farm,
                    access: people,
                    seed: seed_stock,
                    tool,
                    water,
                    food,
                },
            )
            .expect("validated component insertion");
        agriculture
            .insert_crop(&mut sim.host.storage, id, Crop::new(crop_id))
            .expect("validated component insertion");
        agriculture
            .insert_soil(
                &mut sim.host.storage,
                id,
                Soil {
                    fertility: 900,
                    water: 75,
                    capacity: 120,
                },
            )
            .expect("validated component insertion");
        sim.host.state.agriculture = Some(agriculture);
        let private_tool = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(farm),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([sim.host.state.ruler]),
        )?;
        let private_field_id = SiteId(sim.host.allocate_id()?.0);
        let mut private_field = sim
            .host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .field(&sim.host.storage, id)
            .expect("component exists")
            .clone();
        private_field.id = private_field_id;
        private_field.tool = private_tool;
        private_field.access = BTreeSet::from([sim.host.state.ruler]);
        private_field.custodian = Some(sim.host.state.ruler);
        sim.host
            .state
            .agriculture
            .as_mut()
            .unwrap()
            .insert_field(&mut sim.host.storage, private_field)
            .expect("validated component insertion");
        let world = sim.host.state.agriculture.as_mut().unwrap();
        let private_crop = world
            .crop(&sim.host.storage, id)
            .expect("component exists")
            .clone();
        let private_soil = world
            .soil(&sim.host.storage, id)
            .expect("component exists")
            .clone();
        world
            .insert_crop(&mut sim.host.storage, private_field_id, private_crop)
            .expect("validated component insertion");
        world
            .insert_soil(&mut sim.host.storage, private_field_id, private_soil)
            .expect("validated component insertion");
        let crop = world.rules_for(&sim.host.storage, id)?;
        let food_capacity = 10_000_000.max(3_000_000 + crop.food_yield.0);
        let seed_capacity = 1_000_000.max(10_000 + crop.seed_yield.0);
        game_cargo::inventory_mut(&mut sim.host.storage, food)
            .unwrap()
            .capacity = Quantity(food_capacity);
        game_cargo::inventory_mut(&mut sim.host.storage, seed_stock)
            .unwrap()
            .capacity = Quantity(seed_capacity);
        sim.review_decisions()?;
        Ok(sim)
    }

    pub(crate) fn evaluate_agriculture(
        &self,
        next: SimTime,
        day: Option<&game_environment::LocalDayFacts>,
    ) -> Result<Option<game_agriculture::PreparedAdvance>, Error> {
        let Some(world) = &self.host.state.agriculture else {
            return Ok(None);
        };
        if day.is_none()
            && game_agriculture::cultivations(&self.host.storage)
                .next()
                .is_none()
        {
            return Ok(None);
        }
        let day = day.map(|day| game_agriculture::LocalWeatherDay {
            day: day.day,
            day_of_year: day.day_of_year,
            cells: day
                .cells
                .iter()
                .map(|(id, w)| {
                    (
                        *id,
                        game_agriculture::WeatherFacts {
                            temperature: w.temperature,
                            rain: w.rain,
                            evaporation: w.evaporation,
                        },
                    )
                })
                .collect(),
        });
        world
            .prepare_advance(
                &self.host.storage,
                &self.agriculture_queries,
                self.host.clock().time,
                next,
                day.as_ref(),
                |inputs| {
                    self.engine
                        .executor()
                        .map_ordered(inputs, |input| input.evaluate())
                },
            )
            .map(Some)
    }

    pub(crate) fn commit_agriculture(
        &mut self,
        output: Option<game_agriculture::PreparedAdvance>,
    ) -> Result<(), Error> {
        let Some(output) = output else {
            return Ok(());
        };
        let failed: Vec<_> = output.failed_crops().collect();
        self.host
            .state
            .agriculture
            .as_mut()
            .ok_or(Error::Unavailable)?
            .commit_advance(&mut self.host.storage, output)?;
        for field in failed {
            self.emit(EventKind::CropFailed { field });
        }
        Ok(())
    }
    pub(crate) fn cultivation_facts(
        &self,
        actor: PersonId,
        field: SiteId,
        resources: Option<&equipment::RankingResources>,
    ) -> Result<game_agriculture::BeginFacts, Error> {
        let world = self
            .host
            .state
            .agriculture
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let land = world
            .field(&self.host.storage, field)
            .ok_or(Error::Unavailable)?;
        let physical = self
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let body = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
        let environment = self.host.state.environment.as_ref();
        let area = match *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId(
                (self
                    .person_at(*self.person_index.get(&actor).ok_or(Error::Unavailable)?)
                    .id)
                    .0,
            ),
        )
        .expect("location exists")
        {
            Location::AtArea(area) => area,
            _ => return Err(Error::Unavailable),
        };
        let stage =
            self.host.state.development.as_ref().and_then(|_| {
                game_lifecycle::life(&self.host.storage, actor).map(|life| life.stage)
            });
        let available = |id: InventoryId, good: Good| {
            game_agriculture::available_stock(
                game_cargo::inventory(&self.host.storage, id).map(|stock| {
                    game_agriculture::StockFacts {
                        good: stock.good,
                        accessible: self.has_stock_access(actor, stock),
                        area: self.inventory_area(stock),
                        quantity: Quantity(
                            self.unreserved_stock_for_planning(id, actor, resources),
                        ),
                    }
                }),
                land.area,
                good,
            )
        };
        let room = |id: InventoryId| {
            game_cargo::inventory(&self.host.storage, id)
                .and_then(|stock| stock.room().ok())
                .unwrap_or(Quantity(0))
        };
        Ok(game_agriculture::BeginFacts {
            actor,
            now: self.host.clock().time,
            area,
            capable: game_agriculture::cultivation_capable(
                &body.vitals,
                physical.rules.fatigue_limit,
                self.independent(actor),
                self.combat_restricts(actor),
                self.care_index.contains_key(&actor),
                stage,
            ),
            tool_available: game_agriculture::cultivation_tool_available(
                available(land.tool, Good::Tool),
                self.tool_available_for_planning(actor, land.tool, resources),
            ),
            field_available: world.field_available(&self.host.storage, actor, field),
            seed_available: available(land.seed, Good::Seed).unwrap_or(Quantity(0)),
            water_available: available(land.water, Good::Water).unwrap_or(Quantity(0)),
            outputs_accessible: available(land.food, Good::Food).is_some()
                && available(land.seed, Good::Seed).is_some(),
            food_room: room(land.food),
            seed_room: room(land.seed),
            day_of_year: environment.map(|w| w.calendar.day_of_year(self.host.clock().time)),
            temperature_c: environment
                .and_then(|w| w.local_weather(&self.host.storage, land.area))
                .map(|w| w.temperature),
        })
    }
    pub(crate) fn start_cultivation(
        &mut self,
        actor: PersonId,
        field: SiteId,
        operation: FarmOperation,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let facts = self.cultivation_facts(actor, field, None)?;
        let plan = self
            .host
            .state
            .agriculture
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_start(&self.host.storage, field, operation, facts)?;
        let effect = plan.effect().clone();
        let project_input = if let Some(input) = effect.consumed {
            self.prepare_farm_input(field, input.inventory, input.quantity)?
        } else {
            None
        };
        // Irrigation consumes physical water. Prepare Health's receiving-side
        // debit from the opening Cargo quantity so it can join this command's
        // atomic batch instead of reacting after publication.
        let water = effect
            .consumed
            .map(|input| {
                let stock = game_cargo::inventory(&self.host.storage, input.inventory)
                    .ok_or(Error::Unavailable)?;
                if stock.good == Good::Water {
                    self.prepare_health_water_reaction(game_health::WaterReaction::Consume {
                        source: input.inventory,
                        quantity: input.quantity,
                        source_before: stock.quantity,
                    })
                } else {
                    Ok(None)
                }
            })
            .transpose()?
            .flatten();
        let mut batch = game_ecs::Batch::new();
        let guard = plan.stage(
            self.host.state.agriculture.as_ref().unwrap(),
            &self.host.storage,
            &mut batch,
        )?;
        let consumption_guard = effect
            .consumed
            .map(|input| {
                game_cargo::prepare_consumptions(
                    &self.host.storage,
                    &[(input.inventory, input.quantity)],
                )?
                .stage_guarded_into(&self.host.storage, &mut batch)
            })
            .transpose()?;
        let input_event_count = project_input
            .as_ref()
            .map_or(0, |change| change.effects().len());
        let project_finish = project_input
            .map(|change| {
                change.stage_into(
                    self.host
                        .state
                        .projects
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &self.host.state.workshops,
                    &mut batch,
                )
            })
            .transpose()?;
        let water_finish = water
            .map(|change| {
                change.stage_into(
                    self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let completes = effect.job.completes;
        let interruption =
            self.prepare_interruptions(&[actor], commands::ActivityOwner::Agriculture)?;
        self.ensure_event_capacity(
            usize::try_from(interruption.event_count()?)
                .map_err(|_| Error::Overflow)?
                .checked_add(input_event_count)
                .and_then(|count| count.checked_add(usize::from(effect.consumed.is_some())))
                .ok_or(Error::Overflow)?,
        )?;
        let mut finish = interruption.stage_into(
            self,
            &mut batch,
            &BTreeMap::from([(
                actor,
                Activity::Cultivate {
                    field,
                    operation,
                    completes,
                },
            )]),
        )?;
        guard.validate(
            self.host.state.agriculture.as_ref().unwrap(),
            &self.host.storage,
            &batch,
        )?;
        if let Some(consumption) = &consumption_guard {
            consumption.validate(&self.host.storage, &batch)?;
        }
        if let Some(project) = &project_finish {
            project.validate_batch(
                self.host
                    .state
                    .projects
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &self.host.storage,
                &self.host.state.workshops,
                &batch,
            )?;
        }
        if let Some(water) = &water_finish {
            water.validate_batch(
                self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
        }
        finish.validate_batch(self, &batch)?;
        self.host.storage.apply_batch(batch)?;
        if let Some(water) = water_finish {
            water.finish();
        }
        if let Some(project) = project_finish {
            for event in project.finish(self.host.state.projects.as_mut().unwrap()) {
                if let game_projects::Effect::InputUsed {
                    enterprise,
                    site,
                    quantity,
                } = event
                {
                    self.emit(EventKind::EnterpriseInputUsed {
                        enterprise,
                        site,
                        quantity,
                    });
                }
            }
        }
        finish.finish_actor(self, actor);
        if let Some(input) = effect.consumed {
            self.emit_prepared_water(EventKind::Consumed {
                actor,
                source: input.inventory,
                quantity: input.quantity,
            });
        }
        self.host
            .state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(actor));
        self.host.bump_revision(1)?;
        Ok(())
    }

    pub(crate) fn complete_cultivation(&mut self, actor: PersonId) {
        let Some(world) = self.host.state.agriculture.as_ref() else {
            return;
        };
        let Some(job) = world.cultivation(&self.host.storage, actor) else {
            return;
        };
        let plan = world
            .prepare_finish(
                &self.host.storage,
                actor,
                game_agriculture::FinishFacts {
                    now: self.host.clock().time,
                    equipped: self.cultivation_equipment_available(actor),
                    alive: game_physiology::body(&self.host.storage, actor)
                        .is_some_and(|body| body.vitals.alive),
                    access: self.cultivation_facts(actor, job.field, None).ok(),
                },
            )
            .expect("validated cultivation completion")
            .expect("cultivation exists");
        let outcome = plan.outcome().clone();
        let mut batch = game_ecs::Batch::new();
        let guard = plan
            .stage(world, &self.host.storage, &mut batch)
            .expect("current cultivation");
        let mut deposits = Vec::new();
        if let game_agriculture::FinishOutcome::Completed {
            harvest: Some(output),
            ..
        } = &outcome
        {
            for (id, good, quantity) in [
                (output.food_inventory, Good::Food, output.food),
                (output.seed_inventory, Good::Seed, output.seed),
            ] {
                if quantity.0 > 0 {
                    let deposit =
                        game_cargo::prepare_deposit(&self.host.storage, id, good, quantity)
                            .expect("checked harvest output");
                    deposits.push(
                        deposit
                            .stage_into(&self.host.storage, &mut batch)
                            .expect("current output stock"),
                    );
                }
            }
        }
        let mut interruption = match &outcome {
            game_agriculture::FinishOutcome::Interrupted => {
                let prepared = self
                    .prepare_interruptions(&[actor], commands::ActivityOwner::Agriculture)
                    .expect("validated interruption");
                Some(
                    prepared
                        .stage_into(self, &mut batch, &BTreeMap::from([(actor, Activity::Idle)]))
                        .expect("validated interruption batch"),
                )
            }
            game_agriculture::FinishOutcome::Completed { .. } => {
                batch.insert(game_ecs::StableId(actor.0), Activity::Idle);
                None
            }
            game_agriculture::FinishOutcome::Dead => None,
        };
        guard
            .validate(
                self.host.state.agriculture.as_ref().unwrap(),
                &self.host.storage,
                &batch,
            )
            .expect("current cultivation inputs");
        if let Some(finish) = &interruption {
            finish
                .validate_batch(self, &batch)
                .expect("current interruption");
        }
        for deposit in &deposits {
            deposit
                .validate(&self.host.storage, &batch)
                .expect("current harvest stock");
        }
        self.host
            .storage
            .apply_batch(batch)
            .expect("validated cultivation batch");
        if let Some(finish) = &mut interruption {
            finish.finish_actor(self, actor);
        }
        match outcome {
            game_agriculture::FinishOutcome::Interrupted => {
                self.emit(EventKind::Interrupted { actor });
            }
            game_agriculture::FinishOutcome::Dead => {}
            game_agriculture::FinishOutcome::Completed { job, harvest } => {
                self.emit(EventKind::Cultivated {
                    actor,
                    field: job.field,
                    operation: job.operation,
                    labor_ms: job.labor_ms,
                });
                if let Some(output) = harvest {
                    self.emit(EventKind::CropHarvested {
                        actor,
                        field: job.field,
                        food: output.food,
                        seed: output.seed,
                    });
                }
            }
        }
    }

    pub(crate) fn agriculture_view(&self, actor: PersonId) -> Option<AgricultureView> {
        let world = self.host.state.agriculture.as_ref()?;
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive {
            return None;
        }
        let Location::AtArea(area) =
            *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))
                .expect("location exists")
        else {
            return None;
        };
        Some(AgricultureView {
            fields: world
                .fields(&self.host.storage)
                .map(|(_, value)| value)
                .filter(|field| field.area == area)
                .filter_map(|field| {
                    let crop = world.crop(&self.host.storage, field.id)?;
                    let soil = world.soil(&self.host.storage, field.id)?;
                    Some(FieldView {
                        id: field.id,
                        crop: crop.definition.clone(),
                        phase: format!("{:?}", crop.phase),
                        soil_fertility: soil.fertility,
                        soil_water: soil.water,
                        degree_days_milli: crop.degree_days_milli,
                        preparation_ms: crop.preparation_ms,
                        planting_ms: crop.planting_ms,
                        harvest_ms: crop.harvest_ms,
                    })
                })
                .collect(),
        })
    }

    pub(crate) fn available_cultivation(&self, actor: PersonId) -> Vec<Command> {
        let Some(runtime) = &self.host.state.agriculture else {
            return Vec::new();
        };
        let facts = runtime
            .fields(&self.host.storage)
            .filter_map(|(id, _)| {
                self.cultivation_facts(actor, id, None)
                    .ok()
                    .map(|facts| (id, facts))
            })
            .collect();
        game_agriculture::available_operations(runtime, &self.host.storage, &facts)
            .into_iter()
            .map(|(field, operation)| Command::Cultivate { field, operation })
            .collect()
    }

    /// Candidate facts are observed only in the actor's current area. Acceptance
    /// still rechecks physical stocks, access, and competition at commit time.
    pub(crate) fn autonomous_cultivation(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let world = self.host.state.agriculture.as_ref()?;
        world
            .select_cultivation(&self.host.storage, |field| {
                self.cultivation_facts(actor, field, resources).ok()
            })
            .map(|(field, operation)| Command::Cultivate { field, operation })
    }
    pub(crate) fn autonomous_field_cultivation(
        &self,
        actor: PersonId,
        field: SiteId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let world = self.host.state.agriculture.as_ref()?;
        world
            .select_field_cultivation(
                &self.host.storage,
                field,
                self.cultivation_facts(actor, field, resources).ok()?,
            )
            .map(|(field, operation)| Command::Cultivate { field, operation })
    }

    pub(crate) fn validate_agriculture(&self) -> Result<(), Error> {
        game_agriculture::validate_runtime_presence(
            &self.host.storage,
            self.host.state.agriculture.as_ref(),
        )?;
        let Some(world) = &self.host.state.agriculture else {
            return Ok(());
        };
        world.validate(
            &self.host.storage,
            self.host.clock().time,
            self.host
                .state
                .environment
                .as_ref()
                .map(|world| world.calendar.year_days()),
        )?;
        world.validate_links(
            &self.host.storage,
            self.host.clock().time,
            &AgricultureLinks(self),
        )
    }
}

struct AgricultureLinks<'a>(&'a Simulation);
impl game_agriculture::SaveLinks for AgricultureLinks<'_> {
    fn next_id(&self) -> u64 {
        self.0.host.next_id()
    }
    fn physical_available(&self) -> bool {
        self.0.host.state.physical.is_some()
    }
    fn weather_day(&self) -> Option<u64> {
        self.0
            .host
            .state
            .environment
            .as_ref()
            .map(|world| world.weather_day)
    }
    fn area_exists(&self, area: AreaId) -> bool {
        self.0.host.state.areas.iter().any(|row| row.id == area)
    }
    fn person_exists(&self, actor: PersonId) -> bool {
        self.0.person_index.contains_key(&actor)
    }
    fn workshop_exists(&self, site: SiteId) -> bool {
        game_workshop::site(&self.0.host.storage, site).is_some()
    }
    fn inventory(&self, id: InventoryId) -> Option<(Good, Option<AreaId>)> {
        game_cargo::inventory(&self.0.host.storage, id)
            .map(|row| (row.good, self.0.inventory_area(row)))
    }
    fn actor_location(&self, actor: PersonId) -> Option<Location> {
        game_spatial::location(&self.0.host.storage, game_ecs::StableId(actor.0)).cloned()
    }
    fn actor_alive(&self, actor: PersonId) -> bool {
        game_physiology::body(&self.0.host.storage, actor).is_some_and(|row| row.vitals.alive)
    }
    fn activity(&self, actor: PersonId) -> Option<(SiteId, FarmOperation, SimTime)> {
        match self.0.host.storage.activity(actor)? {
            Activity::Cultivate {
                field,
                operation,
                completes,
            } => Some((*field, *operation, *completes)),
            _ => None,
        }
    }
    fn completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool {
        self.0.host.state.due.get(&at).is_some_and(|events| {
            events
                .iter()
                .any(|row| matches!(row,Due::Physical(id) if *id == actor))
        })
    }
}

#[cfg(test)]
mod tests;
