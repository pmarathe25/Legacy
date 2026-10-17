//! The generated-world autonomy preset composes feature-owned enable requests.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Start;
impl Request for Start {
    const KEY: &'static str = "campaign.start";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("campaign", 1)?;
    b.request::<Start>(|tx, _| {
        tx.call(game_agriculture::EnableNatural)?;
        tx.call(game_botany::Enable)?;
        tx.call(game_vitality::Enable)?;
        tx.call(game_nutrition::Enable)?;
        let people: Vec<_> = tx
            .get::<game_physiology::Bodies>()?
            .people
            .keys()
            .copied()
            .collect();
        for person in people {
            tx.call(game_heredity::Founder { person })?;
        }
        tx.call(game_provisioning::Enable)?;
        tx.call(game_grievances::Enable)?;
        if tx.registered::<game_names::Enable>() {
            tx.call(game_names::Enable)?;
        }
        tx.call(game_households::Enable)?;
        tx.call(game_exchange::Enable)?;
        tx.call(game_works::Enable)?;
        tx.call(game_food_storage::Enable)?;
        tx.call(game_preservation::Enable)?;
        tx.call(game_maturation::Enable)?;
        tx.call(game_equipment::Enable)?;
        tx.call(game_shelter::Enable)?;
        tx.call(game_exposure::Enable)?;
        tx.call(game_wayfaring::Enable)?;
        tx.call(game_horticulture::Enable)?;
        tx.call(game_care::Enable)?;
        tx.call(game_livelihoods::Enable)?;
        tx.call(game_pastimes::Enable)?;
        tx.call(game_communities::Enable)?;
        Ok(())
    })
}
