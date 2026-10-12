//! Control validation consumes identity and archive facts, never a playable world.
use crate::*;
use game_core::{Error, PersonId, SimTime};
use game_ecs::Store;
use std::collections::{BTreeMap, BTreeSet};

pub struct ControlValidation<'a> {
    pub now: SimTime,
    pub next_id: u64,
    pub current: Option<PersonId>,
    pub incarnations: &'a BTreeSet<PersonId>,
    pub archived: &'a BTreeSet<PersonId>,
}
impl ControlState {
    pub fn validate(&self, store: &Store, facts: ControlValidation<'_>) -> Result<(), Error> {
        let fail = || Error::InvalidSave("invalid invocation control".into());
        validate_rules(&self.rules).map_err(|_| fail())?;
        let row = one_session(store).map_err(|_| fail())?;
        if row.is_some_and(|(actor, _)| {
            !facts.incarnations.contains(&actor) || facts.current != Some(actor)
        }) {
            return Err(fail());
        }
        let valid_lives = |selected: &BTreeSet<PersonId>| {
            !selected.is_empty() && selected.len() <= 8 && selected.is_subset(facts.archived)
        };
        let consent_valid = |accepted: &BTreeMap<InvocationCost, Consent>| {
            accepted.iter().all(|(kind, consent)| {
                consent.permanent <= 252
                    && consent.temporary
                        <= match kind {
                            InvocationCost::Strain => 4,
                            InvocationCost::LossOfControl => 1,
                            _ => 3,
                        }
                    && (consent.permanent == 0
                        || matches!(kind, InvocationCost::Identity | InvocationCost::Lifespan))
            })
        };
        for (actor, value) in burdens(store) {
            if !facts.incarnations.contains(&actor)
                || value.burden_remainder >= 1_000
                || value.exposure_remainder >= 1_000
                || value.temporary_identity > 15
                || value.emotional_disruption > 300
                || value.permanent_identity > 252 * self.rules.permanent_identity_per_band
                || value.lifespan_loss > self.rules.lifespan_loss_per_band.0 * 252
                || value.uninterrupted > value.exposure
            {
                return Err(fail());
            }
        }
        if let Some(active) = self.active(store) {
            if facts.current != Some(active.actor)
                || !valid_lives(&active.lives)
                || burden(store, active.actor).is_none()
                || active.started > facts.now
                || active.next_loss_burden == 0
                || !consent_valid(&active.accepted)
                || !active.accepted.contains_key(&InvocationCost::Strain)
                || active
                    .dominant
                    .is_some_and(|life| !active.lives.contains(&life))
            {
                return Err(fail());
            }
            if let Some(episode) = &active.episode
                && (episode.actor != active.actor
                    || episode.starts > facts.now
                    || episode.ends <= facts.now
                    || episode.ends.0 - episode.starts.0 != self.rules.loss_duration.0
                    || active.next_controller < facts.now
                    || active.next_controller > episode.ends
                    || self.pending_consent(store).is_some()
                    || self.activation(store).is_some())
            {
                return Err(fail());
            }
        }
        if let Some(activation) = self.activation(store)
            && (facts.current != Some(activation.actor)
                || !valid_lives(&activation.lives)
                || !consent_valid(&activation.accepted)
                || !self.pending_consent(store).is_some_and(|pending| {
                    pending.activation.as_ref() == Some(&activation.lives)
                        && warning(store, pending.id)
                            .is_some_and(|view| view.actor == activation.actor)
                }))
        {
            return Err(fail());
        }
        for (id, value) in warnings(store) {
            if id.0 == 0
                || id.0 >= facts.next_id
                || id != value.id
                || !facts.incarnations.contains(&value.actor)
                || !valid_lives(&value.lives)
                || value.severity > 252
                || value.fatigue_per_mille > 64_000_000
                || value.health_harm_per_mille > 64_000_000
                || value.description.len() > 1_024
            {
                return Err(fail());
            }
        }
        if let Some(pending) = self.pending_consent(store)
            && (warning(store, pending.id).is_none_or(|view| facts.current != Some(view.actor))
                || pending
                    .activation
                    .as_ref()
                    .is_some_and(|lives| !valid_lives(lives)))
        {
            return Err(fail());
        }
        Ok(())
    }
}
