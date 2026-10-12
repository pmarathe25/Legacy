//! Regional work refers to the exact promises and service records of their owners.
use crate::*;
use game_agreements::AgreementStatus;
use game_core::*;
use game_missions::{CompanionStatus, MissionStatus};
#[derive(Clone, Debug)]
pub struct PreparedWorkRequest {
    pub change: PreparedChange,
    pub actor: PersonId,
    pub key: TermKey,
    pub request: Request,
    pub existing: Option<PendingOwner>,
}
impl PreparedWorkRequest {
    /// The receiving owner has prepared this exact requested operation. No
    /// substitute term or personal consent can be introduced by its binding.
    pub fn bind(mut self, owner: PendingOwner) -> Result<PreparedChange, Error> {
        if self
            .existing
            .as_ref()
            .is_some_and(|existing| *existing != owner)
            || !matches!(
                (&self.request, &owner),
                (Request::Delivery { .. }, PendingOwner::Agreement(_))
                    | (Request::Protection { .. }, PendingOwner::Mission(_))
                    | (Request::Passage { .. }, PendingOwner::Passage(_))
            )
        {
            return Err(Error::Unavailable);
        }
        let r = self
            .change
            .after
            .as_mut()
            .unwrap()
            .revisions
            .get_mut(self.key.revision as usize)
            .ok_or(Error::Unavailable)?;
        r.pending.insert(
            self.key.index,
            PendingWork {
                actor: self.actor,
                owner,
            },
        );
        Ok(self.change)
    }
}
impl Ledger<'_> {
    pub fn prepare_request(
        &self,
        actor: PersonId,
        key: TermKey,
        facts: &ProposalFacts,
        now: SimTime,
    ) -> Result<PreparedWorkRequest, Error> {
        let change = self.prepare_work_request(key, actor, facts, now)?;
        let request = change.requests.first().cloned().ok_or(Error::Unavailable)?;
        let existing = match &request {
            Request::Delivery { parties, term, .. } => game_agreements::view(self.store, actor)
                .into_iter()
                .find(|a| {
                    a.parties == *parties
                        && a.terms.contains(term)
                        && !matches!(
                            a.status,
                            AgreementStatus::Rejected | AgreementStatus::Withdrawn
                        )
                })
                .map(|a| PendingOwner::Agreement(a.id)),
            _ => None,
        };
        Ok(PreparedWorkRequest {
            change,
            actor,
            key,
            request,
            existing,
        })
    }
    pub fn work_receipt(&self, key: TermKey) -> Result<Option<WorkReceipt>, Error> {
        let r = self
            .commitments
            .get(&key.commitment)
            .and_then(|c| c.revision(key.revision))
            .ok_or(Error::Unavailable)?;
        let Some(pending) = r.pending.get(&key.index) else {
            return Ok(None);
        };
        let term = r
            .proposal
            .terms
            .get(usize::from(key.index))
            .ok_or(Error::Unavailable)?;
        Ok(match (&pending.owner, term) {
            (PendingOwner::Agreement(id), Term::Delivery { term, .. }) => {
                let a = game_agreements::agreement(self.store, *id).ok_or(Error::Unavailable)?;
                if let Some(o) = a.obligations.iter().find(|o| {
                    game_agreements::obligation(self.store, **o).is_some_and(|o| o.term == *term)
                }) {
                    Some(WorkReceipt::DeliveryOpened {
                        agreement: *id,
                        obligation: Some(*o),
                        accepted: true,
                        term: term.clone(),
                    })
                } else if matches!(
                    a.status,
                    AgreementStatus::Rejected | AgreementStatus::Withdrawn
                ) {
                    Some(WorkReceipt::DeliveryOpened {
                        agreement: *id,
                        obligation: None,
                        accepted: false,
                        term: term.clone(),
                    })
                } else {
                    Some(WorkReceipt::DeliveryProposed {
                        agreement: *id,
                        term: term.clone(),
                    })
                }
            }
            (PendingOwner::Mission(id), Term::Protection { patrol, .. }) => {
                let m = game_missions::mission(self.store, *id).ok_or(Error::Unavailable)?;
                let status = m
                    .members
                    .get(&patrol.leader)
                    .ok_or(Error::Unavailable)?
                    .status;
                if m.proposal.objective != patrol.objective
                    || matches!(
                        status,
                        CompanionStatus::Refused | CompanionStatus::Withdrawn
                    )
                {
                    Some(WorkReceipt::ProtectionOpened {
                        mission: *id,
                        accepted: false,
                        patrol: patrol.clone(),
                    })
                } else if status == CompanionStatus::Accepted {
                    Some(WorkReceipt::ProtectionOpened {
                        mission: *id,
                        accepted: true,
                        patrol: patrol.clone(),
                    })
                } else {
                    Some(WorkReceipt::ProtectionProposed {
                        mission: *id,
                        patrol: patrol.clone(),
                    })
                }
            }
            (PendingOwner::Passage(_), Term::Passage { .. }) => None,
            _ => return Err(Error::Unavailable),
        })
    }
    pub fn outcome_receipt(&self, key: TermKey) -> Result<Option<OutcomeReceipt>, Error> {
        let r = self
            .commitments
            .get(&key.commitment)
            .and_then(|c| c.revision(key.revision))
            .ok_or(Error::Unavailable)?;
        self.outcome_for(key, r)
    }
    fn outcome_for(&self, key: TermKey, r: &Revision) -> Result<Option<OutcomeReceipt>, Error> {
        let Some(b) = r.backed.get(&key.index) else {
            return Ok(None);
        };
        Ok(match b.backing {
            Backing::Delivery {
                obligation: Some(id),
                ..
            } => {
                let o = game_agreements::obligation(self.store, id).ok_or(Error::Unavailable)?;
                let performed = Quantity(o.performed.min(o.amount()));
                (performed != b.performed || o.defaulted && b.defaulted_at.is_none()).then_some(
                    OutcomeReceipt::Delivery {
                        obligation: id,
                        performed,
                        defaulted: o.defaulted,
                    },
                )
            }
            Backing::Protection { mission }
                if !matches!(b.progress, Progress::Refused | Progress::AwaitingWork) =>
            {
                let m = game_missions::mission(self.store, mission).ok_or(Error::Unavailable)?;
                let retasked = matches!(r.proposal.terms.get(usize::from(key.index)),Some(Term::Protection{patrol,..}) if m.proposal.objective!=patrol.objective);
                let progress = if retasked {
                    Progress::Defaulted
                } else {
                    match m.status {
                        MissionStatus::Completed => Progress::Fulfilled,
                        MissionStatus::Failed | MissionStatus::Cancelled => Progress::Defaulted,
                        _ => Progress::Active,
                    }
                };
                (progress != b.progress).then_some(OutcomeReceipt::Protection { mission, progress })
            }
            _ => None,
        })
    }
    pub fn known_work(
        &self,
        actor: PersonId,
        key: TermKey,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<Option<Dated<BackedTerm>>, Error> {
        if actor_area(presence, actor).is_none() {
            return Ok(None);
        }
        let r = self
            .commitments
            .get(&key.commitment)
            .and_then(|c| c.revision(key.revision))
            .ok_or(Error::Unavailable)?;
        let known_default = self
            .view(actor, key.commitment)
            .and_then(|k| k.revisions.get(&key.revision))
            .and_then(|r| r.backed.get(&key.index))
            .and_then(|b| b.value.defaulted_at);
        self.known_work_for(actor, key, now, r, known_default)
    }
    fn known_work_for(
        &self,
        actor: PersonId,
        key: TermKey,
        now: SimTime,
        r: &Revision,
        known_default: Option<SimTime>,
    ) -> Result<Option<Dated<BackedTerm>>, Error> {
        let Some(b) = r.backed.get(&key.index) else {
            return Ok(None);
        };
        let term = r
            .proposal
            .terms
            .get(usize::from(key.index))
            .ok_or(Error::Unavailable)?;
        Ok(match (&b.backing, term) {
            (Backing::Delivery { agreement, .. }, Term::Delivery { term, .. }) => {
                let Some(a) = game_agreements::view(self.store, actor)
                    .into_iter()
                    .find(|a| a.id == *agreement)
                else {
                    return Ok(None);
                };
                let (backing, progress, performed, at, defaulted_at, source) =
                    if let Some(o) = a.obligations.iter().find(|o| o.term == *term) {
                        let performed = Quantity(o.performed.min(o.amount()));
                        let progress = if performed.0 == term.amount() {
                            Progress::Fulfilled
                        } else if o.defaulted {
                            Progress::Defaulted
                        } else {
                            Progress::Active
                        };
                        (
                            Backing::Delivery {
                                agreement: *agreement,
                                obligation: Some(o.id),
                            },
                            progress,
                            performed,
                            o.observed_at,
                            if o.defaulted {
                                known_default.or(Some(o.observed_at))
                            } else {
                                known_default
                            },
                            o.source.clone().unwrap_or(KnowledgeSource::Observed(actor)),
                        )
                    } else {
                        (
                            Backing::Delivery {
                                agreement: *agreement,
                                obligation: None,
                            },
                            if matches!(
                                a.status,
                                AgreementStatus::Rejected | AgreementStatus::Withdrawn
                            ) {
                                Progress::Refused
                            } else {
                                Progress::AwaitingWork
                            },
                            Quantity(0),
                            a.observed_at,
                            None,
                            a.source.unwrap_or(KnowledgeSource::Observed(actor)),
                        )
                    };
                Some(Dated {
                    value: BackedTerm {
                        backing,
                        progress,
                        performed,
                        changed_at: at,
                        defaulted_at,
                    },
                    observed_at: at,
                    learned_at: now,
                    source,
                })
            }
            (Backing::Protection { mission }, Term::Protection { patrol, .. }) => {
                let Some(m) = game_missions::known_mission(self.store, actor, *mission) else {
                    return Ok(None);
                };
                let progress = if m.objective != patrol.objective {
                    Progress::Defaulted
                } else {
                    match m.status {
                        MissionStatus::Completed => Progress::Fulfilled,
                        MissionStatus::Failed | MissionStatus::Cancelled => Progress::Defaulted,
                        _ => match m
                            .recruitments
                            .iter()
                            .find(|r| r.person == patrol.leader)
                            .map(|r| r.status)
                        {
                            Some(CompanionStatus::Accepted) => Progress::Active,
                            Some(CompanionStatus::Refused | CompanionStatus::Withdrawn) => {
                                Progress::Refused
                            }
                            _ => Progress::AwaitingWork,
                        },
                    }
                };
                Some(Dated {
                    value: BackedTerm {
                        backing: Backing::Protection { mission: *mission },
                        progress,
                        performed: Quantity(0),
                        changed_at: m.observed_at,
                        defaulted_at: if progress == Progress::Defaulted {
                            known_default.or(Some(m.observed_at))
                        } else {
                            known_default
                        },
                    },
                    observed_at: m.observed_at,
                    learned_at: now,
                    source: m
                        .reported_by
                        .map_or(KnowledgeSource::Observed(actor), KnowledgeSource::Reported),
                })
            }
            _ => None,
        })
    }
    pub fn pending_keys(&self) -> Vec<(TermKey, PersonId)> {
        self.commitments
            .values()
            .flat_map(|c| {
                c.revisions.iter().flat_map(move |r| {
                    r.pending.iter().map(move |(index, p)| {
                        (
                            TermKey {
                                commitment: c.id,
                                revision: r.number,
                                index: *index,
                            },
                            p.actor,
                        )
                    })
                })
            })
            .collect()
    }
}

