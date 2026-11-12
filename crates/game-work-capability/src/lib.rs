//! Data-defined physical feasibility for production, independent of choice.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthPoint {
    pub age: u64,
    pub strength: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub recipe: u32,
    pub strength: u32,
}

/// Configuration describes physical limits, not willingness or authorization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// Piecewise-linear age-to-strength curve, starting at age zero.
    pub growth: Vec<GrowthPoint>,
    /// Strength multiplier per thousand when genetic vigor is zero.
    pub vigor_base: u32,
    /// Additional per-thousand multiplier when genetic vigor is 1000.
    pub vigor_weight: u32,
    /// Required strength for a recipe without a specific entry.
    pub default_strength: u32,
    /// Missing family records are denied unless an explicit fallback is given.
    #[serde(default)]
    pub missing_strength: Option<u32>,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
}

/// Age and vigor are physical facts, not the actor's desires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    pub person: u64,
    pub age: u64,
    pub vigor: u16,
}

impl Capabilities {
    pub fn validate(&self, recipes: &[u32]) -> Result<(), &'static str> {
        let recipes: std::collections::HashSet<_> = recipes.iter().copied().collect();
        if self.growth.first().is_none_or(|row| row.age != 0)
            || self
                .growth
                .windows(2)
                .any(|rows| rows[0].age >= rows[1].age)
            || self
                .requirements
                .windows(2)
                .any(|rows| rows[0].recipe >= rows[1].recipe)
            || self
                .requirements
                .iter()
                .any(|row| row.recipe == 0 || !recipes.contains(&row.recipe))
        {
            return Err("invalid work capability configuration");
        }
        Ok(())
    }

    pub fn strength(&self, body: Option<Body>) -> Option<u32> {
        let Some(body) = body else {
            return self.missing_strength;
        };
        if body.vigor > 1000 {
            return None;
        }
        let upper = self.growth.partition_point(|row| row.age <= body.age);
        let previous = self.growth.get(upper.checked_sub(1)?)?;
        let base = if let Some(next) = self.growth.get(upper) {
            let span = u128::from(next.age - previous.age);
            let elapsed = u128::from(body.age - previous.age);
            let left = u128::from(previous.strength) * (span - elapsed);
            let right = u128::from(next.strength) * elapsed;
            (left + right) / span
        } else {
            u128::from(previous.strength)
        };
        let multiplier = u128::from(self.vigor_base)
            + u128::from(self.vigor_weight) * u128::from(body.vigor) / 1000;
        Some((base * multiplier / 1000).min(u128::from(u32::MAX)) as u32)
    }

    pub fn permits(&self, body: Option<Body>, recipe: u32) -> bool {
        let required = self
            .requirements
            .binary_search_by_key(&recipe, |row| row.recipe)
            .ok()
            .map_or(self.default_strength, |index| {
                self.requirements[index].strength
            });
        self.strength(body)
            .is_some_and(|strength| strength >= required)
    }

    /// Parallel pure filtering; callers provide actual individual body snapshots.
    pub fn permitted(&self, attempts: &[(Option<Body>, u32)]) -> Vec<bool> {
        attempts
            .par_iter()
            .map(|&(body, recipe)| self.permits(body, recipe))
            .collect()
    }
}
