//! Expressed metabolism combines inherited potential with development and condition.
use game_body::Rates;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
/// A physical owner can supply local thermal costs without creating a dependency cycle.
#[derive(Clone, Serialize, Deserialize)]
pub struct Environment {
    pub person: PersonId,
}
impl Request for Environment {
    const KEY: &'static str = "metabolism.environment";
    type Response = Rates;
}
fn scale(value: u64, by: u64) -> u64 {
    (u128::from(value) * u128::from(by) / 1000).min(u128::from(u64::MAX)) as u64
}
/// Absolute food demand grows with body size. Childhood has higher demand per unit
/// of body mass, but a smaller total demand than the same person's adult body.
pub fn demand(
    traits: &game_traits::Traits,
    life: &game_vitality::Life,
    person: PersonId,
    at: SimTime,
) -> u64 {
    let metabolism = life.ability(traits.ability(person, "metabolism"), "metabolism", at);
    let size = game_vitality::factor(traits.ability(person, "size"));
    let growth = 250 + life.maturity(at, "size") * 750 / 1000;
    let maintenance = 1000
        + traits.ability(person, "constitution").saturating_sub(1000) / 100
        + traits.ability(person, "strength").saturating_sub(1000) / 100
        + traits.ability(person, "recovery").saturating_sub(1000) / 40;
    scale(scale(scale(metabolism, size), growth), maintenance).max(1)
}
pub fn rates(
    traits: &game_traits::Traits,
    life: &game_vitality::Life,
    person: PersonId,
    at: SimTime,
    environment: Rates,
) -> Rates {
    let (stress, _) = life.condition(at);
    let endurance = life.ability(traits.ability(person, "endurance"), "endurance", at);
    let recovery = life.ability(traits.ability(person, "recovery"), "recovery", at);
    let metabolism = life.ability(traits.ability(person, "metabolism"), "metabolism", at);
    Rates {
        energy: scale(
            scale(
                demand(traits, life, person, at),
                1000 + u64::from(1000 - life.physical) / 8,
            ),
            environment.energy,
        ),
        fatigue: scale(
            scale(
                1_000_000 / game_vitality::factor(endurance),
                1000 + u64::from(stress) / 2,
            ),
            environment.fatigue,
        ),
        recovery: scale(
            scale(
                scale(game_vitality::factor(recovery), metabolism),
                1000 - u64::from(stress) / 3,
            ),
            environment.recovery,
        )
        .max(1),
    }
}
