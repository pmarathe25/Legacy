//! Feature-owned typed storage. Backend engine types never cross this boundary.
use super::*;
use game_ecs::{Component, Schema, Snapshot, StableId, Store};
use serde::{Deserializer, Serializer};
use std::fmt;

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
pub(super) fn schema() -> Result<Schema, Error> {
    let mut schema = Schema::new();
    schema.register::<Field>("agriculture/field", validate_field)?;
    schema.register::<Crop>("agriculture/crop", validate_crop)?;
    schema.register::<Soil>("agriculture/soil", Soil::validate)?;
    Ok(schema)
}
impl World {
    pub(super) fn with_components(catalog: Catalog, day: u64, mut components: Store) -> Self {
        let growth_query = components.query3::<Field, Crop, Soil>();
        Self {
            #[cfg(feature = "content")]
            content: None,
            catalog,
            components,
            growth_query,
            jobs: BTreeMap::new(),
            day,
        }
    }
    pub fn fields(&self) -> impl Iterator<Item = (SiteId, &Field)> {
        self.components
            .iter::<Field>()
            .map(|(id, component)| (SiteId(id.0), component))
    }
    pub fn crops(&self) -> impl Iterator<Item = (SiteId, &Crop)> {
        self.components
            .iter::<Crop>()
            .map(|(id, component)| (SiteId(id.0), component))
    }
    pub fn soils(&self) -> impl Iterator<Item = (SiteId, &Soil)> {
        self.components
            .iter::<Soil>()
            .map(|(id, component)| (SiteId(id.0), component))
    }
    pub fn field(&self, id: SiteId) -> Option<&Field> {
        self.components.get(StableId(id.0))
    }
    pub fn crop(&self, id: SiteId) -> Option<&Crop> {
        self.components.get(StableId(id.0))
    }
    pub fn soil(&self, id: SiteId) -> Option<&Soil> {
        self.components.get(StableId(id.0))
    }
    /// Exclusive access to existing data. Structural changes use the insertion
    /// methods, which also refresh this feature's cached component join.
    pub fn field_mut(&mut self, id: SiteId) -> Option<&mut Field> {
        self.components.get_mut(StableId(id.0))
    }
    pub fn crop_mut(&mut self, id: SiteId) -> Option<&mut Crop> {
        self.components.get_mut(StableId(id.0))
    }
    pub fn soil_mut(&mut self, id: SiteId) -> Option<&mut Soil> {
        self.components.get_mut(StableId(id.0))
    }
    fn insert<T: Component>(&mut self, id: SiteId, component: T) -> Result<(), Error> {
        self.components.ensure(StableId(id.0))?;
        self.components.insert(StableId(id.0), component)?;
        self.growth_query.refresh(&self.components)
    }
    pub fn insert_field(&mut self, field: Field) -> Result<(), Error> {
        validate_field(&field)?;
        self.insert(field.id, field)
    }
    pub fn insert_crop(&mut self, id: SiteId, crop: Crop) -> Result<(), Error> {
        validate_crop(&crop)?;
        let rules = self
            .catalog
            .crops
            .get(&crop.definition)
            .ok_or_else(|| Error::InvalidConfiguration("unknown crop definition".into()))?;
        crop.validate(rules)?;
        self.insert(id, crop)
    }
    pub fn insert_soil(&mut self, id: SiteId, soil: Soil) -> Result<(), Error> {
        soil.validate()?;
        self.insert(id, soil)
    }
    fn remove<T: Component>(&mut self, id: SiteId) -> Result<Option<T>, Error> {
        if !self.components.contains(StableId(id.0)) {
            return Ok(None);
        }
        let previous = self.components.remove(StableId(id.0))?;
        self.growth_query.refresh(&self.components)?;
        Ok(previous)
    }
    pub fn remove_field(&mut self, id: SiteId) -> Result<Option<Field>, Error> {
        self.remove(id)
    }
    pub fn remove_crop(&mut self, id: SiteId) -> Result<Option<Crop>, Error> {
        self.remove(id)
    }
    pub fn remove_soil(&mut self, id: SiteId) -> Result<Option<Soil>, Error> {
        self.remove(id)
    }
}

#[derive(Serialize)]
struct SavedRef<'a> {
    #[cfg(feature = "content")]
    content: &'a Option<game_content::Stamp>,
    catalog: &'a Catalog,
    components: Snapshot,
    jobs: &'a BTreeMap<PersonId, Cultivation>,
    day: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    #[cfg(feature = "content")]
    content: Option<game_content::Stamp>,
    catalog: Catalog,
    components: Snapshot,
    jobs: BTreeMap<PersonId, Cultivation>,
    day: u64,
}
impl Serialize for World {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SavedRef {
            #[cfg(feature = "content")]
            content: &self.content,
            catalog: &self.catalog,
            components: self
                .components
                .snapshot()
                .map_err(serde::ser::Error::custom)?,
            jobs: &self.jobs,
            day: self.day,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for World {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = Saved::deserialize(deserializer)?;
        saved
            .catalog
            .validate(None)
            .map_err(serde::de::Error::custom)?;
        let components = Store::restore(
            schema().map_err(serde::de::Error::custom)?,
            saved.components,
        )
        .map_err(serde::de::Error::custom)?;
        let mut world = Self::with_components(saved.catalog, saved.day, components);
        #[cfg(feature = "content")]
        {
            world.content = saved.content;
        }
        world.jobs = saved.jobs;
        Ok(world)
    }
}
impl Clone for World {
    fn clone(&self) -> Self {
        let mut world =
            Self::with_components(self.catalog.clone(), self.day, self.components.clone());
        #[cfg(feature = "content")]
        {
            world.content = self.content.clone();
        }
        world.jobs = self.jobs.clone();
        world
    }
}
impl PartialEq for World {
    fn eq(&self, other: &Self) -> bool {
        #[cfg(feature = "content")]
        if self.content != other.content {
            return false;
        }
        self.catalog == other.catalog
            && self.components == other.components
            && self.jobs == other.jobs
            && self.day == other.day
    }
}
impl Eq for World {}
impl fmt::Debug for World {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("World");
        #[cfg(feature = "content")]
        debug.field("content", &self.content);
        debug
            .field("catalog", &self.catalog)
            .field("fields", &self.fields().collect::<Vec<_>>())
            .field("crops", &self.crops().collect::<Vec<_>>())
            .field("soils", &self.soils().collect::<Vec<_>>())
            .field("jobs", &self.jobs)
            .field("day", &self.day)
            .finish()
    }
}
