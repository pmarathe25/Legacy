//! An adopted Society change prepares Enforcement rows in the producer's batch.
use crate::enforcement::*;
use crate::institutions::{InstitutionChange, RuleProposal, RuleProposalStatus};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

pub struct PreparedAdoption {
    reads: ReadSet,
    proposal: RuleProposal,
    jurisdiction: AreaId,
    case: Option<Case>,
    policies: Option<(InstitutionId, Policies)>,
    event: Option<Event>,
}
pub struct AdoptionPublication {
    guard: ReadGuard,
    proposal: RuleProposal,
    jurisdiction: AreaId,
    event: Option<Event>,
    writes: ReadFootprint,
}
/// The institutional producer selects adoption. This receiver owns the resulting
/// policy/judgment rows; its final validation requires the adopted producer row.
pub fn prepare_adoption(
    store: &Store,
    now: SimTime,
    proposal: &RuleProposal,
    judgment: Option<JudgmentId>,
) -> Result<PreparedAdoption, Error> {
    let institution = crate::institution(store, proposal.institution).ok_or(Error::Unavailable)?;
    let jurisdiction = institution.charter.jurisdiction;
    let mut reads = ReadSet::new();
    let (case, policies, event) = match &proposal.change {
        InstitutionChange::EnforcementPolicy { office, policy } => {
            let entity = StableId(proposal.institution.0);
            let original = reads.read::<Policies>(store, entity);
            let mut next = original.cloned().unwrap_or_default();
            apply_policy_change(
                &mut next.offices,
                proposal.institution,
                *office,
                policy.as_ref(),
                now,
            );
            (
                None,
                Some((proposal.institution, next)),
                Some(Event::EnforcementPolicyChanged {
                    institution: proposal.institution,
                    office: *office,
                }),
            )
        }
        InstitutionChange::Adjudicate { case, remedy } => {
            let original = reads
                .read::<Case>(store, StableId(case.0))
                .ok_or(Error::Unavailable)?;
            if original.institution != proposal.institution
                || original.status != EnforcementStatus::Pending
                || original.judgment.is_some()
                || !original.policy.remedies.contains(&remedy.kind())
            {
                return Err(Error::Unavailable);
            }
            let judgment = judgment.filter(|id| id.0 != 0).ok_or(Error::Unavailable)?;
            let next = adopted_case(original, proposal, remedy, judgment, jurisdiction, now);
            (
                Some(next),
                None,
                Some(Event::EnforcementDecided {
                    case: *case,
                    judgment,
                    proposer: proposal.proposer,
                }),
            )
        }
        _ => (None, None, None),
    };
    Ok(PreparedAdoption {
        reads,
        proposal: proposal.clone(),
        jurisdiction,
        case,
        policies,
        event,
    })
}
pub(super) fn apply_policy_change(
    offices: &mut std::collections::BTreeMap<u16, EnforcementPolicyView>,
    institution: InstitutionId,
    office: u16,
    policy: Option<&EnforcementPolicy>,
    now: SimTime,
) {
    if let Some(policy) = policy {
        offices.insert(
            office,
            EnforcementPolicyView {
                institution,
                office,
                policy: policy.clone(),
                adopted_at: now,
                observed_at: now,
            },
        );
    } else {
        offices.remove(&office);
    }
}
pub(super) fn adopted_case(
    original: &Case,
    proposal: &RuleProposal,
    remedy: &EnforcementRemedy,
    judgment: JudgmentId,
    jurisdiction: AreaId,
    now: SimTime,
) -> Case {
    let mut next = original.clone();
    next.judgment = Some(EnforcementJudgmentView {
        id: judgment,
        proposer: proposal.proposer,
        proposal: proposal.id,
        remedy: remedy.clone(),
        adopted_at: now,
    });
    next.status = if matches!(remedy, EnforcementRemedy::Dismiss) {
        EnforcementStatus::Dismissed
    } else {
        EnforcementStatus::Decided
    };
    next.changed_at = now;
    next.notice_area = Some(jurisdiction);
    next.witnesses = std::collections::BTreeSet::from([proposal.proposer]);
    next.explanation =
        "The institution adopted a remedy; physical compliance remains unresolved".into();
    if matches!(remedy, EnforcementRemedy::Depose { .. }) {
        next.status = EnforcementStatus::Completed;
        next.explanation="The chosen procedure replaced the officeholder; their body, possessions and debts remain".into();
    }
    next
}
impl PreparedAdoption {
    pub fn stage(self, store: &Store, batch: &mut Batch) -> Result<AdoptionPublication, Error> {
        let mut writes = ReadFootprint::new();
        if let Some(case) = self.case {
            let id = StableId(case.id.0);
            writes.protect::<Case>(id);
            batch.insert(id, case);
        }
        if let Some((institution, row)) = self.policies {
            let id = StableId(institution.0);
            writes.protect::<Policies>(id);
            batch.insert(id, row);
        }
        Ok(AdoptionPublication {
            guard: self.reads.bind(store, batch, &writes)?,
            proposal: self.proposal,
            jurisdiction: self.jurisdiction,
            event: self.event,
            writes,
        })
    }
}
impl AdoptionPublication {
    pub fn validate(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.guard.validate(store, batch)?;
        let projected = batch
            .projected::<RuleProposal>(store, StableId(self.proposal.id.0))?
            .ok_or(Error::Unavailable)?;
        let institution = batch
            .projected::<crate::institutions::Institution>(
                store,
                StableId(self.proposal.institution.0),
            )?
            .ok_or(Error::Unavailable)?;
        if projected.id != self.proposal.id
            || projected.institution != self.proposal.institution
            || projected.proposer != self.proposal.proposer
            || projected.change != self.proposal.change
            || projected.status != RuleProposalStatus::Adopted
            || institution.charter.jurisdiction != self.jurisdiction
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn finish(self) -> Option<Event> {
        self.event
    }
}

/// Factual physical joins and event envelopes; selection remains in Enforcement.
pub struct AdoptionObservationFacts<'a> {
    pub now: SimTime,
    pub people: &'a [PersonFacts],
    pub assaults: &'a [WitnessedAssault],
    pub last_event: Option<u64>,
}
pub struct ObservationPublication {
    guard: ReadGuard,
    last_event: Option<u64>,
}
/// Existing adoption performs one observation pass over occupied cells. Historical
/// assault cells are included only while their event is still pending this instant.
pub fn adoption_observer_areas(
    now: SimTime,
    last_event: Option<u64>,
    receipts: &[&AdoptionPublication],
    assaults: &[WitnessedAssault],
    person_areas: &std::collections::BTreeMap<PersonId, Option<AreaId>>,
) -> std::collections::BTreeSet<AreaId> {
    let mut areas = person_areas
        .values()
        .filter_map(|area| *area)
        .collect::<std::collections::BTreeSet<_>>();
    areas.extend(receipts.iter().map(|receipt| receipt.jurisdiction));
    if let Some(cursor) = last_event {
        areas.extend(
            assaults
                .iter()
                .filter(|strike| strike.event.0 > cursor && strike.observed_at == now)
                .map(|strike| strike.area),
        );
    }
    areas
}
pub fn stage_adoption_observation(
    store: &Store,
    batch: &mut Batch,
    receipts: &[&AdoptionPublication],
    facts: AdoptionObservationFacts<'_>,
    mut reads: ReadSet,
) -> Result<ObservationPublication, Error> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut writes = ReadFootprint::new();
    for receipt in receipts {
        receipt.validate(store, batch)?;
        writes.extend(receipt.writes.clone());
    }
    let mut people = BTreeMap::new();
    for person in facts.people {
        if let Some(old) = people.insert(person.id, person)
            && old != person
        {
            return Err(Error::Unavailable);
        }
    }
    let mut by_area = BTreeMap::<AreaId, Vec<PersonId>>::new();
    for person in people.values() {
        if let Location::AtArea(area) = person.location
            && person.vitals.alive
            && person.vitals.health > 0
        {
            by_area.entry(area).or_default().push(person.id);
        }
    }
    let areas = by_area.keys().copied().collect::<BTreeSet<_>>();
    let observers = by_area.values().flatten().copied().collect::<BTreeSet<_>>();
    reads.membership::<Case>(store, |_, _| true);
    reads.membership::<Policies>(store, |_, _| true);
    reads.membership::<crate::institutions::Institution>(store, move |_, row| {
        areas.contains(&row.charter.jurisdiction)
    });
    let case_ids = cases(store)
        .map(|(id, _)| StableId(id.0))
        .chain(batch.edited_ids::<Case>())
        .collect::<BTreeSet<_>>();
    let mut announcements = Vec::new();
    for id in case_ids {
        if let Some(case) = batch.projected::<Case>(store, id)?
            && case.changed_at == facts.now
            && case.status != EnforcementStatus::Filing
            && (case
                .notice_area
                .is_some_and(|area| by_area.contains_key(&area))
                || !case.witnesses.is_disjoint(&observers))
        {
            reads.read::<Case>(store, id);
            announcements.push(case.clone());
        }
    }
    let institutions = crate::institutions(store)
        .map(|(id, _)| id)
        .chain(
            batch
                .edited_ids::<crate::institutions::Institution>()
                .map(|id| InstitutionId(id.0)),
        )
        .collect::<BTreeSet<_>>();
    let mut local_policies = BTreeMap::<AreaId, Vec<(InstitutionId, Policies)>>::new();
    for institution in institutions {
        let id = StableId(institution.0);
        if let Some(row) = batch.projected::<crate::institutions::Institution>(store, id)?
            && by_area.contains_key(&row.charter.jurisdiction)
        {
            let policies = batch
                .projected::<Policies>(store, id)?
                .cloned()
                .unwrap_or_default();
            reads.read::<Policies>(store, id);
            local_policies
                .entry(row.charter.jurisdiction)
                .or_default()
                .push((institution, policies));
        }
    }
    for (area, locals) in &by_area {
        let contacts = locals
            .iter()
            .copied()
            .take(64)
            .map(|person| {
                (
                    person,
                    ReportedContact {
                        person,
                        area: *area,
                        observed_at: facts.now,
                    },
                )
            })
            .collect::<Vec<_>>();
        let strikes = facts
            .assaults
            .iter()
            .filter(|strike| {
                facts
                    .last_event
                    .is_some_and(|cursor| strike.event.0 > cursor)
                    && strike.observed_at == facts.now
                    && strike.area == *area
            })
            .collect::<Vec<_>>();
        for actor in locals {
            let id = StableId(actor.0);
            let opening = reads.read::<Memory>(store, id).cloned();
            let mut memory = opening.clone().unwrap_or_default();
            for (person, contact) in &contacts {
                memory.contacts.insert(*person, contact.clone());
            }
            // Strike hearing uses the same capped local listener order as production.
            if locals.iter().take(64).any(|listener| listener == actor) {
                for strike in &strikes {
                    memory.assaults.insert(strike.event, (*strike).clone());
                }
            }
            for (institution, policies) in local_policies.get(area).into_iter().flatten() {
                let known = memory.known_policies.entry(*institution).or_default();
                known.retain(|office, _| policies.offices.contains_key(office));
                for (office, policy) in &policies.offices {
                    let mut observed = policy.clone();
                    observed.observed_at = facts.now;
                    known.insert(*office, observed);
                }
            }
            for case in &announcements {
                if case.witnesses.contains(actor) || case.notice_area == Some(*area) {
                    memory.known.insert(
                        case.id,
                        super::view_of_case(
                            case,
                            *actor,
                            facts.now,
                            KnowledgeSource::Observed(*actor),
                        ),
                    );
                }
            }
            for view in memory.known.values_mut() {
                if let Some(contact) = memory.contacts.get(&view.target)
                    && view
                        .contact
                        .as_ref()
                        .is_none_or(|old| old.observed_at < contact.observed_at)
                {
                    view.contact = Some(contact.clone());
                }
            }
            if opening.as_ref() != Some(&memory) {
                writes.protect::<Memory>(id);
                batch.insert(id, memory);
            }
        }
    }
    Ok(ObservationPublication {
        guard: reads.bind(store, batch, &writes)?,
        last_event: facts.last_event,
    })
}
impl ObservationPublication {
    pub fn validate(
        &self,
        runtime: Option<&World>,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        if runtime.map(|world| world.last_event) != self.last_event {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
    pub fn finish(self, through: u64) -> World {
        World {
            last_event: through,
        }
    }
}
