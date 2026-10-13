//! Enforcement's canonical case, work claim, policy and personal memory rows.
use crate::enforcement::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub last_event: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policies {
    pub offices: BTreeMap<u16, EnforcementPolicyView>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub known: BTreeMap<EnforcementCaseId, EnforcementCaseView>,
    pub known_policies: BTreeMap<InstitutionId, BTreeMap<u16, EnforcementPolicyView>>,
    pub contacts: BTreeMap<PersonId, ReportedContact>,
    pub assaults: BTreeMap<EventId, WitnessedAssault>,
    pub reconsider_after: Option<SimTime>,
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Case>("society/enforcement-case", |row| {
        if row.id.0 == 0 || row.institution.0 == 0 || row.filer.0 == 0 || row.target.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid enforcement case identity".into(),
            ));
        }
        Ok(())
    })?;
    schema.register::<Talk>("society/enforcement-talk", |row| {
        if row.case.0 == 0 || row.recipient.0 == 0 || row.area.0 == 0 || row.completes.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid enforcement work claim".into(),
            ));
        }
        Ok(())
    })?;
    schema.register::<Policies>("society/enforcement-policies", |_| Ok(()))?;
    schema.register::<Memory>("society/enforcement-memory", |_| Ok(()))
}
pub fn case(store: &Store, id: EnforcementCaseId) -> Option<&Case> {
    store.get(StableId(id.0))
}
pub fn case_mut(store: &mut Store, id: EnforcementCaseId) -> Option<&mut Case> {
    store.get_mut(StableId(id.0))
}
pub fn cases(store: &Store) -> impl Iterator<Item = (EnforcementCaseId, &Case)> {
    store
        .iter::<Case>()
        .map(|(id, row)| (EnforcementCaseId(id.0), row))
}
pub fn talk(store: &Store, actor: PersonId) -> Option<&Talk> {
    store.get(StableId(actor.0))
}
pub fn talks(store: &Store) -> impl Iterator<Item = (PersonId, &Talk)> {
    store.iter::<Talk>().map(|(id, row)| (PersonId(id.0), row))
}
pub fn memory(store: &Store, actor: PersonId) -> Option<&Memory> {
    store.get(StableId(actor.0))
}
pub fn memories(store: &Store) -> impl Iterator<Item = (PersonId, &Memory)> {
    store
        .iter::<Memory>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn memory_mut(store: &mut Store, actor: PersonId) -> Result<&mut Memory, Error> {
    let id = StableId(actor.0);
    if store.get::<Memory>(id).is_none() {
        store.insert_or_spawn(id, Memory::default())?;
    }
    store.get_mut(id).ok_or(Error::Unavailable)
}
pub fn policies(store: &Store) -> impl Iterator<Item = (InstitutionId, &Policies)> {
    store
        .iter::<Policies>()
        .map(|(id, row)| (InstitutionId(id.0), row))
}
pub fn policy(
    store: &Store,
    institution: InstitutionId,
    office: u16,
) -> Option<&EnforcementPolicyView> {
    store
        .get::<Policies>(StableId(institution.0))?
        .offices
        .get(&office)
}
pub fn policies_mut(store: &mut Store, institution: InstitutionId) -> Result<&mut Policies, Error> {
    let id = StableId(institution.0);
    if store.get::<Policies>(id).is_none() {
        store.insert_or_spawn(id, Policies::default())?;
    }
    store.get_mut(id).ok_or(Error::Unavailable)
}
pub fn insert_case(store: &mut Store, row: Case) -> Result<(), Error> {
    store.insert_or_spawn(StableId(row.id.0), row)
}
pub fn insert_talk(store: &mut Store, actor: PersonId, row: Talk) -> Result<(), Error> {
    store.insert_or_spawn(StableId(actor.0), row)
}
pub fn remove_talk(store: &mut Store, actor: PersonId) -> Result<Option<Talk>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
pub struct PreparedInterruption {
    actors: Vec<PersonId>,
    reads: ReadSet,
    changes: BTreeMap<EnforcementCaseId, Case>,
}
impl World {
    pub fn prepare_interruptions(
        &self,
        store: &Store,
        actors: &[PersonId],
        now: SimTime,
    ) -> Result<PreparedInterruption, Error> {
        let mut reads = ReadSet::new();
        let mut changes = BTreeMap::new();
        let mut selected = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for actor in actors.iter().copied().filter(|actor| seen.insert(*actor)) {
            let Some(work) = reads.read::<Talk>(store, StableId(actor.0)) else {
                continue;
            };
            selected.push(actor);
            if let Some(change) =
                complaint_interruption(now, matches!(work.conversation, Conversation::File))
            {
                let original = reads
                    .read::<Case>(store, StableId(work.case.0))
                    .ok_or(Error::Unavailable)?;
                let mut next = original.clone();
                next.status = EnforcementStatus::Withdrawn;
                next.changed_at = change.changed_at;
                next.explanation = change.explanation.into();
                changes.insert(next.id, next);
            }
        }
        Ok(PreparedInterruption {
            actors: selected,
            reads,
            changes,
        })
    }
}
impl PreparedInterruption {
    pub fn actors(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.actors.iter().copied()
    }
    pub fn stage(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        let mut writes = ReadFootprint::new();
        for actor in self.actors {
            let id = StableId(actor.0);
            writes.protect::<Talk>(id);
            batch.remove::<Talk>(id);
        }
        for (case, row) in self.changes {
            let id = StableId(case.0);
            writes.protect::<Case>(id);
            batch.insert(id, row);
        }
        self.reads.bind(store, batch, &writes)
    }
}
