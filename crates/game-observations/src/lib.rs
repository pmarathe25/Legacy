//! Domain adapters retain only the actor's actual perceptual episodes.
use game_core::*;
use game_inquiry::Witness;
use game_reasoning::Observation;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod life;
mod witnesses;
pub use game_food_inquiry::{Experiment, Inspect, Samples};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observations {
    pub trips: Table<PersonId, witnesses::Trip>,
    pub meals: Table<PersonId, witnesses::Meal>,
    pub crafts: Table<PersonId, (SimTime, u64)>,
    #[serde(default)]
    pub care: Table<PersonId, (PersonId, u64, u64)>,
}
impl Resource for Observations {
    const KEY: &'static str = "observations.episodes";
}
fn weather(tx: &Transaction<'_>, person: PersonId) -> BTreeMap<String, i64> {
    game_transport::surrounding(tx.get().unwrap(), person, tx.now())
        .and_then(|a| game_environment::at(tx, a).ok())
        .map(|s| {
            BTreeMap::from([
                ("temperature".into(), i64::from(s.warmth / 5 * 5)),
                ("rain".into(), i64::from(s.rain)),
            ])
        })
        .unwrap_or_default()
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_food_inquiry::plugin(b)?;
    b.plugin("observations", 1)?;
    b.resource::<Observations>()?;
    life::register(b)?;
    witnesses::register(b)
}
