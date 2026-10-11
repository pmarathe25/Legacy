//! Biological rows share the host's stable entities and component store.
use super::*;
use game_ecs::{Schema, StableId};

impl Life {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self
                .parents
                .iter()
                .any(|parent| parent.0 == 0 || *parent == self.id)
            || self.caregivers.iter().any(|carer| carer.0 == 0)
        {
            return Err(Error::InvalidConfiguration(
                "invalid lifecycle life component".into(),
            ));
        }
        Ok(())
    }
}
impl Pregnancy {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.gestator.0 == 0
            || self.partner.0 == 0
            || self.gestator == self.partner
            || self.id.0 == self.gestator.0
            || self.id.0 == self.partner.0
            || match self.state {
                PregnancyState::Conceiving { completes } => completes.0 == 0,
                PregnancyState::Gestating { due } => due.0 == 0,
                PregnancyState::Born(child) => {
                    child.0 == 0
                        || child.0 == self.id.0
                        || child == self.gestator
                        || child == self.partner
                }
                PregnancyState::Ended => false,
            }
        {
            return Err(Error::InvalidConfiguration(
                "invalid lifecycle pregnancy component".into(),
            ));
        }
        Ok(())
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Life>("lifecycle/life", Life::validate)?;
    schema.register::<Pregnancy>("lifecycle/pregnancy", Pregnancy::validate)
}
pub fn life(store: &Store, person: PersonId) -> Option<&Life> {
    store.get(StableId(person.0))
}
pub fn life_mut(store: &mut Store, person: PersonId) -> Option<&mut Life> {
    store.get_mut(StableId(person.0))
}
pub fn all_lives(store: &Store) -> impl Iterator<Item = (PersonId, &Life)> {
    store.iter::<Life>().map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_life(store: &mut Store, row: Life) -> Result<(), Error> {
    row.validate()?;
    store.insert_or_spawn(StableId(row.id.0), row)
}
pub fn remove_life(store: &mut Store, person: PersonId) -> Result<Option<Life>, Error> {
    if !store.contains(StableId(person.0)) {
        return Ok(None);
    }
    store.remove(StableId(person.0))
}
pub fn pregnancy(store: &Store, id: PregnancyId) -> Option<&Pregnancy> {
    store.get(StableId(id.0))
}
pub fn pregnancy_mut(store: &mut Store, id: PregnancyId) -> Option<&mut Pregnancy> {
    store.get_mut(StableId(id.0))
}
pub fn all_pregnancies(store: &Store) -> impl Iterator<Item = (PregnancyId, &Pregnancy)> {
    store
        .iter::<Pregnancy>()
        .map(|(id, row)| (PregnancyId(id.0), row))
}
pub fn insert_pregnancy(store: &mut Store, row: Pregnancy) -> Result<(), Error> {
    row.validate()?;
    store.insert_or_spawn(StableId(row.id.0), row)
}
pub fn remove_pregnancy(store: &mut Store, id: PregnancyId) -> Result<Option<Pregnancy>, Error> {
    if !store.contains(StableId(id.0)) {
        return Ok(None);
    }
    store.remove(StableId(id.0))
}