impl Ledger<'_> {
    /// Reconciliation prepares one selected commitment and its affected personal
    /// memories. Current foreign work changes never refresh remote ignorance.
    pub fn prepare_reconciliation(
        &self,
        id: RealmCommitmentId,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<PreparedChange, Error> {
        let mut plan = self.edit(id)?;
        let keys = plan
            .before
            .as_ref()
            .unwrap()
            .revisions
            .iter()
            .flat_map(|r| {
                r.pending.iter().map(move |(index, p)| {
                    (
                        TermKey {
                            commitment: id,
                            revision: r.number,
                            index: *index,
                        },
                        p.actor,
                    )
                })
            })
            .collect::<Vec<_>>();
        for (key, actor) in keys {
            plan.reads.extend(self.work_reads(key, actor)?);
            let awaiting = plan
                .after
                .as_ref()
                .unwrap()
                .revision(key.revision)
                .unwrap()
                .backed
                .get(&key.index)
                .is_none_or(|b| b.progress == Progress::AwaitingWork);
            if awaiting && let Some(receipt) = self.work_receipt(key)? {
                plan = self.backing_into(plan, key, receipt, now)?;
            }
            let r = plan.after.as_ref().unwrap().revision(key.revision).unwrap();
            if let Some(receipt) = self.outcome_for(key, r)? {
                plan = self.outcome_into(plan, key, receipt, now)?;
            }
            if actor_area(presence, actor).is_none() {
                continue;
            }
            let r = plan.after.as_ref().unwrap().revision(key.revision).unwrap();
            let known_default = plan
                .knowledge
                .iter()
                .find(|p| p.actor == actor)
                .map(|p| &p.after)
                .or_else(|| self.view(actor, key.commitment))
                .and_then(|k| k.revisions.get(&key.revision))
                .and_then(|r| r.backed.get(&key.index))
                .and_then(|b| b.value.defaulted_at);
            let Some(statement) = self.known_work_for(actor, key, now, r, known_default)? else {
                continue;
            };
            let Some(actual) = r.backed.get(&key.index) else {
                continue;
            };
            let Some(term) = r.proposal.terms.get(usize::from(key.index)) else {
                continue;
            };
            if statement.observed_at > statement.learned_at
                || statement.learned_at > now
                || statement.observed_at != statement.value.changed_at
                || !crate::state::compatible_backing(&statement.value.backing, &actual.backing)
                || statement.value.performed > actual.performed
                || !crate::persistence::backing_shape(term, &statement.value)
                || matches!(statement.source,KnowledgeSource::Observed(who) if who!=actor)
            {
                continue;
            }
            if self
                .view(actor, key.commitment)
                .and_then(|k| k.revisions.get(&key.revision))
                .is_none()
            {
                continue;
            }
            let known = plan
                .personal_patch(self.store, actor)
                .revisions
                .get_mut(&key.revision)
                .ok_or(Error::Unavailable)?;
            if known
                .backed
                .get(&key.index)
                .is_none_or(|old| old.observed_at < statement.observed_at)
            {
                known.backed.insert(key.index, statement);
            }
        }
        Ok(plan)
    }
    pub fn reconciliation_ids(&self) -> Vec<RealmCommitmentId> {
        self.commitments
            .values()
            .filter(|c| c.revisions.iter().any(|r| !r.pending.is_empty()))
            .map(|c| c.id)
            .collect()
    }
}
