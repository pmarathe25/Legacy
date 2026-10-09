//! Configurable seasons and crop progress. External stock, tools and bodies are
//! provided by the coordinator; this state does not inspect simulation storage.
use game_core::*;
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
    pub fn weather(&self, seed: u64, day: u64, season: usize) -> Weather {
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
            || crop.soil_recovery > 1_000
            || crop.fallow_labor.0 == 0
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

pub fn grow(field: &mut Field, crop: &CropRules, weather: &Weather) -> Result<(), Error> {
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
fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

impl World {
    pub fn advance(&self, from: SimTime, next: SimTime, seed: u64) -> Result<Self, Error> {
        let mut world = self.clone();
        let elapsed = next.0.checked_sub(from.0).ok_or(Error::Overflow)?;
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
            world.weather = world
                .climate
                .weather(seed, day, world.calendar.season(next));
        }
        Ok(world)
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
/// Closing physical eligibility and storage capacity are external facts. This
/// transition describes output; it cannot credit an inventory or release a tool.
pub fn prepare_completion(
    field: &Field,
    job: &Cultivation,
    crop: &CropRules,
    harvest_allowed: bool,
) -> Result<CropCompletion, Error> {
    if field.id != job.field {
        return Err(Error::Unavailable);
    }
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
mod tests {
    use super::*;
    fn world() -> World {
        let mut crop = CropRules::grain();
        crop.planting_last_day = 9;
        crop.base_temperature = 0;
        crop.ripe_degree_days = 20;
        let calendar = Calendar {
            day: MINUTE,
            season_days: [10; 4],
            starting_day: 0,
        };
        let climate = Climate {
            seasonal_temperature: [10; 4],
            seasonal_rain: [10; 4],
            seasonal_evaporation: [2; 4],
            temperature_variation: 0,
            rainfall_variation: 0,
        };
        let field = Field {
            id: SiteId(1),
            area: AreaId(2),
            access: BTreeSet::from([PersonId(3)]),
            custodian: Some(PersonId(3)),
            seed: InventoryId(4),
            tool: InventoryId(5),
            water: InventoryId(6),
            food: InventoryId(7),
            phase: CropPhase::Growing,
            soil_fertility: 900,
            soil_water: 75,
            water_capacity: 120,
            preparation_ms: crop.prepare_labor.0,
            planting_ms: crop.plant_labor.0,
            harvest_ms: 0,
            degree_days_milli: 0,
            stress_days: 0,
            tending: 1_000,
            contributions: BTreeMap::new(),
        };
        World {
            weather: climate.weather(9, 0, 0),
            calendar,
            climate,
            crop,
            weather_day: 0,
            fields: BTreeMap::from([(field.id, field)]),
            jobs: BTreeMap::new(),
        }
    }
    #[test]
    fn seasonal_growth_and_harvest_describe_real_output_without_stock_storage() {
        let world = world();
        let before = world.clone();
        let day = world.advance(SimTime(0), SimTime(60_000), 9).unwrap();
        assert_eq!(world, before);
        assert_eq!(day.fields[&SiteId(1)].phase, CropPhase::Growing);
        let ripe = day.advance(SimTime(60_000), SimTime(120_000), 9).unwrap();
        assert_eq!(ripe.fields[&SiteId(1)].phase, CropPhase::Ripe);
        let job = Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Harvest,
            labor_ms: ripe.crop.harvest_labor.0,
            completes: SimTime(120_000),
        };
        let result = prepare_completion(&ripe.fields[&SiteId(1)], &job, &ripe.crop, true).unwrap();
        assert_eq!(result.harvest.unwrap().food, Quantity(5_400_000));
        assert_eq!(result.harvest.unwrap().seed, Quantity(1_800));
        assert_eq!(result.field.phase, CropPhase::Fallow);
        assert_eq!(result.field.soil_fertility, 880);
        let blocked =
            prepare_completion(&ripe.fields[&SiteId(1)], &job, &ripe.crop, false).unwrap();
        assert!(blocked.harvest.is_none());
        assert_eq!(blocked.field.phase, CropPhase::Ripe);
    }
    #[test]
    fn climate_failure_and_fallow_recovery_follow_owned_rules() {
        let mut world = world();
        world.crop.failure_stress_days = 2;
        world.fields.get_mut(&SiteId(1)).unwrap().soil_water = 0;
        world.weather.rain = 0;
        world.weather.evaporation = 0;
        world.climate.seasonal_rain = [0; 4];
        world.climate.seasonal_evaporation = [0; 4];
        let day = world.advance(SimTime(0), SimTime(60_000), 9).unwrap();
        let failed = day.advance(SimTime(60_000), SimTime(120_000), 9).unwrap();
        assert_eq!(failed.fields[&SiteId(1)].phase, CropPhase::Failed);
        let job = Cultivation {
            field: SiteId(1),
            operation: FarmOperation::Fallow,
            labor_ms: failed.crop.fallow_labor.0,
            completes: SimTime(120_000),
        };
        let mut field = prepare_completion(&failed.fields[&SiteId(1)], &job, &failed.crop, false)
            .unwrap()
            .field;
        field.soil_water = 50;
        let before = field.soil_fertility;
        grow(&mut field, &failed.crop, &failed.weather).unwrap();
        assert_eq!(field.soil_fertility, before + failed.crop.soil_recovery);
    }
    #[test]
    fn crop_progress_is_independent_of_intermediate_clock_slices() {
        let mut world = world();
        world.fields.get_mut(&SiteId(1)).unwrap().phase = CropPhase::Planting;
        world.fields.get_mut(&SiteId(1)).unwrap().planting_ms = 0;
        world.crop.plant_labor = SimDuration(120_000);
        world.jobs.insert(
            PersonId(3),
            Cultivation {
                field: SiteId(1),
                operation: FarmOperation::Plant,
                labor_ms: 0,
                completes: SimTime(120_000),
            },
        );
        let one = world.advance(SimTime(0), SimTime(60_000), 9).unwrap();
        let half = world.advance(SimTime(0), SimTime(17_001), 9).unwrap();
        let split = half.advance(SimTime(17_001), SimTime(60_000), 9).unwrap();
        assert_eq!(one, split);
    }
}
