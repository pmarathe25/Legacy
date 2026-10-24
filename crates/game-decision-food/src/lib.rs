//! Optional cohort food inputs; action execution remains with provisioning.
use game_core::*;
use game_provisioning::Opportunity;
use game_sim::*;
use serde::{Deserialize, Serialize};
const PROVIDER: &str = "characters.frame-food";
#[derive(Clone, Serialize, Deserialize)]
struct Meals {
    count: u64,
    options: Vec<Opportunity>,
}
impl Request for Meals {
    const KEY: &'static str = "characters.frame-food";
    type Response = ();
}
pub fn register(b: &mut Builder, count: fn(&Transaction<'_>) -> Result<u64>) -> Result<()> {
    b.request::<Meals>(|_, _| Err(Error::Rejected("prepared input is not an action".into())))?;
    game_choice::auxiliary_provider(
        b,
        PROVIDER,
        |tx| game_choice::frame_tuning(tx).batch_food,
        move |tx, e| {
            let count = count(tx)?;
            let options = game_provisioning::options(tx, e.person, count)?;
            game_choice::offer(
                tx,
                e.person,
                PROVIDER,
                game_choice::motive(0, 0, 0, 0, 0),
                Meals { count, options },
            )?;
            Ok(())
        },
    )
}
pub fn options(tx: &Transaction<'_>, person: PersonId, count: u64) -> Result<Vec<Opportunity>> {
    if game_choice::frame_tuning(tx).batch_food
        && let Some(prepared) = game_choice::frame_payload::<Meals>(tx, person, PROVIDER)
        && prepared.count == count
    {
        return Ok(prepared.options);
    }
    game_provisioning::options(tx, person, count)
}
