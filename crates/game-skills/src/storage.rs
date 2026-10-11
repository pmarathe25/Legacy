//! Personal mastery shares the host's entity and component store.
use super::*;
use game_ecs::{Schema, StableId};

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Skills>("skills/personal", Skills::validate)
}

pub fn skills(store: &Store, actor: PersonId) -> Option<&Skills> {
    store.get(StableId(actor.0))
}

pub fn skills_mut(store: &mut Store, actor: PersonId) -> Option<&mut Skills> {
    store.get_mut(StableId(actor.0))
}

pub fn all_skills(store: &Store) -> impl Iterator<Item = (PersonId, &Skills)> {
    store
        .iter::<Skills>()
        .map(|(id, row)| (PersonId(id.0), row))
}

pub fn insert_skills(store: &mut Store, actor: PersonId, skills: Skills) -> Result<(), Error> {
    skills.validate()?;
    store.insert_or_spawn(StableId(actor.0), skills)
}

pub fn remove_skills(store: &mut Store, actor: PersonId) -> Result<Option<Skills>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
