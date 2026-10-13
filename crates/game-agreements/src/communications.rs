//! Reports carry the speaker's frozen dated knowledge through timed contact.
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportFacts {
    pub sender_under_care: bool,
    pub recipient_idle_or_rest: bool,
}
pub fn prepare_report(
    store: &Store,
    now: SimTime,
    sender: PersonId,
    recipient: PersonId,
    id: AgreementId,
    facts: ReportFacts,
    presence: &impl Presence,
) -> Result<PendingReport, Error> {
    let area = contact_area(presence, sender).ok_or(Error::Unavailable)?;
    if sender == recipient
        || contact_area(presence, recipient) != Some(area)
        || facts.sender_under_care
        || !facts.recipient_idle_or_rest
    {
        return Err(Error::Unavailable);
    }
    let snapshot = known_agreements(store, sender)
        .and_then(|known| known.get(&id))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let completes = SimTime(now.0.checked_add(60_000).ok_or(Error::Overflow)?);
    Ok(PendingReport {
        sender,
        recipient,
        started_at: now,
        completes,
        snapshot,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportCompletion {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub agreement: AgreementId,
    pub reported: bool,
    pub knowledge: Vec<NewDefaultKnowledge>,
}
pub fn complete_report(
    store: &mut Store,
    now: SimTime,
    sender: PersonId,
    recipient_idle_or_rest: bool,
    presence: &impl Presence,
) -> Result<ReportCompletion, Error> {
    prepare_report_completion(store, now, sender, recipient_idle_or_rest, presence)?.commit(store)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedReportCompletion {
    before: PendingReport,
    knowledge: Option<PreparedKnowledge>,
    pub completion: ReportCompletion,
}
pub fn prepare_report_completion(
    store: &Store,
    now: SimTime,
    sender: PersonId,
    recipient_idle_or_rest: bool,
    presence: &impl Presence,
) -> Result<PreparedReportCompletion, Error> {
    let before = report(store, sender).cloned().ok_or(Error::Unavailable)?;
    if now < before.completes {
        return Err(Error::Unavailable);
    }
    let reported = contact_area(presence, sender).is_some()
        && contact_area(presence, sender) == contact_area(presence, before.recipient)
        && recipient_idle_or_rest;
    let knowledge = reported.then(|| {
        prepare_received_report(
            store,
            now,
            sender,
            before.recipient,
            before.snapshot.clone(),
        )
    });
    let completion = ReportCompletion {
        sender,
        recipient: before.recipient,
        agreement: before.snapshot.id,
        reported,
        knowledge: knowledge
            .as_ref()
            .map_or_else(Vec::new, |p| p.effects.clone()),
    };
    Ok(PreparedReportCompletion {
        before,
        knowledge,
        completion,
    })
}
impl PreparedReportCompletion {
    pub fn stage_into(&self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        if report(store, self.before.sender) != Some(&self.before)
            || batch.touches::<PendingReport>(StableId(self.before.sender.0))
        {
            return Err(Error::Unavailable);
        }
        if let Some(knowledge) = &self.knowledge {
            knowledge.stage_into(store, batch)?;
        }
        batch.remove::<PendingReport>(StableId(self.before.sender.0));
        Ok(())
    }
    pub fn commit(self, store: &mut Store) -> Result<ReportCompletion, Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(self.completion)
    }
}
pub fn cancel_report(store: &mut Store, sender: PersonId) -> Result<(), Error> {
    remove_report(store, sender)?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedReportCancellations {
    before: BTreeMap<PersonId, Option<PendingReport>>,
}
pub fn prepare_cancel_reports(store: &Store, actors: &[PersonId]) -> PreparedReportCancellations {
    PreparedReportCancellations {
        before: actors
            .iter()
            .map(|id| (*id, report(store, *id).cloned()))
            .collect(),
    }
}
impl PreparedReportCancellations {
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReportBatchGuard, Error> {
        self.stage_into_composed(store, batch, None)
    }
    pub fn stage_into_composed(
        self,
        store: &Store,
        batch: &mut Batch,
        replacement: Option<&ReportReplacement>,
    ) -> Result<ReportBatchGuard, Error> {
        if self
            .before
            .iter()
            .any(|(id, row)| report(store, *id) != row.as_ref())
            || replacement.is_some_and(|r| {
                r.key != batch.key() || self.before.get(&r.sender) != Some(&r.before)
            })
        {
            return Err(Error::Unavailable);
        }
        let replaced = replacement.map(|r| r.sender);
        let mut edited = BTreeSet::new();
        if let Some(id) = replaced {
            edited.insert(id);
        }
        for (id, row) in self.before {
            if Some(id) == replaced {
                continue;
            }
            if batch.touches::<PendingReport>(StableId(id.0)) {
                return Err(Error::Unavailable);
            }
            if row.is_some() {
                batch.remove::<PendingReport>(StableId(id.0));
                edited.insert(id);
            }
        }
        let guard = ReportBatchGuard {
            key: batch.key(),
            edited,
        };
        guard.validate(batch)?;
        Ok(guard)
    }
}
/// Frozen admission guards remain separate from the plain canonical report row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedReportStart {
    before: Option<PendingReport>,
    after: PendingReport,
    facts: ReportFacts,
    sender: ContactFacts,
    recipient: ContactFacts,
}
pub fn prepare_report_start(
    store: &Store,
    now: SimTime,
    sender: PersonId,
    recipient: PersonId,
    id: AgreementId,
    facts: ReportFacts,
    presence: &impl Presence,
) -> Result<PreparedReportStart, Error> {
    let after = prepare_report(store, now, sender, recipient, id, facts, presence)?;
    Ok(PreparedReportStart {
        before: report(store, sender).cloned(),
        after,
        facts,
        sender: presence.contact(sender).ok_or(Error::Unavailable)?,
        recipient: presence.contact(recipient).ok_or(Error::Unavailable)?,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportReplacement {
    key: game_ecs::BatchKey,
    sender: PersonId,
    before: Option<PendingReport>,
    after: PendingReport,
}
impl ReportReplacement {
    pub fn read_guard(&self, store: &Store, batch: &Batch) -> Result<game_ecs::ReadGuard, Error> {
        let id = StableId(self.sender.0);
        if batch.key() != self.key
            || batch.projected::<PendingReport>(store, id)? != Some(&self.after)
        {
            return Err(Error::Unavailable);
        }
        let mut reads = game_ecs::ReadSet::new();
        reads.expect_row(id, self.before.clone());
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<PendingReport>(id);
        reads.bind(store, batch, &admitted)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportBatchGuard {
    key: game_ecs::BatchKey,
    edited: BTreeSet<PersonId>,
}
impl ReportBatchGuard {
    pub fn validate(&self, batch: &Batch) -> Result<(), Error> {
        let edits = batch
            .edited_ids::<PendingReport>()
            .map(|id| PersonId(id.0))
            .collect::<Vec<_>>();
        if batch.key() != self.key
            || edits.len() != self.edited.len()
            || edits.into_iter().collect::<BTreeSet<_>>() != self.edited
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}
impl PreparedReportStart {
    pub fn pending(&self) -> &PendingReport {
        &self.after
    }
    pub fn check(
        &self,
        store: &Store,
        now: SimTime,
        facts: ReportFacts,
        presence: &impl Presence,
    ) -> Result<(), Error> {
        if now != self.after.started_at
            || facts != self.facts
            || presence.contact(self.after.sender) != Some(self.sender)
            || presence.contact(self.after.recipient) != Some(self.recipient)
            || report(store, self.after.sender) != self.before.as_ref()
            || known_agreements(store, self.after.sender)
                .and_then(|known| known.get(&self.after.snapshot.id))
                != Some(&self.after.snapshot)
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into_with_replacement(
        self,
        store: &Store,
        now: SimTime,
        facts: ReportFacts,
        presence: &impl Presence,
        batch: &mut Batch,
    ) -> Result<ReportReplacement, Error> {
        self.check(store, now, facts, presence)?;
        if batch.touches::<PendingReport>(StableId(self.after.sender.0)) {
            return Err(Error::Unavailable);
        }
        let replacement = ReportReplacement {
            key: batch.key(),
            sender: self.after.sender,
            before: self.before,
            after: self.after.clone(),
        };
        batch.insert(StableId(self.after.sender.0), self.after);
        Ok(replacement)
    }
}
