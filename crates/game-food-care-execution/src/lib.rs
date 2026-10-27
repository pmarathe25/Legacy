//! Physical food-care delivery under the care owner.
use game_care_perception::{candidates, concern, local, observed_need};
use game_core::*;
use game_households::{FoodCare, Households, RememberFoodCare};
use game_provisioning::Plan;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod delivery;
pub mod refusals;
pub use delivery::advance;

#[derive(Clone, Serialize, Deserialize)]
pub struct Review {
    pub caregiver: PersonId,
}
impl Request for Review {
    const KEY: &'static str = "care.food-review";
    type Response = game_choice::Outcome;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FeedingFailed {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub good: GoodId,
    pub quantity: u64,
    pub reason: String,
}
impl Event for FeedingFailed {
    const KEY: &'static str = "care.feeding-failed";
}
pub fn remember(
    tx: &mut Transaction<'_>,
    caregiver: PersonId,
    intent: Option<FoodCare>,
) -> Result<()> {
    tx.call(RememberFoodCare { caregiver, intent })
}
pub fn predicted(tx: &Transaction<'_>, p: PersonId, intent: &FoodCare) -> u64 {
    if game_communication::active(tx)
        && let Some(need) = game_communication::estimated_hunger(tx, p, intent.beneficiary)
    {
        return need;
    }
    let rules = &tx.get::<game_physiology::Bodies>().unwrap().rules;
    let elapsed = tx.now().0.saturating_sub(intent.observed_at.0);
    let foresight = game_vitality::ability(tx, p, "foresight");
    let growth = u128::from(elapsed) * u128::from(rules.hunger_per_tick) / u128::from(rules.tick.0);
    intent.observed_need.saturating_add(
        (growth * u128::from(foresight) / (1000 + u128::from(foresight))).min(u128::from(u64::MAX))
            as u64,
    )
}
pub fn prompt(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    if tx
        .get::<Households>()
        .is_ok_and(|h| h.food_care.contains_key(&p) || !candidates(tx, p).is_empty())
    {
        if tx
            .get::<game_characters::Minds>()
            .is_ok_and(|m| m.running && m.people.contains_key(&p))
        {
            return tx.notify(game_choice::Changed { person: p });
        }
        tx.schedule(tx.now(), 45, Review { caregiver: p })?;
    }
    Ok(())
}
