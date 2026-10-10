//! Body data in the host's shared component store; no engine-specific types.
use super::*;
use game_ecs::{Schema, StableId, Store};
impl Body {
    /// Intrinsic data invariants. Age, genetic expression, and current limits
    /// belong to the owners that supply the numerical integration facts.
    pub fn validate(&self) -> Result<(), Error> {
        if self.fatigue_remainder >= 1_000
            || self.health_remainder >= 1_000
            || (!self.vitals.alive && self.vitals.health != 0)
        {
            return Err(Error::InvalidConfiguration("invalid body component".into()));
        }
        // An immortal may remain alive at zero health. Death need not reset
        // fractional fatigue until the next numerical integration interval.
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Body>("physiology/body", Body::validate)
}
pub fn body(store: &Store, actor: PersonId) -> Option<&Body> {
    store.get(StableId(actor.0))
}
pub fn body_mut(store: &mut Store, actor: PersonId) -> Option<&mut Body> {
    store.get_mut(StableId(actor.0))
}
pub fn bodies(store: &Store) -> impl Iterator<Item = (PersonId, &Body)> {
    store
        .iter::<Body>()
        .map(|(id, body)| (PersonId(id.0), body))
}
pub fn insert_body(store: &mut Store, actor: PersonId, body: Body) -> Result<(), Error> {
    body.validate()?;
    store.insert_or_spawn(StableId(actor.0), body)
}
pub fn remove_body(store: &mut Store, actor: PersonId) -> Result<Option<Body>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
