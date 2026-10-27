//! Row-local nourishment and an owner-registered disjoint batch.
use crate::{Body, Rules};
use game_core::*;
use game_packed::Packed;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
pub struct ConsumeEdible {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for ConsumeEdible {
    const KEY: &'static str = "physiology.consume-edible";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Serve {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Serve {
    const KEY: &'static str = "physiology.serve";
    type Response = ();
}

/// Receiver-owned validation after all Serve hooks and before a phased meal's
/// physical effects. The selected intention owner may reject stale evidence.
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckLocalMeal(pub Serve);
impl Request for CheckLocalMeal {
    const KEY: &'static str = "physiology.check-local-meal";
    type Response = bool;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Fed {
    #[serde(default)]
    pub inventory: Option<InventoryId>,
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Event for Fed {
    const KEY: &'static str = "physiology.fed";
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Eating {
    pub person: PersonId,
}
impl Event for Eating {
    const KEY: &'static str = "physiology.eating";
}

/// Receiver-owned admission check for a paused character's selected meal.
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckSelfMeal {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for CheckSelfMeal {
    const KEY: &'static str = "physiology.check-self-meal";
    type Response = bool;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Nourish {
    pub person: PersonId,
    pub relief: u64,
    pub health: i32,
    pub alertness: i32,
}
impl Request for Nourish {
    const KEY: &'static str = "soma.nourish";
    type Response = ();
}

/// Independent nourishment effects, with a caller-measured parallel threshold.
/// Batches with repeated people, scalar hooks or possible death use scalar requests.
#[derive(Clone, Serialize, Deserialize)]
pub struct NourishMany {
    pub effects: Vec<Nourish>,
    pub minimum_parallel: usize,
}
impl Request for NourishMany {
    const KEY: &'static str = "soma.nourish-many";
    type Response = ();
}

pub fn apply_nourishment(body: &mut Body, maximum: u64, effect: &Nourish) {
    if effect.relief > body.hunger {
        body.hunger_fraction = 0;
    }
    body.hunger = body.hunger.saturating_sub(effect.relief);
    body.health = if effect.health >= 0 {
        body.health
            .saturating_add(effect.health as u64)
            .min(maximum)
    } else {
        body.health
            .saturating_sub(u64::from(effect.health.unsigned_abs()))
    };
    body.fatigue = if effect.alertness >= 0 {
        body.fatigue.saturating_sub(effect.alertness as u64)
    } else {
        body.fatigue
            .saturating_add(u64::from(effect.alertness.unsigned_abs()))
    };
}

/// Implemented by the body resource owner; workers never receive a transaction.
pub trait NourishmentOwner: Resource {
    fn body_rows(&self) -> &Packed<PersonId, Body>;
    fn body_rows_mut(&mut self) -> &mut Packed<PersonId, Body>;
    fn body_rules(&self) -> &Rules;
}

/// Register while the body owner is active, after its scalar Nourish handler.
pub fn register_nourishment_batch<S: NourishmentOwner>(b: &mut Builder) -> Result<()> {
    b.request::<NourishMany>(|tx, batch| {
        require(
            batch.effects.len() <= 100_000,
            "too many nourishment effects",
        )?;
        if batch.effects.is_empty() {
            game_diagnostics::interaction_profile!({
                game_diagnostics::count("interaction.owner.nourishment.empty.calls", 1);
            });
            return Ok(());
        }
        let mut by_person = BTreeMap::new();
        let mut scalar = tx.has_before::<Nourish>();
        let state = tx.get::<S>()?;
        for effect in &batch.effects {
            if by_person.insert(effect.person, effect.clone()).is_some() {
                scalar = true;
            }
            let Some(body) = state.body_rows().get(&effect.person) else {
                scalar = true;
                continue;
            };
            let mut after = *body;
            apply_nourishment(&mut after, body.maximum(state.body_rules()), effect);
            if !body.alive || after.health == 0 {
                scalar = true;
            }
        }
        if scalar {
            game_diagnostics::interaction_profile!({
                game_diagnostics::count("interaction.owner.nourishment.scalar_fallback.calls", 1);
                game_diagnostics::count(
                    "interaction.owner.nourishment.scalar_fallback.rows",
                    batch.effects.len() as u64,
                );
            });
            for effect in batch.effects {
                tx.call(effect)?;
            }
            return Ok(());
        }
        let ids: Vec<_> = batch.effects.iter().map(|effect| effect.person).collect();
        let rules = state.body_rules().clone();
        let updated = tx.edit_with_compute::<S, _>(|state, pool| {
            game_diagnostics::interaction_profile!(batch "nourishment", ids.len(),
                ids.len() >= batch.minimum_parallel && pool.current_num_threads() > 1);
            state
                .body_rows_mut()
                .update_selected(pool, &ids, batch.minimum_parallel, |p, body| {
                    apply_nourishment(body, body.maximum(&rules), &by_person[p]);
                })
        })?;
        require(updated, "nourishment rows changed")
    })
}
