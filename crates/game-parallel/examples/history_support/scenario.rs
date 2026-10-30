use game_parallel::World;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Calendar {
    pub minutes_per_tick: u64,
    pub days_per_year: u64,
    pub epoch_year: u64,
}
impl Calendar {
    pub fn validate(&self) -> Result<(), String> {
        if self.minutes_per_tick == 0 || self.days_per_year == 0 {
            return Err("calendar tick and year lengths must be positive".into());
        }
        Ok(())
    }
    pub fn date(&self, tick: u64) -> Result<Date, String> {
        self.validate()?;
        let minutes = u128::from(tick) * u128::from(self.minutes_per_tick);
        let day = minutes / 1440;
        let year = u128::from(self.epoch_year) + day / u128::from(self.days_per_year);
        Ok(Date {
            year: u64::try_from(year).map_err(|_| "calendar year overflow")?,
            day_of_year: (day % u128::from(self.days_per_year)) as u64 + 1,
            minute_of_day: (minutes % 1440) as u64,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Date {
    pub year: u64,
    pub day_of_year: u64,
    pub minute_of_day: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub version: u32,
    pub source: String,
    pub seed: u64,
    pub ticks: u64,
    pub calendar: Calendar,
    pub sample_every: u64,
    pub checkpoint_every: u64,
    pub max_samples: u64,
    #[serde(default = "event_limit")]
    pub max_events: u64,
}
fn event_limit() -> u64 {
    10_000
}
impl Scenario {
    pub fn load(path: &Path) -> Result<(Self, World), String> {
        let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let scenario: Self = toml::from_str(&source).map_err(|e| e.to_string())?;
        if scenario.version != 1 {
            return Err("unsupported scenario version".into());
        }
        scenario.calendar.validate()?;
        if scenario.sample_every == 0 || scenario.checkpoint_every == 0 || scenario.max_samples == 0
        {
            return Err("sample/checkpoint intervals and sample limit must be positive".into());
        }
        let world_path = path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&scenario.source);
        let text = std::fs::read_to_string(world_path).map_err(|e| e.to_string())?;
        let mut world = World::from_toml(&text)?;
        world.seed = scenario.seed;
        Ok((scenario, world))
    }
}
