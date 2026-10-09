//! A configurable calendar and physical crop cycle. Rights reference the same
//! people and inventories as other work; the climate establishes no ownership.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub day: SimDuration,
    pub season_days: [u32; 4],
    pub starting_day: u32,
}
impl Calendar {
    pub fn human() -> Self {
        Self {
            day: SimDuration(86_400_000),
            season_days: [92, 92, 91, 90],
            starting_day: 0,
        }
    }
    pub fn year_days(&self) -> u64 {
        self.season_days.iter().map(|v| u64::from(*v)).sum()
    }
    pub fn day_of_year(&self, time: SimTime) -> u64 {
        (time.0 / self.day.0 % self.year_days() + u64::from(self.starting_day)) % self.year_days()
    }
    pub fn season(&self, time: SimTime) -> usize {
        let mut day = self.day_of_year(time);
        for (season, length) in self.season_days.iter().enumerate() {
            if day < u64::from(*length) {
                return season;
            }
            day -= u64::from(*length);
        }
        unreachable!("day is inside the configured year")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Climate {
    /// Degrees Celsius and water units per day, intentionally fixed point free.
    pub seasonal_temperature: [i32; 4],
    pub seasonal_rain: [u64; 4],
    pub seasonal_evaporation: [u64; 4],
    pub temperature_variation: u32,
    pub rainfall_variation: u32,
}
impl Climate {
    pub fn temperate() -> Self {
        Self {
            seasonal_temperature: [12, 20, 10, 0],
            seasonal_rain: [6, 4, 6, 3],
            seasonal_evaporation: [3, 5, 2, 1],
            temperature_variation: 3,
            rainfall_variation: 3,
        }
    }
    fn weather(&self, seed: u64, day: u64, season: usize) -> Weather {
        let random = mix(seed ^ day.wrapping_mul(0x9e3779b97f4a7c15));
        let temperature_width = u64::from(self.temperature_variation) * 2 + 1;
        let rain_width = u64::from(self.rainfall_variation) * 2 + 1;
        let temperature = i64::from(self.seasonal_temperature[season])
            + (random % temperature_width) as i64
            - i64::from(self.temperature_variation);
        let rain_adjustment =
            ((random >> 32) % rain_width) as i64 - i64::from(self.rainfall_variation);
        Weather {
            temperature: temperature as i32,
            rain: self.seasonal_rain[season].saturating_add_signed(rain_adjustment),
            evaporation: self.seasonal_evaporation[season],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Weather {
    pub temperature: i32,
    pub rain: u64,
    pub evaporation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropRules {
    pub planting_first_day: u32,
    pub planting_last_day: u32,
    pub prepare_labor: SimDuration,
    pub plant_labor: SimDuration,
    pub tend_labor: SimDuration,
    pub irrigation_labor: SimDuration,
    pub harvest_labor: SimDuration,
    pub seed_input: Quantity,
    pub irrigation_input: Quantity,
    pub irrigation_water: u64,
    pub base_temperature: i32,
    pub ripe_degree_days: u64,
    pub optimal_water: u64,
    pub failure_stress_days: u64,
    pub food_yield: Quantity,
    pub seed_yield: Quantity,
    pub soil_depletion: u16,
}
impl CropRules {
    pub fn grain() -> Self {
        let hour = 3_600_000;
        Self {
            planting_first_day: 0,
            planting_last_day: 91,
            prepare_labor: SimDuration(4 * hour),
            plant_labor: SimDuration(2 * hour),
            tend_labor: SimDuration(hour),
            irrigation_labor: SimDuration(hour),
            harvest_labor: SimDuration(4 * hour),
            seed_input: Quantity(1_000),
            irrigation_input: Quantity(1_000),
            irrigation_water: 30,
            base_temperature: 5,
            ripe_degree_days: 1_200,
            optimal_water: 50,
            failure_stress_days: 12,
            food_yield: Quantity(6_000_000),
            seed_yield: Quantity(2_000),
            soil_depletion: 20,
        }
    }
    fn labor(&self, operation: FarmOperation) -> u64 {
        match operation {
            FarmOperation::Prepare => self.prepare_labor.0,
            FarmOperation::Plant => self.plant_labor.0,
            FarmOperation::Tend => self.tend_labor.0,
            FarmOperation::Irrigate => self.irrigation_labor.0,
            FarmOperation::Harvest => self.harvest_labor.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CropPhase {
    Fallow,
    Prepared,
    Planting,
    Growing,
    Ripe,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub id: SiteId,
    pub area: AreaId,
    pub access: BTreeSet<PersonId>,
    pub seed: InventoryId,
    pub tool: InventoryId,
    pub water: InventoryId,
    pub food: InventoryId,
    pub phase: CropPhase,
    pub soil_fertility: u16,
    pub soil_water: u64,
    pub water_capacity: u64,
    pub preparation_ms: u64,
    pub planting_ms: u64,
    pub harvest_ms: u64,
    pub degree_days_milli: u64,
    pub stress_days: u64,
    pub tending: u16,
    /// Actual eligible cultivation time; no travel or idle time is credited.
    pub contributions: BTreeMap<PersonId, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cultivation {
    pub field: SiteId,
    pub operation: FarmOperation,
    pub labor_ms: u64,
    pub completes: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub calendar: Calendar,
    pub climate: Climate,
    pub crop: CropRules,
    pub weather: Weather,
    pub weather_day: u64,
    pub fields: BTreeMap<SiteId, Field>,
    pub jobs: BTreeMap<PersonId, Cultivation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub calendar: Calendar,
    pub climate: Climate,
    pub crop: CropRules,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            calendar: Calendar::human(),
            climate: Climate::temperate(),
            crop: CropRules::grain(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), Error> {
        let crop = &self.crop;
        if self.calendar.day.0 == 0
            || self.calendar.season_days.contains(&0)
            || u64::from(self.calendar.starting_day) >= self.calendar.year_days()
            || self.climate.temperature_variation > 100
            || self.climate.rainfall_variation > 100
            || self
                .climate
                .seasonal_temperature
                .iter()
                .any(|t| !(-100..=100).contains(t))
            || self
                .climate
                .seasonal_rain
                .iter()
                .chain(&self.climate.seasonal_evaporation)
                .any(|v| *v > 1_000_000)
            || crop.planting_first_day > crop.planting_last_day
            || u64::from(crop.planting_last_day) >= self.calendar.year_days()
            || [
                crop.prepare_labor.0,
                crop.plant_labor.0,
                crop.tend_labor.0,
                crop.irrigation_labor.0,
                crop.harvest_labor.0,
                crop.seed_input.0,
                crop.irrigation_input.0,
                crop.ripe_degree_days,
                crop.optimal_water,
                crop.failure_stress_days,
            ]
            .contains(&0)
            || crop.ripe_degree_days > u64::MAX / 1_000
            || crop.soil_depletion > 1_000
            || !(-100..=100).contains(&crop.base_temperature)
            || crop.food_yield.0 > u64::MAX - 3_000_000
            || crop.seed_yield.0 > u64::MAX - 10_000
        {
            return Err(Error::InvalidConfiguration(
                "invalid calendar, climate or crop rules".into(),
            ));
        }
        Ok(())
    }
}

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
            sim.interrupt_physical(actor);
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
        let Some(mut world) = self.state.environment.clone() else {
            return Ok(None);
        };
        let elapsed = next
            .0
            .checked_sub(self.state.time.0)
            .ok_or(Error::Overflow)?;
        for (actor, job) in &mut world.jobs {
            let field = world
                .fields
                .get_mut(&job.field)
                .ok_or_else(|| Error::InvalidSave("cultivation field missing".into()))?;
            job.labor_ms = job.labor_ms.checked_add(elapsed).ok_or(Error::Overflow)?;
            let entry = field.contributions.entry(*actor).or_default();
            *entry = entry.checked_add(elapsed).ok_or(Error::Overflow)?;
            let progress = match job.operation {
                FarmOperation::Prepare => Some(&mut field.preparation_ms),
                FarmOperation::Plant => Some(&mut field.planting_ms),
                FarmOperation::Harvest => Some(&mut field.harvest_ms),
                _ => None,
            };
            if let Some(progress) = progress {
                *progress = progress.checked_add(elapsed).ok_or(Error::Overflow)?;
            }
        }
        let day = next.0 / world.calendar.day.0;
        if day != world.weather_day {
            // The caller clips each interval to one day boundary. Growth uses
            // weather for the elapsed day, then publishes the following day.
            if day != world.weather_day + 1 {
                return Err(Error::InvalidSave("environment skipped a day".into()));
            }
            for field in world.fields.values_mut() {
                grow(field, &world.crop, &world.weather)?;
            }
            world.weather_day = day;
            world.weather =
                world
                    .climate
                    .weather(self.state.seed, day, world.calendar.season(next));
        }
        Ok(Some(world))
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

    fn cultivation_possible(
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
                    && stock.access.contains(&actor)
                    && self.inventory_area(stock) == Some(field.area)
                    && self.unreserved_stock(id, actor) >= quantity
            })
        };
        if !usable(field.tool, Good::Tool, 1_000) || !self.tool_available_for(actor, field.tool) {
            return false;
        }
        match operation {
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
        self.ensure_event_capacity(2)?;
        self.interrupt_physical(actor);
        if let Some((source, quantity)) = consume {
            self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
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
            self.interrupt_physical(actor);
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
        let world = self.state.environment.as_mut().unwrap();
        let crop = world.crop.clone();
        let field = world.fields.get_mut(&job.field).unwrap();
        let mut harvested = None;
        match job.operation {
            FarmOperation::Prepare => {
                field.phase = CropPhase::Prepared;
                field.preparation_ms = crop.prepare_labor.0;
            }
            FarmOperation::Plant => {
                field.phase = CropPhase::Growing;
                field.planting_ms = crop.plant_labor.0;
            }
            FarmOperation::Tend if field.phase == CropPhase::Growing => {
                field.tending = (field.tending + 250).min(1_000);
            }
            FarmOperation::Irrigate if field.phase == CropPhase::Growing => {
                field.soil_water = field
                    .soil_water
                    .saturating_add(crop.irrigation_water)
                    .min(field.water_capacity);
            }
            FarmOperation::Harvest if field.phase == CropPhase::Ripe && harvest_allowed => {
                let (food, seed) = yield_amounts(field, &crop);
                self.state
                    .inventories
                    .get_mut(&field.food)
                    .unwrap()
                    .quantity
                    .0 += food.0;
                self.state
                    .inventories
                    .get_mut(&field.seed)
                    .unwrap()
                    .quantity
                    .0 += seed.0;
                harvested = Some((food, seed));
                field.phase = CropPhase::Fallow;
                field.soil_fertility = field.soil_fertility.saturating_sub(crop.soil_depletion);
                field.preparation_ms = 0;
                field.planting_ms = 0;
                field.harvest_ms = 0;
                field.degree_days_milli = 0;
                field.stress_days = 0;
                field.tending = 0;
            }
            _ => {}
        }
        // If storage filled during harvest, hold the completed labor and crop.
        // A later attempt retries delivery without inventing a second work shift.
        if job.operation == FarmOperation::Harvest && harvested.is_none() {
            field.harvest_ms = crop.harvest_labor.0.saturating_sub(1);
        }
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
        for field in world.fields.values() {
            let operation = match field.phase {
                CropPhase::Fallow | CropPhase::Failed => FarmOperation::Prepare,
                CropPhase::Prepared | CropPhase::Planting => FarmOperation::Plant,
                CropPhase::Ripe => FarmOperation::Harvest,
                CropPhase::Growing if field.soil_water < world.crop.optimal_water / 2 => {
                    FarmOperation::Irrigate
                }
                CropPhase::Growing if field.tending < 1_000 => FarmOperation::Tend,
                _ => continue,
            };
            if self.cultivation_possible(actor, field.id, operation) {
                return Some(Command::Cultivate {
                    field: field.id,
                    operation,
                });
            }
        }
        None
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

fn grow(field: &mut Field, crop: &CropRules, weather: &Weather) -> Result<(), Error> {
    field.soil_water = field
        .soil_water
        .saturating_add(if weather.temperature > 0 {
            weather.rain
        } else {
            0
        })
        .min(field.water_capacity)
        .saturating_sub(weather.evaporation);
    if field.phase != CropPhase::Growing {
        return Ok(());
    }
    if field.soil_water == 0 || weather.temperature < 0 {
        field.stress_days = field.stress_days.checked_add(1).ok_or(Error::Overflow)?;
        if field.stress_days >= crop.failure_stress_days {
            field.phase = CropPhase::Failed;
            return Ok(());
        }
    }
    let heat = u64::try_from((weather.temperature - crop.base_temperature).max(0))
        .map_err(|_| Error::Overflow)?;
    let moisture = (u128::from(field.soil_water.min(crop.optimal_water)) * 1_000
        / u128::from(crop.optimal_water)) as u64;
    field.degree_days_milli = field
        .degree_days_milli
        .checked_add(heat.checked_mul(moisture).ok_or(Error::Overflow)?)
        .ok_or(Error::Overflow)?;
    if field.degree_days_milli >= crop.ripe_degree_days * 1_000 {
        field.phase = CropPhase::Ripe;
    }
    Ok(())
}
fn yield_amounts(field: &Field, crop: &CropRules) -> (Quantity, Quantity) {
    let quality = u128::from(field.soil_fertility) * u128::from(500 + field.tending / 2);
    let scale = |q: Quantity| Quantity((u128::from(q.0) * quality / 1_000_000) as u64);
    (scale(crop.food_yield), scale(crop.seed_yield))
}
fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
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
            sim.interrupt_physical(actor);
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
        sim.debug_physical_command(
            ruler,
            Command::Cultivate {
                field,
                operation: FarmOperation::Plant,
            },
        )
        .unwrap();
        sim.advance(SimDuration(4_999)).unwrap();
        sim.debug_physical_command(ruler, Command::Cancel).unwrap();
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
            sim.debug_physical_command(
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
            sim.debug_physical_command(
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
        sim.debug_physical_command(
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
        sim.debug_physical_command(ruler, Command::Rest).unwrap();
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
            sim.debug_physical_command(
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
