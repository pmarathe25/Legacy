//! Serializable physical production state.
use game_parallel_lot_model::State;
use game_parallel_production_model::{Good, Job, Recipe};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Working,
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
