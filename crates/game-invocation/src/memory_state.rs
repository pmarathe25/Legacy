//! Historical archives, physical records, and timed memory work are feature data.
use crate::{ControlState, RecordedMemory};
use game_characters::decisions::Mind;
use game_core::*;
use game_ecs::{Component, Schema, StableId, Store};
use game_society::institutions::{InstitutionView, SocialCaseView};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PastLife<A> {
    pub person: PersonId,
    pub died_at: SimTime,
    pub mind: Mind<A>,
    pub skills: Vec<SkillView>,
    pub institutions: Vec<InstitutionView>,
    pub cases: Vec<SocialCaseView>,
}
impl<A> PastLife<A> {
    pub fn validate(&self) -> Result<(), Error> {
        let mut skills = BTreeSet::new();
        if self.person.0 == 0
            || self
                .skills
                .iter()
                .any(|skill| !skills.insert(skill.skill) || skill.level > 100)
        {
            return Err(Error::InvalidSave("invalid historical archive".into()));
        }
        self.mind.validate()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: RecordId,
    pub inventory: InventoryId,
    pub content: RecordedMemory,
    pub author: PersonId,
    pub written_at: SimTime,
    pub readers: BTreeSet<PersonId>,
    pub intact: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MemoryWork {
    Write {
        content: RecordedMemory,
        material: InventoryId,
        tool: InventoryId,
    },
    Read {
        record: RecordId,
    },
    Move {
        record: RecordId,
        destination: StockPosition,
    },
    Teach {
        content: RecordedMemory,
        recipient: PersonId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryAction {
    pub actor: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub work: MemoryWork,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub control: ControlState,
}
impl Record {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.inventory.0 == 0
            || self.author.0 == 0
            || self.readers.iter().any(|id| id.0 == 0)
        {
            return Err(Error::InvalidSave("invalid memory record".into()));
        }
        self.content.validate()
    }
}
impl MemoryAction {
    pub fn validate(&self) -> Result<(), Error> {
        let valid = match &self.work {
            MemoryWork::Write {
                content,
                material,
                tool,
            } => material.0 != 0 && tool.0 != 0 && content.validate().is_ok(),
            MemoryWork::Read { record } | MemoryWork::Move { record, .. } => record.0 != 0,
            MemoryWork::Teach { content, recipient } => {
                recipient.0 != 0 && *recipient != self.actor && content.validate().is_ok()
            }
        };
        if self.actor.0 == 0 || self.area.0 == 0 || self.completes.0 == 0 || !valid {
            return Err(Error::InvalidSave("invalid memory action".into()));
        }
        Ok(())
    }
}
pub fn register_memory_components<A: Component>(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<PastLife<A>>("invocation/archive", PastLife::<A>::validate)?;
    schema.register::<Record>("invocation/record", Record::validate)?;
    schema.register::<MemoryAction>("invocation/action", MemoryAction::validate)
}
pub fn archive<A: Component>(store: &Store, person: PersonId) -> Option<&PastLife<A>> {
    store.get(StableId(person.0))
}
pub fn archive_mut<A: Component>(store: &mut Store, person: PersonId) -> Option<&mut PastLife<A>> {
    store.get_mut(StableId(person.0))
}
pub fn archives<A: Component>(store: &Store) -> impl Iterator<Item = (PersonId, &PastLife<A>)> {
    store
        .iter::<PastLife<A>>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn record(store: &Store, id: RecordId) -> Option<&Record> {
    store.get(StableId(id.0))
}
pub fn record_mut(store: &mut Store, id: RecordId) -> Option<&mut Record> {
    store.get_mut(StableId(id.0))
}
pub fn records(store: &Store) -> impl Iterator<Item = (RecordId, &Record)> {
    store
        .iter::<Record>()
        .map(|(id, row)| (RecordId(id.0), row))
}
pub fn memory_action(store: &Store, actor: PersonId) -> Option<&MemoryAction> {
    store.get(StableId(actor.0))
}
pub fn memory_actions(store: &Store) -> impl Iterator<Item = (PersonId, &MemoryAction)> {
    store
        .iter::<MemoryAction>()
        .map(|(id, row)| (PersonId(id.0), row))
}
