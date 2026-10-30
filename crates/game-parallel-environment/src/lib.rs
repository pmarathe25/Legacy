//! Data-defined regional exposure and finite shelter.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub regions: Vec<Hazard>,
    /// Repeating calendar; an empty list retains constant conditions.
    #[serde(default)]
    pub seasons: Vec<Season>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Season {
    pub name: String,
    pub duration: u64,
    pub food_growth_per_thousand: u32,
    pub exposure_per_thousand: u32,
    #[serde(default = "allowed")]
    pub planting_allowed: bool,
    #[serde(default = "allowed")]
    pub tending_allowed: bool,
    #[serde(default = "allowed")]
    pub harvest_allowed: bool,
}
fn allowed() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hazard {
    pub region: u64,
    pub damage: u32,
    pub shelter: u32,
    pub shelter_reduction: u32,
    /// Each stored unit of this good provides one additional shelter space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shelter_good: Option<u32>,
}
impl Environment {
    pub fn validate(&self, regions: &HashSet<u64>) -> Result<(), String> {
        let mut seen = HashSet::new();
        let mut names = HashSet::new();
        let mut duration = 0u64;
        for season in &self.seasons {
            if season.name.trim().is_empty()
                || !names.insert(&season.name)
                || season.duration == 0
                || season.food_growth_per_thousand > 10_000
                || season.exposure_per_thousand > 10_000
            {
                return Err("invalid season".into());
            }
            duration = duration
                .checked_add(season.duration)
                .ok_or("season cycle overflow")?;
        }
        if self.regions.iter().any(|hazard| {
            !regions.contains(&hazard.region)
                || !seen.insert(hazard.region)
                || hazard.shelter_reduction > hazard.damage
        }) {
            return Err("invalid regional exposure".into());
        }
        Ok(())
    }
    /// Tick one is the first tick of the first season; tick zero shares its name.
    pub fn season(&self, tick: u64) -> Option<&Season> {
        let duration = self
            .seasons
            .iter()
            .try_fold(0u64, |sum, season| sum.checked_add(season.duration))?;
        if duration == 0 {
            return None;
        }
        let mut offset = tick.saturating_sub(1) % duration;
        self.seasons.iter().find(|season| {
            if offset < season.duration {
                true
            } else {
                offset -= season.duration;
                false
            }
        })
    }
}
impl Season {
    pub fn food_growth(&self, amount: u32) -> u32 {
        scale(amount, self.food_growth_per_thousand)
    }
    pub fn exposure(&self, amount: u32) -> u32 {
        scale(amount, self.exposure_per_thousand)
    }
}
fn scale(amount: u32, factor: u32) -> u32 {
    (u64::from(amount) * u64::from(factor) / 1000).min(u64::from(u32::MAX)) as u32
}
