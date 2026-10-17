//! Pure bodily state and time evolution, shared by physiology and its callers.
use game_core::*;
use serde::{Deserialize, Serialize};
mod compute;
mod events;
pub use compute::{advance_body, advance_scaled, hunger_threshold};
pub use events::{Damaged, Died, Rested};
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

            max_health: 100,
        }
    }
}
