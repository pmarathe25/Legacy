//! Dated local notices, physical observations, and received reports.
use crate::*;
pub fn refresh_known_status(view: &mut AgreementView) {
    if matches!(
        view.status,
        AgreementStatus::Active | AgreementStatus::Fulfilled | AgreementStatus::Breached
    ) {
        view.status = if !view.obligations.is_empty()
            && view
                .obligations
                .iter()
                .all(|o| o.performed == o.amount() && stream_ended(&o.term, view.observed_at))
        {
            AgreementStatus::Fulfilled
        } else if view.obligations.iter().any(|o| o.defaulted) {
            AgreementStatus::Breached
        } else {
            AgreementStatus::Active
        };
    }
}

pub fn merge_agreement_knowledge(old: AgreementView, incoming: &mut AgreementView) {
    if old.revision > incoming.revision {
        *incoming = old;
        return;
    }
    if old.revision == incoming.revision {
        incoming.accepted.extend(old.accepted.iter().copied());
        if matches!(
            old.status,
            AgreementStatus::Withdrawn | AgreementStatus::Rejected
        ) || incoming.status == AgreementStatus::Proposed
            && old.status != AgreementStatus::Proposed
        {
            incoming.status = old.status;
        }
        for prior in old.obligations {
            if let Some(new) = incoming.obligations.iter_mut().find(|o| o.id == prior.id) {
                if prior.observed_at > new.observed_at {
                    *new = prior;
                } else if prior.term == new.term {
                    new.performed = new.performed.max(prior.performed);
                    new.defaulted |= prior.defaulted;
                }
            } else {
                incoming.obligations.push(prior);
            }
        }
        for prior in old.amendments {
            if let Some(new) = incoming.amendments.iter_mut().find(|a| a.id == prior.id) {
                for (actor, support) in prior.votes {
                    new.votes.entry(actor).or_insert(support);
                }
                if prior.status != AmendmentStatus::Proposed {
                    new.status = prior.status;
                }
            } else {
                incoming.amendments.push(prior);
            }
        }
    }
    incoming.obligations.sort_by_key(|o| o.id);
    incoming.amendments.sort_by_key(|a| a.id);
}

