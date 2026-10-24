//! Deliberation policy; optional routine reuse leaves execution with action owners.
mod deadlines;
mod rules;
pub use deadlines::{Recall, Resume, next};
mod consideration;
mod persistence;
pub use persistence::{continuation, failure_risk};
mod state;
pub use consideration::{Consideration, Options, consider};
pub use game_attention::{Rates as AttentionRates, ReviewClock};
use game_choice::{Proposal, Run, Undertake};
use game_core::*;
use game_sim::{Actions, Request, Transaction};
pub use rules::{Configure, Rules, load};
use serde::{Deserialize, Serialize};
pub use state::Mind;
use std::collections::BTreeSet;

/// Experimental attention window, explicitly selected by content or a study.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_floor: Option<SimDuration>,
    /// Opt-in: self-completion care prompts do not invalidate a retained routine.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preserve_on_care_completion: bool,
    pub minimum: SimDuration,
    pub maximum: SimDuration,
    pub kinds: BTreeSet<String>,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        require(
            self.minimum.0 > 0
                && self
                    .review_floor
                    .is_none_or(|f| f.0 > 0 && f <= self.maximum)
                && self.maximum >= self.minimum
                && !self.kinds.is_empty()
                && self.kinds.iter().all(|k| !k.is_empty() && k.len() <= 40),
            "invalid routine attention policy",
        )
    }
}

/// A chosen executable, never a cached assertion that its prerequisites still hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routine {
    pub until: SimTime,
    action: u64,
    proposal: Proposal,
}
impl Routine {
    pub fn capture(
        tx: &Transaction<'_>,
        person: PersonId,
        proposal: &Proposal,
        policy: Option<&Policy>,
    ) -> Result<Option<Self>> {
        let Some(policy) = policy else {
            return Ok(None);
        };
        let Some(action) = tx.get::<Actions>()?.0.get(&person) else {
            return Ok(None);
        };
        // Only repeat timed, receiver-validated undertakings. Food plans, rest,
        // journeys and raw callbacks retain their ordinary decision path.
        if proposal.request.key() != Undertake::KEY
            || proposal.continuation.is_some()
            || !policy.kinds.contains(&action.kind)
        {
            return Ok(None);
        }
        let span = SimDuration(
            MINUTE
                .0
                .saturating_mul(game_vitality::attention_horizon(tx, person, 1)),
        )
        .max(policy.minimum)
        .min(policy.maximum);
        let until = tx.now().after(span)?;
        if action.until.is_none_or(|finish| finish >= until) {
            return Ok(None);
        }
        // A retained executable must satisfy the same wire contract as a job.
        serde_json::to_writer(std::io::sink(), proposal)
            .map_err(|e| Error::InvalidSave(e.to_string()))?;
        Ok(Some(Self {
            until,
            action: action.id,
            proposal: proposal.clone(),
        }))
    }
    pub fn resume(
        &mut self,
        tx: &mut Transaction<'_>,
        person: PersonId,
        urgent_hunger: u64,
        rest_fatigue: u64,
    ) -> Result<Option<u64>> {
        let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
        if tx.now() >= self.until
            || !body.alive
            || body.health == 0
            || body.hunger >= urgent_hunger
            || body.fatigue > rest_fatigue
        {
            return Ok(None);
        }
        if let Some(action) = tx.get::<Actions>()?.0.get(&person) {
            return Ok((action.id == self.action).then_some(action.id));
        }
        let _measurement = game_diagnostics::span("routine continuation");
        if tx
            .attempt(Run(self.proposal.request.clone()))
            .is_ok_and(|outcome| outcome == game_choice::Outcome::Performed)
        {
            let action = tx.get::<Actions>()?.0.get(&person).map(|a| a.id);
            if let Some(id) = action {
                self.action = id;
            }
            return Ok(action);
        }
        Ok(None)
    }
}
