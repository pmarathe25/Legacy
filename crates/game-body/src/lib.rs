//! Pure bodily state and time evolution, shared by physiology and its callers.
use game_core::*;
use serde::{Deserialize, Serialize};
mod compute;
mod events;
mod nourishment;
mod resting;
pub use compute::{
    advance_body, advance_rates, advance_scaled, hunger_threshold, hunger_threshold_scaled,
};
pub use events::{Damaged, Died, Rested};
pub use nourishment::{
    CheckLocalMeal, CheckSelfMeal, ConsumeEdible, Eating, Fed, Nourish, NourishMany,
    NourishmentOwner, Serve, apply_nourishment, register_nourishment_batch,
};
pub use resting::{SetRestingMany, register_resting_batch};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Mortal,
    Guardian,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    pub kind: Kind,
    pub born: SimTime,
    pub alive: bool,
    pub hunger: u64,
    pub fatigue: u64,
    #[serde(default, skip_serializing_if = "zero")]
    pub fatigue_fraction: u16,
    #[serde(default, skip_serializing_if = "zero")]
    pub hunger_fraction: u16,
    #[serde(default, skip_serializing_if = "zero")]
    pub recovery_fraction: u16,
    #[serde(default, skip_serializing_if = "zero_healing")]
    pub healing_fraction: u64,
    #[serde(default, skip_serializing_if = "normal_rates")]
    pub metabolic_rates: Rates,
    pub health: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_health: Option<u64>,
    pub resting: bool,
    pub last: SimTime,
    pub life_limit: Option<SimTime>,
}
fn zero(value: &u16) -> bool {
    *value == 0
}
fn zero_healing(value: &u64) -> bool {
    *value == 0
}
/// Multipliers in thousandths of the authored adult baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rates {
    pub energy: u64,
    pub fatigue: u64,
    pub recovery: u64,
}
impl Default for Rates {
    fn default() -> Self {
        Self {
            energy: 1000,
            fatigue: 1000,
            recovery: 1000,
        }
    }
}
fn normal_rates(rates: &Rates) -> bool {
    *rates == Rates::default()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyView {
    pub kind: Kind,
    pub born: SimTime,
    pub alive: bool,
    pub hunger: u64,
    pub fatigue: u64,
    pub health: u64,
}
impl From<&Body> for BodyView {
    fn from(b: &Body) -> Self {
        Self {
            kind: b.kind,
            born: b.born,
            alive: b.alive,
            hunger: b.hunger,
            fatigue: b.fatigue,
            health: b.health,
        }
    }
}
impl Body {
    pub fn validate(&self, rules: &Rules) -> Result<()> {
        require(
            self.health <= self.maximum(rules)
                && self.fatigue_fraction < 1000
                && self.hunger_fraction < 1000
                && self.recovery_fraction < 1000
                && self.healing_fraction < DAY.0 * 1000
                && (self.kind == Kind::Mortal || self.alive),
            "invalid body",
        )
    }
    pub fn exert(&mut self, energy: u64, fatigue: u64, endurance: u64) {
        let efficiency = 500 + 500000 / endurance.max(1);
        if self.kind == Kind::Mortal {
            let gain = u128::from(energy) * u128::from(efficiency) / 1000
                + u128::from(self.hunger_fraction);
            self.hunger = self
                .hunger
                .saturating_add((gain / 1000).min(u128::from(u64::MAX)) as u64);
            self.hunger_fraction = (gain % 1000) as u16;
        }
        let gain = u128::from(fatigue) * 1000 / u128::from(endurance.max(1))
            + u128::from(self.fatigue_fraction);
        self.fatigue = self
            .fatigue
            .saturating_add((gain / 1000).min(u128::from(u64::MAX)) as u64);
        self.fatigue_fraction = (gain % 1000) as u16;
    }
    pub fn maximum(&self, rules: &Rules) -> u64 {
        self.maximum_health.unwrap_or(rules.max_health)
    }
    pub fn cause(&self, injury: bool, rules: &Rules, at: SimTime) -> DeathCause {
        if injury {
            DeathCause::Damage
        } else {
            DeathCause::Physiology {
                lifespan: self.life_limit.is_some_and(|limit| limit <= at),
                deprivation: self.hunger >= rules.deprivation,
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeathCause {
    Damage,
    Physiology { lifespan: bool, deprivation: bool },
}
pub fn load(store: &mut game_content::Store, key: &str) -> Result<Rules> {
    let doc: game_content::Document<Rules> = store.load(&format!("physiology/{key}.toml"))?;
    require(
        doc.data.tick.0 > 0 && doc.data.max_health > 0,
        "invalid physiology rates",
    )?;
    Ok(doc.data)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub lifespan: SimDuration,
    pub tick: SimDuration,
    pub hunger_per_tick: u64,
    pub fatigue_per_tick: u64,
    pub rest_per_tick: u64,
    pub deprivation: u64,
    pub harm_per_tick: u64,
    pub guardian_recovery: u64,
    /// Mortal recovery during nourished rest, in health points per biological day.
    #[serde(default)]
    pub natural_recovery: u64,

    pub max_health: u64,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            lifespan: SimDuration(DAY.0 * 365 * 80),
            tick: MINUTE,
            hunger_per_tick: 1,
            fatigue_per_tick: 1,
            rest_per_tick: 4,
            deprivation: 120,
            harm_per_tick: 1,
            guardian_recovery: 2,
            natural_recovery: 0,

            max_health: 100,
        }
    }
}

/// Explicit activity costs in thousandths of hunger and fatigue units.
#[derive(Clone, Serialize, Deserialize)]
pub struct Exert {
    pub person: PersonId,
    pub energy: u64,
    pub fatigue: u64,
}
impl game_sim::Request for Exert {
    const KEY: &'static str = "physiology.exert";
    type Response = ();
}
