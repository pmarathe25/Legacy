//! Disjoint exertion, registered and mutated exclusively by the soma owner.
use game_body::{Exert, NourishmentOwner};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
pub struct ExertMany(pub Vec<Exert>);
impl Request for ExertMany {
    const KEY: &'static str = "soma.exert-many";
    type Response = ();
}
pub fn register<S: NourishmentOwner>(b: &mut Builder) -> Result<()> {
    b.request::<ExertMany>(|tx, r| {
        require(r.0.len() <= 100_000, "too many effort rows")?;
        require(
            r.0.windows(2).all(|p| p[0].person < p[1].person),
            "unordered effort rows",
        )?;
        if tx.has_before::<Exert>() {
            for effect in r.0 {
                tx.call(effect)?;
            }
            return Ok(());
        }
        let mut effects = BTreeMap::new();
        for effect in &r.0 {
            require(tx.get::<S>()?.body_rows().contains_key(&effect.person), "missing effort body")?;
            effects.insert(effect.person, (effect, game_vitality::factor(game_vitality::ability(tx, effect.person, "endurance"))));
        }
        let people: Vec<_> = r.0.iter().map(|r| r.person).collect();
        let updated = tx.edit_with_compute::<S, _>(|state, pool| {
            game_diagnostics::interaction_profile!(batch "exertion", people.len(), people.len() >= 4 && pool.current_num_threads() > 1);
            state.body_rows_mut().update_selected(pool, &people, 4, |p, body| {
                let (effect, endurance) = effects[p];
                body.exert(effect.energy, effect.fatigue, endurance);
            })
        })?;
        require(updated, "effort rows changed")
    })
}
