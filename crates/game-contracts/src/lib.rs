//! Runtime data contracts, erased resource codecs and registration metadata.
mod messages;
use game_core::*;
pub use messages::*;
use rustc_hash::FxHashMap as HashMap;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    any::{Any, TypeId},
    sync::Arc,
};
pub type Checkpoint = Box<dyn Any + Send + Sync>;
/// Mutable fields must clone independently; resource state cannot share mutable aliases.
pub trait Resource: Clone + Default + Serialize + DeserializeOwned + Send + Sync + 'static {
    const KEY: &'static str;
    /// Snapshot metadata or begin a row journal. These storage hooks must not run gameplay.
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new(self.clone())
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        *self = *checkpoint
            .downcast::<Self>()
            .expect("resource checkpoint type");
    }
    /// Infallible storage housekeeping; keep nested undo until the outer commit.
    fn commit(&mut self, _checkpoint: Checkpoint) {}
    fn validate_checkpoint(&self, checkpoint: &dyn Any) -> Result<()> {
        self.validate_changes(
            checkpoint
                .downcast_ref::<Self>()
                .expect("resource checkpoint type"),
        )
    }
    fn allocated_ids(&self) -> Vec<u64> {
        vec![]
    }
    fn validate_changes(&self, _previous: &Self) -> Result<()> {
        self.validate()
    }
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}
pub type Stored = Arc<dyn Any + Send + Sync>;
pub type Data = HashMap<TypeId, Stored>;
pub struct Codec {
    pub key: &'static str,
    pub owner: usize,
    pub encode: fn(&Stored, &mut dyn std::io::Write) -> Result<()>,
    pub decode: fn(&str) -> Result<Stored>,
    pub ids: fn(&Stored) -> Vec<u64>,
    pub validate: fn(&Stored) -> Result<()>,
    pub checkpoint: fn(&mut Stored) -> Checkpoint,
    pub restore: fn(&mut Stored, Checkpoint),
    pub commit: fn(&mut Stored, Checkpoint),
    pub validate_checkpoint: fn(&Stored, &dyn Any) -> Result<()>,
}
#[derive(Default)]
pub struct Catalog(HashMap<TypeId, Codec>);
impl std::ops::Deref for Catalog {
    type Target = HashMap<TypeId, Codec>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<'a> IntoIterator for &'a Catalog {
    type Item = (&'a TypeId, &'a Codec);
    type IntoIter = std::collections::hash_map::Iter<'a, TypeId, Codec>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
impl Catalog {
    pub fn validate(&self, data: &Data, next_id: u64) -> Result<std::collections::BTreeSet<u64>> {
        let mut seen = std::collections::BTreeSet::new();
        for (id, codec) in &self.0 {
            (codec.validate)(&data[id])?;
            for allocated in (codec.ids)(&data[id]) {
                require(
                    allocated > 0 && allocated < next_id && seen.insert(allocated),
                    "invalid or reused saved identity",
                )?;
            }
        }
        Ok(seen)
    }
    pub fn write_entries(&self, data: &Data, out: &mut dyn std::io::Write) -> Result<()> {
        let io = |e: std::io::Error| Error::InvalidSave(e.to_string());
        let mut codecs: Vec<_> = self.0.iter().collect();
        codecs.sort_by_key(|(_, c)| c.key);
        for (n, (id, codec)) in codecs.into_iter().enumerate() {
            if n > 0 {
                out.write_all(b",").map_err(&io)?;
            }
            serde_json::to_writer(&mut *out, codec.key).map_err(json_error)?;
            out.write_all(b":").map_err(&io)?;
            (codec.encode)(&data[id], out)?;
        }
        out.write_all(b"}").map_err(&io)?;
        Ok(())
    }
    pub fn restore(
        &self,
        mut resources: std::collections::BTreeMap<String, Stored>,
    ) -> Result<Data> {
        require(resources.len() == self.0.len(), "save resource set differs")?;
        let mut data = Data::default();
        for (id, codec) in &self.0 {
            let value = resources
                .remove(codec.key)
                .ok_or_else(|| Error::InvalidSave(format!("missing {}", codec.key)))?;
            data.insert(*id, value);
        }
        Ok(data)
    }
    pub fn register<T: Resource>(&mut self, owner: usize, data: &mut Data) -> Result<()> {
        if self.0.contains_key(&TypeId::of::<T>()) || self.0.values().any(|c| c.key == T::KEY) {
            return Err(Error::Duplicate(T::KEY.into()));
        }
        self.0.insert(
            TypeId::of::<T>(),
            Codec {
                key: T::KEY,
                owner,
                encode: |s, out| serde_json::to_writer(out, value::<T>(s)).map_err(json_error),
                decode: |v| {
                    let x: T = serde_json::from_str(v).map_err(json_error)?;
                    Ok(Arc::new(x))
                },
                ids: |s| value::<T>(s).allocated_ids(),
                validate: |s| value::<T>(s).validate(),
                checkpoint: |s| mutable::<T>(s).checkpoint(),
                restore: |s, checkpoint| mutable::<T>(s).restore(checkpoint),
                commit: |s, checkpoint| mutable::<T>(s).commit(checkpoint),
                validate_checkpoint: |s, checkpoint| value::<T>(s).validate_checkpoint(checkpoint),
            },
        );
        let initial = T::default();
        initial.validate()?;
        data.insert(TypeId::of::<T>(), Arc::new(initial));
        Ok(())
    }
}
pub fn json_error(e: impl std::fmt::Display) -> Error {
    Error::InvalidSave(e.to_string())
}
pub fn mutable<T: Resource>(stored: &mut Stored) -> &mut T {
    Arc::get_mut(stored)
        .expect("exclusive transaction resource")
        .downcast_mut()
        .expect("registered resource type")
}

fn value<T: Resource>(stored: &Stored) -> &T {
    stored
        .downcast_ref::<T>()
        .expect("registered resource type")
}
pub fn read<T: Resource>(data: &HashMap<TypeId, Stored>) -> Result<&T> {
    data.get(&TypeId::of::<T>())
        .and_then(|s| s.downcast_ref())
        .ok_or_else(|| Error::Missing(T::KEY.into()))
}
