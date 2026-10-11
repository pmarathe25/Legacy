//! Plain agricultural components in the host's authoritative typed store.
use super::*;
use game_ecs::{Component, Schema, StableId, Store};

fn validate_field(field: &Field) -> Result<(), Error> {
    if field.id.0 == 0
        || field.area.0 == 0
        || [field.seed.0, field.tool.0, field.water.0, field.food.0].contains(&0)
    {
        return Err(Error::InvalidConfiguration(
            "invalid field component identity".into(),
        ));
    }
    Ok(())
}
fn validate_crop(crop: &Crop) -> Result<(), Error> {
    if crop.tending > 1_000
        || crop.definition.0.is_empty()
        || !crop
            .definition
            .0
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(&byte))
    {
        return Err(Error::InvalidConfiguration("invalid crop component".into()));
    }
    Ok(())
}
/// Registers this feature's plain data and intrinsic save validators. Contextual
/// invariants involving crop definitions remain with the agricultural runtime.
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Field>("agriculture/field", validate_field)?;
    schema.register::<Crop>("agriculture/crop", validate_crop)?;
    schema.register::<Cultivation>("agriculture/cultivation", Cultivation::validate)?;
    Ok(())
}
impl Cultivation {
    pub fn validate(&self) -> Result<(), Error> {
        if self.field.0 == 0 || self.completes.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "invalid cultivation component".into(),
            ));
        }
        Ok(())
    }
}
impl Runtime {
    pub fn cultivation<'a>(&self, store: &'a Store, actor: PersonId) -> Option<&'a Cultivation> {
        cultivation(store, actor)
    }
    pub fn cultivation_mut<'a>(
        &self,
        store: &'a mut Store,
        actor: PersonId,
    ) -> Option<&'a mut Cultivation> {
        cultivation_mut(store, actor)
    }
    pub fn cultivations<'a>(
        &self,
        store: &'a Store,
    ) -> impl Iterator<Item = (PersonId, &'a Cultivation)> {
        cultivations(store)
    }
    pub fn insert_cultivation(
        &self,
        store: &mut Store,
        actor: PersonId,
        job: Cultivation,
    ) -> Result<(), Error> {
        insert_cultivation(store, actor, job)
    }
    pub fn remove_cultivation(
        &self,
        store: &mut Store,
        actor: PersonId,
    ) -> Result<Option<Cultivation>, Error> {
        remove_cultivation(store, actor)
    }
}
/// Cached metadata for the host's store; neither serialized nor authoritative.
pub struct Queries {
    pub(super) growth: game_ecs::Query3<Field, Crop, Soil>,
}
impl Queries {
    /// Refresh after any host structural edit, including edits by another feature.
    pub fn refresh(&mut self, store: &Store) -> Result<(), Error> {
        self.growth.refresh(store)
    }
}
impl Runtime {
    pub fn bind(store: &mut Store) -> Queries {
        Queries {
            growth: store.query3::<Field, Crop, Soil>(),
        }
    }
    pub fn fields<'a>(&self, store: &'a Store) -> impl Iterator<Item = (SiteId, &'a Field)> {
        store
            .iter::<Field>()
            .map(|(id, component)| (SiteId(id.0), component))
    }
    pub fn crops<'a>(&self, store: &'a Store) -> impl Iterator<Item = (SiteId, &'a Crop)> {
        store
            .iter::<Crop>()
            .map(|(id, component)| (SiteId(id.0), component))
    }
    pub fn soils<'a>(&self, store: &'a Store) -> impl Iterator<Item = (SiteId, &'a Soil)> {
        game_land::soils(store)
    }
    pub fn field<'a>(&self, store: &'a Store, id: SiteId) -> Option<&'a Field> {
        store.get(StableId(id.0))
    }
    pub fn crop<'a>(&self, store: &'a Store, id: SiteId) -> Option<&'a Crop> {
        store.get(StableId(id.0))
    }
    pub fn soil<'a>(&self, store: &'a Store, id: SiteId) -> Option<&'a Soil> {
        game_land::soil(store, id)
    }
    pub fn field_mut<'a>(&self, store: &'a mut Store, id: SiteId) -> Option<&'a mut Field> {
        store.get_mut(StableId(id.0))
    }
    pub fn crop_mut<'a>(&self, store: &'a mut Store, id: SiteId) -> Option<&'a mut Crop> {
        store.get_mut(StableId(id.0))
    }
    pub fn soil_mut<'a>(&self, store: &'a mut Store, id: SiteId) -> Option<&'a mut Soil> {
        game_land::soil_mut(store, id)
    }
    fn insert<T: Component>(
        &self,
        store: &mut Store,
        id: SiteId,
        component: T,
    ) -> Result<(), Error> {
        store.insert_or_spawn(StableId(id.0), component)
    }
    pub fn insert_field(&self, store: &mut Store, field: Field) -> Result<(), Error> {
        validate_field(&field)?;
        self.insert(store, field.id, field)
    }
    pub fn insert_crop(&self, store: &mut Store, id: SiteId, crop: Crop) -> Result<(), Error> {
        validate_crop(&crop)?;
        let rules = self
            .catalog
            .crops
            .get(&crop.definition)
            .ok_or_else(|| Error::InvalidConfiguration("unknown crop definition".into()))?;
        crop.validate(rules)?;
        self.insert(store, id, crop)
    }
    pub fn insert_soil(&self, store: &mut Store, id: SiteId, soil: Soil) -> Result<(), Error> {
        game_land::insert_soil(store, id, soil)
    }
    fn remove<T: Component>(&self, store: &mut Store, id: SiteId) -> Result<Option<T>, Error> {
        if !store.contains(StableId(id.0)) {
            return Ok(None);
        }
        store.remove(StableId(id.0))
    }
    pub fn remove_field(&self, store: &mut Store, id: SiteId) -> Result<Option<Field>, Error> {
        self.remove(store, id)
    }
    pub fn remove_crop(&self, store: &mut Store, id: SiteId) -> Result<Option<Crop>, Error> {
        self.remove(store, id)
    }
    pub fn remove_soil(&self, store: &mut Store, id: SiteId) -> Result<Option<Soil>, Error> {
        game_land::remove_soil(store, id)
    }
}

pub fn cultivation(store: &Store, actor: PersonId) -> Option<&Cultivation> {
    store.get(StableId(actor.0))
}

pub fn cultivation_mut(store: &mut Store, actor: PersonId) -> Option<&mut Cultivation> {
    store.get_mut(StableId(actor.0))
}

pub fn cultivations(store: &Store) -> impl Iterator<Item = (PersonId, &Cultivation)> {
    store
        .iter::<Cultivation>()
        .map(|(id, row)| (PersonId(id.0), row))
}

pub fn insert_cultivation(
    store: &mut Store,
    actor: PersonId,
    job: Cultivation,
) -> Result<(), Error> {
    job.validate()?;
    store.insert_or_spawn(StableId(actor.0), job)
}

pub fn remove_cultivation(
    store: &mut Store,
    actor: PersonId,
) -> Result<Option<Cultivation>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
