//! Composition registers feature-owned schemas; the host owns one component store.
use game_api::Error;
use game_ecs::{Schema, Snapshot, Store};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::{Deref, DerefMut};

pub(crate) fn schema() -> Result<Schema, Error> {
    let mut schema = Schema::new();
    game_cargo::register_components(&mut schema)?;
    game_agriculture::register_components(&mut schema)?;
    game_physiology::register_components(&mut schema)?;
    game_workshop::register_components(&mut schema)?;
    game_skills::register_components(&mut schema)?;
    game_health::register_components(&mut schema)?;
    game_lifecycle::register_components(&mut schema)?;
    game_property::register_components(&mut schema)?;
    game_characters::decisions::register_minds::<game_api::Command>(&mut schema)?;
    game_genetics::register_components(&mut schema)?;
    game_physiology::traits::register_components(&mut schema)?;
    game_characters::temperament::register_components(&mut schema)?;
    game_spatial::register_components(&mut schema)?;
    game_environment::register_components(&mut schema)?;
    game_ecology::register_components(&mut schema)?;
    game_land::register_components(&mut schema)?;
    schema.register::<game_api::Activity>("coordination/activity", |_| Ok(()))?;
    Ok(schema)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Components(Store);
impl Components {
    pub fn new() -> Result<Self, Error> {
        Ok(Self(Store::with_schema(schema()?)))
    }
    pub(crate) fn validate_land(&self) -> Result<(), Error> {
        game_land::validate_uses(
            &self.0,
            self.0
                .iter::<game_agriculture::Field>()
                .map(|(id, _)| game_core::SiteId(id.0))
                .chain(
                    self.0
                        .iter::<game_ecology::Plot>()
                        .map(|(id, _)| game_core::SiteId(id.0)),
                ),
        )
    }

    pub fn activity(&self, actor: game_api::PersonId) -> Option<&game_api::Activity> {
        self.0.get(game_ecs::StableId(actor.0))
    }
    pub fn activity_mut(&mut self, actor: game_api::PersonId) -> Option<&mut game_api::Activity> {
        self.0.get_mut(game_ecs::StableId(actor.0))
    }
    pub fn activities(&self) -> impl Iterator<Item = (game_api::PersonId, &game_api::Activity)> {
        self.0
            .iter::<game_api::Activity>()
            .map(|(id, activity)| (game_api::PersonId(id.0), activity))
    }
    pub fn set_activity(
        &mut self,
        actor: game_api::PersonId,
        activity: game_api::Activity,
    ) -> Result<(), Error> {
        self.0.insert(game_ecs::StableId(actor.0), activity)
    }
}
impl From<Store> for Components {
    fn from(store: Store) -> Self {
        Self(store)
    }
}
impl game_world::StorageAccess for Components {
    fn store(&self) -> &Store {
        &self.0
    }
    fn store_mut(&mut self) -> &mut Store {
        &mut self.0
    }
}
impl Deref for Components {
    type Target = Store;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Components {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Serialize for Components {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0
            .snapshot()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Components {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let snapshot = Snapshot::deserialize(deserializer)?;
        Store::restore(schema().map_err(serde::de::Error::custom)?, snapshot)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
