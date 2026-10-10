use crate::validation::authoritative_actor;
use crate::*;
use game_core::*;
use game_economy::agreements::AgreementTerm;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) struct KnowledgePatch {
    pub actor: PersonId,
    pub before: Option<Knowledge>,
    pub after: Knowledge,
}
/// The adapter first preflights all owner work and allocation. Committing this
/// patch cannot execute an agreement, permission grant or mission itself.
#[derive(Clone, Debug)]
pub struct PreparedChange {
    pub(crate) id: RealmCommitmentId,
    pub(crate) before: Option<Commitment>,
    pub(crate) after: Option<Commitment>,
    pub(crate) knowledge: Vec<KnowledgePatch>,
    pub requests: Vec<Request>,
}
impl PreparedChange {
    pub fn commit(self, world: &mut World) -> Result<Vec<Request>, Error> {
        if self.after.is_some() && world.commitments.get(&self.id) != self.before.as_ref()
            || self
                .knowledge
                .iter()
                .any(|p| world.view(p.actor, self.id) != p.before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        if let Some(after) = self.after {
            world.commitments.insert(self.id, after);
        }
        for p in self.knowledge {
            world
                .known
                .entry(p.actor)
                .or_default()
                .insert(self.id, p.after);
        }
        Ok(self.requests)
    }
}
impl Revision {
    pub fn status(&self) -> Status {
        if self.expired_at.is_some() {
            return Status::Expired;
        }
        if self
            .ratifications
            .values()
            .any(|r| r.outcome == Decision::Rejected)
        {
            return Status::Rejected;
        }
        if self.ratifications.len() != self.proposal.participants.len() {
            return Status::AwaitingRatification;
        }
        if self
            .backed
            .values()
            .any(|b| matches!(b.progress, Progress::Defaulted | Progress::Refused))
        {
            return Status::Blocked;
        }
        if self.backed.len() == self.proposal.terms.len()
            && self
                .backed
                .values()
                .all(|b| b.progress == Progress::Fulfilled)
        {
            return Status::Fulfilled;
        }
        if self
            .backed
            .values()
            .any(|b| b.progress != Progress::AwaitingWork)
        {
            Status::Active
        } else {
            Status::Ratified
        }
    }
}
impl Commitment {
    pub fn current(&self) -> &Revision {
        self.revisions
            .last()
            .expect("commitment has its initial proposal")
    }
    pub fn revision(&self, revision: u32) -> Option<&Revision> {
        self.revisions
            .get(revision as usize)
            .filter(|r| r.number == revision)
    }
}
impl World {
    pub fn view(&self, actor: PersonId, id: RealmCommitmentId) -> Option<&Knowledge> {
        self.known.get(&actor)?.get(&id)
    }
    pub(crate) fn edit(&self, id: RealmCommitmentId) -> Result<PreparedChange, Error> {
        let before = self.commitments.get(&id).ok_or(Error::Unavailable)?.clone();
        Ok(PreparedChange {
            id,
            before: Some(before.clone()),
            after: Some(before),
            knowledge: Vec::new(),
            requests: Vec::new(),
        })
    }
    pub fn prepare_propose(
        &self,
        id: RealmCommitmentId,
        proposal: Proposal,
        facts: &ProposalFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        if id.0 == 0 || self.commitments.contains_key(&id) {
            return Err(Error::Unavailable);
        }
        proposal.validate(facts, now)?;
        let revision = new_revision(0, proposal, now);
        let actor = revision.proposal.proposer;
        let known = Knowledge {
            revisions: BTreeMap::from([(
                0,
                KnownRevision {
                    proposal: dated(revision.proposal.clone(), actor, now),
                    ratifications: BTreeMap::new(),
                    backed: BTreeMap::new(),
                },
            )]),
        };
        Ok(PreparedChange {
            id,
            before: None,
            after: Some(Commitment {
                id,
                revisions: vec![revision],
            }),
            knowledge: vec![KnowledgePatch {
                actor,
                before: self.view(actor, id).cloned(),
                after: known,
            }],
            requests: Vec::new(),
        })
    }
    pub fn prepare_revise(
        &self,
        id: RealmCommitmentId,
        proposal: Proposal,
        facts: &ProposalFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        proposal.validate(facts, now)?;
        let mut plan = self.edit(id)?;
        let commitment = plan.after.as_mut().unwrap();
        if now < commitment.current().proposed_at
            || proposal
                .participants
                .keys()
                .ne(commitment.current().proposal.participants.keys())
            || !commitment
                .current()
                .proposal
                .participants
                .keys()
                .any(|i| authoritative_actor(&proposal, *i, proposal.proposer, facts))
        {
            return Err(Error::Unavailable);
        }
        let number = commitment
            .current()
            .number
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let actor = proposal.proposer;
        let mut known = self.view(actor, id).cloned().ok_or(Error::Unavailable)?;
        known.revisions.insert(
            number,
            KnownRevision {
                proposal: dated(proposal.clone(), actor, now),
                ratifications: BTreeMap::new(),
                backed: BTreeMap::new(),
            },
        );
        commitment
            .revisions
            .push(new_revision(number, proposal, now));
        plan.knowledge.push(KnowledgePatch {
            actor,
            before: self.view(actor, id).cloned(),
            after: known,
        });
        Ok(plan)
    }
    /// This request must become a real native proposal under that institution's
    /// procedure. Its ID is registered by prepare_bind_ratification afterward.
    pub fn prepare_ratification_request(
        &self,
        id: RealmCommitmentId,
        institution: InstitutionId,
        actor: PersonId,
        facts: &ProposalFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let r = plan.after.as_ref().unwrap().current();
        if r.status() != Status::AwaitingRatification
            || now > r.proposal.ratify_by
            || !authoritative_actor(&r.proposal, institution, actor, facts)
            || r.ratification_proposals.contains_key(&institution)
            || self
                .view(actor, id)
                .and_then(|k| k.revisions.get(&r.number))
                .is_none_or(|k| k.proposal.value != r.proposal)
        {
            return Err(Error::Unavailable);
        }
        plan.requests.push(Request::Ratification {
            commitment: id,
            revision: r.number,
            institution,
            representative: actor,
        });
        Ok(plan)
    }
    pub fn prepare_bind_ratification(
        &self,
        id: RealmCommitmentId,
        revision: u32,
        institution: InstitutionId,
        proposal: RuleProposalId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let r = plan.after.as_mut().unwrap().revisions.last_mut().unwrap();
        if r.number != revision
            || proposal.0 == 0
            || !r.proposal.participants.contains_key(&institution)
            || now < r.proposed_at
            || now > r.proposal.ratify_by
            || r.status() != Status::AwaitingRatification
        {
            return Err(Error::Unavailable);
        }
        if let Some(old) = r.ratification_proposals.get(&institution) {
            if *old != proposal {
                return Err(Error::Unavailable);
            }
        } else {
            if r.ratification_proposals.values().any(|id| *id == proposal) {
                return Err(Error::Unavailable);
            }
            r.ratification_proposals.insert(institution, proposal);
        }
        Ok(plan)
    }
    /// Supply only an actual outcome from the registered native proposal. This
    /// has no quorum/default-law resolver and does not refresh personal views.
    pub fn prepare_decision(
        &self,
        id: RealmCommitmentId,
        revision: u32,
        institution: InstitutionId,
        decision: Ratification,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let r = plan.after.as_mut().unwrap().revisions.last_mut().unwrap();
        if r.number != revision
            || r.ratification_proposals.get(&institution) != Some(&decision.proposal)
            || decision.decided_at < r.proposed_at
            || decision.decided_at > now
            || decision.decided_at > r.proposal.ratify_by
            || r.expired_at.is_some()
        {
            return Err(Error::Unavailable);
        }
        if let Some(old) = r.ratifications.get(&institution) {
            if old != &decision {
                return Err(Error::Unavailable);
            }
        } else {
            r.ratifications.insert(institution, decision);
        }
        Ok(plan)
    }
    /// Combine the actual native owner's newly registered proposal and its
    /// terminal outcome in one atomic patch. The caller reads that owner's
    /// proposal; this is not an actor command or an alternative voting resolver.
    pub fn prepare_native_decision(
        &self,
        id: RealmCommitmentId,
        revision: u32,
        institution: InstitutionId,
        decision: Ratification,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let selected = plan.before.as_ref().unwrap().clone();
        let mut scratch = World {
            commitments: BTreeMap::from([(id, selected)]),
            known: BTreeMap::new(),
        };
        if scratch.commitments[&id]
            .revision(revision)
            .is_none_or(|r| !r.ratification_proposals.contains_key(&institution))
        {
            scratch
                .prepare_bind_ratification(id, revision, institution, decision.proposal, now)?
                .commit(&mut scratch)?;
        }
        scratch
            .prepare_decision(id, revision, institution, decision, now)?
            .commit(&mut scratch)?;
        plan.after = scratch.commitments.remove(&id);
        Ok(plan)
    }
    pub fn prepare_expire(
        &self,
        id: RealmCommitmentId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let r = plan.after.as_mut().unwrap().revisions.last_mut().unwrap();
        if now <= r.proposal.ratify_by || r.status() != Status::AwaitingRatification {
            return Err(Error::Unavailable);
        }
        r.expired_at = Some(now);
        Ok(plan)
    }
    pub fn prepare_work_request(
        &self,
        key: TermKey,
        actor: PersonId,
        facts: &ProposalFacts,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(key.commitment)?;
        let r = revision_mut(&mut plan, key)?;
        let term = r
            .proposal
            .terms
            .get(key.index as usize)
            .ok_or(Error::Unavailable)?
            .clone();
        let known = self
            .view(actor, key.commitment)
            .and_then(|k| k.revisions.get(&key.revision))
            .ok_or(Error::Unavailable)?;
        if !fully_ratified(r)
            || r.expired_at.is_some()
            || r.requested.contains(&key.index)
            || r.backed.contains_key(&key.index)
            || known.proposal.value != r.proposal
            || known.ratifications.len() != r.proposal.participants.len()
            || known
                .ratifications
                .iter()
                .any(|(i, d)| r.ratifications.get(i) != Some(&d.value))
            || known
                .ratifications
                .values()
                .any(|r| r.value.outcome != Decision::Adopted)
            || !authoritative_actor(&r.proposal, term.provider(), actor, facts)
        {
            return Err(Error::Unavailable);
        }
        // Fresh authority can withdraw permission to undertake new spending;
        // existing obligations in any revision remain untouched.
        r.proposal.validate(facts, r.proposed_at)?;
        let request = match term {
            Term::Delivery { term, .. } => {
                if term.deadline().is_none_or(|at| at <= now) {
                    return Err(Error::Unavailable);
                }
                Request::Delivery {
                    key,
                    parties: BTreeSet::from(term.parties()),
                    term,
                }
            }
            Term::Passage {
                grantor,
                beneficiary,
                areas,
                until,
            } => {
                if until <= now {
                    return Err(Error::Unavailable);
                }
                Request::Passage {
                    key,
                    institution: grantor,
                    beneficiary,
                    areas,
                    until,
                }
            }
            Term::Protection { patrol, .. } => {
                if patrol.deadline <= now {
                    return Err(Error::Unavailable);
                }
                Request::Protection {
                    key,
                    sponsor: actor,
                    patrol,
                }
            }
        };
        r.requested.insert(key.index);
        plan.requests.push(request);
        Ok(plan)
    }
    /// Accepted existing owner records may back a ratified clause directly;
    /// this also permits linking an agreement negotiated before ratification.
    pub fn prepare_backing(
        &self,
        key: TermKey,
        receipt: WorkReceipt,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(key.commitment)?;
        let r = revision_mut(&mut plan, key)?;
        if !fully_ratified(r) || now < r.proposed_at {
            return Err(Error::Unavailable);
        }
        let term = r
            .proposal
            .terms
            .get(key.index as usize)
            .ok_or(Error::Unavailable)?;
        let (backing, progress) = match (term, receipt) {
            (
                Term::Delivery { term: expected, .. },
                WorkReceipt::DeliveryProposed { agreement, term },
            ) if *expected == term && agreement.0 > 0 => (
                Backing::Delivery {
                    agreement,
                    obligation: None,
                },
                Progress::AwaitingWork,
            ),
            (
                Term::Delivery { term: expected, .. },
                WorkReceipt::DeliveryOpened {
                    agreement,
                    obligation,
                    accepted,
                    term,
                },
            ) if *expected == term
                && agreement.0 > 0
                && obligation.is_some() == accepted
                && obligation.is_none_or(|o| o.0 > 0) =>
            {
                (
                    Backing::Delivery {
                        agreement,
                        obligation,
                    },
                    if accepted {
                        Progress::Active
                    } else {
                        Progress::Refused
                    },
                )
            }
            (
                Term::Passage {
                    grantor,
                    beneficiary: expected,
                    areas: expected_areas,
                    until: expected_until,
                },
                WorkReceipt::PassageApplied {
                    institution,
                    proposal,
                    adopted,
                    beneficiary,
                    areas,
                    until,
                },
            ) if *grantor == institution
                && *expected == beneficiary
                && *expected_areas == areas
                && *expected_until == until
                && proposal.0 > 0 =>
            {
                (
                    Backing::Passage {
                        institution,
                        proposal,
                    },
                    if adopted {
                        Progress::Active
                    } else {
                        Progress::Refused
                    },
                )
            }
            (
                Term::Protection {
                    patrol: expected, ..
                },
                WorkReceipt::ProtectionProposed { mission, patrol },
            ) if *expected == patrol && mission.0 > 0 => {
                (Backing::Protection { mission }, Progress::AwaitingWork)
            }
            (
                Term::Protection {
                    patrol: expected, ..
                },
                WorkReceipt::ProtectionOpened {
                    mission,
                    accepted,
                    patrol,
                },
            ) if *expected == patrol && mission.0 > 0 => (
                Backing::Protection { mission },
                if accepted {
                    Progress::Active
                } else {
                    Progress::Refused
                },
            ),
            _ => return Err(Error::Unavailable),
        };
        if let Some(old) = r.backed.get(&key.index) {
            if old.progress != Progress::AwaitingWork {
                if old.backing != backing
                    || (old.progress == Progress::Refused) != (progress == Progress::Refused)
                {
                    return Err(Error::Unavailable);
                }
                return Ok(plan);
            }
            if !compatible_backing(&old.backing, &backing) || now < old.changed_at {
                return Err(Error::Unavailable);
            }
            if progress == Progress::AwaitingWork {
                return Ok(plan);
            }
        }
        if let Backing::Delivery {obligation:Some(obligation),..}=backing
            && self.commitments.values().flat_map(|c|&c.revisions).flat_map(|r|r.backed.values())
                .any(|b|matches!(b.backing,Backing::Delivery {obligation:Some(existing),..} if existing==obligation)) {
            return Err(Error::Unavailable);
        }
        r.requested.insert(key.index);
        r.backed.insert(
            key.index,
            BackedTerm {
                backing,
                progress,
                performed: Quantity(0),
                changed_at: now,
                defaulted_at: None,
            },
        );
        Ok(plan)
    }
    pub fn prepare_outcome(
        &self,
        key: TermKey,
        receipt: OutcomeReceipt,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(key.commitment)?;
        let r = revision_mut(&mut plan, key)?;
        let term = r
            .proposal
            .terms
            .get(key.index as usize)
            .ok_or(Error::Unavailable)?;
        let b = r.backed.get_mut(&key.index).ok_or(Error::Unavailable)?;
        if now < b.changed_at {
            return Err(Error::Unavailable);
        }
        match (term, &b.backing, receipt) {
            (
                Term::Delivery {
                    term: AgreementTerm::Delivery { quantity, .. },
                    ..
                },
                Backing::Delivery {
                    obligation: Some(expected),
                    ..
                },
                OutcomeReceipt::Delivery {
                    obligation,
                    performed,
                    defaulted,
                },
            ) if *expected == obligation && performed >= b.performed && performed <= *quantity => {
                b.performed = performed;
                b.progress = if performed == *quantity {
                    Progress::Fulfilled
                } else if defaulted {
                    Progress::Defaulted
                } else {
                    Progress::Active
                };
                if defaulted {
                    b.defaulted_at.get_or_insert(now);
                }
            }
            (
                Term::Passage { until, .. },
                Backing::Passage {
                    proposal: expected, ..
                },
                OutcomeReceipt::Passage {
                    proposal,
                    available,
                },
            ) if *expected == proposal => {
                if b.progress == Progress::Refused {
                    if available {
                        return Err(Error::Unavailable);
                    }
                } else if b.progress == Progress::Fulfilled {
                    // A later withdrawal cannot breach an already ended term.
                } else if now >= *until {
                    b.progress = if b.defaulted_at.is_some() {
                        Progress::Defaulted
                    } else {
                        Progress::Fulfilled
                    };
                } else {
                    if !available {
                        b.defaulted_at.get_or_insert(now);
                    }
                    b.progress = if b.defaulted_at.is_some() {
                        Progress::Defaulted
                    } else {
                        Progress::Active
                    };
                }
            }
            (
                Term::Protection { patrol, .. },
                Backing::Protection { mission: expected },
                OutcomeReceipt::Protection { mission, progress },
            ) if *expected == mission => {
                if progress == Progress::AwaitingWork
                    || b.progress == Progress::Refused && progress != Progress::Refused
                    || b.progress == Progress::Fulfilled && progress != Progress::Fulfilled
                    || progress == Progress::Fulfilled
                        && matches!(patrol.objective,game_missions::MissionObjective::Patrol {until,..} if now<until)
                {
                    return Err(Error::Unavailable);
                }
                b.progress = progress;
                if progress == Progress::Defaulted {
                    b.defaulted_at.get_or_insert(now);
                }
            }
            _ => return Err(Error::Unavailable),
        }
        b.changed_at = now;
        Ok(plan)
    }
}

fn new_revision(number: u32, proposal: Proposal, now: SimTime) -> Revision {
    Revision {
        number,
        proposal,
        proposed_at: now,
        ratification_proposals: BTreeMap::new(),
        ratifications: BTreeMap::new(),
        requested: BTreeSet::new(),
        backed: BTreeMap::new(),
        expired_at: None,
    }
}
pub(crate) fn dated<T>(value: T, actor: PersonId, now: SimTime) -> Dated<T> {
    Dated {
        value,
        observed_at: now,
        learned_at: now,
        source: KnowledgeSource::Observed(actor),
    }
}
pub(crate) fn fully_ratified(r: &Revision) -> bool {
    r.ratifications.len() == r.proposal.participants.len()
        && r.ratifications
            .values()
            .all(|r| r.outcome == Decision::Adopted)
}
fn revision_mut(plan: &mut PreparedChange, key: TermKey) -> Result<&mut Revision, Error> {
    plan.after
        .as_mut()
        .unwrap()
        .revisions
        .get_mut(key.revision as usize)
        .filter(|r| r.number == key.revision)
        .ok_or(Error::Unavailable)
}

pub(crate) fn compatible_backing(known: &Backing, current: &Backing) -> bool {
    known == current
        || matches!((known,current),(Backing::Delivery {agreement:a,obligation:None},Backing::Delivery {agreement:b,..}) if a==b)
}
