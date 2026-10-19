//! Personally motivated thermal comfort offers; physical owners retain clothing and rooms.
use game_choice::{Proposal, motive};
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod clothing;
mod heating;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Plans {
    pub fuel_for: Table<PersonId, AreaId>,
}
impl Resource for Plans {
    const KEY: &'static str = "comfort.intentions";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("comfort", 1)?;
    b.resource::<Plans>()?;
    heating::register(b)?;
    game_choice::provider(b, "comfort.options", |tx, e| {
        if tx
            .get::<game_lineage::Lineage>()
            .ok()
            .and_then(|l| l.current)
            == Some(e.person)
        {
            return Ok(());
        }
        let mut offers = vec![];
        if tx
            .get::<game_equipment::Equipment>()
            .is_ok_and(|s| s.enabled)
        {
            offers.extend(clothing::options(tx, e.person)?);
        }
        if tx.get::<game_shelter::Shelters>().is_ok_and(|s| s.enabled) {
            offers.extend(heating::options(tx, e.person)?);
        }
        if game_choice::collecting(tx, e.person) {
            for o in offers {
                game_choice::submit(tx, e.person, o)?;
            }
        } else if let Some(o) = offers
            .into_iter()
            .filter(|o| o.motive.score() > 0)
            .max_by_key(|o| o.motive.score())
        {
            tx.invoke(o.request)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("comfort.forget", |tx, e| {
        tx.edit::<Plans>()?.fuel_for.remove(&e.person);
        Ok(())
    })
}
