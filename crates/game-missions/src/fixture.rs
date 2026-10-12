//! Independent owner tests compose the same runtime and native storage as a host.
use crate::*;
use game_ecs::{Schema, Store};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ops::{Deref, DerefMut};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixture {
    pub runtime: Runtime,
    pub store: Store,
}
impl Fixture {
    pub fn new() -> Self {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        Self {
            runtime: Runtime::default(),
            store: Store::with_schema(schema),
        }
    }
}
impl Deref for Fixture {
    type Target = Runtime;
    fn deref(&self) -> &Runtime {
        &self.runtime
    }
}
impl DerefMut for Fixture {
    fn deref_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }
}
#[derive(Serialize, Deserialize)]
struct Saved {
    runtime: Runtime,
    components: game_ecs::Snapshot,
}
impl Serialize for Fixture {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Saved {
            runtime: self.runtime.clone(),
            components: self.store.snapshot().map_err(serde::ser::Error::custom)?,
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Fixture {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = Saved::deserialize(deserializer)?;
        let mut schema = Schema::new();
        register_components(&mut schema).map_err(serde::de::Error::custom)?;
        Ok(Self {
            runtime: saved.runtime,
            store: Store::restore(schema, saved.components).map_err(serde::de::Error::custom)?,
        })
    }
}
