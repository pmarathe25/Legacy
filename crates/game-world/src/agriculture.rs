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
        sim.state.scenario = ScenarioInfo {
            id: "farming-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let day = 86_400_000;
        let rules = &mut sim.state.physical.as_mut().unwrap().rules;
        rules.hunger_harm = 3 * day;
        rules.meal_relief = day;
        rules.fatigue_limit = 16 * 3_600_000;
        rules.max_health = 2 * day;
        let actors: Vec<_> = sim
            .state
            .components
            .bodies()
            .map(|(actor, _)| actor)
            .collect();
        for actor in actors {
            sim.state.components.body_mut(actor).unwrap().vitals.health = 2 * day;
        }
        // Authoring an initial shared field is fixture data, not a government mode.
        // Remove the earlier timber-to-food recipe from this seasonal scenario.
        for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            sim.interrupt_actor_activity(actor);
            sim.state.components.set_job(actor, None)?;
        }
        sim.state.physical.as_mut().unwrap().sites.clear();
        for mind in sim.state.decisions.as_mut().unwrap().minds.values_mut() {
            mind.sites.clear();
            mind.plan = None;
            mind.retry_after = sim.state.time;
        }
        let development_rules = development::Rules::human();
        let lives = sim
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
        sim.state.development = Some(development::World {
            rules: development_rules,
            lives,
            pregnancies: BTreeMap::new(),
        });
        for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            sim.schedule_age(actor);
        }
        let farm = sim.state.areas[1].id;
        // This resident starts physically at the farm; later workers must travel.
        let farmer = sim.state.people[3].id;
        sim.state.people[sim.person_index[&farmer]].location = Location::AtArea(farm);
        let people = sim
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
        for stock in
            sim.state.inventories.values_mut().filter(|i| {
                i.good == Good::Food && matches!(i.location, InventoryLocation::Area(_))
            })
        {
            stock.capacity = Quantity(10_000_000);
            stock.quantity = Quantity(3_000_000);
        }
        let id = SiteId(allocate(&mut sim.state)?);
        sim.state.environment = Some(game_environment::World::new(
            config.environment,
            sim.state.time,
            seed,
        )?);
        sim.register_weather_cells()?;
        let agriculture = Runtime::from_content(
            crop_document,
            sim.state.environment.as_ref().unwrap().weather_day,
        )?;
        let crop_id = agriculture.catalog.default_crop.clone();
        agriculture
            .insert_field(
                &mut sim.state.components,
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
            .insert_crop(&mut sim.state.components, id, Crop::new(crop_id))
            .expect("validated component insertion");
        agriculture
            .insert_soil(
                &mut sim.state.components,
                id,
                Soil {
                    fertility: 900,
                    water: 75,
                    capacity: 120,
                },
            )
            .expect("validated component insertion");
        sim.state.agriculture = Some(agriculture);
        let private_tool = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(farm),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([sim.state.ruler]),
        )?;
        let private_field_id = SiteId(allocate(&mut sim.state)?);
        let mut private_field = sim
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .field(&sim.state.components, id)
            .expect("component exists")
            .clone();
        private_field.id = private_field_id;
        private_field.tool = private_tool;
        private_field.access = BTreeSet::from([sim.state.ruler]);
        private_field.custodian = Some(sim.state.ruler);
        sim.state
            .agriculture
            .as_mut()
            .unwrap()
            .insert_field(&mut sim.state.components, private_field)
            .expect("validated component insertion");
        let world = sim.state.agriculture.as_mut().unwrap();
        let private_crop = world
            .crop(&sim.state.components, id)
            .expect("component exists")
            .clone();
        let private_soil = world
            .soil(&sim.state.components, id)
            .expect("component exists")
            .clone();
        world
            .insert_crop(&mut sim.state.components, private_field_id, private_crop)
            .expect("validated component insertion");
        world
            .insert_soil(&mut sim.state.components, private_field_id, private_soil)
            .expect("validated component insertion");
        let crop = world.rules_for(&sim.state.components, id)?;
        let food_capacity = 10_000_000.max(3_000_000 + crop.food_yield.0);
        let seed_capacity = 1_000_000.max(10_000 + crop.seed_yield.0);
        sim.state.inventories.get_mut(&food).unwrap().capacity = Quantity(food_capacity);
        sim.state.inventories.get_mut(&seed_stock).unwrap().capacity = Quantity(seed_capacity);
        sim.review_decisions()?;
        Ok(sim)
    }

    pub(crate) fn evaluate_agriculture(
        &self,
        next: SimTime,
        day: Option<&game_environment::LocalDayFacts>,
    ) -> Result<Option<game_agriculture::PreparedAdvance>, Error> {
        let Some(world) = &self.state.agriculture else {
            return Ok(None);
        };
        if day.is_none() && world.jobs.is_empty() {
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
                &self.state.components,
                &self.agriculture_queries,
                self.state.time,
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
        self.state
            .agriculture
            .as_mut()
            .ok_or(Error::Unavailable)?
            .commit_advance(&mut self.state.components, output)?;
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
        let world = self.state.agriculture.as_ref().ok_or(Error::Unavailable)?;
        let land = world
            .field(&self.state.components, field)
            .ok_or(Error::Unavailable)?;
        let physical = self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let body = self
            .state
            .components
            .body(actor)
            .ok_or(Error::Unavailable)?;
        let environment = self.state.environment.as_ref();
        let area = match self.state.people
            [*self.person_index.get(&actor).ok_or(Error::Unavailable)?]
        .location
        {
            Location::AtArea(area) => area,
            _ => return Err(Error::Unavailable),
        };
        let mature =
            self.state.development.as_ref().is_none_or(|w| {
                matches!(w.lives[&actor].stage, LifeStage::Adult | LifeStage::Elder)
            });
        let available = |id: InventoryId, good: Good| {
            self.state
                .inventories
                .get(&id)
                .filter(|stock| {
                    stock.good == good
                        && self.has_stock_access(actor, stock)
                        && self.inventory_area(stock) == Some(land.area)
                })
                .map(|_| Quantity(self.unreserved_stock_for_planning(id, actor, resources)))
        };
        let room = |id: InventoryId| {
            self.state
                .inventories
                .get(&id)
                .map_or(Quantity(0), |stock| {
                    Quantity(stock.capacity.0.saturating_sub(stock.quantity.0))
                })
        };
        Ok(game_agriculture::BeginFacts {
            actor,
            now: self.state.time,
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
            field_available: !world
                .jobs
                .iter()
                .any(|(person, job)| *person != actor && job.field == field),
            seed_available: available(land.seed, Good::Seed).unwrap_or(Quantity(0)),
            water_available: available(land.water, Good::Water).unwrap_or(Quantity(0)),
            outputs_accessible: available(land.food, Good::Food).is_some()
                && available(land.seed, Good::Seed).is_some(),
            food_room: room(land.food),
            seed_room: room(land.seed),
            day_of_year: environment.map(|w| w.calendar.day_of_year(self.state.time)),
            temperature_c: environment
                .and_then(|w| w.local_weather(land.area))
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
        let world = self.state.agriculture.as_ref().ok_or(Error::Unavailable)?;
        let (land, crop, soil, rules) = world.field_bundle(&self.state.components, field)?;
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
            self.state
                .inventories
                .get_mut(&input.inventory)
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
        let world = self.state.agriculture.as_mut().unwrap();
        world
            .insert_crop(&mut self.state.components, field, plan.crop)
            .expect("validated component insertion");
        world.jobs.insert(actor, plan.job);
        *self.state.components.activity_mut(actor).unwrap() = Activity::Cultivate {
            field,
            operation,
            completes,
        };
        self.state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(actor));
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn interrupt_cultivation(&mut self, actor: PersonId) {
        if let Some(world) = &mut self.state.agriculture {
            world.jobs.remove(&actor);
        }
    }

    pub(crate) fn complete_cultivation(&mut self, actor: PersonId) {
        if !self.cultivation_equipment_available(actor) {
            self.interrupt_actor_activity(actor);
            self.emit(EventKind::Interrupted { actor });
            return;
        }
        let Some(job) = self
            .state
            .agriculture
            .as_mut()
            .and_then(|w| w.jobs.remove(&actor))
        else {
            return;
        };
        let valid = self
            .state
            .components
            .body(actor)
            .expect("body exists")
            .vitals
            .alive;
        if !valid {
            return;
        }
        let access = self
            .cultivation_facts(actor, job.field, None)
            .expect("validated cultivation actor");
        let world = self.state.agriculture.as_ref().unwrap();
        let field = &world
            .field(&self.state.components, job.field)
            .expect("component exists");
        let room = |id: InventoryId| {
            let stock = &self.state.inventories[&id];
            Quantity(stock.capacity.0.saturating_sub(stock.quantity.0))
        };
        let plan = game_agriculture::prepare_completion(
            field,
            world
                .crop(&self.state.components, job.field)
                .expect("component exists"),
            world
                .soil(&self.state.components, job.field)
                .expect("component exists"),
            &job,
            world
                .rules_for(&self.state.components, job.field)
                .expect("validated crop definition"),
            game_agriculture::CompletionFacts {
                now: self.state.time,
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
            self.state
                .inventories
                .get_mut(&output.food_inventory)
                .unwrap()
                .quantity
                .0 += output.food.0;
            self.state
                .inventories
                .get_mut(&output.seed_inventory)
                .unwrap()
                .quantity
                .0 += output.seed.0;
            (output.food, output.seed)
        });
        let world = self.state.agriculture.as_mut().unwrap();
        world
            .insert_crop(&mut self.state.components, job.field, plan.crop)
            .expect("validated component insertion");
        world
            .insert_soil(&mut self.state.components, job.field, plan.soil)
            .expect("validated component insertion");
        *self.state.components.activity_mut(actor).unwrap() = Activity::Idle;
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
        let world = self.state.agriculture.as_ref()?;
        let body = self.state.components.body(actor)?;
        if !body.vitals.alive {
            return None;
        }
        let Location::AtArea(area) = self.state.people[self.person_index[&actor]].location else {
            return None;
        };
        Some(AgricultureView {
            fields: world
                .fields(&self.state.components)
                .map(|(_, value)| value)
                .filter(|field| field.area == area)
                .filter_map(|field| {
                    let crop = world.crop(&self.state.components, field.id)?;
                    let soil = world.soil(&self.state.components, field.id)?;
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
        let Some(world) = &self.state.agriculture else {
            return Vec::new();
        };
        world
            .fields(&self.state.components)
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
        let world = self.state.agriculture.as_ref()?;
        world
            .fields(&self.state.components)
            .map(|(id, _)| id)
            .find_map(|field| self.autonomous_field_cultivation(actor, field, resources))
    }
    pub(crate) fn autonomous_field_cultivation(
        &self,
        actor: PersonId,
        field: SiteId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let world = self.state.agriculture.as_ref()?;
        let (_, crop, soil, rules) = world.field_bundle(&self.state.components, field).ok()?;
        let operation = game_agriculture::next_operation(crop, soil, rules)?;
        self.prepare_cultivation_start(actor, field, operation, resources)
            .is_ok()
            .then_some(Command::Cultivate { field, operation })
    }

    pub(crate) fn validate_agriculture(&self) -> Result<(), Error> {
        let Some(world) = &self.state.agriculture else {
            if self.state.components.iter::<Field>().next().is_some()
                || self.state.components.iter::<Crop>().next().is_some()
                || self.state.components.iter::<Soil>().next().is_some()
            {
                return Err(Error::InvalidSave(
                    "agricultural components have no runtime".into(),
                ));
            }
            return Ok(());
        };
        let invalid = || Error::InvalidSave("invalid calendar, climate, crop or field".into());
        let environment = self.state.environment.as_ref();
        world
            .validate(
                &self.state.components,
                self.state.time,
                environment.map(|w| w.calendar.year_days()),
            )
            .map_err(|_| invalid())?;
        if environment.is_some_and(|w| world.day != w.weather_day) {
            return Err(invalid());
        }
        for (id, field) in world.fields(&self.state.components) {
            if id != field.id
                || id.0 >= self.state.next_id
                || self
                    .state
                    .physical
                    .as_ref()
                    .is_none_or(|w| w.sites.contains_key(&id))
                || !self.state.areas.iter().any(|a| a.id == field.area)
                || field
                    .custodian
                    .is_some_and(|id| !self.person_index.contains_key(&id))
                || field
                    .access
                    .iter()
                    .chain(
                        world
                            .crop(&self.state.components, id)
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
                    || self.state.inventories.get(&stock).is_none_or(|i| {
                        i.good != good || self.inventory_area(i) != Some(field.area)
                    })
                {
                    return Err(invalid());
                }
            }
        }
        for (actor, job) in &world.jobs {
            let Some(field) = world.field(&self.state.components, job.field) else {
                return Err(invalid());
            };
            if !self.person_index.contains_key(actor)
                || job.completes <= self.state.time
                || self.state.people[self.person_index[actor]].location
                    != Location::AtArea(field.area)
                || self
                    .state
                    .physical
                    .as_ref()
                    .and_then(|_| self.state.components.body(*actor))
                    .is_none_or(|body| !body.vitals.alive)
                || self.state.components.activity(*actor)
                    != Some(&Activity::Cultivate {
                        field: job.field,
                        operation: job.operation,
                        completes: job.completes,
                    })
                || !self.state.due.get(&job.completes).is_some_and(|events| {
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
