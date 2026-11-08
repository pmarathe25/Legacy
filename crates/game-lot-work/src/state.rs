//! Serializable physical production state.
use game_lot_model::State;
use game_production_model::{Good, Job, Recipe};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Working,
    /// The last lead has departed; custody and site capacity remain reserved
    /// until the given boundary, when normal return begins if nobody takes over.
    Suspended {
        until: u64,
    },
    Waiting,
    Returning,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub job: Job,
    pub phase: Phase,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub custody: State,
    pub goods: Vec<Good>,
    pub recipes: Vec<Recipe>,
    pub tick_millis: u64,
    pub work: Vec<Work>,
}
