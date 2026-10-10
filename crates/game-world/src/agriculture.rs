//! Agricultural adapter: factual inputs and atomic stock/event application.
use crate::*;
pub use game_agriculture::{Config, CropPhase, CropRules, Cultivation, Field, World};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FarmingConfig {
    pub environment: game_environment::Config,
    pub agriculture: Config,
}
impl FarmingConfig {
    pub fn validate(&self) -> Result<(), Error> {
        self.environment.validate()?;
        self.agriculture
            .validate(self.environment.calendar.year_days())
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
        for body in sim.state.physical.as_mut().unwrap().actors.values_mut() {
            body.vitals.health = 2 * day;
        }
        // Authoring an initial shared field is fixture data, not a government mode.
        // Remove the earlier timber-to-food recipe from this seasonal scenario.
        for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            sim.interrupt_actor_activity(actor);
            sim.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .job = None;
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
        sim.state.agriculture = Some(World {
            crop: config.agriculture.crop,
            day: sim.state.environment.as_ref().unwrap().weather_day,
            fields: BTreeMap::from([(
                id,
                Field {
                    custodian: None,
                    id,
                    area: farm,
                    access: people,
                    seed: seed_stock,
                    tool,
                    water,
                    food,
                    phase: CropPhase::Fallow,
                    soil_fertility: 900,
                    soil_water: 75,
                    water_capacity: 120,
                    preparation_ms: 0,
                    planting_ms: 0,
                    harvest_ms: 0,
                    degree_days_milli: 0,
                    stress_days: 0,
                    tending: 0,
                    contributions: BTreeMap::new(),
                },
            )]),
            jobs: BTreeMap::new(),
        });
        let private_tool = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(farm),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([sim.state.ruler]),
        )?;
        let private_field_id = SiteId(allocate(&mut sim.state)?);
        let mut private_field = sim.state.agriculture.as_ref().unwrap().fields[&id].clone();
        private_field.id = private_field_id;
        private_field.tool = private_tool;
        private_field.access = BTreeSet::from([sim.state.ruler]);
        private_field.custodian = Some(sim.state.ruler);
        sim.state
            .agriculture
            .as_mut()
            .unwrap()
            .fields
            .insert(private_field_id, private_field);
        let crop = &sim.state.agriculture.as_ref().unwrap().crop;
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
        day: Option<&game_environment::DayFacts>,
    ) -> Result<Option<World>, Error> {
        let Some(world) = &self.state.agriculture else {
            return Ok(None);
        };
        if day.is_none() && world.jobs.is_empty() {
            return Ok(None);
        }
        world.advance(self.state.time, next, day).map(Some)
    }

    pub(crate) fn commit_agriculture(&mut self, output: Option<World>) {
        let Some(output) = output else {
            return;
        };
        let failed: Vec<_> = output
            .fields
            .values()
            .filter(|field| {
                field.phase == CropPhase::Failed
                    && self
                        .state
                        .agriculture
                        .as_ref()
                        .is_some_and(|w| w.fields[&field.id].phase != CropPhase::Failed)
            })
            .map(|f| f.id)
            .collect();
        self.state.agriculture = Some(output);
        for field in failed {
            self.emit(EventKind::CropFailed { field });
        }
    }

    fn cultivation_facts(
        &self,
        actor: PersonId,
        field: SiteId,
        resources: Option<&equipment::RankingResources>,
    ) -> Result<game_agriculture::BeginFacts, Error> {
        let world = self.state.agriculture.as_ref().ok_or(Error::Unavailable)?;
        let land = world.fields.get(&field).ok_or(Error::Unavailable)?;
        let physical = self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let body = physical.actors.get(&actor).ok_or(Error::Unavailable)?;
        let environment = self.state.environment.as_ref().ok_or(Error::Unavailable)?;
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
            day_of_year: environment.calendar.day_of_year(self.state.time),
            temperature_c: environment.weather.temperature,
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
        game_agriculture::prepare_begin(&world.fields[&field], &world.crop, operation, facts)
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
        world.fields.insert(field, plan.field);
        world.jobs.insert(actor, plan.job);
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Cultivate {
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
        let valid = self.state.physical.as_ref().unwrap().actors[&actor]
            .vitals
            .alive;
        if !valid {
            return;
        }
        let access = self
            .cultivation_facts(actor, job.field, None)
            .expect("validated cultivation actor");
        let world = self.state.agriculture.as_ref().unwrap();
        let field = &world.fields[&job.field];
        let room = |id: InventoryId| {
            let stock = &self.state.inventories[&id];
            Quantity(stock.capacity.0.saturating_sub(stock.quantity.0))
        };
        let plan = game_agriculture::prepare_completion(
            field,
            &job,
            &world.crop,
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
        self.state
            .agriculture
            .as_mut()
            .unwrap()
            .fields
            .insert(job.field, plan.field);
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Idle;
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
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive {
            return None;
        }
        let Location::AtArea(area) = self.state.people[self.person_index[&actor]].location else {
            return None;
        };
        Some(AgricultureView {
            fields: world
                .fields
                .values()
                .filter(|field| field.area == area)
                .map(|field| FieldView {
                    id: field.id,
                    phase: format!("{:?}", field.phase),
                    soil_fertility: field.soil_fertility,
                    soil_water: field.soil_water,
                    degree_days_milli: field.degree_days_milli,
                    preparation_ms: field.preparation_ms,
                    planting_ms: field.planting_ms,
                    harvest_ms: field.harvest_ms,
                })
                .collect(),
        })
    }

    pub(crate) fn available_cultivation(&self, actor: PersonId) -> Vec<Command> {
        let Some(world) = &self.state.agriculture else {
            return Vec::new();
        };
        world
            .fields
            .keys()
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
                .filter_map(|operation| {
                    self.cultivation_possible(actor, *field, operation)
                        .then_some(Command::Cultivate {
                            field: *field,
                            operation,
                        })
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
            .fields
            .keys()
            .find_map(|field| self.autonomous_field_cultivation(actor, *field, resources))
    }
    pub(crate) fn autonomous_field_cultivation(
        &self,
        actor: PersonId,
        field: SiteId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let world = self.state.agriculture.as_ref()?;
        let land = world.fields.get(&field)?;
        let operation = game_agriculture::next_operation(land, &world.crop)?;
        self.prepare_cultivation_start(actor, field, operation, resources)
            .is_ok()
            .then_some(Command::Cultivate { field, operation })
    }

    pub(crate) fn validate_agriculture(&self) -> Result<(), Error> {
        let Some(world) = &self.state.agriculture else {
            return Ok(());
        };
        let invalid = || Error::InvalidSave("invalid calendar, climate, crop or field".into());
        let environment = self.state.environment.as_ref().ok_or_else(invalid)?;
        world
            .validate(self.state.time, environment.calendar.year_days())
            .map_err(|_| invalid())?;
        if world.day != environment.weather_day {
            return Err(invalid());
        }
        for (id, field) in &world.fields {
            if *id != field.id
                || id.0 >= self.state.next_id
                || self
                    .state
                    .physical
                    .as_ref()
                    .is_none_or(|w| w.sites.contains_key(id))
                || !self.state.areas.iter().any(|a| a.id == field.area)
                || field
                    .custodian
                    .is_some_and(|id| !self.person_index.contains_key(&id))
                || field
                    .access
                    .iter()
                    .chain(field.contributions.keys())
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
            let Some(field) = world.fields.get(&job.field) else {
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
                    .and_then(|w| w.actors.get(actor))
                    .is_none_or(|body| {
                        !body.vitals.alive
                            || body.activity
                                != (Activity::Cultivate {
                                    field: job.field,
                                    operation: job.operation,
                                    completes: job.completes,
                                })
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
mod tests {
    use super::*;
    use game_environment::{Calendar, Climate};

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
        world.weather = world.climate.weather(sim.state.seed, 0, 0);
        let world = sim.state.agriculture.as_mut().unwrap();
        world.crop.planting_last_day = 9;
        world.crop.food_yield = Quantity(30_000);
        world.crop.prepare_labor = SimDuration(20_000);
        world.crop.plant_labor = SimDuration(10_000);
        world.crop.tend_labor = SimDuration(5_000);
        world.crop.irrigation_labor = SimDuration(5_000);
        world.crop.harvest_labor = SimDuration(20_000);
        world.crop.base_temperature = 0;
        world.crop.ripe_degree_days = 20;
        let field = world.fields.values_mut().next().unwrap();
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

    #[test]
    fn deliberate_fallow_loses_planted_seed_and_warm_wet_soil_recovers_without_harvest() {
        let (mut sim, field) = compact(1);
        sim.state.agriculture.as_mut().unwrap().crop.fallow_labor = SimDuration(5_000);
        sim.state.agriculture.as_mut().unwrap().crop.soil_recovery = 5;
        plant(&mut sim, field);
        let land = sim.state.agriculture.as_ref().unwrap().fields[&field].clone();
        let consumed = sim.state.inventories[&land.seed].quantity;
        let food = sim.state.inventories[&land.food].quantity;
        sim.state
            .agriculture
            .as_mut()
            .unwrap()
            .fields
            .get_mut(&field)
            .unwrap()
            .soil_fertility = 500;
        perform(&mut sim, field, FarmOperation::Fallow);
        assert_eq!(sim.state.inventories[&land.seed].quantity, consumed);
        assert_eq!(sim.state.inventories[&land.food].quantity, food);
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].phase,
            CropPhase::Fallow
        );
        sim.advance(SimDuration(600_000)).unwrap();
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].soil_fertility,
            505
        );
        sim.validate_environment().unwrap();
    }
    #[test]
    fn full_cycle_consumes_seed_and_credits_only_real_labor_and_harvest() {
        let (mut sim, field) = compact(1);
        let land = sim.state.agriculture.as_ref().unwrap().fields[&field].clone();
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
            sim.state.agriculture.as_ref().unwrap().fields[&field].phase,
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
        let land = &sim.state.agriculture.as_ref().unwrap().fields[&field];
        assert_eq!(land.phase, CropPhase::Fallow);
        assert_eq!(land.soil_fertility, 880);
        assert_eq!(land.contributions[&sim.state.ruler], 70_000);
        assert!(sim.state.events.iter().any(|event| matches!(event.kind, EventKind::CropHarvested { field: id, food: Quantity(27_000), seed: Quantity(1_800), .. } if id == field)));
    }

    #[test]
    fn interrupted_planting_preserves_labor_and_consumed_seed() {
        let (mut sim, field) = compact(1);
        perform(&mut sim, field, FarmOperation::Prepare);
        let seed = sim.state.agriculture.as_ref().unwrap().fields[&field].seed;
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
            sim.state.agriculture.as_ref().unwrap().fields[&field].planting_ms,
            4_999
        );
        assert_eq!(sim.state.inventories[&seed].quantity.0, opening.0 - 1_000);
        perform(&mut sim, field, FarmOperation::Plant);
        assert_eq!(sim.state.inventories[&seed].quantity.0, opening.0 - 1_000);
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].phase,
            CropPhase::Growing
        );
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].contributions[&ruler],
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
        world.weather = world.climate.weather(
            sim.state.seed,
            world.weather_day,
            world.calendar.season(sim.state.time),
        );
        let world = sim.state.agriculture.as_mut().unwrap();
        world.crop.failure_stress_days = 2;
        world.fields.get_mut(&field).unwrap().soil_water = 0;
        let water = world.fields[&field].water;
        let opening = sim.state.inventories[&water].quantity.0;
        perform(&mut sim, field, FarmOperation::Irrigate);
        assert_eq!(sim.state.inventories[&water].quantity.0, opening - 1_000);
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].soil_water,
            30
        );
        sim.state
            .agriculture
            .as_mut()
            .unwrap()
            .fields
            .get_mut(&field)
            .unwrap()
            .soil_water = 0;
        sim.advance(SimDuration(1_200_000 - sim.state.time.0))
            .unwrap();
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].phase,
            CropPhase::Failed
        );
        let before = sim.state.time;
        perform(&mut sim, field, FarmOperation::Prepare);
        assert_eq!(sim.state.time.0 - before.0, 20_000);
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].phase,
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
        sim.state
            .agriculture
            .as_mut()
            .unwrap()
            .crop
            .planting_first_day = 1;
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
        assert_eq!(
            configured.observe().environment.unwrap().temperature_c,
            config.environment.climate.weather(4, 0, 1).temperature
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
        sim.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&ruler)
            .unwrap()
            .vitals
            .fatigue = 0;
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
            sim.state.agriculture.as_ref().unwrap().fields[&field].planting_ms,
            7_000
        );
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&ruler].activity,
            Activity::Idle
        );
        assert!(sim.state.agriculture.as_ref().unwrap().jobs.is_empty());
        sim.debug_actor_command(ruler, Command::Rest).unwrap();
        sim.advance(SimDuration(3_500)).unwrap();
        perform(&mut sim, field, FarmOperation::Plant);
        assert_eq!(
            sim.state.agriculture.as_ref().unwrap().fields[&field].planting_ms,
            10_000
        );
    }

    #[test]
    fn harvest_waits_for_real_output_capacity_and_local_queries_are_passive() {
        let (mut sim, field) = compact(1);
        plant(&mut sim, field);
        sim.advance(SimDuration(1_200_000 - sim.state.time.0))
            .unwrap();
        let food = sim.state.agriculture.as_ref().unwrap().fields[&field].food;
        sim.state.inventories.get_mut(&food).unwrap().capacity =
            sim.state.inventories[&food].quantity;
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
            sim.state.agriculture.as_ref().unwrap().fields[&field].phase,
            CropPhase::Fallow
        );
        let village = sim.state.areas[0].id;
        sim.start_movement(sim.state.ruler, village).unwrap();
        assert!(sim.observe().agriculture.is_none());
        sim.advance(SimDuration(120_000)).unwrap();
        assert!(sim.observe().agriculture.unwrap().fields.is_empty());
    }
}
