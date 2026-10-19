//! Lazy stomach fullness is a bodily consequence, never a prohibition on eating.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stomach {
    pub amount: u64,
    pub at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digestion(pub Table<PersonId, Stomach>);
impl Resource for Digestion {
    const KEY: &'static str = "digestion.stomachs";
}
fn remaining(tx: &Transaction<'_>, p: PersonId) -> u64 {
    let Some(s) = tx.get::<Digestion>().ok().and_then(|d| d.0.get(&p)) else {
        return 0;
    };
    let Ok(bodies) = tx.get::<game_soma::Bodies>() else {
        return s.amount;
    };
    let rate = bodies
        .people
        .get(&p)
        .map_or(1000, |b| b.metabolic_rates.energy);
    // Follow the world's physiological clock, including accelerated test profiles.
    let used = u128::from(tx.now().0.saturating_sub(s.at.0))
        .saturating_mul(u128::from(rate))
        .saturating_mul(u128::from(bodies.rules.hunger_per_tick.max(1)))
        / (u128::from(bodies.rules.tick.0) * 20);
    s.amount
        .saturating_sub(used.min(u128::from(u64::MAX)) as u64)
}
/// Personal discomfort grows continuously with fullness; powerful motives can outweigh it.
pub fn discomfort(tx: &Transaction<'_>, p: PersonId) -> i64 {
    let size = game_vitality::ability(tx, p, "size").max(100);
    let fullness = u128::from(remaining(tx, p)) * 1000 / u128::from(size.saturating_mul(3) / 2);
    (fullness.saturating_mul(fullness) / 10000).min(i64::MAX as u128) as i64
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Ingest {
    pub person: PersonId,
    /// Thousandths of an ordinary portion, including partial servings.
    pub amount: u64,
}
impl Request for Ingest {
    const KEY: &'static str = "digestion.ingest";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("digestion", 1)?;
    b.resource::<Digestion>()?;
    b.request::<Ingest>(|tx, r| {
        let amount = remaining(tx, r.person).saturating_add(r.amount);
        let at = tx.now();
        tx.edit::<Digestion>()?
            .0
            .insert(r.person, Stomach { amount, at });
        Ok(())
    })
}
