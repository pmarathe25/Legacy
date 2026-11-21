//! Data-defined regional exposure and finite shelter.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub regions: Vec<Hazard>,
    /// Apply a regional exposure event at each multiple of this elapsed duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure_period_millis: Option<u64>,
    /// Milliseconds represented by one calendar duration unit. Absent in legacy saves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season_unit_millis: Option<u64>,
    /// Repeating calendar; an empty list retains constant conditions.
    #[serde(default)]
    pub seasons: Vec<Season>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clothing: Vec<Clothing>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clothing_choices: Vec<ClothingChoice>,
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
/// An actual carried good can provide this much protection when worn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clothing {
    pub good: u32,
    pub insulation: u32,
}

/// A person evaluates one garment using their own situation and inclinations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingChoice {
    pub person: u64,
    pub good: u32,
    pub protection_weight: i32,
    #[serde(default)]
    pub hunger_weight: i32,
    #[serde(default)]
    pub effort: i32,
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
    pub fn choose_clothing(
        &self,
        person: u64,
        hunger: u32,
        damage: u32,
        possesses: impl Fn(u32) -> bool,
    ) -> u32 {
        if damage == 0 || self.clothing_choices.is_empty() {
            return 0;
        }
        let start = self
            .clothing_choices
            .partition_point(|row| row.person < person);
        self.clothing_choices[start..]
            .iter()
            .take_while(|row| row.person == person)
            .filter_map(|choice| {
                let i = self
                    .clothing
                    .binary_search_by_key(&choice.good, |row| row.good)
                    .ok()?;
                let insulation = self.clothing[i].insulation.min(damage);
                let value = i128::from(choice.protection_weight) * i128::from(insulation)
                    + i128::from(choice.hunger_weight) * i128::from(hunger)
                    - i128::from(choice.effort);
                (value > 0 && possesses(choice.good)).then_some((
                    value,
                    std::cmp::Reverse(choice.good),
                    insulation,
                ))
            })
            .max()
            .map_or(0, |(_, _, insulation)| insulation)
    }

    pub fn normalize_clothing(
        &mut self,
        goods: &HashSet<u32>,
        person_exists: impl Fn(u64) -> bool,
    ) -> Result<(), String> {
        self.clothing.sort_unstable_by_key(|row| row.good);
        self.clothing_choices
            .sort_unstable_by_key(|row| (row.person, row.good));
        if self
            .clothing
            .windows(2)
            .any(|pair| pair[0].good == pair[1].good)
            || self
                .clothing
                .iter()
                .any(|row| row.insulation == 0 || !goods.contains(&row.good))
            || self
                .clothing_choices
                .windows(2)
                .any(|pair| (pair[0].person, pair[0].good) == (pair[1].person, pair[1].good))
            || self.clothing_choices.iter().any(|row| {
                !person_exists(row.person)
                    || self
                        .clothing
                        .binary_search_by_key(&row.good, |item| item.good)
                        .is_err()
            })
        {
            return Err("invalid clothing definition or personal choice".into());
        }
        Ok(())
    }

    pub fn validate(&self, regions: &HashSet<u64>) -> Result<(), String> {
        if self.season_unit_millis == Some(0) {
            return Err("season duration unit must be positive".into());
        }
        if self.exposure_period_millis == Some(0)
            || (self.exposure_period_millis.is_some()
                && !self.seasons.is_empty()
                && self.season_unit_millis.is_none())
        {
            return Err("invalid elapsed regional exposure cadence or calendar".into());
        }
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

    pub fn next_exposure(&self, now: u128) -> Option<u64> {
        let period = self.exposure_period_millis.filter(|&period| period != 0)?;
        (!self.regions.is_empty()).then(|| period - (now % u128::from(period)) as u64)
    }

    /// Elapsed time to the next named season, including across cycle wrap.
    pub fn next_season_change(&self, now: u128) -> Option<u128> {
        let unit = u128::from(self.season_unit_millis.filter(|unit| *unit > 0)?);
        if self.seasons.len() < 2 {
            return None;
        }
        let cycle = self.seasons.iter().try_fold(0u128, |sum, season| {
            sum.checked_add(u128::from(season.duration) * unit)
        })?;
        if cycle == 0 {
            return None;
        }
        let mut offset = now % cycle;
        for season in &self.seasons {
            let duration = u128::from(season.duration) * unit;
            if offset < duration {
                return Some(duration - offset);
            }
            offset -= duration;
        }
        None
    }
    /// Tick one is the first tick of the first season; tick zero shares its name.
    pub fn season(&self, tick: u64) -> Option<&Season> {
        self.season_at_unit(u128::from(tick.saturating_sub(1)))
    }

    /// Select conditions at an absolute simulated instant using the authored
    /// calendar. A season owns its opening instant and ends at the next opening.
    pub fn season_at(&self, millis: u128, legacy_tick: u64) -> Option<&Season> {
        match self.season_unit_millis {
            Some(unit) if unit > 0 => self.season_at_unit(millis / u128::from(unit)),
            Some(_) => None,
            None => self.season(legacy_tick),
        }
    }

    /// Integral of the seasonal growth multiplier, in per-thousand milliseconds.
    /// Complete cycles are counted analytically even for a very long advance.
    pub fn food_growth_exposure(&self, opening_millis: u128, elapsed_millis: u64) -> Option<u128> {
        let unit = u128::from(self.season_unit_millis.filter(|unit| *unit > 0)?);
        let duration = self
            .seasons
            .iter()
            .try_fold(0u64, |sum, season| sum.checked_add(season.duration))?;
        let cycle = u128::from(duration) * unit;
        if cycle == 0 {
            return Some(u128::from(elapsed_millis) * 1000);
        }
        let closing_millis = opening_millis.checked_add(u128::from(elapsed_millis))?;
        let mut start = 0;
        let mut exposure = 0;
        for season in &self.seasons {
            let length = u128::from(season.duration) * unit;
            let time_in_season = |instant: u128| {
                instant / cycle * length + (instant % cycle).saturating_sub(start).min(length)
            };
            exposure += (time_in_season(closing_millis) - time_in_season(opening_millis))
                * u128::from(season.food_growth_per_thousand);
            start += length;
        }
        Some(exposure)
    }

    fn season_at_unit(&self, elapsed: u128) -> Option<&Season> {
        let duration = self
            .seasons
            .iter()
            .try_fold(0u64, |sum, season| sum.checked_add(season.duration))?;
        if duration == 0 {
            return None;
        }
        let mut offset = (elapsed % u128::from(duration)) as u64;
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
