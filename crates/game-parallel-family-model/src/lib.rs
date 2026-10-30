//! Family data contracts.
use game_parallel_genetics::Genome;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pregnancy {
    pub father: u64,
    pub due: u64,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub id: u64,
    /// Founders may predate the simulation epoch and have no recorded birth tick.
    #[serde(default)]
    pub born: Option<u64>,
    pub age: u64,
    /// Permanent shortening from explicitly modeled lifespan costs.
    #[serde(default)]
    pub lifespan_loss: u64,
    pub genome: Genome,
    #[serde(default)]
    pub parents: smallvec::SmallVec<[u64; 2]>,
    #[serde(default)]
    pub partner: Option<u64>,
    #[serde(default)]
    pub pregnancy: Option<Pregnancy>,
    #[serde(default)]
    pub next_conception: u64,
    #[serde(default = "yes")]
    pub alive: bool,
    /// Explicit model-level reproductive opt-in; no inference from proximity alone.
    #[serde(default = "yes")]
    pub reproduction_allowed: bool,
    /// Willing to leave a partnership when the other person no longer wishes or can conceive.
    #[serde(default)]
    pub reconsider_partnership: bool,
}
#[derive(Clone, Copy)]
pub struct LifeInput {
    pub id: u64,
    pub region: Option<u64>,
    pub health: u32,
    pub hunger: u32,
}
pub struct Birth {
    pub id: u64,
    pub mother: u64,
    pub region: Option<u64>,
    pub health: u32,
}

/// Committed or staged physical reads keyed by stable identity.
pub trait PhysicalView: Sync {
    fn read(&self, id: u64) -> LifeInput;
    /// A player-controlled person does not make this autonomous decision.
    fn controlled(&self, _id: u64) -> bool {
        false
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyTotals {
    pub births: u64,
    pub deaths: u64,
    pub conceptions: u64,
    pub pregnancy_losses: u64,
    pub adulthoods: u64,
    pub partnerships: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyEvent {
    pub tick: u64,
    pub kind: String,
    pub people: Vec<u64>,
    pub region: Option<u64>,
}
