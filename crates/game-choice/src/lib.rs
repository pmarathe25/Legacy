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
    PlanCohort, StoreDecisions, Tune as TuneFrames, action_changed as frame_action_changed,
    auxiliary as auxiliary_provider, deadline as frame_deadline, decision as frame_decision,
    demand as frame_demand, payload as frame_payload, recheck as frame_stale,
    recheck_provider as frame_stale_provider, start as frame_start, tuning as frame_tuning,
    wake as wake_frame,
};
pub use game_offers::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
pub use undertaking::{Resolved, Undertake};
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
