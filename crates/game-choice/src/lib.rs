//! Feature offers are gathered before a character chooses an executable owner request.
mod keys;
mod templates;
pub use keys::Keys;
pub use templates::Templates;
mod agenda;
mod execution;
mod undertaking;
pub use agenda::{Next, Schedule};
pub use execution::{ConfigureExecution, GoalReady};
use game_core::*;
pub use game_decision_frames::{
    AuxiliaryReview, Configure as ConfigureFrames, ConfigureDemand as ConfigureFrameDemand,
    InvalidatePerson as InvalidateFramePerson, PlanCohort, Recheck as FrameRecheck, StoreDecisions,
    Tune as TuneFrames, action_changed as frame_action_changed, auxiliary as auxiliary_provider,
    deadline as frame_deadline, decision as frame_decision,
    decode_pending as inspect_frame_request, demand as frame_demand, payload as frame_payload,
    recheck as frame_stale, recheck_provider as frame_stale_provider, start as frame_start,
    tuning as frame_tuning, wake as wake_frame,
};
pub use game_frame_arbitration::NextMealRound;
pub use game_offers::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
pub use undertaking::{Resolved, Undertake};
/// Collect ordered provider effects before evaluating the caller's query.
pub fn collect_with<T: Send>(
    tx: &mut Transaction<'_>,
    person: PersonId,
    query: impl Fn(&Transaction<'_>) -> Result<T> + Sync,
) -> Result<(Vec<Proposal>, Result<T>)> {
    Ok((tx.call(Collect { person })?, query(tx)))
}
/// Inert fallback; the historical wire key remains readable in saved requests.
#[derive(Clone, Serialize, Deserialize)]
pub struct Wait;
impl Request for Wait {
    const KEY: &'static str = "characters.wait";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Run(pub Pending);
impl Request for Run {
    const KEY: &'static str = "choice.run";
    type Response = Outcome;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CollectScoped {
    pub person: PersonId,
    pub fast: bool,
}
impl Request for CollectScoped {
    const KEY: &'static str = "choice.collect-scoped";
    type Response = Vec<Proposal>;
}
/// Profile proposal providers without coupling the kernel to gameplay diagnostics.
pub fn provider(
    b: &mut Builder,
    name: &'static str,
    listener: impl Fn(&mut Transaction<'_>, &Review) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    game_decision_frames::provider(b, name, move |tx, e| {
        let _span = game_diagnostics::span(name);
        listener(tx, e)
    })
}
/// A provider may explicitly defer pure discovery during a scoped fast review.
/// Do not opt in a callback with required observations, sensing or other effects:
/// the callback is skipped, so the framework cannot detect those effects.
/// Unknown, safety-critical and ordinary serial providers remain live.
pub fn deliberative_provider(
    b: &mut Builder,
    name: &'static str,
    listener: impl Fn(&mut Transaction<'_>, &Review) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    provider(b, name, move |tx, event| {
        if fast_review(tx) {
            Ok(())
        } else {
            listener(tx, event)
        }
    })
}
/// An observation-producing provider is an ordered barrier between pure drafts.
pub fn serial_provider(
    b: &mut Builder,
    name: &'static str,
    listener: impl Fn(&mut Transaction<'_>, &Review) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    b.subscribe::<Review>(name, move |tx, e| {
        let _span = game_diagnostics::span(name);
        with_provider(tx, None, |tx| listener(tx, e))
    })
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_identity::plugin(b)?;
    b.plugin("choice", 1)?;
    b.resource::<Offers>()?;
    game_anticipation::register(b)?;
    undertaking::register(b)?;
    b.request::<Wait>(|_, _| Ok(()))?;
    game_offers::register(b)?;
    b.request::<CollectScoped>(|tx, request| {
        let previous = tx.edit::<Offers>()?.set_fast_review(request.fast);
        let result = tx.call(Collect {
            person: request.person,
        });
        tx.edit::<Offers>()?.set_fast_review(previous);
        result
    })?;
    game_frame_arbitration::register(b)?;
    execution::register(b)?;
    game_decision_frames::register(b)?;
    b.request::<Run>(|tx, r| {
        let response = tx.invoke(r.0)?;
        require(
            response != serde_json::Value::Bool(false),
            "opportunity no longer available",
        )?;
        Ok(serde_json::from_value(response).unwrap_or(Outcome::Performed))
    })
}
