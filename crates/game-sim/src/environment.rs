//! A configurable calendar and physical crop cycle. Rights reference the same
//! people and inventories as other work; the climate establishes no ownership.
use crate::*;
use game_economy::environment::yield_amounts;
pub use game_economy::environment::{
    Calendar, Climate, Config, CropPhase, CropRules, Cultivation, Field, Weather, World,
};

impl Simulation {
    pub fn farming_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        Self::farming_with_config(seed, execution, Config::default())
    }

    pub fn farming_with_config(
        seed: u64,
        execution: Execution,
        config: Config,
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
        let Config {
            calendar,
            climate,
            crop,
        } = config;
        let weather = climate.weather(seed, 0, calendar.season(SimTime(0)));
        sim.state.environment = Some(World {
            calendar,
            climate,
            crop,
            weather,
            weather_day: 0,
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
        let mut private_field = sim.state.environment.as_ref().unwrap().fields[&id].clone();
        private_field.id = private_field_id;
        private_field.tool = private_tool;
        private_field.access = BTreeSet::from([sim.state.ruler]);
        private_field.custodian = Some(sim.state.ruler);
        sim.state
            .environment
            .as_mut()
            .unwrap()
            .fields
            .insert(private_field_id, private_field);
        let crop = &sim.state.environment.as_ref().unwrap().crop;
        let food_capacity = 10_000_000.max(3_000_000 + crop.food_yield.0);
        let seed_capacity = 1_000_000.max(10_000 + crop.seed_yield.0);
        sim.state.inventories.get_mut(&food).unwrap().capacity = Quantity(food_capacity);
        sim.state.inventories.get_mut(&seed_stock).unwrap().capacity = Quantity(seed_capacity);
        sim.review_decisions()?;
        Ok(sim)
    }

    pub(crate) fn environment_boundary(&self, requested: SimTime) -> SimTime {
        let Some(world) = &self.state.environment else {
            return requested;
        };
        let day = world.calendar.day.0;
        let following_day = (self.state.time.0 / day)
            .checked_add(1)
            .and_then(|d| d.checked_mul(day));
        following_day
            .map(SimTime)
            .unwrap_or(requested)
            .min(requested)
    }

    pub(crate) fn evaluate_environment(&self, next: SimTime) -> Result<Option<World>, Error> {
        self.state
            .environment
            .as_ref()
            .map(|world| world.advance(self.state.time, next, self.state.seed))
            .transpose()
    }

    pub(crate) fn commit_environment(&mut self, output: Option<World>) {
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
                        .environment
                        .as_ref()
                        .is_some_and(|w| w.fields[&field.id].phase != CropPhase::Failed)
            })
            .map(|f| f.id)
            .collect();
        self.state.environment = Some(output);
        for field in failed {
            self.emit(EventKind::CropFailed { field });
        }
    }

    pub(crate) fn cultivation_possible(
        &self,
        actor: PersonId,
        field: SiteId,
        operation: FarmOperation,
    ) -> bool {
        let Some(world) = &self.state.environment else {
            return false;
        };
        let Some(field) = world.fields.get(&field) else {
            return false;
        };
        let Some(body) = self
            .state
            .physical
            .as_ref()
            .and_then(|w| w.actors.get(&actor))
        else {
            return false;
        };
        if !body.vitals.alive
            || body.vitals.fatigue >= self.state.physical.as_ref().unwrap().rules.fatigue_limit
            || body.vitals.health == 0
            || !self.independent(actor)
            || self.combat_restricts(actor)
            || self.care_index.contains_key(&actor)
        {
            return false;
        }
        if self.state.people[self.person_index[&actor]].location != Location::AtArea(field.area)
            || !field.access.contains(&actor)
        {
            return false;
        }
        if self
            .state
            .development
            .as_ref()
            .is_some_and(|w| !matches!(w.lives[&actor].stage, LifeStage::Adult | LifeStage::Elder))
        {
            return false;
        }
        if world
            .jobs
            .iter()
            .any(|(person, job)| *person != actor && job.field == field.id)
        {
            return false;
        }
        let usable = |id: InventoryId, good: Good, quantity: u64| {
            self.state.inventories.get(&id).is_some_and(|stock| {
                stock.good == good
                    && self.has_stock_access(actor, stock)
                    && self.inventory_area(stock) == Some(field.area)
                    && self.unreserved_stock(id, actor) >= quantity
            })
        };
        if !usable(field.tool, Good::Tool, 1_000) || !self.tool_available_for(actor, field.tool) {
            return false;
        }
        match operation {
            FarmOperation::Fallow => field.phase != CropPhase::Fallow,
            FarmOperation::Prepare => {
                matches!(field.phase, CropPhase::Fallow | CropPhase::Failed)
                    && world.weather.temperature > 0
            }
            FarmOperation::Plant => {
                field.phase == CropPhase::Planting
                    || field.phase == CropPhase::Prepared
                        && (u64::from(world.crop.planting_first_day)
                            ..=u64::from(world.crop.planting_last_day))
                            .contains(&world.calendar.day_of_year(self.state.time))
                        && usable(field.seed, Good::Seed, world.crop.seed_input.0)
            }
            FarmOperation::Tend => field.phase == CropPhase::Growing && field.tending < 1_000,
            FarmOperation::Irrigate => {
                field.phase == CropPhase::Growing
                    && field.soil_water < field.water_capacity
                    && usable(field.water, Good::Water, world.crop.irrigation_input.0)
            }
            FarmOperation::Harvest => {
                field.phase == CropPhase::Ripe
                    && usable(field.food, Good::Food, 0)
                    && usable(field.seed, Good::Seed, 0)
                    && self.harvest_room(field)
            }
        }
    }

    fn harvest_room(&self, field: &Field) -> bool {
        let crop = &self.state.environment.as_ref().unwrap().crop;
        let (food, seed) = yield_amounts(field, crop);
        let output = &self.state.inventories[&field.food];
        let seed_stock = &self.state.inventories[&field.seed];
        output.good == Good::Food
            && self.inventory_area(output) == Some(field.area)
            && self.inventory_area(seed_stock) == Some(field.area)
            && output.capacity.0.saturating_sub(output.quantity.0) >= food.0
            && seed_stock.capacity.0.saturating_sub(seed_stock.quantity.0) >= seed.0
    }

    pub(crate) fn start_cultivation(
        &mut self,
        actor: PersonId,
        field: SiteId,
        operation: FarmOperation,
    ) -> Result<(), Error> {
        if !self.cultivation_possible(actor, field, operation) {
            return Err(Error::Unavailable);
        }
        let world = self.state.environment.as_ref().unwrap();
        let land = &world.fields[&field];
        let progress = match operation {
            FarmOperation::Prepare if land.phase == CropPhase::Failed => 0,
            FarmOperation::Prepare => land.preparation_ms,
            FarmOperation::Plant => land.planting_ms,
            FarmOperation::Harvest => land.harvest_ms,
            _ => 0,
        };
        let remaining = world
            .crop
            .labor(operation)
            .checked_sub(progress)
            .ok_or(Error::Overflow)?;
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(remaining)
                .ok_or(Error::Overflow)?,
        );
        let consume = match operation {
            FarmOperation::Plant if land.phase == CropPhase::Prepared => {
                Some((land.seed, world.crop.seed_input))
            }
            FarmOperation::Irrigate => Some((land.water, world.crop.irrigation_input)),
            _ => None,
        };
        if let Some((source, quantity)) = consume {
            self.preflight_farm_input(field, source, quantity)?;
        }
        self.ensure_event_capacity(2)?;
        self.interrupt_actor_activity(actor);
        if let Some((source, quantity)) = consume {
            self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
            self.record_farm_input(field, source, quantity);
            self.emit(EventKind::Consumed {
                actor,
                source,
                quantity,
            });
        }
        let world = self.state.environment.as_mut().unwrap();
        let land = world.fields.get_mut(&field).unwrap();
        if operation == FarmOperation::Plant {
            land.phase = CropPhase::Planting;
        }
        if operation == FarmOperation::Prepare && land.phase == CropPhase::Failed {
            land.phase = CropPhase::Fallow;
            land.preparation_ms = 0;
            land.planting_ms = 0;
            land.harvest_ms = 0;
            land.degree_days_milli = 0;
            land.stress_days = 0;
            land.tending = 0;
        }
        world.jobs.insert(
            actor,
            Cultivation {
                field,
                operation,
                labor_ms: 0,
                completes,
            },
        );
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
        if let Some(world) = &mut self.state.environment {
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
            .environment
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
        let harvest_allowed = self.cultivation_possible(actor, job.field, FarmOperation::Harvest);
        let world = self.state.environment.as_ref().unwrap();
        let plan = game_economy::environment::prepare_completion(
            &world.fields[&job.field],
            &job,
            &world.crop,
            harvest_allowed,
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
            .environment
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

    pub(crate) fn environment_view(&self, actor: PersonId) -> Option<EnvironmentView> {
        let world = self.state.environment.as_ref()?;
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive {
            return None;
        }
        let Location::AtArea(area) = self.state.people[self.person_index[&actor]].location else {
            return None;
        };
        Some(EnvironmentView {
            day: world.weather_day,
            day_of_year: world.calendar.day_of_year(self.state.time),
            season: world.calendar.season(self.state.time) as u8,
            temperature_c: world.weather.temperature,
            rain: world.weather.rain,
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
        let Some(world) = &self.state.environment else {
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
    pub(crate) fn autonomous_cultivation(&self, actor: PersonId) -> Option<Command> {
        let world = self.state.environment.as_ref()?;
        world
            .fields
            .keys()
            .find_map(|field| self.autonomous_field_cultivation(actor, *field))
    }
    pub(crate) fn autonomous_field_cultivation(
        &self,
        actor: PersonId,
        field: SiteId,
    ) -> Option<Command> {
        let world = self.state.environment.as_ref()?;
        let land = world.fields.get(&field)?;
        let operation = match land.phase {
            CropPhase::Fallow | CropPhase::Failed => FarmOperation::Prepare,
            CropPhase::Prepared | CropPhase::Planting => FarmOperation::Plant,
            CropPhase::Ripe => FarmOperation::Harvest,
            CropPhase::Growing if land.soil_water < world.crop.optimal_water / 2 => {
                FarmOperation::Irrigate
            }
            CropPhase::Growing if land.tending < 1_000 => FarmOperation::Tend,
            _ => return None,
        };
        self.cultivation_possible(actor, field, operation)
            .then_some(Command::Cultivate { field, operation })
    }

    pub(crate) fn validate_environment(&self) -> Result<(), Error> {
        let Some(world) = &self.state.environment else {
            return Ok(());
        };
        let invalid = || Error::InvalidSave("invalid calendar, climate, crop or field".into());
        let crop = &world.crop;
        Config {
            calendar: world.calendar.clone(),
            climate: world.climate.clone(),
            crop: world.crop.clone(),
        }
        .validate()
        .map_err(|_| invalid())?;
        if world.weather_day != self.state.time.0 / world.calendar.day.0
            || world.weather
                != world.climate.weather(
                    self.state.seed,
                    world.weather_day,
                    world.calendar.season(self.state.time),
                )
        {
            return Err(invalid());
        }
        let mut fields_in_use = BTreeSet::new();
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
                || field.soil_fertility > 1_000
                || field.tending > 1_000
                || field.water_capacity == 0
                || field.soil_water > field.water_capacity
                || field.preparation_ms > crop.prepare_labor.0
                || field.planting_ms > crop.plant_labor.0
                || field.harvest_ms > crop.harvest_labor.0
                || field
                    .access
                    .iter()
                    .chain(field.contributions.keys())
                    .any(|person| !self.person_index.contains_key(person))
            {
                return Err(invalid());
            }
            let phase_valid = match field.phase {
                CropPhase::Fallow => {
                    field.planting_ms == 0 && field.degree_days_milli == 0 && field.harvest_ms == 0
                }
                CropPhase::Prepared => {
                    field.preparation_ms == crop.prepare_labor.0
                        && field.planting_ms == 0
                        && field.degree_days_milli == 0
                        && field.harvest_ms == 0
                }
                CropPhase::Planting => {
                    field.preparation_ms == crop.prepare_labor.0
                        && field.planting_ms < crop.plant_labor.0
                        && field.degree_days_milli == 0
                        && field.harvest_ms == 0
                }
                CropPhase::Growing | CropPhase::Failed => {
                    field.preparation_ms == crop.prepare_labor.0
                        && field.planting_ms == crop.plant_labor.0
                        && field.degree_days_milli < crop.ripe_degree_days * 1_000
                        && field.harvest_ms == 0
                }
                CropPhase::Ripe => {
                    field.preparation_ms == crop.prepare_labor.0
                        && field.planting_ms == crop.plant_labor.0
                        && field.degree_days_milli >= crop.ripe_degree_days * 1_000
                }
            };
            if !phase_valid {
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
                || !fields_in_use.insert(job.field)
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
            let phase_valid = match job.operation {
                FarmOperation::Fallow => field.phase != CropPhase::Fallow,
                FarmOperation::Prepare => field.phase == CropPhase::Fallow,
                FarmOperation::Plant => field.phase == CropPhase::Planting,
                FarmOperation::Tend | FarmOperation::Irrigate => {
                    matches!(field.phase, CropPhase::Growing | CropPhase::Failed)
                }
                FarmOperation::Harvest => field.phase == CropPhase::Ripe,
            };
            if !phase_valid {
                return Err(invalid());
            }
            let progress = match job.operation {
                FarmOperation::Prepare => field.preparation_ms,
                FarmOperation::Plant => field.planting_ms,
                FarmOperation::Harvest => field.harvest_ms,
                _ => job.labor_ms,
            };
            if job.labor_ms > progress
                || progress >= crop.labor(job.operation)
                || job.completes.0 - self.state.time.0 != crop.labor(job.operation) - progress
            {
                return Err(invalid());
            }
        }
        let mut exclusive_time: BTreeMap<PersonId, u128> = BTreeMap::new();
        for field in world.fields.values() {
            for (actor, elapsed) in &field.contributions {
                *exclusive_time.entry(*actor).or_default() += u128::from(*elapsed);
            }
        }
        if exclusive_time
            .values()
            .any(|elapsed| *elapsed > u128::from(self.state.time.0))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let completes = sim.state.environment.as_ref().unwrap().jobs[&sim.state.ruler].completes;
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
        sim.state.environment.as_mut().unwrap().crop.fallow_labor = SimDuration(5_000);
        sim.state.environment.as_mut().unwrap().crop.soil_recovery = 5;
        plant(&mut sim, field);
        let land = sim.state.environment.as_ref().unwrap().fields[&field].clone();
        let consumed = sim.state.inventories[&land.seed].quantity;
        let food = sim.state.inventories[&land.food].quantity;
        sim.state
            .environment
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
            sim.state.environment.as_ref().unwrap().fields[&field].phase,
            CropPhase::Fallow
        );
        sim.advance(SimDuration(600_000)).unwrap();
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field].soil_fertility,
            505
        );
        sim.validate_environment().unwrap();
    }
    #[test]
    fn full_cycle_consumes_seed_and_credits_only_real_labor_and_harvest() {
        let (mut sim, field) = compact(1);
        let land = sim.state.environment.as_ref().unwrap().fields[&field].clone();
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
            sim.state.environment.as_ref().unwrap().fields[&field].phase,
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
        let land = &sim.state.environment.as_ref().unwrap().fields[&field];
        assert_eq!(land.phase, CropPhase::Fallow);
        assert_eq!(land.soil_fertility, 880);
        assert_eq!(land.contributions[&sim.state.ruler], 70_000);
        assert!(sim.state.events.iter().any(|event| matches!(event.kind, EventKind::CropHarvested { field: id, food: Quantity(27_000), seed: Quantity(1_800), .. } if id == field)));
    }

    #[test]
    fn interrupted_planting_preserves_labor_and_consumed_seed() {
        let (mut sim, field) = compact(1);
        perform(&mut sim, field, FarmOperation::Prepare);
        let seed = sim.state.environment.as_ref().unwrap().fields[&field].seed;
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
            sim.state.environment.as_ref().unwrap().fields[&field].planting_ms,
            4_999
        );
        assert_eq!(sim.state.inventories[&seed].quantity.0, opening.0 - 1_000);
        perform(&mut sim, field, FarmOperation::Plant);
        assert_eq!(sim.state.inventories[&seed].quantity.0, opening.0 - 1_000);
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field].phase,
            CropPhase::Growing
        );
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field].contributions[&ruler],
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
        world.crop.failure_stress_days = 2;
        world.fields.get_mut(&field).unwrap().soil_water = 0;
        let water = world.fields[&field].water;
        let opening = sim.state.inventories[&water].quantity.0;
        perform(&mut sim, field, FarmOperation::Irrigate);
        assert_eq!(sim.state.inventories[&water].quantity.0, opening - 1_000);
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field].soil_water,
            30
        );
        sim.state
            .environment
            .as_mut()
            .unwrap()
            .fields
            .get_mut(&field)
            .unwrap()
            .soil_water = 0;
        sim.advance(SimDuration(1_200_000 - sim.state.time.0))
            .unwrap();
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field].phase,
            CropPhase::Failed
        );
        let before = sim.state.time;
        perform(&mut sim, field, FarmOperation::Prepare);
        assert_eq!(sim.state.time.0 - before.0, 20_000);
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field].phase,
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
            .environment
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
        let mut config = Config::default();
        config.calendar.starting_day = 100;
        config.climate.seasonal_temperature = [30; 4];
        let configured =
            Simulation::farming_with_config(4, Execution::default(), config.clone()).unwrap();
        assert_eq!(
            configured.observe().environment.unwrap().temperature_c,
            config.climate.weather(4, 0, 1).temperature
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
        config.calendar.day = SimDuration(0);
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
            sim.state.environment.as_ref().unwrap().fields[&field].planting_ms,
            7_000
        );
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&ruler].activity,
            Activity::Idle
        );
        assert!(sim.state.environment.as_ref().unwrap().jobs.is_empty());
        sim.debug_actor_command(ruler, Command::Rest).unwrap();
        sim.advance(SimDuration(3_500)).unwrap();
        perform(&mut sim, field, FarmOperation::Plant);
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field].planting_ms,
            10_000
        );
    }

    #[test]
    fn harvest_waits_for_real_output_capacity_and_local_queries_are_passive() {
        let (mut sim, field) = compact(1);
        plant(&mut sim, field);
        sim.advance(SimDuration(1_200_000 - sim.state.time.0))
            .unwrap();
        let food = sim.state.environment.as_ref().unwrap().fields[&field].food;
        sim.state.inventories.get_mut(&food).unwrap().capacity =
            sim.state.inventories[&food].quantity;
        let before = sim.state.clone();
        for _ in 0..40 {
            let view = sim.observe();
            assert_eq!(view.environment.as_ref().unwrap().fields[0].phase, "Ripe");
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
            sim.state.environment.as_ref().unwrap().fields[&field].phase,
            CropPhase::Fallow
        );
        let village = sim.state.areas[0].id;
        sim.start_movement(sim.state.ruler, village).unwrap();
        assert!(sim.observe().environment.is_none());
        sim.advance(SimDuration(120_000)).unwrap();
        assert!(sim.observe().environment.unwrap().fields.is_empty());
    }
}
