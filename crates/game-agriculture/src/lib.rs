//! Agricultural production: crop state and work produce explicit material effects.
use game_core::*;
use game_environment::{DayFacts, Weather};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FarmOperation {
    Prepare,
    Plant,
    Tend,
    Irrigate,
    Harvest,
    Fallow,
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
    #[serde(default = "default_soil_recovery")]
    pub soil_recovery: u16,
    #[serde(default = "default_fallow_labor")]
    pub fallow_labor: SimDuration,
}
fn default_soil_recovery() -> u16 {
    1
}
fn default_fallow_labor() -> SimDuration {
    SimDuration(3_600_000)
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
            soil_recovery: default_soil_recovery(),
            fallow_labor: default_fallow_labor(),
        }
    }
    pub fn labor(&self, operation: FarmOperation) -> u64 {
        match operation {
            FarmOperation::Fallow => self.fallow_labor.0,
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
    #[serde(default)]
    pub custodian: Option<PersonId>,
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
impl Field {
    pub fn validate(&self, crop: &CropRules) -> Result<(), Error> {
        if self.soil_fertility > 1_000
            || self.water_capacity == 0
            || self.soil_water > self.water_capacity
            || self.tending > 1_000
            || self.preparation_ms > crop.prepare_labor.0
            || self.planting_ms > crop.plant_labor.0
            || self.harvest_ms > crop.harvest_labor.0
        {
            return Err(Error::InvalidConfiguration(
                "invalid cultivated field".into(),
            ));
        }
        Ok(())
    }
    pub fn validate_phase(&self, crop: &CropRules) -> Result<(), Error> {
        let phase_valid = match self.phase {
            CropPhase::Fallow => {
                self.planting_ms == 0 && self.degree_days_milli == 0 && self.harvest_ms == 0
            }
            CropPhase::Prepared => {
                self.preparation_ms == crop.prepare_labor.0
                    && self.planting_ms == 0
                    && self.degree_days_milli == 0
                    && self.harvest_ms == 0
            }
            CropPhase::Planting => {
                self.preparation_ms == crop.prepare_labor.0
                    && self.planting_ms < crop.plant_labor.0
                    && self.degree_days_milli == 0
                    && self.harvest_ms == 0
            }
            CropPhase::Growing | CropPhase::Failed => {
                self.preparation_ms == crop.prepare_labor.0
                    && self.planting_ms == crop.plant_labor.0
                    && self.degree_days_milli < crop.ripe_degree_days.saturating_mul(1_000)
                    && self.harvest_ms == 0
            }
            CropPhase::Ripe => {
                self.preparation_ms == crop.prepare_labor.0
                    && self.planting_ms == crop.plant_labor.0
                    && self.degree_days_milli >= crop.ripe_degree_days.saturating_mul(1_000)
            }
        };
        if !phase_valid {
            return Err(Error::InvalidConfiguration(
                "inconsistent crop phase".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cultivation {
    pub field: SiteId,
    pub operation: FarmOperation,
    pub labor_ms: u64,
    pub completes: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub crop: CropRules,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            crop: CropRules::grain(),
        }
    }
}
impl Config {
    pub fn validate(&self, year_days: u64) -> Result<(), Error> {
        let crop = &self.crop;
        if year_days == 0
            || crop.planting_first_day > crop.planting_last_day
            || u64::from(crop.planting_last_day) >= year_days
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
                crop.fallow_labor.0,
            ]
            .contains(&0)
            || crop.ripe_degree_days > u64::MAX / 1_000
            || crop.soil_depletion > 1_000
            || crop.soil_recovery > 1_000
            || !(-100..=100).contains(&crop.base_temperature)
            || crop.food_yield.0 > u64::MAX - 3_000_000
            || crop.seed_yield.0 > u64::MAX - 10_000
        {
            return Err(Error::InvalidConfiguration("invalid crop rules".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub crop: CropRules,
    pub fields: BTreeMap<SiteId, Field>,
    pub jobs: BTreeMap<PersonId, Cultivation>,
    /// Last consumed environmental day, not an independently advanced clock.
    pub day: u64,
}
impl World {
    pub fn validate(&self, now: SimTime, year_days: u64) -> Result<(), Error> {
        Config {
            crop: self.crop.clone(),
        }
        .validate(year_days)?;
        let bad = || Error::InvalidSave("inconsistent cultivation progress".into());
        let mut fields_in_use = BTreeSet::new();
        let mut contributed: BTreeMap<PersonId, u128> = BTreeMap::new();
        for (id, field) in &self.fields {
            if *id != field.id {
                return Err(bad());
            }
            field.validate(&self.crop)?;
            field.validate_phase(&self.crop)?;
            for (actor, elapsed) in &field.contributions {
                let total = contributed.entry(*actor).or_default();
                *total = total
                    .checked_add(u128::from(*elapsed))
                    .ok_or(Error::Overflow)?;
            }
        }
        if contributed
            .values()
            .any(|elapsed| *elapsed > u128::from(now.0))
        {
            return Err(bad());
        }
        for job in self.jobs.values() {
            let field = self.fields.get(&job.field).ok_or_else(bad)?;
            if job.completes <= now || !fields_in_use.insert(job.field) {
                return Err(bad());
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
            let progress = match job.operation {
                FarmOperation::Prepare => field.preparation_ms,
                FarmOperation::Plant => field.planting_ms,
                FarmOperation::Harvest => field.harvest_ms,
                _ => job.labor_ms,
            };
            if !phase_valid
                || job.labor_ms > progress
                || progress >= self.crop.labor(job.operation)
                || job.completes.0 - now.0 != self.crop.labor(job.operation) - progress
            {
                return Err(bad());
            }
        }
        Ok(())
    }
    pub fn advance(
        &self,
        from: SimTime,
        next: SimTime,
        day: Option<&DayFacts>,
    ) -> Result<Self, Error> {
        for field in self.fields.values() {
            field.validate(&self.crop)?;
        }
        let mut occupied = BTreeSet::new();
        if self.jobs.values().any(|job| !occupied.insert(job.field)) {
            return Err(Error::InvalidSave(
                "several cultivation jobs share one field".into(),
            ));
        }
        let mut world = self.clone();
        let elapsed = next.0.checked_sub(from.0).ok_or(Error::Overflow)?;
        for (actor, job) in &mut world.jobs {
            if next > job.completes {
                return Err(Error::InvalidSave("cultivation passed completion".into()));
            }
            let field = world
                .fields
                .get_mut(&job.field)
                .ok_or_else(|| Error::InvalidSave("cultivation field missing".into()))?;
            job.labor_ms = job.labor_ms.checked_add(elapsed).ok_or(Error::Overflow)?;
            let contribution = field.contributions.entry(*actor).or_default();
            *contribution = contribution.checked_add(elapsed).ok_or(Error::Overflow)?;
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
        if let Some(day) = day {
            if day.day != world.day.checked_add(1).ok_or(Error::Overflow)? {
                return Err(Error::InvalidSave(
                    "agriculture skipped or repeated environmental day".into(),
                ));
            }
            for field in world.fields.values_mut() {
                grow(field, &world.crop, &day.weather)?;
            }
            world.day = day.day;
        }
        Ok(world)
    }
}
pub fn grow(field: &mut Field, crop: &CropRules, weather: &Weather) -> Result<(), Error> {
    let mut next = field.clone();
    integrate_growth(&mut next, crop, weather)?;
    *field = next;
    Ok(())
}
fn integrate_growth(field: &mut Field, crop: &CropRules, weather: &Weather) -> Result<(), Error> {
    weather.validate()?;
    field.validate(crop)?;
    if crop.optimal_water == 0
        || crop.ripe_degree_days > u64::MAX / 1_000
        || !(-100..=100).contains(&crop.base_temperature)
        || crop.soil_recovery > 1_000
    {
        return Err(Error::InvalidConfiguration(
            "invalid crop growth scale".into(),
        ));
    }
    field.soil_water = field
        .soil_water
        .saturating_add(if weather.temperature > 0 {
            weather.rain
        } else {
            0
        })
        .min(field.water_capacity)
        .saturating_sub(weather.evaporation);
    if field.phase == CropPhase::Fallow
        && weather.temperature > crop.base_temperature
        && field.soil_water > 0
    {
        field.soil_fertility = field
            .soil_fertility
            .saturating_add(crop.soil_recovery)
            .min(1_000);
    }
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
pub fn yield_amounts(field: &Field, crop: &CropRules) -> (Quantity, Quantity) {
    let quality = u128::from(field.soil_fertility) * u128::from(500 + field.tending / 2);
    let scale = |q: Quantity| Quantity((u128::from(q.0) * quality / 1_000_000) as u64);
    (scale(crop.food_yield), scale(crop.seed_yield))
}
pub fn next_operation(field: &Field, crop: &CropRules) -> Option<FarmOperation> {
    match field.phase {
        CropPhase::Fallow | CropPhase::Failed => Some(FarmOperation::Prepare),
        CropPhase::Prepared | CropPhase::Planting => Some(FarmOperation::Plant),
        CropPhase::Ripe => Some(FarmOperation::Harvest),
        CropPhase::Growing if field.soil_water < crop.optimal_water / 2 => {
            Some(FarmOperation::Irrigate)
        }
        CropPhase::Growing if field.tending < 1_000 => Some(FarmOperation::Tend),
        _ => None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CropCompletion {
    pub field: Field,
    pub harvest: Option<HarvestOutput>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarvestOutput {
    pub food_inventory: InventoryId,
    pub seed_inventory: InventoryId,
    pub food: Quantity,
    pub seed: Quantity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputConsumption {
    pub inventory: InventoryId,
    pub quantity: Quantity,
}
/// The coordinator supplies actual external capability and stock availability;
/// the field retains its own physical access and crop-state prerequisites.
#[derive(Clone, Copy, Debug)]
pub struct BeginFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub area: AreaId,
    pub capable: bool,
    pub tool_available: bool,
    pub field_available: bool,
    pub seed_available: Quantity,
    pub water_available: Quantity,
    pub outputs_accessible: bool,
    pub food_room: Quantity,
    pub seed_room: Quantity,
    pub day_of_year: u64,
    pub temperature_c: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeginEffect {
    pub field: Field,
    pub job: Cultivation,
    pub consumed: Option<InputConsumption>,
}
pub fn prepare_begin(
    field: &Field,
    crop: &CropRules,
    operation: FarmOperation,
    facts: BeginFacts,
) -> Result<BeginEffect, Error> {
    field.validate(crop)?;
    if !facts.capable
        || !facts.tool_available
        || !facts.field_available
        || facts.area != field.area
        || !field.access.contains(&facts.actor)
    {
        return Err(Error::Unavailable);
    }
    let (food, seed) = yield_amounts(field, crop);
    let allowed = match operation {
        FarmOperation::Fallow => field.phase != CropPhase::Fallow,
        FarmOperation::Prepare => {
            matches!(field.phase, CropPhase::Fallow | CropPhase::Failed) && facts.temperature_c > 0
        }
        FarmOperation::Plant => {
            field.phase == CropPhase::Planting
                || field.phase == CropPhase::Prepared
                    && (u64::from(crop.planting_first_day)..=u64::from(crop.planting_last_day))
                        .contains(&facts.day_of_year)
                    && facts.seed_available >= crop.seed_input
        }
        FarmOperation::Tend => field.phase == CropPhase::Growing && field.tending < 1_000,
        FarmOperation::Irrigate => {
            field.phase == CropPhase::Growing
                && field.soil_water < field.water_capacity
                && facts.water_available >= crop.irrigation_input
        }
        FarmOperation::Harvest => {
            field.phase == CropPhase::Ripe
                && facts.outputs_accessible
                && facts.food_room >= food
                && facts.seed_room >= seed
        }
    };
    if !allowed {
        return Err(Error::Unavailable);
    }
    let progress = match operation {
        FarmOperation::Prepare if field.phase == CropPhase::Failed => 0,
        FarmOperation::Prepare => field.preparation_ms,
        FarmOperation::Plant => field.planting_ms,
        FarmOperation::Harvest => field.harvest_ms,
        _ => 0,
    };
    let remaining = crop
        .labor(operation)
        .checked_sub(progress)
        .ok_or(Error::Overflow)?;
    if remaining == 0 {
        return Err(Error::Unavailable);
    }
    let completes = SimTime(facts.now.0.checked_add(remaining).ok_or(Error::Overflow)?);
    let consumed = match operation {
        FarmOperation::Plant if field.phase == CropPhase::Prepared => Some(InputConsumption {
            inventory: field.seed,
            quantity: crop.seed_input,
        }),
        FarmOperation::Irrigate => Some(InputConsumption {
            inventory: field.water,
            quantity: crop.irrigation_input,
        }),
        _ => None,
    };
    let mut field = field.clone();
    if operation == FarmOperation::Plant {
        field.phase = CropPhase::Planting;
    }
    if operation == FarmOperation::Prepare && field.phase == CropPhase::Failed {
        field.phase = CropPhase::Fallow;
        field.preparation_ms = 0;
        field.planting_ms = 0;
        field.harvest_ms = 0;
        field.degree_days_milli = 0;
        field.stress_days = 0;
        field.tending = 0;
    }
    Ok(BeginEffect {
        field: field.clone(),
        job: Cultivation {
            field: field.id,
            operation,
            labor_ms: 0,
            completes,
        },
        consumed,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct CompletionFacts {
    pub now: SimTime,
    pub eligible: bool,
    pub outputs_accessible: bool,
    pub food_room: Quantity,
    pub seed_room: Quantity,
}
/// Closing physical eligibility and storage capacity are external facts. This
/// transition describes output; it cannot credit an inventory or release a tool.
pub fn prepare_completion(
    field: &Field,
    job: &Cultivation,
    crop: &CropRules,
    facts: CompletionFacts,
) -> Result<CropCompletion, Error> {
    field.validate(crop)?;
    if field.id != job.field || !facts.eligible || facts.now < job.completes {
        return Err(Error::Unavailable);
    }
    let (food, seed) = yield_amounts(field, crop);
    let harvest_allowed =
        facts.outputs_accessible && facts.food_room >= food && facts.seed_room >= seed;
    let mut field = field.clone();
    let mut harvest = None;
    match job.operation {
        FarmOperation::Fallow => {
            field.phase = CropPhase::Fallow;
            field.preparation_ms = 0;
            field.planting_ms = 0;
            field.harvest_ms = 0;
            field.degree_days_milli = 0;
            field.stress_days = 0;
            field.tending = 0;
        }
        FarmOperation::Prepare => {
            field.phase = CropPhase::Prepared;
            field.preparation_ms = crop.prepare_labor.0;
        }
        FarmOperation::Plant => {
            field.phase = CropPhase::Growing;
            field.planting_ms = crop.plant_labor.0;
        }
        FarmOperation::Tend if field.phase == CropPhase::Growing => {
            field.tending = (field.tending + 250).min(1_000)
        }
        FarmOperation::Irrigate if field.phase == CropPhase::Growing => {
            field.soil_water = field
                .soil_water
                .saturating_add(crop.irrigation_water)
                .min(field.water_capacity)
        }
        FarmOperation::Harvest if field.phase == CropPhase::Ripe && harvest_allowed => {
            let (food, seed) = yield_amounts(&field, crop);
            harvest = Some(HarvestOutput {
                food_inventory: field.food,
                seed_inventory: field.seed,
                food,
                seed,
            });
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
    if job.operation == FarmOperation::Harvest && harvest.is_none() {
        field.harvest_ms = crop.harvest_labor.0.saturating_sub(1);
    }
    Ok(CropCompletion { field, harvest })
}

#[cfg(test)]
mod tests;
