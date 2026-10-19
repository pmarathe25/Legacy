//! Personally inspected food samples and finite storage experiments.
use game_core::*;
use game_inquiry::Witness;
use game_reasoning::Observation;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod choices;
mod food;
pub use food::{Experiment, Inspect};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Samples {
    pub probes: Table<PersonId, Table<String, food::Probe>>,
    pub trials: Table<PersonId, food::Split>,
    #[serde(default)]
    pub inspections: Table<PersonId, Table<InventoryId, (SimTime, GoodId)>>,
}
impl Resource for Samples {
    const KEY: &'static str = "food-inquiry.samples";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("food-inquiry", 1)?;
    b.resource::<Samples>()?;
    food::register(b)?;
    choices::register(b)
}
