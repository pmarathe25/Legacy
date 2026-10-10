//! Calendar and deterministic weather. This domain owns no crops, people or goods.
use game_core::{AreaId, Error, SimDuration, SimTime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
impl Weather {
    pub fn validate(&self) -> Result<(), Error> {
        if !(-200..=200).contains(&self.temperature)
            || self.rain > 1_000_100
            || self.evaporation > 1_000_000
        {
            return Err(Error::InvalidConfiguration(
                "invalid daily weather forcing".into(),
            ));
        }
        Ok(())
    }
}

/// Local forcing policy. Neutral values preserve the regional weather exactly.
/// Region-specific offsets and scales are authored data, rather than area types
/// interpreted by the weather system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalClimate {
    pub temperature_offset: i32,
    pub rain_multiplier_milli: u16,
    pub evaporation_multiplier_milli: u16,
}
impl Default for LocalClimate {
    fn default() -> Self {
        Self {
            temperature_offset: 0,
            rain_multiplier_milli: 1_000,
            evaporation_multiplier_milli: 1_000,
        }
    }
}
impl LocalClimate {
    pub fn validate(&self) -> Result<(), Error> {
        if !(-100..=100).contains(&self.temperature_offset)
            || self.rain_multiplier_milli > 10_000
            || self.evaporation_multiplier_milli > 10_000
        {
            return Err(Error::InvalidConfiguration(
                "invalid local climate policy".into(),
            ));
        }
        Ok(())
    }
    pub fn apply(&self, weather: &Weather) -> Result<Weather, Error> {
        self.validate()?;
        weather.validate()?;
        let scale = |quantity: u64, factor: u16| {
            u64::try_from(u128::from(quantity) * u128::from(factor) / 1_000)
                .map_err(|_| Error::Overflow)
        };
        let adjusted = Weather {
            temperature: weather
                .temperature
                .checked_add(self.temperature_offset)
                .ok_or(Error::Overflow)?,
            rain: scale(weather.rain, self.rain_multiplier_milli)?,
            evaporation: scale(weather.evaporation, self.evaporation_multiplier_milli)?,
        };
        adjusted.validate()?;
        Ok(adjusted)
    }
    fn validate_climate(&self, climate: &Climate) -> Result<(), Error> {
        self.validate()?;
        // A newly registered cell must remain valid on every possible later day,
        // not merely under the weather sampled at registration.
        for season in 0..4 {
            for sign in [-1_i32, 1] {
                self.apply(&Weather {
                    temperature: climate.seasonal_temperature[season]
                        + sign * climate.temperature_variation as i32,
                    rain: if sign < 0 {
                        climate.seasonal_rain[season]
                            .saturating_sub(u64::from(climate.rainfall_variation))
                    } else {
                        climate.seasonal_rain[season]
                            .checked_add(u64::from(climate.rainfall_variation))
                            .ok_or(Error::Overflow)?
                    },
                    evaporation: climate.seasonal_evaporation[season],
                })?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherCell {
    pub policy: LocalClimate,
    pub weather: Weather,
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub calendar: Calendar,
    pub climate: Climate,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            calendar: Calendar::human(),
            climate: Climate::temperate(),
        }
    }
}
impl Config {
    #[cfg(feature = "content")]
    pub fn read(path: impl AsRef<std::path::Path>) -> Result<game_content::Document<Self>, Error> {
        let document: game_content::Document<Self> = game_content::read(path)?;
        document.data.validate()?;
        Ok(document)
    }
    pub fn validate(&self) -> Result<(), Error> {
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
        {
            return Err(Error::InvalidConfiguration(
                "invalid calendar or climate rules".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub calendar: Calendar,
    pub climate: Climate,
    pub weather: Weather,
    pub weather_day: u64,
    pub cells: BTreeMap<AreaId, WeatherCell>,
    /// The runtime owns its temporal position, independently of a host clock.
    at: SimTime,
    seed: u64,
    #[cfg(feature = "content")]
    content: Option<game_content::Stamp>,
}

/// Calendar and weather facts at the runtime's current epoch. Consumers decide
/// whether a person can observe these facts; no person or visibility rules live here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub at: SimTime,
    pub day: u64,
    pub day_of_year: u64,
    pub season: u8,
    pub weather: Weather,
}

/// One completed day's forcing. `day` is the closing day count; weather and
/// day_of_year describe the elapsed day, before tomorrow's weather is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayFacts {
    pub day: u64,
    pub day_of_year: u64,
    pub weather: Weather,
}
/// Closing-day forcing by local cell. Each value describes the elapsed day,
/// before the next day's weather is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDayFacts {
    pub day: u64,
    pub day_of_year: u64,
    pub cells: BTreeMap<AreaId, Weather>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub elapsed_day: Option<DayFacts>,
    pub elapsed_local_day: Option<LocalDayFacts>,
    opening: Header,
    next: SimTime,
    daily: Option<DailyChange>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Header {
    calendar: Calendar,
    climate: Climate,
    at: SimTime,
    seed: u64,
    weather_day: u64,
    weather: Weather,
    #[cfg(feature = "content")]
    content: Option<game_content::Stamp>,
}
impl Header {
    fn capture(world: &World) -> Self {
        Self {
            calendar: world.calendar.clone(),
            climate: world.climate.clone(),
            at: world.at,
            seed: world.seed,
            weather_day: world.weather_day,
            weather: world.weather.clone(),
            #[cfg(feature = "content")]
            content: world.content.clone(),
        }
    }
    fn matches(&self, world: &World) -> bool {
        self.calendar == world.calendar
            && self.climate == world.climate
            && self.at == world.at
            && self.seed == world.seed
            && self.weather_day == world.weather_day
            && self.weather == world.weather
            && {
                #[cfg(feature = "content")]
                {
                    self.content == world.content
                }
                #[cfg(not(feature = "content"))]
                {
                    true
                }
            }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct DailyChange {
    weather_day: u64,
    weather: Weather,
    cells: Vec<CellChange>,
    cell_count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct CellChange {
    area: AreaId,
    before: WeatherCell,
    weather: Weather,
}
impl World {
    pub fn new(config: Config, at: SimTime, seed: u64) -> Result<Self, Error> {
        config.validate()?;
        let weather_day = at.0 / config.calendar.day.0;
        let weather = config
            .climate
            .weather(seed, weather_day, config.calendar.season(at));
        Ok(Self {
            calendar: config.calendar,
            climate: config.climate,
            weather,
            weather_day,
            cells: BTreeMap::new(),
            at,
            seed,
            #[cfg(feature = "content")]
            content: None,
        })
    }
    #[cfg(feature = "content")]
    pub fn from_content(
        document: game_content::Document<Config>,
        at: SimTime,
        seed: u64,
    ) -> Result<Self, Error> {
        document.data.validate()?;
        let content = document.stamp()?;
        let mut runtime = Self::new(document.data, at, seed)?;
        runtime.content = Some(content);
        Ok(runtime)
    }
    #[cfg(feature = "content")]
    pub fn content_stamp(&self) -> Option<&game_content::Stamp> {
        self.content.as_ref()
    }
    /// Attach provenance after scenario construction without replacing local
    /// components or moving the runtime's epoch.
    #[cfg(feature = "content")]
    pub fn attach_content(
        &mut self,
        document: game_content::Document<Config>,
    ) -> Result<(), Error> {
        self.validate(self.at, self.seed)?;
        document.data.validate()?;
        let stamp = document.stamp()?;
        if document.data.calendar != self.calendar || document.data.climate != self.climate {
            return Err(Error::InvalidConfiguration(
                "environment content differs from the active definitions".into(),
            ));
        }
        self.content = Some(stamp);
        Ok(())
    }
    pub fn epoch(&self) -> SimTime {
        self.at
    }
    pub fn observe(&self) -> Observation {
        Observation {
            at: self.at,
            day: self.weather_day,
            day_of_year: self.calendar.day_of_year(self.at),
            season: self.calendar.season(self.at) as u8,
            weather: self.weather.clone(),
        }
    }
    pub fn register_cell(&mut self, area: AreaId, policy: LocalClimate) -> Result<(), Error> {
        if area.0 == 0 || self.cells.contains_key(&area) {
            return Err(Error::InvalidConfiguration(
                "invalid or duplicate weather cell".into(),
            ));
        }
        Config {
            calendar: self.calendar.clone(),
            climate: self.climate.clone(),
        }
        .validate()?;
        policy.validate_climate(&self.climate)?;
        let weather = policy.apply(&self.weather)?;
        self.cells.insert(area, WeatherCell { policy, weather });
        Ok(())
    }
    pub fn local_weather(&self, area: AreaId) -> Option<&Weather> {
        self.cells.get(&area).map(|cell| &cell.weather)
    }
    pub fn observe_cell(&self, area: AreaId) -> Option<Observation> {
        let weather = self.local_weather(area)?;
        Some(Observation {
            weather: weather.clone(),
            ..self.observe()
        })
    }
    /// Place a configured scenario at its opening epoch. This does not emit
    /// elapsed-day effects and must not be used to advance an active simulation.
    pub fn reset_epoch(&mut self, at: SimTime, seed: u64) -> Result<(), Error> {
        let configured = Config {
            calendar: self.calendar.clone(),
            climate: self.climate.clone(),
        };
        let mut replacement = Self::new(configured, at, seed)?;
        for (area, cell) in &self.cells {
            replacement.register_cell(*area, cell.policy)?;
        }
        #[cfg(feature = "content")]
        let replacement = Self {
            content: self.content.clone(),
            ..replacement
        };
        replacement.validate(at, seed)?;
        *self = replacement;
        Ok(())
    }
    pub fn validate(&self, at: SimTime, seed: u64) -> Result<(), Error> {
        self.validate_header(at, seed)?;
        for (area, cell) in &self.cells {
            cell.policy.validate_climate(&self.climate)?;
            if area.0 == 0 || cell.weather != cell.policy.apply(&self.weather)? {
                return Err(Error::InvalidSave("inconsistent local weather cell".into()));
            }
        }
        Ok(())
    }
    fn validate_header(&self, at: SimTime, seed: u64) -> Result<(), Error> {
        Config {
            calendar: self.calendar.clone(),
            climate: self.climate.clone(),
        }
        .validate()?;
        #[cfg(feature = "content")]
        if let Some(content) = &self.content {
            let document = game_content::Document {
                id: content.id.clone(),
                version: content.version,
                data: Config {
                    calendar: self.calendar.clone(),
                    climate: self.climate.clone(),
                },
            };
            if &document.stamp()? != content {
                return Err(Error::InvalidSave(
                    "environment definitions do not match their content stamp".into(),
                ));
            }
        }
        if self.at != at
            || self.seed != seed
            || self.weather_day != at.0 / self.calendar.day.0
            || self.weather
                != self
                    .climate
                    .weather(seed, self.weather_day, self.calendar.season(at))
        {
            return Err(Error::InvalidSave(
                "inconsistent environmental day or weather".into(),
            ));
        }
        Ok(())
    }
    pub fn boundary(&self, from: SimTime, requested: SimTime) -> Result<SimTime, Error> {
        if from != self.at || requested < from || self.calendar.day.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid environmental interval".into(),
            ));
        }
        Ok((from.0 / self.calendar.day.0)
            .checked_add(1)
            .and_then(|day| day.checked_mul(self.calendar.day.0))
            .map(SimTime)
            .unwrap_or(requested)
            .min(requested))
    }
    pub fn deadline(&self, requested: SimTime) -> Result<SimTime, Error> {
        self.boundary(self.at, requested)
    }
    /// Prepare an immutable transition. Foreign features can consume the
    /// completed-day facts before any runtime publishes its closing state.
    pub fn prepare(&self, next: SimTime) -> Result<Step, Error> {
        self.advance(self.at, next, self.seed)
    }
    pub fn admit(&self, step: &Step) -> Result<(), Error> {
        if !step.opening.matches(self) {
            return Err(Error::SequenceConflict);
        }
        if step.next < self.at || step.next > self.deadline(step.next)? {
            return Err(Error::SequenceConflict);
        }
        let next_day = step.next.0 / self.calendar.day.0;
        match &step.daily {
            None => {
                if next_day != self.weather_day
                    || step.elapsed_day.is_some()
                    || step.elapsed_local_day.is_some()
                {
                    return Err(Error::SequenceConflict);
                }
            }
            Some(daily) => {
                let expected_weather =
                    self.climate
                        .weather(self.seed, next_day, self.calendar.season(step.next));
                let day_of_year = self.calendar.day_of_year(self.at);
                let elapsed = step.elapsed_day.as_ref().ok_or(Error::SequenceConflict)?;
                let local = step
                    .elapsed_local_day
                    .as_ref()
                    .ok_or(Error::SequenceConflict)?;
                if next_day == self.weather_day
                    || daily.weather_day != next_day
                    || daily.weather != expected_weather
                    || daily.cell_count != self.cells.len()
                    || daily.cells.len() != daily.cell_count
                    || local.cells.len() != daily.cell_count
                    || elapsed.day != next_day
                    || local.day != next_day
                    || elapsed.day_of_year != day_of_year
                    || local.day_of_year != day_of_year
                    || elapsed.weather != self.weather
                {
                    return Err(Error::SequenceConflict);
                }
                let mut previous = None;
                for change in &daily.cells {
                    if previous.is_some_and(|area| area >= change.area)
                        || self.cells.get(&change.area) != Some(&change.before)
                        || local.cells.get(&change.area) != Some(&change.before.weather)
                        || change.weather != change.before.policy.apply(&daily.weather)?
                    {
                        return Err(Error::SequenceConflict);
                    }
                    previous = Some(change.area);
                }
            }
        }
        Ok(())
    }
    pub fn commit(&mut self, step: Step) -> Result<(), Error> {
        self.admit(&step)?;
        if let Some(daily) = step.daily {
            for change in daily.cells {
                self.cells
                    .get_mut(&change.area)
                    .expect("admitted weather cell")
                    .weather = change.weather;
            }
            self.weather_day = daily.weather_day;
            self.weather = daily.weather;
        }
        self.at = step.next;
        Ok(())
    }
    /// Snapshot convenience for inspectors and tests. Production commits apply
    /// sparse prepared changes directly and never clone the containing runtime.
    pub fn preview(&self, step: &Step) -> Result<Self, Error> {
        self.admit(step)?;
        let mut snapshot = self.clone();
        snapshot.commit(step.clone())?;
        Ok(snapshot)
    }
    pub fn advance(&self, from: SimTime, next: SimTime, seed: u64) -> Result<Step, Error> {
        self.validate_header(from, seed)?;
        if next < from || next > self.boundary(from, next)? {
            return Err(Error::InvalidSave(
                "environment skipped a day or reversed time".into(),
            ));
        }
        let next_day = next.0 / self.calendar.day.0;
        let elapsed_day = if next_day != self.weather_day {
            Some(DayFacts {
                day: next_day,
                day_of_year: self.calendar.day_of_year(from),
                weather: self.weather.clone(),
            })
        } else {
            None
        };
        let mut elapsed_local_day = None;
        let mut daily = None;
        if let Some(day) = &elapsed_day {
            let weather = self
                .climate
                .weather(seed, next_day, self.calendar.season(next));
            let mut cells = Vec::with_capacity(self.cells.len());
            let mut forcing = BTreeMap::new();
            for (area, cell) in &self.cells {
                cell.policy.validate_climate(&self.climate)?;
                if area.0 == 0 || cell.weather != cell.policy.apply(&self.weather)? {
                    return Err(Error::InvalidSave("inconsistent local weather cell".into()));
                }
                cells.push(CellChange {
                    area: *area,
                    before: cell.clone(),
                    weather: cell.policy.apply(&weather)?,
                });
                forcing.insert(*area, cell.weather.clone());
            }
            elapsed_local_day = Some(LocalDayFacts {
                day: day.day,
                day_of_year: day.day_of_year,
                cells: forcing,
            });
            daily = Some(DailyChange {
                weather_day: next_day,
                weather,
                cells,
                cell_count: self.cells.len(),
            });
        }
        Ok(Step {
            elapsed_day,
            elapsed_local_day,
            opening: Header::capture(self),
            next,
            daily,
        })
    }
}

#[cfg(test)]
mod tests;
