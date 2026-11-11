//! Family data contracts.
use game_genetics::Genome;
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
/// An individual's subjective inclinations; weights may be negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MatePreference {
    /// Baseline desire per thousand before genetic and personal circumstances.
    pub desire: i32,
    /// Contribution of the inherited mate-desire locus, scaled by 1000.
    pub genetic_weight: i32,
    /// Personal response to directed affinity toward the prospective mate.
    pub affinity_weight: i32,
    /// Personal response to a stable, directed attraction toward this person.
    pub attraction_weight: i32,
    /// Individual attraction adjustments for the candidate's modeled sex.
    pub female_attraction: i32,
    pub male_attraction: i32,
    /// Personal response to own hunger.
    pub hunger_weight: i32,
    /// Personal response to each living minor child; either sign is permitted.
    pub dependent_weight: i32,
    /// Minimum acceptable subjective evaluation of a prospective mate.
    pub minimum: i32,
}
impl Default for MatePreference {
    fn default() -> Self {
        Self {
            desire: 500,
            genetic_weight: 250,
            affinity_weight: 10,
            attraction_weight: 100,
            female_attraction: 0,
            male_attraction: 0,
            hunger_weight: -2,
            dependent_weight: -100,
            minimum: 0,
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mate_preference: Option<MatePreference>,
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

/// Matching people must share both sides of a staged physical partition.
pub type ContactPartition = (u64, u64);

/// Committed or staged physical reads keyed by stable identity.
pub trait PhysicalView: Sync {
    fn read(&self, id: u64) -> LifeInput;
    /// A player-controlled person does not make this autonomous decision.
    fn controlled(&self, _id: u64) -> bool {
        false
    }
    /// The observer's own directed feeling, not a shared relationship score.
    fn affinity(&self, _observer: u64, _target: u64) -> i32 {
        0
    }
    /// Zero denotes the unpartitioned region; distinct values cannot touch.
    fn contact_partition(&self, _id: u64) -> ContactPartition {
        (0, 0)
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