pub fn actual_snapshot(
    store: &Store,
    now: SimTime,
    actor: PersonId,
    agreement: AgreementId,
) -> Option<AgreementView> {
    let world = Ledger::new(store);
    let a = world.agreements().get(&agreement)?;
    Some(AgreementView {
        id: a.id,
        proposer: a.proposer,
        parties: a.parties.clone(),
        accepted: a.accepted.clone(),
        status: a.status,
        terms: a.terms.clone(),
        obligations: a
            .obligations
            .iter()
            .map(|id| {
                let o = &world.obligations()[id];
                ObligationView {
                    accrued: o.accrued,
                    share_remainder: o.share_remainder,
                    share_scale: o.share_scale,
                    id: *id,
                    term: o.term.clone(),
                    performed: o.performed,
                    defaulted: o.defaulted,
                    cargo: if o.term.parties()[0] == actor {
                        o.cargo.clone()
                    } else {
                        BTreeMap::new()
                    },
                    observed_at: now,
                    learned_at: now,
                    source: Some(KnowledgeSource::Observed(actor)),
                }
            })
            .collect(),
        procedure: a.procedure.clone(),
        amendments: world
            .amendments()
            .values()
            .filter(|m| m.agreement == a.id)
            .map(|m| AmendmentView {
                id: m.id,
                proposer: m.proposer,
                replacements: m.replacements.clone(),
                votes: m.votes.clone(),
                status: m.status,
                base_revision: m.base_revision,
            })
            .collect(),
        revision: a.revision,
        observed_at: now,
        learned_at: now,
        source: Some(KnowledgeSource::Observed(actor)),
    })
}
pub fn notice_snapshot(
    store: &Store,
    now: SimTime,
    actor: PersonId,
    agreement: AgreementId,
) -> Option<AgreementView> {
    let view = actual_snapshot(store, now, actor, agreement)?;
    let old = known_agreements(store, actor)
        .and_then(|known| known.get(&agreement))
        .cloned();
    Some(notice_view(now, actor, view, old))
}
fn notice_view(
    now: SimTime,
    actor: PersonId,
    mut view: AgreementView,
    old: Option<AgreementView>,
) -> AgreementView {
    view.amendments = old
        .as_ref()
        .map_or_else(Vec::new, |old| old.amendments.clone());
    for amendment in &mut view.amendments {
        if amendment.status == AmendmentStatus::Proposed && amendment.base_revision < view.revision
        {
            amendment.status = AmendmentStatus::Rejected;
        }
    }
    for obligation in &mut view.obligations {
        let prior = old
            .as_ref()
            .and_then(|old| old.obligations.iter().find(|o| o.id == obligation.id));
        if let Some(prior) = prior {
            obligation.accrued = prior.accrued;
            obligation.share_remainder = prior.share_remainder;
            obligation.share_scale = prior.share_scale;
            obligation.performed = if same_duty(&prior.term, &obligation.term) {
                prior.performed.min(obligation.amount())
            } else if matches!(
                obligation.term,
                AgreementTerm::Access { .. } | AgreementTerm::SiteAccess { .. }
            ) {
                1
            } else {
                0
            };
            obligation.defaulted = if prior.term == obligation.term {
                prior.defaulted
            } else {
                obligation.performed < obligation.amount()
                    && obligation.term.deadline().is_some_and(|d| d <= now)
                    && obligation.term.parties()[0] == actor
            };
            obligation.cargo = if same_duty(&prior.term, &obligation.term) {
                prior.cargo.clone()
            } else {
                BTreeMap::new()
            };
            let mut remaining = obligation.amount() - obligation.performed;
            for quantity in obligation.cargo.values_mut() {
                *quantity = (*quantity).min(remaining);
                remaining -= *quantity;
            }
            obligation.cargo.retain(|_, quantity| *quantity > 0);
            obligation.observed_at = prior.observed_at;
            obligation.learned_at = prior.learned_at;
            obligation.source = prior.source.clone();
            if obligation.defaulted && prior.term != obligation.term {
                obligation.observed_at = now;
                obligation.learned_at = now;
                obligation.source = Some(KnowledgeSource::Observed(actor));
            }
        } else {
            obligation.accrued = 0;
            obligation.share_remainder = 0;
            obligation.share_scale = 0;
            obligation.performed = if matches!(
                obligation.term,
                AgreementTerm::Access { .. } | AgreementTerm::SiteAccess { .. }
            ) {
                obligation.performed
            } else {
                0
            };
            obligation.defaulted = false;
            obligation.cargo.clear();
        }
    }
    if matches!(
        view.status,
        AgreementStatus::Active | AgreementStatus::Fulfilled | AgreementStatus::Breached
    ) {
        view.status = AgreementStatus::Active;
    }
    refresh_known_status(&mut view);
    view
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewDefaultKnowledge {
    pub observer: PersonId,
    pub debtor: PersonId,
}
/// A recipient learns this fact once; character traits own the trust reaction.
pub fn store_knowledge(
    store: &mut Store,
    actor: PersonId,
    view: AgreementView,
) -> Result<Vec<NewDefaultKnowledge>, Error> {
    let mut writer = KnowledgeWriter::new(store);
    writer.store(actor, view);
    writer.finish().commit(store)
}
pub fn notice(
    store: &mut Store,
    now: SimTime,
    actor: PersonId,
    id: AgreementId,
) -> Result<Vec<NewDefaultKnowledge>, Error> {
    let mut writer = KnowledgeWriter::new(store);
    writer.notice(now, actor, id);
    writer.finish().commit(store)
}
pub fn receive_report(
    store: &mut Store,
    now: SimTime,
    sender: PersonId,
    recipient: PersonId,
    incoming: AgreementView,
) -> Result<Vec<NewDefaultKnowledge>, Error> {
    prepare_received_report(store, now, sender, recipient, incoming).commit(store)
}
pub fn view(store: &Store, actor: PersonId) -> Vec<AgreementView> {
    known_agreements(store, actor)
        .into_iter()
        .flat_map(|known| known.values())
        .take(64)
        .cloned()
        .collect()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactFacts {
    pub alive: bool,
    pub health: u64,
    pub independent: bool,
    pub in_combat: bool,
    pub area: Option<AreaId>,
}
impl ContactFacts {
    pub fn area(self) -> Option<AreaId> {
        (self.alive && self.health > 0 && self.independent && !self.in_combat)
            .then_some(self.area)
            .flatten()
    }
    pub fn conscious(self) -> bool {
        self.alive && self.health > 0
    }
}
/// Each lookup supplies a person's physical and control facts. The agreement
/// owner decides which of those facts permit contact or personal observation.
pub trait Presence {
    fn contact(&self, actor: PersonId) -> Option<ContactFacts>;
}
pub fn contact_area(presence: &impl Presence, actor: PersonId) -> Option<AreaId> {
    presence.contact(actor)?.area()
}
pub fn local_notice(
    store: &mut Store,
    now: SimTime,
    id: AgreementId,
    speaker: PersonId,
    change: Option<AmendmentId>,
    presence: &impl Presence,
) -> Result<Vec<NewDefaultKnowledge>, Error> {
    let mut writer = KnowledgeWriter::new(store);
    writer.local(now, id, speaker, change, presence)?;
    writer.finish().commit(store)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationKind {
    Proposed {
        agreement: AgreementId,
        proposer: PersonId,
    },
    Activated {
        agreement: AgreementId,
    },
    Rejected {
        agreement: AgreementId,
        actor: PersonId,
    },
    Obligation {
        obligation: ObligationId,
        performer: PersonId,
    },
    Defaulted {
        obligation: ObligationId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationEvent {
    pub at: SimTime,
    pub kind: ObservationKind,
}
pub fn observe_obligation(
    store: &mut Store,
    now: SimTime,
    id: ObligationId,
    performer: PersonId,
    at: SimTime,
    presence: &impl Presence,
) -> Result<Vec<NewDefaultKnowledge>, Error> {
    let mut writer = KnowledgeWriter::new(store);
    writer.observe(now, id, performer, at, presence);
    writer.finish().commit(store)
}
pub fn observe_events(
    store: &mut Store,
    runtime: &mut Runtime,
    now: SimTime,
    cursor: u64,
    events: &[ObservationEvent],
    presence: &impl Presence,
) -> Result<Vec<NewDefaultKnowledge>, Error> {
    prepare_observations(store, runtime, now, cursor, events, presence)?.commit(store, runtime)
}
/// Transient contact facts for precisely the owner-requested IDs.
#[derive(Clone, Debug, Default)]
pub struct ContactSet(pub BTreeMap<PersonId, ContactFacts>);
impl Presence for ContactSet {
    fn contact(&self, id: PersonId) -> Option<ContactFacts> {
        self.0.get(&id).copied()
    }
}
pub fn observation_people(store: &Store, events: &[ObservationEvent]) -> BTreeSet<PersonId> {
    let mut people = knowledges(store).map(|(id, _)| id).collect::<BTreeSet<_>>();
    for event in events {
        match event.kind {
            ObservationKind::Proposed {
                agreement,
                proposer,
            } => {
                people.insert(proposer);
                if let Some(row) = crate::agreement(store, agreement) {
                    people.extend(row.parties.iter().copied());
                }
            }
            ObservationKind::Activated { agreement }
            | ObservationKind::Rejected { agreement, .. } => {
                if let Some(row) = crate::agreement(store, agreement) {
                    people.insert(row.proposer);
                    people.extend(row.parties.iter().copied());
                }
            }
            ObservationKind::Obligation { performer, .. } => {
                people.insert(performer);
            }
            ObservationKind::Defaulted { obligation } => {
                if let Some(row) = crate::obligation(store, obligation) {
                    people.insert(row.term.parties()[0]);
                }
            }
        }
    }
    people
}

/// Sparse personal-row effects and their exact canonical opening dependencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedKnowledge {
    before: BTreeMap<PersonId, Option<Knowledge>>,
    after: BTreeMap<PersonId, Knowledge>,
    agreements: BTreeMap<AgreementId, Agreement>,
    obligations: BTreeMap<ObligationId, Obligation>,
    amendments: BTreeMap<AmendmentId, Amendment>,
    membership: Option<BTreeSet<PersonId>>,
    pub effects: Vec<NewDefaultKnowledge>,
}
impl PreparedKnowledge {
    pub fn check(&self, store: &Store) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(id, row)| knowledge(store, *id) != row.as_ref())
            || self
                .agreements
                .iter()
                .any(|(id, row)| agreement(store, *id) != Some(row))
            || self
                .obligations
                .iter()
                .any(|(id, row)| obligation(store, *id) != Some(row))
            || self
                .amendments
                .iter()
                .any(|(id, row)| amendment(store, *id) != Some(row))
            || self.membership.as_ref().is_some_and(|ids| {
                knowledges(store).map(|(id, _)| id).collect::<BTreeSet<_>>() != *ids
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(&self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        self.check(store)?;
        if self
            .after
            .keys()
            .any(|id| batch.touches::<Knowledge>(StableId(id.0)))
        {
            return Err(Error::Unavailable);
        }
        for (id, row) in &self.after {
            batch.insert(StableId(id.0), row.clone());
        }
        Ok(())
    }
    pub fn commit(self, store: &mut Store) -> Result<Vec<NewDefaultKnowledge>, Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(self.effects)
    }
}
struct KnowledgeWriter<'a> {
    store: &'a Store,
    plan: PreparedKnowledge,
}
impl<'a> KnowledgeWriter<'a> {
    fn new(store: &'a Store) -> Self {
        Self {
            store,
            plan: PreparedKnowledge {
                before: BTreeMap::new(),
                after: BTreeMap::new(),
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                membership: None,
                effects: Vec::new(),
            },
        }
    }
    fn read(&mut self, actor: PersonId) -> Option<&Knowledge> {
        self.plan
            .before
            .entry(actor)
            .or_insert_with(|| knowledge(self.store, actor).cloned());
        self.plan
            .after
            .get(&actor)
            .or_else(|| knowledge(self.store, actor))
    }
    fn personal(&mut self, actor: PersonId) -> &mut Knowledge {
        self.plan
            .before
            .entry(actor)
            .or_insert_with(|| knowledge(self.store, actor).cloned());
        self.plan
            .after
            .entry(actor)
            .or_insert_with(|| knowledge(self.store, actor).cloned().unwrap_or_default())
    }
    fn store(&mut self, actor: PersonId, view: AgreementView) {
        let old = self
            .read(actor)
            .and_then(|row| row.agreements.get(&view.id));
        let failures = view
            .obligations
            .iter()
            .filter(|o| {
                o.defaulted
                    && o.term.parties()[1] == actor
                    && !old.is_some_and(|known| {
                        known
                            .obligations
                            .iter()
                            .any(|prior| prior.id == o.id && prior.defaulted)
                    })
            })
            .map(|o| NewDefaultKnowledge {
                observer: actor,
                debtor: o.term.parties()[0],
            })
            .collect::<Vec<_>>();
        self.plan.effects.extend(failures);
        self.personal(actor).agreements.insert(view.id, view);
    }
    fn snapshot(
        &mut self,
        now: SimTime,
        actor: PersonId,
        id: AgreementId,
    ) -> Option<AgreementView> {
        let actual = agreement(self.store, id)?;
        self.plan
            .agreements
            .entry(id)
            .or_insert_with(|| actual.clone());
        for id in &actual.obligations {
            if let Some(row) = obligation(self.store, *id) {
                self.plan
                    .obligations
                    .entry(*id)
                    .or_insert_with(|| row.clone());
            }
        }
        actual_snapshot(self.store, now, actor, id)
    }
    fn notice(&mut self, now: SimTime, actor: PersonId, id: AgreementId) {
        let Some(view) = self.snapshot(now, actor, id) else {
            return;
        };
        let old = self
            .read(actor)
            .and_then(|row| row.agreements.get(&id))
            .cloned();
        self.store(actor, notice_view(now, actor, view, old));
    }
    fn receive(
        &mut self,
        now: SimTime,
        sender: PersonId,
        recipient: PersonId,
        mut incoming: AgreementView,
    ) {
        incoming.learned_at = now;
        incoming.source = Some(KnowledgeSource::Reported(sender));
        for o in &mut incoming.obligations {
            o.learned_at = now;
            o.source = Some(KnowledgeSource::Reported(sender));
        }
        if let Some(old) = self
            .read(recipient)
            .and_then(|row| row.agreements.get(&incoming.id))
            .cloned()
        {
            merge_agreement_knowledge(old, &mut incoming);
        }
        refresh_known_status(&mut incoming);
        self.store(recipient, incoming);
    }
    fn local(
        &mut self,
        now: SimTime,
        id: AgreementId,
        speaker: PersonId,
        change: Option<AmendmentId>,
        presence: &impl Presence,
    ) -> Result<(), Error> {
        let Some(area) = contact_area(presence, speaker) else {
            return Ok(());
        };
        let actual = agreement(self.store, id).ok_or(Error::Unavailable)?;
        self.plan
            .agreements
            .entry(id)
            .or_insert_with(|| actual.clone());
        for actor in actual
            .parties
            .iter()
            .copied()
            .filter(|actor| contact_area(presence, *actor) == Some(area))
        {
            self.notice(now, actor, id);
            if let Some(change) = change
                && let Some(row) = amendment(self.store, change)
            {
                self.plan
                    .amendments
                    .entry(change)
                    .or_insert_with(|| row.clone());
                let learned = AmendmentView {
                    id: row.id,
                    proposer: row.proposer,
                    replacements: row.replacements.clone(),
                    votes: row.votes.clone(),
                    status: row.status,
                    base_revision: row.base_revision,
                };
                let known = self.personal(actor).agreements.get_mut(&id).unwrap();
                if let Some(old) = known.amendments.iter_mut().find(|old| old.id == change) {
                    *old = learned;
                } else {
                    known.amendments.push(learned);
                    known.amendments.sort_by_key(|row| row.id);
                }
            }
        }
        Ok(())
    }
    fn actors(&self) -> BTreeSet<PersonId> {
        knowledges(self.store)
            .map(|(id, _)| id)
            .chain(self.plan.after.keys().copied())
            .collect()
    }
    fn observe(
        &mut self,
        now: SimTime,
        id: ObligationId,
        performer: PersonId,
        at: SimTime,
        presence: &impl Presence,
    ) {
        let Some(actual) = obligation(self.store, id).cloned() else {
            return;
        };
        self.plan
            .obligations
            .entry(id)
            .or_insert_with(|| actual.clone());
        let area = contact_area(presence, performer);
        for actor in self.actors() {
            let known = self
                .read(actor)
                .and_then(|row| row.agreements.get(&actual.agreement))
                .cloned();
            let Some(mut view) = known.filter(|view| view.obligations.iter().any(|o| o.id == id))
            else {
                continue;
            };
            if !presence.contact(actor).is_some_and(ContactFacts::conscious)
                || !(actor == performer
                    || at == now && area.is_some() && contact_area(presence, actor) == area)
            {
                continue;
            }
            let fact = view
                .obligations
                .iter_mut()
                .find(|fact| fact.id == id)
                .unwrap();
            if fact.term != actual.term {
                continue;
            }
            fact.accrued = actual.accrued;
            fact.share_remainder = actual.share_remainder;
            fact.share_scale = actual.share_scale;
            fact.performed = actual.performed;
            fact.defaulted = actual.defaulted;
            if actual.term.parties()[0] == actor {
                fact.cargo = actual.cargo.clone();
            }
            fact.observed_at = at;
            fact.learned_at = now;
            fact.source = Some(KnowledgeSource::Observed(actor));
            view.observed_at = at;
            view.learned_at = now;
            refresh_known_status(&mut view);
            self.store(actor, view);
        }
    }
    fn finish(self) -> PreparedKnowledge {
        self.plan
    }
}
pub fn prepare_received_report(
    store: &Store,
    now: SimTime,
    sender: PersonId,
    recipient: PersonId,
    incoming: AgreementView,
) -> PreparedKnowledge {
    let mut writer = KnowledgeWriter::new(store);
    writer.receive(now, sender, recipient, incoming);
    writer.finish()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedObservation {
    knowledge: PreparedKnowledge,
    before: u64,
    after: u64,
}
impl PreparedObservation {
    pub fn commit(
        self,
        store: &mut Store,
        runtime: &mut Runtime,
    ) -> Result<Vec<NewDefaultKnowledge>, Error> {
        if runtime.last_observation_event != self.before {
            return Err(Error::Unavailable);
        }
        let effects = self.knowledge.commit(store)?;
        runtime.last_observation_event = self.after;
        Ok(effects)
    }
}
pub fn prepare_observations(
    store: &Store,
    runtime: &Runtime,
    now: SimTime,
    cursor: u64,
    events: &[ObservationEvent],
    presence: &impl Presence,
) -> Result<PreparedObservation, Error> {
    if cursor < runtime.last_observation_event {
        return Err(Error::Unavailable);
    }
    let mut writer = KnowledgeWriter::new(store);
    writer.plan.membership = Some(knowledges(store).map(|(id, _)| id).collect());
    for event in events {
        match event.kind {
            ObservationKind::Proposed {
                agreement,
                proposer,
            } if event.at == now => writer.local(now, agreement, proposer, None, presence)?,
            ObservationKind::Activated { agreement } if event.at == now => {
                let proposer = crate::agreement(store, agreement)
                    .ok_or(Error::Unavailable)?
                    .proposer;
                writer.local(now, agreement, proposer, None, presence)?;
            }
            ObservationKind::Rejected { agreement, actor } if event.at == now => {
                writer.local(now, agreement, actor, None, presence)?
            }
            ObservationKind::Obligation {
                obligation,
                performer,
            } => writer.observe(now, obligation, performer, event.at, presence),
            ObservationKind::Defaulted { obligation } => {
                let actor = crate::obligation(store, obligation)
                    .ok_or(Error::Unavailable)?
                    .term
                    .parties()[0];
                writer.observe(now, obligation, actor, event.at, presence);
            }
            _ => {}
        }
    }
    for actor in writer.actors() {
        if !presence.contact(actor).is_some_and(ContactFacts::conscious) {
            continue;
        }
        let known = writer.read(actor).map_or_else(Vec::new, |row| {
            row.agreements.values().cloned().collect::<Vec<_>>()
        });
        for mut view in known {
            for fact in &mut view.obligations {
                if fact.term.parties()[0] != actor {
                    continue;
                }
                if let Some(actual) = obligation(store, fact.id) {
                    writer
                        .plan
                        .obligations
                        .entry(fact.id)
                        .or_insert_with(|| actual.clone());
                    if actual.term == fact.term
                        && (fact.performed != actual.performed
                            || fact.defaulted != actual.defaulted
                            || fact.cargo != actual.cargo)
                    {
                        fact.performed = actual.performed;
                        fact.defaulted = actual.defaulted;
                        fact.cargo = actual.cargo.clone();
                        fact.observed_at = now;
                        fact.learned_at = now;
                        fact.source = Some(KnowledgeSource::Observed(actor));
                    }
                }
            }
            refresh_known_status(&mut view);
            writer.store(actor, view);
        }
    }
    Ok(PreparedObservation {
        knowledge: writer.finish(),
        before: runtime.last_observation_event,
        after: cursor,
    })
}
