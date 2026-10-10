//! Calendar and deterministic weather. This domain owns no crops, people or goods.
use game_core::{Error, SimDuration, SimTime};
use serde::{Deserialize, Serialize};
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

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// One completed day's forcing. `day` is the closing day count; weather and
/// day_of_year describe the elapsed day, before tomorrow's weather is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayFacts {
    pub day: u64,
    pub day_of_year: u64,
    pub weather: Weather,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub world: World,
    pub elapsed_day: Option<DayFacts>,
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
        })
    }
    pub fn validate(&self, at: SimTime, seed: u64) -> Result<(), Error> {
        Config {
            calendar: self.calendar.clone(),
            climate: self.climate.clone(),
        }
        .validate()?;
        if self.weather_day != at.0 / self.calendar.day.0
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
        if requested < from || self.calendar.day.0 == 0 {
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
    pub fn advance(&self, from: SimTime, next: SimTime, seed: u64) -> Result<Step, Error> {
        self.validate(from, seed)?;
        if next < from || next > self.boundary(from, next)? {
            return Err(Error::InvalidSave(
                "environment skipped a day or reversed time".into(),
            ));
        }
        let mut world = self.clone();
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
        if elapsed_day.is_some() {
            world.weather_day = next_day;
            world.weather = self
                .climate
                .weather(seed, next_day, self.calendar.season(next));
        }
        Ok(Step { world, elapsed_day })
    }
}

#[cfg(test)]
mod tests;
