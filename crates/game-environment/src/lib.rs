//! Calendar and deterministic weather. This domain owns no crops, people or goods.
use game_core::{AreaId, Error, Location, SimDuration, SimTime};
use game_ecs::{StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod storage;
pub use storage::{Queries, register_components};
mod system;
pub use system::{
    CalendarReading, EnvironmentSystem, ObserveEnvironment, ReadCalendar, ReadElapsedDay,
    ReadLocalWeather, SystemInput, SystemOutput, register_schedule, register_system,
};
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
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub calendar: Calendar,
    pub climate: Climate,
    pub weather: Weather,
    pub weather_day: u64,
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
impl Step {
    pub(crate) fn weather_updates(&self) -> impl Iterator<Item = (AreaId, Weather)> + '_ {
        self.daily
            .iter()
            .flat_map(|daily| daily.cells.iter())
            .map(|change| (change.area, change.weather.clone()))
    }
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
    fn capture(world: &Runtime) -> Self {
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
    fn matches(&self, world: &Runtime) -> bool {
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
impl Runtime {
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
        store: &Store,
        document: game_content::Document<Config>,
    ) -> Result<(), Error> {
        self.validate(store, self.at, self.seed)?;
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
    #[cfg(feature = "content")]
    pub(crate) fn attach_content_rows(
        &mut self,
        document: game_content::Document<Config>,
        cells: Vec<(StableId, LocalClimate, Weather)>,
    ) -> Result<(), Error> {
        self.validate_cells(
            self.at,
            self.seed,
            cells
                .into_iter()
                .map(|(id, policy, weather)| (AreaId(id.0), policy, weather)),
        )?;
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
    pub fn register_cell(
        &self,
        store: &mut Store,
        area: AreaId,
        policy: LocalClimate,
    ) -> Result<(), Error> {
        if area.0 == 0
            || store.get::<LocalClimate>(StableId(area.0)).is_some()
            || store.get::<Weather>(StableId(area.0)).is_some()
        {
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
        store.insert_pair_or_spawn(StableId(area.0), policy, weather)
    }
    pub(crate) fn initial_cell(&self, area: AreaId) -> Result<(LocalClimate, Weather), Error> {
        if area.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid or duplicate weather cell".into(),
            ));
        }
        let policy = LocalClimate::default();
        policy.validate_climate(&self.climate)?;
        Ok((policy, policy.apply(&self.weather)?))
    }
    /// Register neutral local cells for known areas that do not have one yet.
    /// A partial or malformed component pair is rejected by `register_cell`;
    /// this method never repairs corrupted saved state implicitly.
    pub fn register_missing_cells(
        &self,
        store: &mut Store,
        areas: impl IntoIterator<Item = AreaId>,
    ) -> Result<(), Error> {
        for area in areas {
            if self.local_weather(store, area).is_none() {
                self.register_cell(store, area, LocalClimate::default())?;
            }
        }
        Ok(())
    }
    pub fn cells<'a>(
        &self,
        store: &'a Store,
    ) -> impl Iterator<Item = (AreaId, &'a LocalClimate, &'a Weather)> {
        store
            .iter::<LocalClimate>()
            .filter_map(move |(id, policy)| Some((AreaId(id.0), policy, store.get::<Weather>(id)?)))
    }
    pub fn local_weather<'a>(&self, store: &'a Store, area: AreaId) -> Option<&'a Weather> {
        store.get::<Weather>(StableId(area.0))
    }
    pub fn observe_cell(&self, store: &Store, area: AreaId) -> Option<Observation> {
        let weather = self.local_weather(store, area)?;
        Some(Observation {
            weather: weather.clone(),
            ..self.observe()
        })
    }
    /// Observe local conditions for a living actor who is stationary in an
    /// area. The host supplies eligibility and location facts; this domain
    /// supplies the calendar and weather observation.
    pub fn observe_for_actor(
        &self,
        store: &Store,
        alive: bool,
        location: &Location,
    ) -> Option<Observation> {
        self.observation_for_actor(alive, location, |area| {
            self.local_weather(store, area).cloned()
        })
    }
    pub(crate) fn observation_for_actor(
        &self,
        alive: bool,
        location: &Location,
        mut weather: impl FnMut(AreaId) -> Option<Weather>,
    ) -> Option<Observation> {
        if !alive {
            return None;
        }
        let Location::AtArea(area) = location else {
            return None;
        };
        Some(Observation {
            weather: weather(*area)?,
            ..self.observe()
        })
    }
    /// Resample an opening epoch without emitting completed-day effects.
    pub fn reset_epoch(&mut self, store: &mut Store, at: SimTime, seed: u64) -> Result<(), Error> {
        if store
            .iter::<LocalClimate>()
            .map(|(id, _)| id)
            .ne(store.iter::<Weather>().map(|(id, _)| id))
        {
            return Err(Error::InvalidSave(
                "unpaired local weather components".into(),
            ));
        }
        let config = Config {
            calendar: self.calendar.clone(),
            climate: self.climate.clone(),
        };
        let replacement = Self::new(config, at, seed)?;
        let changes = self
            .cells(store)
            .map(|(area, policy, _)| {
                policy.validate_climate(&replacement.climate)?;
                Ok((area, policy.apply(&replacement.weather)?))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        #[cfg(feature = "content")]
        let replacement = Self {
            content: self.content.clone(),
            ..replacement
        };
        replacement.validate_header(at, seed)?;
        for (area, weather) in changes {
            *store
                .get_mut::<Weather>(StableId(area.0))
                .expect("opening weather component") = weather;
        }
        *self = replacement;
        Ok(())
    }
    pub fn validate(&self, store: &Store, at: SimTime, seed: u64) -> Result<(), Error> {
        self.validate_header(at, seed)?;
        let cells = paired_cells(
            store
                .iter::<LocalClimate>()
                .map(|(id, policy)| (id, *policy))
                .collect(),
            store
                .iter::<Weather>()
                .map(|(id, weather)| (id, weather.clone()))
                .collect(),
        )?;
        self.validate_cells(
            at,
            seed,
            cells
                .iter()
                .cloned()
                .map(|(id, policy, weather)| (AreaId(id.0), policy, weather)),
        )
    }
    pub(crate) fn validate_cells(
        &self,
        at: SimTime,
        seed: u64,
        cells: impl IntoIterator<Item = (AreaId, LocalClimate, Weather)>,
    ) -> Result<(), Error> {
        self.validate_header(at, seed)?;
        for (area, policy, weather) in cells {
            policy.validate_climate(&self.climate)?;
            if area.0 == 0 || weather != policy.apply(&self.weather)? {
                return Err(Error::InvalidSave("inconsistent local weather cell".into()));
            }
        }
        Ok(())
    }
    /// Validate environment state against host-owned area identities and clock
    /// facts. A weather cell may exist only for an area in `areas`.
    pub fn validate_host(
        &self,
        store: &Store,
        at: SimTime,
        seed: u64,
        areas: impl IntoIterator<Item = AreaId>,
    ) -> Result<(), Error> {
        let areas = areas.into_iter().collect::<std::collections::BTreeSet<_>>();
        let cells = paired_cells(
            store
                .iter::<LocalClimate>()
                .map(|(id, policy)| (id, *policy))
                .collect(),
            store
                .iter::<Weather>()
                .map(|(id, weather)| (id, weather.clone()))
                .collect(),
        )?;
        self.validate_host_cells(at, seed, areas, cells)
    }
    pub(crate) fn validate_host_cells(
        &self,
        at: SimTime,
        seed: u64,
        areas: std::collections::BTreeSet<AreaId>,
        cells: Vec<(StableId, LocalClimate, Weather)>,
    ) -> Result<(), Error> {
        let local = cells
            .iter()
            .map(|(id, policy, weather)| (AreaId(id.0), *policy, weather.clone()))
            .collect::<Vec<_>>();
        self.validate_cells(at, seed, local)?;
        if cells
            .iter()
            .any(|(id, _, _)| !areas.contains(&AreaId(id.0)))
        {
            return Err(Error::InvalidSave("unknown weather cell area".into()));
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
    pub fn prepare(&self, store: &Store, queries: &Queries, next: SimTime) -> Result<Step, Error> {
        self.advance(store, queries, self.at, next, self.seed)
    }
    pub fn admit(&self, store: &Store, queries: &mut Queries, step: &Step) -> Result<(), Error> {
        if !step.opening.matches(self) {
            return Err(Error::SequenceConflict);
        }
        if step.next < self.at || step.next > self.deadline(step.next)? {
            return Err(Error::SequenceConflict);
        }
        let next_day = step.next.0 / self.calendar.day.0;
        queries.refresh(store)?;
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
                    || daily.cell_count != queries.rows(store)?.count()
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
                        || (store.get::<LocalClimate>(StableId(change.area.0))
                            != Some(&change.before.policy)
                            || store.get::<Weather>(StableId(change.area.0))
                                != Some(&change.before.weather))
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
    pub fn commit(
        &mut self,
        store: &mut Store,
        queries: &mut Queries,
        step: Step,
    ) -> Result<(), Error> {
        self.admit(store, queries, &step)?;
        if let Some(daily) = step.daily {
            for change in daily.cells {
                *store
                    .get_mut::<Weather>(StableId(change.area.0))
                    .expect("admitted weather cell") = change.weather;
            }
            self.weather_day = daily.weather_day;
            self.weather = daily.weather;
        }
        self.at = step.next;
        Ok(())
    }
    pub fn advance(
        &self,
        store: &Store,
        queries: &Queries,
        from: SimTime,
        next: SimTime,
        seed: u64,
    ) -> Result<Step, Error> {
        let cells = queries
            .rows(store)?
            .map(|(id, policy, weather)| (id, *policy, weather.clone()))
            .collect();
        self.advance_cells(from, next, seed, cells)
    }
    pub(crate) fn advance_cells(
        &self,
        from: SimTime,
        next: SimTime,
        seed: u64,
        cell_rows: Vec<(StableId, LocalClimate, Weather)>,
    ) -> Result<Step, Error> {
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
            let mut cells = Vec::new();
            let mut forcing = BTreeMap::new();
            for (id, policy, local_weather) in cell_rows {
                let area = AreaId(id.0);
                policy.validate_climate(&self.climate)?;
                if area.0 == 0 || local_weather != policy.apply(&self.weather)? {
                    return Err(Error::InvalidSave("inconsistent local weather cell".into()));
                }
                cells.push(CellChange {
                    area,
                    before: WeatherCell {
                        policy,
                        weather: local_weather.clone(),
                    },
                    weather: policy.apply(&weather)?,
                });
                forcing.insert(area, local_weather);
            }
            cells.sort_unstable_by_key(|change| change.area);
            let cell_count = cells.len();
            elapsed_local_day = Some(LocalDayFacts {
                day: day.day,
                day_of_year: day.day_of_year,
                cells: forcing,
            });
            daily = Some(DailyChange {
                weather_day: next_day,
                weather,
                cells,
                cell_count,
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
    pub(crate) fn finish_step(&mut self, step: &Step) -> Result<(), Error> {
        if !step.opening.matches(self)
            || step.next < self.at
            || step.next > self.deadline(step.next)?
        {
            return Err(Error::SequenceConflict);
        }
        if let Some(daily) = &step.daily {
            self.weather_day = daily.weather_day;
            self.weather = daily.weather.clone();
        }
        self.at = step.next;
        Ok(())
    }
}

pub(crate) fn paired_cells(
    policies: Vec<(StableId, LocalClimate)>,
    weather: Vec<(StableId, Weather)>,
) -> Result<Vec<(StableId, LocalClimate, Weather)>, Error> {
    if policies.len() != weather.len()
        || policies
            .iter()
            .map(|(id, _)| *id)
            .ne(weather.iter().map(|(id, _)| *id))
    {
        return Err(Error::InvalidSave(
            "unpaired local weather components".into(),
        ));
    }
    Ok(policies
        .into_iter()
        .zip(weather)
        .map(|((id, policy), (_, weather))| (id, policy, weather))
        .collect())
}

pub(crate) fn validate_host_rows(
    runtime: Option<&Runtime>,
    at: SimTime,
    seed: u64,
    areas: impl IntoIterator<Item = AreaId>,
    policies: Vec<(StableId, LocalClimate)>,
    weather: Vec<(StableId, Weather)>,
) -> Result<(), Error> {
    if runtime.is_none() {
        if !policies.is_empty() || !weather.is_empty() {
            return Err(Error::InvalidSave(
                "weather components have no environment runtime".into(),
            ));
        }
        return Ok(());
    }
    let cells = paired_cells(policies, weather)?;
    if let Some(runtime) = runtime {
        return runtime.validate_host_cells(at, seed, areas.into_iter().collect(), cells);
    }
    unreachable!("runtime presence was checked above")
}

/// Validate the environment component columns and their runtime as one host
/// feature. This also rejects weather components left behind when the runtime
/// is absent.
pub fn validate_host_presence(
    store: &Store,
    runtime: Option<&Runtime>,
    at: SimTime,
    seed: u64,
    areas: impl IntoIterator<Item = AreaId>,
) -> Result<(), Error> {
    if let Some(runtime) = runtime {
        return runtime.validate_host(store, at, seed, areas);
    }
    if store.iter::<LocalClimate>().next().is_some() || store.iter::<Weather>().next().is_some() {
        return Err(Error::InvalidSave(
            "weather components have no environment runtime".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
