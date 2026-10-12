//! Native mission records share the host's stable entity namespace.
use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub missions: BTreeMap<MissionId, MissionView>,
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Mission>("missions/mission", Mission::validate)?;
    schema.register::<Recruitment>("missions/recruitment", Recruitment::validate)?;
    schema.register::<Knowledge>("missions/knowledge", Knowledge::validate)?;
    schema.register::<PendingTalk>("missions/talk", PendingTalk::validate)
}
macro_rules! row {
    ($get:ident,$get_mut:ident,$all:ident,$insert:ident,$remove:ident,$ty:ty,$id:ident,$consistent:expr) => {
        pub fn $get(store: &Store, id: $id) -> Option<&$ty> {
            store.get(StableId(id.0))
        }
        pub fn $get_mut(store: &mut Store, id: $id) -> Option<&mut $ty> {
            store.get_mut(StableId(id.0))
        }
        pub fn $all(store: &Store) -> impl Iterator<Item = ($id, &$ty)> {
            store.iter::<$ty>().map(|(id, row)| ($id(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: $id, row: $ty) -> Result<(), Error> {
            if !($consistent)(&row, id) {
                return Err(Error::InvalidSave(
                    "mission component identity mismatch".into(),
                ));
            }
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $remove(store: &mut Store, id: $id) -> Result<Option<$ty>, Error> {
            store.remove(StableId(id.0))
        }
    };
}
row!(
    mission,
    mission_mut,
    missions,
    insert_mission,
    remove_mission,
    Mission,
    MissionId,
    |row: &Mission, id: MissionId| row.id == id
);
row!(
    recruitment,
    recruitment_mut,
    recruitments,
    insert_recruitment,
    remove_recruitment,
    Recruitment,
    RecruitmentId,
    |row: &Recruitment, id: RecruitmentId| row.id == id
);
row!(
    knowledge,
    knowledge_mut,
    knowledges,
    insert_knowledge,
    remove_knowledge,
    Knowledge,
    PersonId,
    |_: &Knowledge, _: PersonId| true
);
row!(
    talk,
    talk_mut,
    talks,
    insert_talk,
    remove_talk,
    PendingTalk,
    PersonId,
    |row: &PendingTalk, id: PersonId| row.speaker == id
);

/// Derived roster IDs for a coordinated immutable planning pass. The host
/// rebuilds this after roster changes; live status and dated knowledge are read
/// from the shared store at lookup time rather than copied into the index.
#[derive(Clone, Debug, Default)]
pub struct MembershipIndex {
    by_person: BTreeMap<PersonId, Vec<MissionId>>,
}
impl MembershipIndex {
    pub fn build(store: &Store) -> Self {
        let mut index = Self::default();
        for (id, row) in missions(store) {
            for person in row.members.keys() {
                index.by_person.entry(*person).or_default().push(id);
            }
        }
        index
    }
    pub fn register_created(&mut self, store: &Store, ids: impl IntoIterator<Item = MissionId>) {
        for id in ids {
            if let Some(mission) = mission(store, id) {
                for person in mission.members.keys() {
                    let ids = self.by_person.entry(*person).or_default();
                    if let Err(position) = ids.binary_search(&id) {
                        ids.insert(position, id);
                    }
                }
            }
        }
    }
    pub fn membership(&self, store: &Store, actor: PersonId) -> Option<MissionId> {
        self.by_person.get(&actor)?.iter().copied().find(|id| {
            mission(store, *id)
                .and_then(|m| m.members.get(&actor))
                .is_some_and(|m| m.status == CompanionStatus::Accepted)
                && !knowledge(store, actor)
                    .and_then(|k| k.missions.get(id))
                    .is_some_and(|view| {
                        matches!(
                            view.status,
                            MissionStatus::Completed
                                | MissionStatus::Failed
                                | MissionStatus::Cancelled
                        )
                    })
        })
    }
}

/// Borrow one dated service record through the same cap/order as public views.
pub fn known_mission(store: &Store, actor: PersonId, id: MissionId) -> Option<&MissionView> {
    knowledge(store, actor)?
        .missions
        .values()
        .rev()
        .take(64)
        .find(|view| view.id == id)
}
