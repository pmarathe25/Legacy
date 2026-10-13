//! Negotiation, institutional reception, judgment and autonomous choices.
use crate::*;
use game_core::*;
use game_ecs::{Component, Store};
use game_society::institutions::{
    AuthorityFacts, Institution, InstitutionChange, RuleProposalStatus,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RealmEvent {
    Proposed {
        commitment: RealmCommitmentId,
        actor: PersonId,
        revision: u32,
    },
    Ratified {
        commitment: RealmCommitmentId,
        revision: u32,
        institution: InstitutionId,
        proposal: RuleProposalId,
        adopted: bool,
    },
    WorkRequested {
        key: TermKey,
        actor: PersonId,
    },
    Reported {
        commitment: RealmCommitmentId,
        sender: PersonId,
        recipient: PersonId,
    },
    Interrupted {
        actor: PersonId,
    },
}
pub struct JudgmentFacts<'a> {
    pub hunger: u64,
    pub hunger_exempt: bool,
    pub meal_relief: u64,
    pub meal_size: Quantity,
    pub hydration_urgency: i64,
    pub drink_quantity: Quantity,
    pub concepts: &'a BTreeMap<ConceptId, Good>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewChoice {
    Ratify {
        institution: InstitutionId,
        change: InstitutionChange,
    },
    Work {
        key: TermKey,
    },
}
impl Runtime {
    pub fn view(&self, store: &Store, actor: PersonId) -> View {
        View {
            commitments: knowledge(store, actor).map_or_else(Vec::new, |known| {
                known
                    .commitments
                    .iter()
                    .rev()
                    .take(64)
                    .map(|(id, k)| KnownCommitment {
                        id: *id,
                        knowledge: k.clone(),
                    })
                    .collect()
            }),
        }
    }
    pub fn validate_change(
        &self,
        store: &Store,
        institution: &Institution,
        change: &InstitutionChange,
        now: SimTime,
    ) -> Result<(), Error> {
        match change {
            InstitutionChange::RatifyRegional {
                commitment: id,
                revision,
                ..
            } => {
                let r = commitment(store, *id).ok_or(Error::Unavailable)?.current();
                if r.number != *revision
                    || !r.proposal.participants.contains_key(&institution.id)
                    || r.proposal.ratify_by < now
                    || r.status() != Status::AwaitingRatification
                {
                    return Err(Error::Unavailable);
                }
            }
            InstitutionChange::AuthorizeRegionalPassage {
                commitment: id,
                revision,
                index,
            } => {
                let r = commitment(store, *id)
                    .and_then(|c| c.revision(*revision))
                    .ok_or(Error::Unavailable)?;
                if !matches!(r.proposal.terms.get(usize::from(*index)),Some(Term::Passage{grantor,until,..}) if *grantor==institution.id && *until>now)
                    || !crate::state::fully_ratified(r)
                {
                    return Err(Error::Unavailable);
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn native_outcome(
        &self,
        store: &Store,
        institution: InstitutionId,
        proposal: RuleProposalId,
        status: RuleProposalStatus,
        now: SimTime,
    ) -> Result<Option<(PreparedChange, RealmEvent)>, Error> {
        let record = game_society::proposal(store, proposal).ok_or(Error::Unavailable)?;
        let InstitutionChange::RatifyRegional {
            commitment,
            revision,
            support,
        } = record.change
        else {
            return Ok(None);
        };
        if record.institution != institution || status == RuleProposalStatus::Proposed {
            return Err(Error::Unavailable);
        }
        if !support && status == RuleProposalStatus::Rejected {
            return Ok(None);
        }
        let adopted = support && status == RuleProposalStatus::Adopted;
        let decision = Ratification {
            proposal,
            outcome: if adopted {
                Decision::Adopted
            } else {
                Decision::Rejected
            },
            decided_at: now,
        };
        let change = Ledger::new(store).prepare_native_decision(
            commitment,
            revision,
            institution,
            decision,
            now,
        )?;
        Ok(Some((
            change,
            RealmEvent::Ratified {
                commitment,
                revision,
                institution,
                proposal,
                adopted,
            },
        )))
    }
    pub fn vote_score<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        change: &InstitutionChange,
        facts: &JudgmentFacts<'_>,
    ) -> i64 {
        let (id, revision, support) = match change {
            InstitutionChange::RatifyRegional {
                commitment,
                revision,
                support,
            } => (*commitment, *revision, *support),
            InstitutionChange::AuthorizeRegionalPassage {
                commitment,
                revision,
                ..
            } => (*commitment, *revision, true),
            _ => return 0,
        };
        let Some(known) = Ledger::new(store)
            .view(actor, id)
            .and_then(|k| k.revisions.get(&revision))
        else {
            return -10_000;
        };
        let Some(mind) = game_characters::decisions::mind::<A>(store, actor) else {
            return -10_000;
        };
        let affiliations = institution_view(store, actor)
            .iter()
            .filter(|i| i.members.contains(&actor))
            .map(|i| i.id)
            .collect::<BTreeSet<_>>();
        use game_characters::proposals::{Context, Effect, Evidence};
        let mut effects = Vec::new();
        let resource = |affiliation, gained, term: &game_agreements::AgreementTerm| {
            if let game_agreements::AgreementTerm::Delivery {
                source, quantity, ..
            } = term
            {
                Some(Effect::Resource {
                    affiliation,
                    gained,
                    good: game_characters::decisions::stocks(store, actor)
                        .and_then(|s| s.known.get(source))
                        .map(|k| k.good),
                    quantity: *quantity,
                })
            } else {
                None
            }
        };
        for term in &known.proposal.value.terms {
            match term {
                Term::Delivery {
                    provider,
                    beneficiary,
                    term,
                } => {
                    effects.extend(resource(*provider, false, term));
                    effects.extend(resource(*beneficiary, true, term));
                }
                Term::Passage { beneficiary, .. } => effects.push(Effect::Passage {
                    beneficiary: *beneficiary,
                }),
                Term::Protection {
                    provider,
                    beneficiary,
                    patrol,
                } => {
                    effects.push(Effect::Protection {
                        beneficiary: *beneficiary,
                    });
                    effects.push(Effect::Danger {
                        provider: *provider,
                    });
                    for term in patrol.compensation.values().flatten() {
                        effects.extend(resource(*provider, false, term));
                    }
                }
            }
        }
        let context = Context {
            affiliations: &affiliations,
            hunger: facts.hunger,
            hunger_exempt: facts.hunger_exempt,
            meal_relief: facts.meal_relief,
            meal_size: facts.meal_size,
            hydration_urgency: facts.hydration_urgency,
            drink_quantity: facts.drink_quantity,
            concepts: facts.concepts,
        };
        let score = game_characters::proposals::assess(
            &mind.personality,
            &mind.values,
            &context,
            &Evidence {
                proposal: (id, revision),
                effects,
            },
        )
        .total;
        if support {
            score
        } else {
            score.saturating_neg()
        }
    }
    pub fn available_actions<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        authority: &AuthorityFacts,
        presence: &impl Presence,
    ) -> Vec<Action> {
        let Some(area) = actor_area(presence, actor) else {
            return Vec::new();
        };
        let ledger = Ledger::new(store);
        let mut actions = Vec::new();
        for entry in self.view(store, actor).commitments {
            for (number, known) in &entry.knowledge.revisions {
                if let Ok(facts) = self.proposal_facts::<A>(
                    store,
                    actor,
                    &known.proposal.value,
                    true,
                    &RealmContext {
                        now,
                        authority,
                        presence,
                    },
                ) {
                    for index in 0..known.proposal.value.terms.len() {
                        let key = TermKey {
                            commitment: entry.id,
                            revision: *number,
                            index: index as u16,
                        };
                        if ledger.prepare_work_request(key, actor, &facts, now).is_ok() {
                            actions.push(Action::RequestWork { key });
                        }
                    }
                }
            }
            for recipient in positioned_people(presence, area)
                .filter(|p| {
                    *p != actor
                        && actor_area(presence, *p) == Some(area)
                        && presence.actor(*p).is_some_and(|p| p.idle_or_rest)
                })
                .take(4)
            {
                actions.push(Action::Report {
                    commitment: entry.id,
                    recipient,
                });
            }
        }
        actions.truncate(64);
        actions
    }
    pub fn review_eligible(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        presence: &impl Presence,
    ) -> bool {
        presence.actor(actor).is_some_and(|p| {
            !p.controlled && !p.past_life_controlled && p.idle_or_rest && contact_area(p).is_some()
        }) && requests(store, actor).is_none_or(|r| r.reconsider_after <= now)
    }
    pub fn review_choices<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        authority: &AuthorityFacts,
        presence: &impl Presence,
        judgment: &JudgmentFacts<'_>,
    ) -> Vec<ReviewChoice> {
        if !self.review_eligible(store, actor, now, presence) {
            return Vec::new();
        }
        let mut choices = Vec::new();
        let ledger = Ledger::new(store);
        for offer in self.view(store, actor).commitments {
            let Some((revision, known)) = offer.knowledge.revisions.last_key_value() else {
                continue;
            };
            let Ok(facts) = self.proposal_facts::<A>(
                store,
                actor,
                &known.proposal.value,
                true,
                &RealmContext {
                    now,
                    authority,
                    presence,
                },
            ) else {
                continue;
            };
            for p in known
                .proposal
                .value
                .participants
                .values()
                .filter(|p| p.representative == actor)
            {
                if ledger
                    .prepare_ratification_request(offer.id, p.institution, actor, &facts, now)
                    .is_err()
                {
                    continue;
                }
                let positive = InstitutionChange::RatifyRegional {
                    commitment: offer.id,
                    revision: *revision,
                    support: true,
                };
                let support = self.vote_score::<A>(store, actor, &positive, judgment) >= 0;
                choices.push(ReviewChoice::Ratify {
                    institution: p.institution,
                    change: InstitutionChange::RatifyRegional {
                        commitment: offer.id,
                        revision: *revision,
                        support,
                    },
                });
            }
        }
        choices.extend(
            self.available_actions::<A>(store, actor, now, authority, presence)
                .into_iter()
                .filter_map(|a| {
                    if let Action::RequestWork { key } = a {
                        Some(ReviewChoice::Work { key })
                    } else {
                        None
                    }
                }),
        );
        choices
    }
    pub fn expiry_ids(&self, store: &Store, now: SimTime) -> Vec<RealmCommitmentId> {
        commitments(store)
            .filter(|(_, c)| {
                c.current().status() == Status::AwaitingRatification
                    && c.current().proposal.ratify_by < now
            })
            .map(|(id, _)| id)
            .collect()
    }
    pub fn boundary(&self, store: &Store, now: SimTime, limit: SimTime) -> SimTime {
        commitments(store)
            .filter(|(_, c)| c.current().status() == Status::AwaitingRatification)
            .filter_map(|(_, c)| c.current().proposal.ratify_by.0.checked_add(1).map(SimTime))
            .filter(|at| *at > now)
            .min()
            .map_or(limit, |at| at.min(limit))
    }
    pub fn allocation_bound(&self, people: usize) -> u64 {
        (people as u64).saturating_mul(512)
    }
    pub fn reconsideration(&self, now: SimTime) -> Requests {
        Requests {
            reconsider_after: SimTime(now.0.saturating_add(180_000)),
        }
    }
}

pub struct PreparedReview {
    choices: std::vec::IntoIter<ReviewChoice>,
    done: bool,
    pub reconsideration: Requests,
}
impl PreparedReview {
    pub fn next_attempt(&mut self) -> Option<ReviewChoice> {
        if self.done { None } else { self.choices.next() }
    }
    pub fn receive_result(&mut self, succeeded: bool) {
        self.done |= succeeded;
    }
}
impl Runtime {
    pub fn prepare_review<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        context: &RealmContext<'_, impl Presence>,
        judgment: &JudgmentFacts<'_>,
    ) -> Option<PreparedReview> {
        self.review_eligible(store, actor, context.now, context.presence)
            .then(|| PreparedReview {
                choices: self
                    .review_choices::<A>(
                        store,
                        actor,
                        context.now,
                        context.authority,
                        context.presence,
                        judgment,
                    )
                    .into_iter(),
                done: false,
                reconsideration: self.reconsideration(context.now),
            })
    }
}
impl Runtime {
    pub fn has_commitments(&self, store: &Store) -> bool {
        store.count::<Commitment>() > 0
    }
}

pub fn regional_change(change: &InstitutionChange) -> bool {
    matches!(
        change,
        InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. }
    )
}
impl Runtime {
    pub fn prepare_published_native_outcome(
        &self,
        store: &Store,
        institution: InstitutionId,
        proposal: RuleProposalId,
        status: RuleProposalStatus,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<Option<(PreparedChange, RealmEvent)>, Error> {
        self.native_outcome(store, institution, proposal, status, now)?
            .map(|(plan, event)| {
                let area = game_society::institution(store, institution)
                    .ok_or(Error::Unavailable)?
                    .charter
                    .jurisdiction;
                let revision = plan.revision_number().ok_or(Error::Unavailable)?;
                Ok((
                    plan.publish_decision(store, revision, institution, area, now, presence)?,
                    event,
                ))
            })
            .transpose()
    }
}
