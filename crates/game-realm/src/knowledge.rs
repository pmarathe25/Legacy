use crate::state::KnowledgePatch;
use crate::*;
use game_core::*;
use std::collections::BTreeMap;

impl Ledger<'_> {
    /// Project personally known owner records, preserving their dates and
    /// sources even when canonical work has advanced beyond the received view.
    pub fn prepare_learn_work(
        &self,
        actor: PersonId,
        key: TermKey,
        statement: Dated<BackedTerm>,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let r = self
            .commitments
            .get(&key.commitment)
            .and_then(|c| c.revision(key.revision))
            .ok_or(Error::Unavailable)?;
        let term = r
            .proposal
            .terms
            .get(usize::from(key.index))
            .ok_or(Error::Unavailable)?;
        let actual = r.backed.get(&key.index).ok_or(Error::Unavailable)?;
        if actor.0 == 0
            || !valid(&statement, now)
            || statement.observed_at != statement.value.changed_at
            || !crate::state::compatible_backing(&statement.value.backing, &actual.backing)
            || statement.value.performed > actual.performed
            || !crate::persistence::backing_shape(term, &statement.value)
            || matches!(statement.source,KnowledgeSource::Observed(who) if who!=actor)
        {
            return Err(Error::Unavailable);
        }
        let mut plan = self.personal_change(actor, key.commitment);
        let known = plan.knowledge[0]
            .after
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
        Ok(plan)
    }
    fn personal_change(&self, actor: PersonId, id: RealmCommitmentId) -> PreparedChange {
        PreparedChange {
            id,
            before: self.commitments.get(&id).cloned(),
            after: None,
            knowledge: vec![KnowledgePatch {
                actor,
                before: self.view(actor, id).cloned(),
                after: self.view(actor, id).cloned().unwrap_or_default(),
            }],
            requests: Vec::new(),
            reads: Vec::new(),
            projection: None,
        }
    }
    /// The adapter establishes actual local hearing/inspection. This statement
    /// reveals proposal terms only, never all global decisions or work outcomes.
    pub fn prepare_observe_proposal(
        &self,
        witness: LocalWitness,
        id: RealmCommitmentId,
        revision: u32,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let actor = witness.actor(now)?;
        let r = self
            .commitments
            .get(&id)
            .and_then(|c| c.revision(revision))
            .ok_or(Error::Unavailable)?;
        if witness.observed_at < r.proposed_at
            || !r
                .proposal
                .participants
                .values()
                .any(|p| p.settlement == witness.statement_area)
        {
            return Err(Error::Unavailable);
        }
        let mut plan = self.personal_change(actor, id);
        let known = &mut plan.knowledge[0].after;
        let statement = Dated {
            value: r.proposal.clone(),
            observed_at: witness.observed_at,
            learned_at: now,
            source: KnowledgeSource::Observed(actor),
        };
        if let Some(old) = known.revisions.get_mut(&revision) {
            if statement.observed_at >= old.proposal.observed_at {
                old.proposal = statement;
            }
        } else {
            known.revisions.insert(
                revision,
                KnownRevision {
                    proposal: statement,
                    ratifications: BTreeMap::new(),
                    backed: BTreeMap::new(),
                },
            );
        }
        Ok(plan)
    }
    pub fn prepare_observe_decision(
        &self,
        witness: LocalWitness,
        id: RealmCommitmentId,
        revision: u32,
        institution: InstitutionId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let actor = witness.actor(now)?;
        let r = self
            .commitments
            .get(&id)
            .and_then(|c| c.revision(revision))
            .ok_or(Error::Unavailable)?;
        let decision = r
            .ratifications
            .get(&institution)
            .ok_or(Error::Unavailable)?;
        if witness.observed_at != decision.decided_at
            || r.proposal.participants[&institution].settlement != witness.statement_area
        {
            return Err(Error::Unavailable);
        }
        let mut plan = self.personal_change(actor, id);
        let known = plan.knowledge[0]
            .after
            .revisions
            .get_mut(&revision)
            .ok_or(Error::Unavailable)?;
        known.ratifications.insert(
            institution,
            Dated {
                value: decision.clone(),
                observed_at: decision.decided_at,
                learned_at: now,
                source: KnowledgeSource::Observed(actor),
            },
        );
        Ok(plan)
    }
    pub fn prepare_observe_work(
        &self,
        witness: LocalWitness,
        key: TermKey,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let actor = witness.actor(now)?;
        let b = self
            .commitments
            .get(&key.commitment)
            .and_then(|c| c.revision(key.revision))
            .and_then(|r| r.backed.get(&key.index))
            .ok_or(Error::Unavailable)?;
        if witness.observed_at != b.changed_at {
            return Err(Error::Unavailable);
        }
        let mut plan = self.personal_change(actor, key.commitment);
        let known = plan.knowledge[0]
            .after
            .revisions
            .get_mut(&key.revision)
            .ok_or(Error::Unavailable)?;
        known.backed.insert(
            key.index,
            Dated {
                value: b.clone(),
                observed_at: b.changed_at,
                learned_at: now,
                source: KnowledgeSource::Observed(actor),
            },
        );
        Ok(plan)
    }
    pub fn prepare_report(
        &self,
        speaker: PersonId,
        id: RealmCommitmentId,
        now: SimTime,
    ) -> Result<Report, Error> {
        let snapshot = self.view(speaker, id).cloned().ok_or(Error::Unavailable)?;
        if !knowledge_dates_valid(&snapshot, now) {
            return Err(Error::Unavailable);
        }
        Ok(Report {
            commitment: id,
            speaker,
            sent_at: now,
            snapshot,
        })
    }
    /// Actual travel/conversation delivers this frozen packet. No global status
    /// is read or substituted for a sender's missing, older information.
    pub fn prepare_receive_report(
        &self,
        recipient: PersonId,
        report: &Report,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        if recipient.0 == 0
            || report.speaker.0 == 0
            || report.speaker == recipient
            || report.sent_at > now
            || !knowledge_dates_valid(&report.snapshot, report.sent_at)
        {
            return Err(Error::Unavailable);
        }
        let actual = self
            .commitments
            .get(&report.commitment)
            .ok_or(Error::Unavailable)?;
        let mut plan = self.personal_change(recipient, report.commitment);
        for (number, incoming) in &report.snapshot.revisions {
            let r = actual.revision(*number).ok_or(Error::Unavailable)?;
            if incoming.proposal.value != r.proposal
                || incoming.proposal.observed_at < r.proposed_at
                || incoming.ratifications.iter().any(|(i, d)| {
                    r.ratifications.get(i) != Some(&d.value) || d.observed_at != d.value.decided_at
                })
                || incoming.backed.iter().any(|(index, b)| {
                    b.observed_at != b.value.changed_at
                        || r.proposal
                            .terms
                            .get(usize::from(*index))
                            .is_none_or(|t| !crate::persistence::backing_shape(t, &b.value))
                        || r.backed.get(index).is_none_or(|real| {
                            !crate::state::compatible_backing(&b.value.backing, &real.backing)
                                || b.value.performed > real.performed
                        })
                })
            {
                return Err(Error::Unavailable);
            }
            let target = plan.knowledge[0]
                .after
                .revisions
                .entry(*number)
                .or_insert_with(|| KnownRevision {
                    proposal: reported(&incoming.proposal, report.speaker, now),
                    ratifications: BTreeMap::new(),
                    backed: BTreeMap::new(),
                });
            merge(
                &mut target.proposal,
                &incoming.proposal,
                report.speaker,
                now,
            );
            for (id, d) in &incoming.ratifications {
                if let Some(old) = target.ratifications.get_mut(id) {
                    merge(old, d, report.speaker, now);
                } else {
                    target
                        .ratifications
                        .insert(*id, reported(d, report.speaker, now));
                }
            }
            for (index, b) in &incoming.backed {
                if let Some(old) = target.backed.get_mut(index) {
                    merge(old, b, report.speaker, now);
                } else {
                    target
                        .backed
                        .insert(*index, reported(b, report.speaker, now));
                }
            }
        }
        Ok(plan)
    }
}
fn reported<T: Clone>(incoming: &Dated<T>, speaker: PersonId, now: SimTime) -> Dated<T> {
    Dated {
        value: incoming.value.clone(),
        observed_at: incoming.observed_at,
        learned_at: now,
        source: KnowledgeSource::Reported(speaker),
    }
}
fn merge<T: Clone>(old: &mut Dated<T>, incoming: &Dated<T>, speaker: PersonId, now: SimTime) {
    if incoming.observed_at > old.observed_at {
        *old = reported(incoming, speaker, now);
    }
}
fn knowledge_dates_valid(k: &Knowledge, now: SimTime) -> bool {
    k.revisions.values().all(|r| {
        valid(&r.proposal, now)
            && r.ratifications.values().all(|d| valid(d, now))
            && r.backed.values().all(|b| valid(b, now))
    })
}
fn valid<T>(d: &Dated<T>, now: SimTime) -> bool {
    d.observed_at <= d.learned_at && d.learned_at <= now
}
