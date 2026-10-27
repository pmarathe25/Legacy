//! Persistent participation cursor for experimental cohort arbitration.
use game_core::*;
use game_offers::Offers;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
pub struct NextMealRound;
impl Request for NextMealRound {
    const KEY: &'static str = "choice.next-meal-round";
    type Response = u64;
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<NextMealRound>(|tx, _| {
        let frame = tx
            .edit::<Offers>()?
            .frame
            .as_mut()
            .ok_or_else(|| Error::Rejected("frames disabled".into()))?;
        let frame = Arc::make_mut(frame);
        require(frame.tuning.meal_arbitration, "meal arbitration disabled")?;
        let round = frame.meal_round;
        frame.meal_round = round.checked_add(1).ok_or(Error::Overflow)?;
        Ok(round)
    })
}
