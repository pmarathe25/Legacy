//! Transaction-local urgent invalidation for character-owned review phases.
use game_character_state::Minds;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
/// Mark a selected review only while its enclosing transaction remains open.
#[derive(Clone, Serialize, Deserialize)]
pub struct PauseReview(pub PersonId);
impl Request for PauseReview {
    const KEY: &'static str = "characters.pause-review";
    type Response = ();
}
/// Clear a paused review and report whether an urgent cue invalidated it.
#[derive(Clone, Serialize, Deserialize)]
pub struct FinishPausedReview(pub PersonId);
impl Request for FinishPausedReview {
    const KEY: &'static str = "characters.finish-paused-review";
    type Response = bool;
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<PauseReview>(|tx, r| {
        let state = tx.get::<Minds>()?;
        require(
            state.running
                && state.people.contains_key(&r.0)
                && state.schedule.waking.get(&r.0) == Some(&tx.now()),
            "review is not active",
        )?;
        require(
            !state.paused_reviews.contains_key(&r.0),
            "review already paused",
        )?;
        tx.edit::<Minds>()?.paused_reviews.insert(r.0, false);
        Ok(())
    })?;
    b.request::<FinishPausedReview>(|tx, r| {
        tx.edit::<Minds>()?
            .paused_reviews
            .remove(&r.0)
            .ok_or_else(|| Error::Rejected("review is not paused".into()))
    })?;
    Ok(())
}
/// Cancel the optional frame-cache deadline before its owning mind is cleared.
pub fn cancel_consideration(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if game_choice::frame_tuning(tx).nonurgent_millis > 0
        && let Some(memory) = tx
            .get::<Minds>()?
            .people
            .get(&person)
            .and_then(|m| m.considered.clone())
    {
        memory.cancel(tx);
    }
    Ok(())
}

/// The cohort path must use the same admission checks as an ordinary Resume.
pub fn resume_eligible(tx: &Transaction<'_>, person: PersonId) -> Result<bool> {
    let state = tx.get::<Minds>()?;
    if !state.running || !state.people.contains_key(&person) {
        return Ok(false);
    }
    if game_choice::frame_tuning(tx).urgent_immediate
        && state.schedule.waking.get(&person) != Some(&tx.now())
        && !tx
            .get::<game_choice::Offers>()?
            .frame
            .as_ref()
            .is_some_and(|frame| {
                frame.people.contains(&person) && game_choice::frame_start(tx) == Some(tx.now())
            })
    {
        return Ok(false);
    }
    Ok(true)
}
