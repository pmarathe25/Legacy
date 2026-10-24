//! Pure developmental expression and bounded exact-input memoization. No world access.
use game_core::*;
use serde::{Deserialize, Serialize};
mod cache;
mod growth;
mod projection;
mod slots;
pub use cache::{ability, disposition};
pub use growth::{Upbringing, maturation};
pub use projection::Projection;
pub use slots::{ability_slot, disposition_slot};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Life {
    pub born: SimTime,
    pub span: Option<SimDuration>,
    pub base_health: u64,
    pub expressed: bool,
    pub physical: u32,
    pub stress: u32,
    pub satisfaction: i32,
    pub updated: SimTime,
    #[serde(default)]
    pub upbringing: Upbringing,
}
fn scaled(value: u64, numerator: u64, denominator: u64) -> u64 {
    (u128::from(value) * u128::from(numerator) / u128::from(denominator)).min(u128::from(u64::MAX))
        as u64
}
impl Life {
    pub fn condition(&self, at: SimTime) -> (u32, i32) {
        let days = at.0.saturating_sub(self.updated.0) / DAY.0;
        let stress = self.stress.saturating_sub(days.min(1000) as u32 * 3);
        let satisfaction = self.satisfaction.signum()
            * self
                .satisfaction
                .abs()
                .saturating_sub(days.min(1000) as i32 * 2);
        (stress, satisfaction)
    }
    pub fn aging(&self, at: SimTime, mental: bool) -> u64 {
        let Some(span) = self.span else {
            return 1000;
        };
        let fraction = scaled(at.0.saturating_sub(self.born.0), 1000, span.0).min(1000);
        // Decline follows biological age relative to one's natural lifespan.
        let late = fraction.saturating_sub(500);
        1000 - late * if mental { 2 } else { 4 } / 5
    }
}
const MENTAL: &[&str] = &[
    "intelligence",
    "attention",
    "learning",
    "foresight",
    "resilience",
    "interpretation",
    "hearing",
];
const PHYSICAL: &[&str] = &[
    "strength",
    "speed",
    "endurance",
    "agility",
    "hardiness",
    "robustness",
    "size",
    "constitution",
    "metabolism",
    "recovery",
    "vocalization",
    "gesturing",
];
impl Life {
    pub fn ability(&self, raw: u64, key: &str, at: SimTime) -> u64 {
        let mental = MENTAL.contains(&key) || key.starts_with("memory_");
        let physical = PHYSICAL.contains(&key);
        if !mental && !physical {
            return raw;
        }
        let (stress, _) = self.condition(at);
        let condition = if mental {
            1000 - u64::from(stress) / 3
        } else {
            500 + u64::from(self.physical) / 2
        };
        let development = self.maturity(at, key);
        let fulfillment = 1000 - self.upbringing_at(at).deprivation() / 4;
        scaled(
            scaled(
                scaled(scaled(raw, development, 1000), fulfillment, 1000),
                self.aging(at, mental),
                1000,
            ),
            condition,
            1000,
        )
    }
}
