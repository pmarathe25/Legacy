//! Agricultural production: crop state and work produce explicit material effects.
use game_core::*;
pub use game_land::Soil;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod advancing;
mod storage;
pub use advancing::{GrowthInput, GrowthOutput, PreparedAdvance};
pub use storage::{Queries, register_components};
pub use storage::{
    cultivation, cultivation_mut, cultivations, insert_cultivation, remove_cultivation,
};
/// Facts supplied by a local weather provider; no climate runtime is required.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeatherFacts {
    pub temperature: i32,
    pub rain: u64,
    pub evaporation: u64,
}
impl WeatherFacts {
    pub fn validate(&self) -> Result<(), Error> {
        if !(-200..=200).contains(&self.temperature) {
            return Err(Error::InvalidConfiguration(
                "invalid local crop weather".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalWeatherDay {
    pub day: u64,
    pub day_of_year: u64,
    pub cells: BTreeMap<AreaId, WeatherFacts>,
}
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
#[serde(deny_unknown_fields)]
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
    pub soil_recovery: u16,
    pub fallow_labor: SimDuration,
}
impl CropRules {
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
}
/// Pure crop state, attached to the same site entity as its field and soil.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crop {
    pub definition: CropId,
    pub phase: CropPhase,
    pub preparation_ms: u64,
    pub planting_ms: u64,
    pub harvest_ms: u64,
    pub degree_days_milli: u64,
    pub stress_days: u64,
    pub tending: u16,
    /// Actual eligible cultivation time; no travel or idle time is credited.
    pub contributions: BTreeMap<PersonId, u64>,
}
impl Crop {
    pub fn new(definition: CropId) -> Self {
        Self {
            definition,
            phase: CropPhase::Fallow,
            preparation_ms: 0,
            planting_ms: 0,
            harvest_ms: 0,
            degree_days_milli: 0,
            stress_days: 0,
            tending: 0,
            contributions: BTreeMap::new(),
        }
    }
    pub fn validate(&self, crop: &CropRules) -> Result<(), Error> {
        if self.tending > 1_000
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
#[serde(deny_unknown_fields)]
pub struct Cultivation {
    pub field: SiteId,
    pub operation: FarmOperation,
    pub labor_ms: u64,
    pub completes: SimTime,
}

/// Stable authored identifier; adding a crop does not change Rust types.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CropId(pub String);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub default_crop: CropId,
    pub crops: BTreeMap<CropId, CropRules>,
}
impl Catalog {
    #[cfg(feature = "content")]
    pub fn read(path: impl AsRef<std::path::Path>) -> Result<game_content::Document<Self>, Error> {
        let document: game_content::Document<Self> = game_content::read(path)?;
        document.data.validate(None)?;
        Ok(document)
    }
    pub fn validate(&self, year_days: Option<u64>) -> Result<(), Error> {
        if !self.crops.contains_key(&self.default_crop)
            || self.crops.keys().any(|id| {
                id.0.is_empty()
                    || !id
                        .0
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
            })
        {
            return Err(Error::InvalidConfiguration("invalid crop catalog".into()));
        }
        for crop in self.crops.values() {
            crop.validate(year_days)?;
        }
        Ok(())
    }
}
impl CropRules {
    pub fn validate(&self, year_days: Option<u64>) -> Result<(), Error> {
        let crop = self;
        if year_days.is_some_and(|days| days == 0 || u64::from(crop.planting_last_day) >= days)
            || crop.planting_first_day > crop.planting_last_day
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
#[serde(deny_unknown_fields)]
pub struct Runtime {
    #[cfg(feature = "content")]
    pub content: Option<game_content::Stamp>,
    pub catalog: Catalog,
    /// Last consumed environmental day, not an independently advanced clock.
    pub day: u64,
}
impl Runtime {
    pub fn new(catalog: Catalog, day: u64) -> Result<Self, Error> {
        catalog.validate(None)?;
        Ok(Self {
            #[cfg(feature = "content")]
            content: None,
            catalog,
            day,
        })
    }
    #[cfg(feature = "content")]
    pub fn from_content(
        document: game_content::Document<Catalog>,
        day: u64,
    ) -> Result<Self, Error> {
        let stamp = document.stamp()?;
        let mut world = Self::new(document.data, day)?;
        world.content = Some(stamp);
        Ok(world)
    }
    pub fn rules_for(&self, store: &game_ecs::Store, field: SiteId) -> Result<&CropRules, Error> {
        let crop = self.crop(store, field).ok_or(Error::Unavailable)?;
        self.catalog
            .crops
            .get(&crop.definition)
            .ok_or_else(|| Error::InvalidSave("unknown crop definition".into()))
    }
    pub fn field_bundle<'a>(
        &'a self,
        store: &'a game_ecs::Store,
        field: SiteId,
    ) -> Result<(&'a Field, &'a Crop, &'a Soil, &'a CropRules), Error> {
        Ok((
            self.field(store, field).ok_or(Error::Unavailable)?,
            self.crop(store, field).ok_or(Error::Unavailable)?,
            self.soil(store, field).ok_or(Error::Unavailable)?,
            self.rules_for(store, field)?,
        ))
    }
    pub fn validate(
        &self,
        store: &game_ecs::Store,
        now: SimTime,
        year_days: Option<u64>,
    ) -> Result<(), Error> {
        self.catalog.validate(year_days)?;
        #[cfg(feature = "content")]
        if let Some(stamp) = &self.content {
            let actual = game_content::Document {
                id: stamp.id.clone(),
                version: stamp.version,
                data: &self.catalog,
            }
            .stamp()
            .map_err(|_| Error::InvalidSave("invalid crop content provenance".into()))?;
            if actual != *stamp {
                return Err(Error::InvalidSave(
                    "crop definitions differ from content provenance".into(),
                ));
            }
        }
        let bad = || Error::InvalidSave("inconsistent cultivation progress".into());
        for (id, crop) in self.crops(store) {
            self.field(store, id).ok_or_else(bad)?;
            let rules = self.catalog.crops.get(&crop.definition).ok_or_else(bad)?;
            crop.validate(rules)?;
            crop.validate_phase(rules)?;
            if let Some(soil) = self.soil(store, id) {
                soil.validate()?;
            }
        }
        let mut fields_in_use = BTreeSet::new();
        let mut contributed: BTreeMap<PersonId, u128> = BTreeMap::new();
        for (id, field) in self.fields(store) {
            if id != field.id {
                return Err(bad());
            }
            let crop = self.crop(store, id).ok_or_else(bad)?;
            let soil = self.soil(store, id).ok_or_else(bad)?;
            let rules = self.catalog.crops.get(&crop.definition).ok_or_else(bad)?;
            crop.validate(rules)?;
            crop.validate_phase(rules)?;
            soil.validate()?;
            for (actor, elapsed) in &crop.contributions {
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
        for (_, job) in self.cultivations(store) {
            job.validate()?;
            self.field(store, job.field).ok_or_else(bad)?;
            let crop = self.crop(store, job.field).ok_or_else(bad)?;
            let rules = self.catalog.crops.get(&crop.definition).ok_or_else(bad)?;
            if job.completes <= now || !fields_in_use.insert(job.field) {
                return Err(bad());
            }
            let phase_valid = match job.operation {
                FarmOperation::Fallow => crop.phase != CropPhase::Fallow,
                FarmOperation::Prepare => crop.phase == CropPhase::Fallow,
                FarmOperation::Plant => crop.phase == CropPhase::Planting,
                FarmOperation::Tend | FarmOperation::Irrigate => {
                    matches!(crop.phase, CropPhase::Growing | CropPhase::Failed)
                }
                FarmOperation::Harvest => crop.phase == CropPhase::Ripe,
            };
            let progress = match job.operation {
                FarmOperation::Prepare => crop.preparation_ms,
                FarmOperation::Plant => crop.planting_ms,
                FarmOperation::Harvest => crop.harvest_ms,
                _ => job.labor_ms,
            };
            if !phase_valid
                || job.labor_ms > progress
                || progress >= rules.labor(job.operation)
                || job.completes.0 - now.0 != rules.labor(job.operation) - progress
            {
                return Err(bad());
            }
        }
        Ok(())
    }
}
/// A daily system over a crop, its local soil, its definition, and local weather.
/// Returns component changes without touching field ownership or inventories.
pub fn grow(
    crop: &Crop,
    soil: &Soil,
    rules: &CropRules,
    weather: &WeatherFacts,
) -> Result<(Crop, Soil), Error> {
    let mut next_crop = crop.clone();
    let mut next_soil = soil.clone();
    integrate_growth(&mut next_crop, &mut next_soil, rules, weather)?;
    Ok((next_crop, next_soil))
}
fn integrate_growth(
    state: &mut Crop,
    soil: &mut Soil,
    crop: &CropRules,
    weather: &WeatherFacts,
) -> Result<(), Error> {
    weather.validate()?;
    crop.validate(None)?;
    state.validate(crop)?;
    soil.validate()?;
    if crop.optimal_water == 0
        || crop.ripe_degree_days > u64::MAX / 1_000
        || !(-100..=100).contains(&crop.base_temperature)
        || crop.soil_recovery > 1_000
    {
        return Err(Error::InvalidConfiguration(
            "invalid crop growth scale".into(),
        ));
    }
    soil.water = soil
        .water
        .saturating_add(if weather.temperature > 0 {
            weather.rain
        } else {
            0
        })
        .min(soil.capacity)
        .saturating_sub(weather.evaporation);
    if state.phase == CropPhase::Fallow
        && weather.temperature > crop.base_temperature
        && soil.water > 0
    {
        soil.fertility = soil.fertility.saturating_add(crop.soil_recovery).min(1_000);
    }
    if state.phase != CropPhase::Growing {
        return Ok(());
    }
    if soil.water == 0 || weather.temperature < 0 {
        state.stress_days = state.stress_days.checked_add(1).ok_or(Error::Overflow)?;
        if state.stress_days >= crop.failure_stress_days {
            state.phase = CropPhase::Failed;
            return Ok(());
        }
    }
    let heat = u64::try_from((weather.temperature - crop.base_temperature).max(0))
        .map_err(|_| Error::Overflow)?;
    let moisture = (u128::from(soil.water.min(crop.optimal_water)) * 1_000
        / u128::from(crop.optimal_water)) as u64;
    state.degree_days_milli = state
        .degree_days_milli
        .checked_add(heat.checked_mul(moisture).ok_or(Error::Overflow)?)
        .ok_or(Error::Overflow)?;
    if state.degree_days_milli >= crop.ripe_degree_days * 1_000 {
        state.phase = CropPhase::Ripe;
    }
    Ok(())
}
pub fn yield_amounts(state: &Crop, soil: &Soil, crop: &CropRules) -> (Quantity, Quantity) {
    let quality = u128::from(soil.fertility) * u128::from(500 + state.tending / 2);
    let scale = |q: Quantity| Quantity((u128::from(q.0) * quality / 1_000_000) as u64);
    (scale(crop.food_yield), scale(crop.seed_yield))
}
pub fn next_operation(state: &Crop, soil: &Soil, crop: &CropRules) -> Option<FarmOperation> {
    match state.phase {
        CropPhase::Fallow | CropPhase::Failed => Some(FarmOperation::Prepare),
        CropPhase::Prepared | CropPhase::Planting => Some(FarmOperation::Plant),
        CropPhase::Ripe => Some(FarmOperation::Harvest),
        CropPhase::Growing if soil.water < crop.optimal_water / 2 => Some(FarmOperation::Irrigate),
        CropPhase::Growing if state.tending < 1_000 => Some(FarmOperation::Tend),
        _ => None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CropCompletion {
    pub crop: Crop,
    pub soil: Soil,
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
    pub day_of_year: Option<u64>,
    pub temperature_c: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeginEffect {
    pub crop: Crop,
    pub job: Cultivation,
    pub consumed: Option<InputConsumption>,
}
pub fn prepare_begin(
    field: &Field,
    state: &Crop,
    soil: &Soil,
    crop: &CropRules,
    operation: FarmOperation,
    facts: BeginFacts,
) -> Result<BeginEffect, Error> {
    state.validate(crop)?;
    soil.validate()?;
    if !facts.capable
        || !facts.tool_available
        || !facts.field_available
        || facts.area != field.area
        || !field.access.contains(&facts.actor)
    {
        return Err(Error::Unavailable);
    }
    let (food, seed) = yield_amounts(state, soil, crop);
    let allowed = match operation {
        FarmOperation::Fallow => state.phase != CropPhase::Fallow,
        FarmOperation::Prepare => {
            matches!(state.phase, CropPhase::Fallow | CropPhase::Failed)
                && facts
                    .temperature_c
                    .is_some_and(|temperature| temperature > 0)
        }
        FarmOperation::Plant => {
            state.phase == CropPhase::Planting
                || state.phase == CropPhase::Prepared
                    && facts.day_of_year.is_some_and(|day| {
                        (u64::from(crop.planting_first_day)..=u64::from(crop.planting_last_day))
                            .contains(&day)
                    })
                    && facts.seed_available >= crop.seed_input
        }
        FarmOperation::Tend => state.phase == CropPhase::Growing && state.tending < 1_000,
        FarmOperation::Irrigate => {
            state.phase == CropPhase::Growing
                && soil.water < soil.capacity
                && facts.water_available >= crop.irrigation_input
        }
        FarmOperation::Harvest => {
            state.phase == CropPhase::Ripe
                && facts.outputs_accessible
                && facts.food_room >= food
                && facts.seed_room >= seed
        }
    };
    if !allowed {
        return Err(Error::Unavailable);
    }
    let progress = match operation {
        FarmOperation::Prepare if state.phase == CropPhase::Failed => 0,
        FarmOperation::Prepare => state.preparation_ms,
        FarmOperation::Plant => state.planting_ms,
        FarmOperation::Harvest => state.harvest_ms,
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
        FarmOperation::Plant if state.phase == CropPhase::Prepared => Some(InputConsumption {
            inventory: field.seed,
            quantity: crop.seed_input,
        }),
        FarmOperation::Irrigate => Some(InputConsumption {
            inventory: field.water,
            quantity: crop.irrigation_input,
        }),
        _ => None,
    };
    let mut state = state.clone();
    if operation == FarmOperation::Plant {
        state.phase = CropPhase::Planting;
    }
    if operation == FarmOperation::Prepare && state.phase == CropPhase::Failed {
        state.phase = CropPhase::Fallow;
        state.preparation_ms = 0;
        state.planting_ms = 0;
        state.harvest_ms = 0;
        state.degree_days_milli = 0;
        state.stress_days = 0;
        state.tending = 0;
    }
    Ok(BeginEffect {
        crop: state,
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
    state: &Crop,
    soil: &Soil,
    job: &Cultivation,
    crop: &CropRules,
    facts: CompletionFacts,
) -> Result<CropCompletion, Error> {
    state.validate(crop)?;
    soil.validate()?;
    if field.id != job.field || !facts.eligible || facts.now < job.completes {
        return Err(Error::Unavailable);
    }
    let (food, seed) = yield_amounts(state, soil, crop);
    let harvest_allowed =
        facts.outputs_accessible && facts.food_room >= food && facts.seed_room >= seed;
    let mut state = state.clone();
    let mut soil = soil.clone();
    let mut harvest = None;
    match job.operation {
        FarmOperation::Fallow => {
            state.phase = CropPhase::Fallow;
            state.preparation_ms = 0;
            state.planting_ms = 0;
            state.harvest_ms = 0;
            state.degree_days_milli = 0;
            state.stress_days = 0;
            state.tending = 0;
        }
        FarmOperation::Prepare => {
            state.phase = CropPhase::Prepared;
            state.preparation_ms = crop.prepare_labor.0;
        }
        FarmOperation::Plant => {
            state.phase = CropPhase::Growing;
            state.planting_ms = crop.plant_labor.0;
        }
        FarmOperation::Tend if state.phase == CropPhase::Growing => {
            state.tending = (state.tending + 250).min(1_000)
        }
        FarmOperation::Irrigate if state.phase == CropPhase::Growing => {
            soil.water = soil
                .water
                .saturating_add(crop.irrigation_water)
                .min(soil.capacity)
        }
        FarmOperation::Harvest if state.phase == CropPhase::Ripe && harvest_allowed => {
            let (food, seed) = yield_amounts(&state, &soil, crop);
            harvest = Some(HarvestOutput {
                food_inventory: field.food,
                seed_inventory: field.seed,
                food,
                seed,
            });
            state.phase = CropPhase::Fallow;
            soil.fertility = soil.fertility.saturating_sub(crop.soil_depletion);
            state.preparation_ms = 0;
            state.planting_ms = 0;
            state.harvest_ms = 0;
            state.degree_days_milli = 0;
            state.stress_days = 0;
            state.tending = 0;
        }
        _ => {}
    }
    if job.operation == FarmOperation::Harvest && harvest.is_none() {
        state.harvest_ms = crop.harvest_labor.0.saturating_sub(1);
    }
    Ok(CropCompletion {
        crop: state,
        soil,
        harvest,
    })
}

#[cfg(test)]
mod tests;

/// Project a promised share from a remembered fertility measurement. Agreement
/// preferences may discount this quantity; they never compute crop production.
pub fn projected_food_share(
    crop: &CropRules,
    fertility: u16,
    numerator: u64,
    denominator: u64,
) -> Result<u128, Error> {
    if fertility > 1_000 || denominator == 0 || numerator > denominator {
        return Err(Error::InvalidConfiguration(
            "invalid projected crop share".into(),
        ));
    }
    u128::from(crop.food_yield.0)
        .checked_mul(u128::from(fertility))
        .and_then(|yield_at_fertility| yield_at_fertility.checked_mul(u128::from(numerator)))
        .map(|scaled| scaled / u128::from(denominator) / 1_000)
        .ok_or(Error::Overflow)
}

#[cfg(test)]
mod free_port_tests;
