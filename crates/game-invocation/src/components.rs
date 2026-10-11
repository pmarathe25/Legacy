//! Invocation owns its person-bound costs and allocated warning records.
use crate::{Burden, InvocationWarning};
use game_core::{Error, PersonId, WarningId};
use game_ecs::{Schema, StableId, Store};

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Burden>("invocation/burden", validate_burden)?;
    schema.register::<InvocationWarning>("invocation/warning", validate_warning)
}
fn validate_burden(value: &Burden) -> Result<(), Error> {
    if value.burden_remainder >= 1_000
        || value.exposure_remainder >= 1_000
        || value.temporary_identity > 15
        || value.emotional_disruption > 300
        || value.uninterrupted > value.exposure
    {
        return Err(Error::InvalidSave("invalid invocation burden".into()));
    }
    Ok(())
}
fn validate_warning(value: &InvocationWarning) -> Result<(), Error> {
    if value.id.0 == 0
        || value.actor.0 == 0
        || value.lives.is_empty()
        || value.lives.len() > 8
        || value.lives.iter().any(|life| life.0 == 0)
        || value.severity > 252
        || value.fatigue_per_mille > 64_000_000
        || value.health_harm_per_mille > 64_000_000
        || value.description.len() > 1_024
    {
        return Err(Error::InvalidSave("invalid invocation warning".into()));
    }
    Ok(())
}
pub fn burden(store: &Store, actor: PersonId) -> Option<&Burden> {
    store.get(StableId(actor.0))
}
pub fn burden_mut(store: &mut Store, actor: PersonId) -> Option<&mut Burden> {
    store.get_mut(StableId(actor.0))
}
pub fn burdens(store: &Store) -> impl Iterator<Item = (PersonId, &Burden)> {
    store
        .iter::<Burden>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_burden(store: &mut Store, actor: PersonId, value: Burden) -> Result<(), Error> {
    store.insert_or_spawn(StableId(actor.0), value)
}
pub fn warning(store: &Store, id: WarningId) -> Option<&InvocationWarning> {
    store.get(StableId(id.0))
}
pub fn warnings(store: &Store) -> impl Iterator<Item = (WarningId, &InvocationWarning)> {
    store
        .iter::<InvocationWarning>()
        .map(|(id, row)| (WarningId(id.0), row))
}
pub fn validate_absent(store: &Store) -> Result<(), Error> {
    if store.count::<Burden>() != 0 || store.count::<InvocationWarning>() != 0 {
        return Err(Error::InvalidSave(
            "invocation components without runtime".into(),
        ));
    }
    Ok(())
}
