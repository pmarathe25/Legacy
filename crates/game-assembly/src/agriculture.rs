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
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
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
            .host
            .state
            .people
            .iter()
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
        let farmer = sim.host.state.people[3].id;
        *game_spatial::location_mut(
            &mut sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&farmer]].id).0),
        )
        .expect("location exists") = Location::AtArea(farm);
        let people = sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<BTreeSet<_>>();
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
    fn cultivation_facts(
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
                (self.host.state.people
                    [*self.person_index.get(&actor).ok_or(Error::Unavailable)?]
                .id)
                    .0,
            ),
        )
        .expect("location exists")
        {
            Location::AtArea(area) => area,
            _ => return Err(Error::Unavailable),
        };
        let mature = self.host.state.development.as_ref().is_none_or(|_| {
            matches!(
                game_lifecycle::life(&self.host.storage, actor)
                    .unwrap()
                    .stage,
                LifeStage::Adult | LifeStage::Elder
            )
        });
        let available = |id: InventoryId, good: Good| {
            game_cargo::inventory(&self.host.storage, id)
                .filter(|stock| {
                    stock.good == good
                        && self.has_stock_access(actor, stock)
                        && self.inventory_area(stock) == Some(land.area)
                })
                .map(|_| Quantity(self.unreserved_stock_for_planning(id, actor, resources)))
        };
        let room = |id: InventoryId| {
            game_cargo::inventory(&self.host.storage, id).map_or(Quantity(0), |stock| {
                Quantity(stock.capacity.0.saturating_sub(stock.quantity.0))
            })
        };
        Ok(game_agriculture::BeginFacts {
            actor,
            now: self.host.clock().time,
            area,
            capable: body.vitals.alive
                && body.vitals.health > 0
                && body.vitals.fatigue < physical.rules.fatigue_limit
                && self.independent(actor)
                && !self.combat_restricts(actor)
                && !self.care_index.contains_key(&actor)
                && mature,
            tool_available: available(land.tool, Good::Tool).is_some_and(|q| q.0 >= 1_000)
                && self.tool_available_for_planning(actor, land.tool, resources),
            field_available: !game_agriculture::cultivations(&self.host.storage)
                .any(|(person, job)| person != actor && job.field == field),
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
    fn prepare_cultivation_start(
        &self,
        actor: PersonId,
        field: SiteId,
        operation: FarmOperation,
        resources: Option<&equipment::RankingResources>,
    ) -> Result<game_agriculture::BeginEffect, Error> {
        let facts = self.cultivation_facts(actor, field, resources)?;
        let world = self
            .host
            .state
            .agriculture
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let (land, crop, soil, rules) = world.field_bundle(&self.host.storage, field)?;
        game_agriculture::prepare_begin(land, crop, soil, rules, operation, facts)
    }
    pub(crate) fn cultivation_possible(
        &self,
        actor: PersonId,
        field: SiteId,
        operation: FarmOperation,
    ) -> bool {
        self.prepare_cultivation_start(actor, field, operation, None)
            .is_ok()
    }
    pub(crate) fn start_cultivation(
        &mut self,
        actor: PersonId,
        field: SiteId,
        operation: FarmOperation,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let plan = self.prepare_cultivation_start(actor, field, operation, None)?;
        let project_input = if let Some(input) = plan.consumed {
            self.prepare_farm_input(field, input.inventory, input.quantity)?
        } else {
            None
        };
        self.ensure_event_capacity(2)?;
        self.commit_project_input(project_input)?;
        self.interrupt_actor_activity(actor);
        if let Some(input) = plan.consumed {
            game_cargo::inventory_mut(&mut self.host.storage, input.inventory)
                .unwrap()
                .quantity
                .0 -= input.quantity.0;
            self.emit(EventKind::Consumed {
                actor,
                source: input.inventory,
                quantity: input.quantity,
            });
        }
        let completes = plan.job.completes;
        let world = self.host.state.agriculture.as_mut().unwrap();
        world
            .insert_crop(&mut self.host.storage, field, plan.crop)
            .expect("validated component insertion");
        world
            .insert_cultivation(&mut self.host.storage, actor, plan.job)
            .expect("validated cultivation");
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Cultivate {
            field,
            operation,
            completes,
        };
        self.host
            .state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(actor));
        self.host.bump_revision(1)?;
        Ok(())
    }

    pub(crate) fn interrupt_cultivation(&mut self, actor: PersonId) {
        if let Some(world) = &mut self.host.state.agriculture {
            world
                .remove_cultivation(&mut self.host.storage, actor)
                .expect("registered cultivation");
        }
    }

    pub(crate) fn complete_cultivation(&mut self, actor: PersonId) {
        if !self.cultivation_equipment_available(actor) {
            self.interrupt_actor_activity(actor);
            self.emit(EventKind::Interrupted { actor });
            return;
        }
        let Some(job) = self.host.state.agriculture.as_mut().and_then(|w| {
            w.remove_cultivation(&mut self.host.storage, actor)
                .expect("registered cultivation")
        }) else {
            return;
        };
        let valid = game_physiology::body(&self.host.storage, actor)
            .expect("body exists")
            .vitals
            .alive;
        if !valid {
            return;
        }
        let access = self
            .cultivation_facts(actor, job.field, None)
            .expect("validated cultivation actor");
        let world = self.host.state.agriculture.as_ref().unwrap();
        let field = &world
            .field(&self.host.storage, job.field)
            .expect("component exists");
        let room = |id: InventoryId| {
            let stock = game_cargo::inventory(&self.host.storage, id).expect("inventory exists");
            Quantity(stock.capacity.0.saturating_sub(stock.quantity.0))
        };
        let plan = game_agriculture::prepare_completion(
            field,
            world
                .crop(&self.host.storage, job.field)
                .expect("component exists"),
            world
                .soil(&self.host.storage, job.field)
                .expect("component exists"),
            &job,
            world
                .rules_for(&self.host.storage, job.field)
                .expect("validated crop definition"),
            game_agriculture::CompletionFacts {
                now: self.host.clock().time,
                eligible: valid,
                outputs_accessible: access.capable
                    && access.tool_available
                    && access.field_available
                    && field.access.contains(&actor)
                    && access.outputs_accessible,
                food_room: room(field.food),
                seed_room: room(field.seed),
            },
        )
        .expect("validated cultivation completion");
        let harvested = plan.harvest.map(|output| {
            game_cargo::inventory_mut(&mut self.host.storage, output.food_inventory)
                .unwrap()
                .quantity
                .0 += output.food.0;
            game_cargo::inventory_mut(&mut self.host.storage, output.seed_inventory)
                .unwrap()
                .quantity
                .0 += output.seed.0;
            (output.food, output.seed)
        });
        let world = self.host.state.agriculture.as_mut().unwrap();
        world
            .insert_crop(&mut self.host.storage, job.field, plan.crop)
            .expect("validated component insertion");
        world
            .insert_soil(&mut self.host.storage, job.field, plan.soil)
            .expect("validated component insertion");
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
        self.emit(EventKind::Cultivated {
            actor,
            field: job.field,
            operation: job.operation,
            labor_ms: job.labor_ms,
        });
        if let Some((food, seed)) = harvested {
            self.emit(EventKind::CropHarvested {
                actor,
                field: job.field,
                food,
                seed,
            });
        }
    }

    pub(crate) fn agriculture_view(&self, actor: PersonId) -> Option<AgricultureView> {
        let world = self.host.state.agriculture.as_ref()?;
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive {
            return None;
        }
        let Location::AtArea(area) = *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId((self.host.state.people[self.person_index[&actor]].id).0),
        )
        .expect("location exists") else {
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
        let Some(world) = &self.host.state.agriculture else {
            return Vec::new();
        };
        world
            .fields(&self.host.storage)
            .map(|(id, _)| id)
            .flat_map(|field| {
                [
                    FarmOperation::Prepare,
                    FarmOperation::Plant,
                    FarmOperation::Tend,
                    FarmOperation::Irrigate,
                    FarmOperation::Harvest,
                    FarmOperation::Fallow,
                ]
                .into_iter()
                .filter_map(move |operation| {
                    self.cultivation_possible(actor, field, operation)
                        .then_some(Command::Cultivate { field, operation })
                })
            })
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
            .fields(&self.host.storage)
            .map(|(id, _)| id)
            .find_map(|field| self.autonomous_field_cultivation(actor, field, resources))
    }
    pub(crate) fn autonomous_field_cultivation(
        &self,
        actor: PersonId,
        field: SiteId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let world = self.host.state.agriculture.as_ref()?;
        let (_, crop, soil, rules) = world.field_bundle(&self.host.storage, field).ok()?;
        let operation = game_agriculture::next_operation(crop, soil, rules)?;
        self.prepare_cultivation_start(actor, field, operation, resources)
            .is_ok()
            .then_some(Command::Cultivate { field, operation })
    }

    pub(crate) fn validate_agriculture(&self) -> Result<(), Error> {
        let Some(world) = &self.host.state.agriculture else {
            if self.host.storage.iter::<Field>().next().is_some()
                || self.host.storage.iter::<Crop>().next().is_some()
                || game_agriculture::cultivations(&self.host.storage)
                    .next()
                    .is_some()
            {
                return Err(Error::InvalidSave(
                    "agricultural components have no runtime".into(),
                ));
            }
            return Ok(());
        };
        let invalid = || Error::InvalidSave("invalid calendar, climate, crop or field".into());
        let environment = self.host.state.environment.as_ref();
        world
            .validate(
                &self.host.storage,
                self.host.clock().time,
                environment.map(|w| w.calendar.year_days()),
            )
            .map_err(|_| invalid())?;
        if environment.is_some_and(|w| world.day != w.weather_day) {
            return Err(invalid());
        }
        for (id, field) in world.fields(&self.host.storage) {
            if id != field.id
                || id.0 >= self.host.next_id()
                || self
                    .host
                    .state
                    .physical
                    .as_ref()
                    .is_none_or(|_| game_workshop::site(&self.host.storage, id).is_some())
                || !self.host.state.areas.iter().any(|a| a.id == field.area)
                || field
                    .custodian
                    .is_some_and(|id| !self.person_index.contains_key(&id))
                || field
                    .access
                    .iter()
                    .chain(
                        world
                            .crop(&self.host.storage, id)
                            .expect("component exists")
                            .contributions
                            .keys(),
                    )
                    .any(|person| !self.person_index.contains_key(person))
            {
                return Err(invalid());
            }
            let refs = [
                (field.seed, Good::Seed),
                (field.tool, Good::Tool),
                (field.water, Good::Water),
                (field.food, Good::Food),
            ];
            let mut unique = BTreeSet::new();
            for (stock, good) in refs {
                if !unique.insert(stock)
                    || game_cargo::inventory(&self.host.storage, stock).is_none_or(|i| {
                        i.good != good || self.inventory_area(i) != Some(field.area)
                    })
                {
                    return Err(invalid());
                }
            }
        }
        for (actor, job) in game_agriculture::cultivations(&self.host.storage) {
            let actor = &actor;
            let Some(field) = world.field(&self.host.storage, job.field) else {
                return Err(invalid());
            };
            if !self.person_index.contains_key(actor)
                || job.completes <= self.host.clock().time
                || *game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId((self.host.state.people[self.person_index[actor]].id).0),
                )
                .expect("location exists")
                    != Location::AtArea(field.area)
                || self
                    .host
                    .state
                    .physical
                    .as_ref()
                    .and_then(|_| game_physiology::body(&self.host.storage, *actor))
                    .is_none_or(|body| !body.vitals.alive)
                || self.host.storage.activity(*actor)
                    != Some(&Activity::Cultivate {
                        field: job.field,
                        operation: job.operation,
                        completes: job.completes,
                    })
                || !self
                    .host
                    .state
                    .due
                    .get(&job.completes)
                    .is_some_and(|events| {
                        events
                            .iter()
                            .any(|d| matches!(d, Due::Physical(id) if id == actor))
                    })
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
