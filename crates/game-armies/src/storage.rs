//! Canonical army records and dated knowledge live in the host's shared store.
use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub armies: BTreeMap<ArmyId, ArmyKnowledge>,
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Army>("armies/army", |row| {
        if !crate::validation::army_shape_valid(row) {
            return Err(Error::InvalidSave("invalid army record".into()));
        }
        Ok(())
    })?;
    schema.register::<Knowledge>("armies/knowledge", |_| Ok(()))?;
    schema.register::<Talk>("armies/talk", |row| {
        if row.speaker.0 == 0
            || row.listener.0 == 0
            || row.area.0 == 0
            || row.message.army().0 == 0
            || row.speaker == row.listener
            || row.started >= row.completes
        {
            return Err(Error::InvalidSave("invalid army conversation".into()));
        }
        Ok(())
    })?;
    schema.register::<Queue>("armies/queue", |row| {
        if row.messages.len() > 128 {
            return Err(Error::InvalidSave("invalid army message queue".into()));
        }
        Ok(())
    })?;
    schema.register::<Approach>("armies/approach", |row| {
        if row.actor.0 == 0
            || row.mission.0 == 0
            || row.army.0 == 0
            || row.order.0 == 0
            || row.encounter.0 == 0
            || row.area.0 == 0
            || row.target.0 == 0
            || row.actor == row.target
            || row.started >= row.completes
        {
            return Err(Error::InvalidSave("invalid army approach".into()));
        }
        Ok(())
    })
}
macro_rules! rows {
    ($get:ident,$all:ident,$insert:ident,$remove:ident,$ty:ty,$id:ident,$consistent:expr) => {
        pub fn $get(store: &Store, id: $id) -> Option<&$ty> {
            store.get(StableId(id.0))
        }
        pub fn $all(store: &Store) -> impl Iterator<Item = ($id, &$ty)> {
            store.iter::<$ty>().map(|(id, row)| ($id(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: $id, row: $ty) -> Result<(), Error> {
            if !($consistent)(&row, id) {
                return Err(Error::InvalidSave(
                    "army component identity mismatch".into(),
                ));
            }
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $remove(store: &mut Store, id: $id) -> Result<Option<$ty>, Error> {
            store.remove(StableId(id.0))
        }
    };
}
rows!(
    army,
    armies,
    insert_army,
    remove_army,
    Army,
    ArmyId,
    |row: &Army, id: ArmyId| row.id == id
);
rows!(
    knowledge,
    knowledges,
    insert_knowledge,
    remove_knowledge,
    Knowledge,
    PersonId,
    |_: &Knowledge, _: PersonId| true
);
rows!(
    talk,
    talks,
    insert_talk,
    remove_talk,
    Talk,
    PersonId,
    |row: &Talk, id: PersonId| row.speaker == id
);
rows!(
    queue,
    queues,
    insert_queue,
    remove_queue,
    Queue,
    PersonId,
    |_: &Queue, _: PersonId| true
);
rows!(
    approach,
    approaches,
    insert_approach,
    remove_approach,
    Approach,
    PersonId,
    |row: &Approach, id: PersonId| row.actor == id
);

/// A borrowed planning facade; it never clones or retains entity maps.
pub struct Kernel<'a> {
    pub(crate) store: &'a Store,
}
impl<'a> Kernel<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }
}
pub(crate) fn enqueue(
    store: &mut Store,
    actor: PersonId,
    message: QueuedMessage,
) -> Result<(), Error> {
    let mut row = queue(store, actor).cloned().unwrap_or_default();
    row.messages.push_back(message);
    insert_queue(store, actor, row)
}
pub(crate) fn pop_queue(store: &mut Store, actor: PersonId) {
    store
        .get_mut::<Queue>(StableId(actor.0))
        .expect("queue exists")
        .messages
        .pop_front();
}
/// Stage the exact owner-selected conversation removal into a composed batch.
pub fn stage_cancel_talk(
    store: &Store,
    opening: &Talk,
    batch: &mut game_ecs::Batch,
) -> Result<game_ecs::ReadGuard, Error> {
    let id = StableId(opening.speaker.0);
    let mut reads = game_ecs::ReadSet::new();
    reads.expect_row(id, Some(opening.clone()));
    reads.validate(store)?;
    batch.remove::<Talk>(id);
    let mut admitted = game_ecs::ReadFootprint::new();
    admitted.protect::<Talk>(id);
    reads.bind(store, batch, &admitted)
}
pub fn stage_cancel_approach(
    store: &Store,
    opening: &Approach,
    batch: &mut game_ecs::Batch,
) -> Result<game_ecs::ReadGuard, Error> {
    let id = StableId(opening.actor.0);
    let mut reads = game_ecs::ReadSet::new();
    reads.expect_row(id, Some(opening.clone()));
    reads.validate(store)?;
    batch.remove::<Approach>(id);
    let mut admitted = game_ecs::ReadFootprint::new();
    admitted.protect::<Approach>(id);
    reads.bind(store, batch, &admitted)
}
