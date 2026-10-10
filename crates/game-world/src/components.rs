//! Composition registers feature-owned schemas; the host owns one component store.
use game_api::Error;
use game_ecs::{Schema, Snapshot, Store};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::{Deref, DerefMut};

fn schema() -> Result<Schema, Error> {
    let mut schema = Schema::new();
    game_agriculture::register_components(&mut schema)?;
    Ok(schema)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Components(Store);
impl Components {
    pub fn new() -> Result<Self, Error> {
        Ok(Self(Store::with_schema(schema()?)))
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
mod tests {
    use crate::{Execution, Simulation};
    use game_core::SimDuration;

    #[test]
    fn one_host_snapshot_and_metadata_only_runtime_survive_clone_and_restore() {
        let mut sim = Simulation::farming_fixture(77, Execution::default()).unwrap();
        let state = sim.canonical_state();
        let json = serde_json::to_value(state).unwrap();
        assert!(
            json["components"]["columns"]["agriculture/field"]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
        let agriculture = json["agriculture"].as_object().unwrap();
        assert!(!agriculture.contains_key("components"));
        assert!(!agriculture.contains_key("fields"));
        assert!(!agriculture.contains_key("crops"));
        assert!(!agriculture.contains_key("soils"));
        let cloned = state.clone();
        assert_eq!(state, &cloned);
        assert!(sim.agriculture_queries.refresh(&cloned.components).is_err());
        let mut rebound = Simulation::from_state(
            cloned,
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
            0,
        )
        .unwrap();
        let mut restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(rebound.canonical_state(), restored.canonical_state());
        rebound.advance(SimDuration(120_000)).unwrap();
        restored.advance(SimDuration(120_000)).unwrap();
        assert_eq!(rebound.canonical_state(), restored.canonical_state());
    }
}
