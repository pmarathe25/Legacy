//! Cross-owner references enter through facts; invocation owns their invariants.
use crate::*;
use game_core::*;
use game_ecs::{Component, Store};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryActivity {
    Write(SimTime),
    Read(SimTime),
    Move(SimTime),
    Teach(SimTime),
    Other,
}
#[derive(Clone, Copy, Debug)]
pub struct MemoryValidationPerson {
    pub alive: bool,
    pub activity: MemoryActivity,
}
pub trait MemoryReferences {
    fn person(&self, id: PersonId) -> Option<MemoryValidationPerson>;
    fn death(&self, id: PersonId) -> Option<SimTime>;
    fn area_exists(&self, id: AreaId) -> bool;
    fn memory_activities(&self) -> impl Iterator<Item = (PersonId, MemoryActivity)>;
}
pub struct MemoryValidation<'a> {
    pub now: SimTime,
    pub next_id: u64,
    pub current: Option<PersonId>,
    pub incarnations: &'a BTreeSet<PersonId>,
}
pub fn validate_absent_memory<A: Component>(store: &Store) -> Result<(), Error> {
    validate_absent(store)?;
    if store.count::<PastLife<A>>() != 0
        || store.count::<Record>() != 0
        || store.count::<MemoryAction>() != 0
    {
        return Err(Error::InvalidSave(
            "memory components without invocation runtime".into(),
        ));
    }
    Ok(())
}
impl Runtime {
    pub fn validate_memory<A: Component>(
        &self,
        store: &Store,
        references: &impl MemoryReferences,
        facts: MemoryValidation<'_>,
    ) -> Result<(), Error> {
        let fail = || Error::InvalidSave("invalid invocation memory".into());
        let source_valid = |source: &KnowledgeSource| match source {
            KnowledgeSource::Observed(person) | KnowledgeSource::Reported(person) => {
                references.person(*person).is_some()
            }
            KnowledgeSource::Recorded(id) => record(store, *id).is_some(),
        };
        let fact_valid = |fact: &StockKnowledge| {
            game_cargo::inventory(store, fact.inventory)
                .is_some_and(|stock| stock.good == fact.good)
                && references.area_exists(fact.area)
                && fact.observed_at <= fact.learned_at
                && fact.learned_at <= facts.now
                && source_valid(&fact.source)
        };
        for (id, past) in archives::<A>(store) {
            past.validate().map_err(|_| fail())?;
            if id != past.person
                || references.death(id) != Some(past.died_at)
                || past.died_at > facts.now
                || !references.person(id).is_some_and(|person| !person.alive)
                || past
                    .stocks
                    .known
                    .values()
                    .any(|fact| !fact_valid(fact) || fact.observed_at > past.died_at)
                || past
                    .mind
                    .relationships
                    .keys()
                    .any(|id| references.person(*id).is_none())
                || past.mind.routes.iter().any(|(from, links)| {
                    !references.area_exists(*from)
                        || links
                            .iter()
                            .any(|(to, duration)| !references.area_exists(*to) || duration.0 == 0)
                })
            {
                return Err(fail());
            }
        }
        let archived = archives::<A>(store).map(|(id, _)| id).collect();
        self.control.validate(
            store,
            ControlValidation {
                now: facts.now,
                next_id: facts.next_id,
                current: facts.current,
                incarnations: facts.incarnations,
                archived: &archived,
            },
        )?;
        let content_valid = |content: &RecordedMemory| match content {
            RecordedMemory::Stock { life, fact } => {
                archive::<A>(store, *life).is_some() && fact_valid(fact)
            }
            RecordedMemory::Route {
                life,
                origin,
                destination,
                duration,
            } => {
                archive::<A>(store, *life).is_some()
                    && references.area_exists(*origin)
                    && references.area_exists(*destination)
                    && origin != destination
                    && duration.0 > 0
            }
        };
        let mut totals = BTreeMap::<InventoryId, u64>::new();
        for (id, row) in records(store) {
            if id.0 == 0
                || id.0 >= facts.next_id
                || id != row.id
                || references.person(row.author).is_none()
                || row
                    .readers
                    .iter()
                    .any(|id| references.person(*id).is_none())
                || row.written_at > facts.now
                || !content_valid(&row.content)
                || !game_cargo::inventory(store, row.inventory)
                    .is_some_and(|stock| stock.good == Good::Record)
            {
                return Err(fail());
            }
            if row.intact {
                let total = totals.entry(row.inventory).or_default();
                *total = total.checked_add(1000).ok_or_else(fail)?;
            }
        }
        for (_, stock) in
            game_cargo::inventories(store).filter(|(_, stock)| stock.good == Good::Record)
        {
            if stock.quantity.0 != totals.get(&stock.id).copied().unwrap_or(0) {
                return Err(fail());
            }
        }
        let mut participants = BTreeMap::new();
        for (actor, action) in memory_actions(store) {
            let person = references.person(actor).ok_or_else(fail)?;
            if actor != action.actor
                || !references.area_exists(action.area)
                || action.completes <= facts.now
                || !person.alive
            {
                return Err(fail());
            }
            let valid = match &action.work {
                MemoryWork::Write {
                    content,
                    material,
                    tool,
                } => {
                    content_valid(content)
                        && game_cargo::inventory(store, *material)
                            .is_some_and(|stock| stock.good == Good::Timber)
                        && game_cargo::inventory(store, *tool)
                            .is_some_and(|stock| stock.good == Good::Tool)
                        && person.activity == MemoryActivity::Write(action.completes)
                }
                MemoryWork::Read { record: id } => {
                    record(store, *id).is_some()
                        && person.activity == MemoryActivity::Read(action.completes)
                }
                MemoryWork::Move {
                    record: id,
                    destination,
                } => {
                    record(store, *id).is_some()
                        && match destination {
                            StockPosition::Area(area) => references.area_exists(*area),
                            StockPosition::Carried(person) => references.person(*person).is_some(),
                        }
                        && person.activity == MemoryActivity::Move(action.completes)
                }
                MemoryWork::Teach { content, recipient } => {
                    content_valid(content)
                        && *recipient != actor
                        && person.activity == MemoryActivity::Teach(action.completes)
                        && references.person(*recipient).is_some_and(|person| {
                            person.alive
                                && person.activity == MemoryActivity::Teach(action.completes)
                        })
                }
            };
            if !valid {
                return Err(fail());
            }
            if participants.insert(actor, person.activity).is_some() {
                return Err(fail());
            }
            if let MemoryWork::Teach { recipient, .. } = action.work
                && participants
                    .insert(recipient, MemoryActivity::Teach(action.completes))
                    .is_some()
            {
                return Err(fail());
            }
        }
        for (person, activity) in references.memory_activities() {
            if activity != MemoryActivity::Other && participants.get(&person) != Some(&activity) {
                return Err(fail());
            }
        }
        Ok(())
    }
}
