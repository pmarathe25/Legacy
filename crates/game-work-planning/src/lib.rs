//! Personally perceived work goals and bounded recipe search. Execution belongs to livelihoods.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
mod memo;
pub use memo::{Query, choose, prepare};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub object: String,
    pub quantity: u64,
    pub priority: u32,
    pub affinity: String,
    pub concept: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub inventory: InventoryId,
    pub goal: GoodId,
    pub quantity: u64,
    pub concept: String,
    pub steps: VecDeque<Step>,
}
pub use game_craft_planning::{MAX_STEPS, Search, Step};
mod search;
pub fn uncached(
    tx: &Transaction<'_>,
    person: PersonId,
    inventories: &[InventoryId],
    goals: &[Goal],
) -> Result<Option<Plan>> {
    let _measurement = game_diagnostics::span("work search");
    search::capture(tx, person, inventories, goals)?.map_or(Ok(None), |input| input.run())
}
