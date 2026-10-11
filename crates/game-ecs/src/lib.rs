//! Backend-independent typed storage, stable identity, and explicit save schemas.
mod backend;
use game_core::Error;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::value::RawValue;
use std::{collections::BTreeMap, fmt};

/// Plain feature-owned data; no engine derive or vendor trait is required.
pub trait Component: Clone + Eq + Serialize + DeserializeOwned + Send + Sync + 'static {}
impl<T: Clone + Eq + Serialize + DeserializeOwned + Send + Sync + 'static> Component for T {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StableId(pub u64);

#[derive(Clone, Default)]
pub struct Schema {
    components: BTreeMap<String, backend::Registration>,
}
impl Schema {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register<C: Component>(
        &mut self,
        key: &str,
        validate: fn(&C) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(&byte))
            || self.components.contains_key(key)
            || backend::is_stable_id::<C>()
            || self
                .components
                .values()
                .any(|registration| registration.matches::<C>())
        {
            return Err(Error::InvalidConfiguration(
                "duplicate or invalid component registration".into(),
            ));
        }
        self.components
            .insert(key.into(), backend::Registration::new(validate));
        Ok(())
    }
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(String::as_str)
    }
    pub fn is_registered<C: Component>(&self) -> bool {
        self.registration::<C>().is_ok()
    }
    fn registration<C: Component>(&self) -> Result<&backend::Registration, Error> {
        self.components
            .values()
            .find(|registration| registration.matches::<C>())
            .ok_or_else(|| Error::InvalidConfiguration("component type is not registered".into()))
    }
}
impl PartialEq for Schema {
    fn eq(&self, other: &Self) -> bool {
        self.components.len() == other.components.len()
            && self
                .components
                .iter()
                .zip(&other.components)
                .all(|((a_key, a), (b_key, b))| a_key == b_key && a.same_type(b))
    }
}
impl Eq for Schema {}
impl fmt::Debug for Schema {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Schema")
            .field("keys", &self.components.keys().collect::<Vec<_>>())
            .finish()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentRow {
    pub entity: u64,
    pub value: Box<RawValue>,
}
impl PartialEq for ComponentRow {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity && self.value.get() == other.value.get()
    }
}
impl Eq for ComponentRow {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub entities: Vec<u64>,
    pub columns: BTreeMap<String, Vec<ComponentRow>>,
}

pub struct Store {
    backend: backend::Storage,
}
impl Store {
    pub fn with_schema(schema: Schema) -> Self {
        Self {
            backend: backend::Storage::new(schema),
        }
    }
    pub fn schema(&self) -> &Schema {
        self.backend.schema()
    }
    pub fn len(&self) -> usize {
        self.backend.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn ids(&self) -> impl Iterator<Item = StableId> + '_ {
        self.backend.ids()
    }
    pub fn contains(&self, id: StableId) -> bool {
        self.backend.contains(id)
    }
    pub fn spawn(&mut self, id: StableId) -> Result<(), Error> {
        self.backend.spawn(id)
    }
    pub fn ensure(&mut self, id: StableId) -> Result<(), Error> {
        if self.contains(id) {
            Ok(())
        } else {
            self.spawn(id)
        }
    }
    pub fn get<C: Component>(&self, id: StableId) -> Option<&C> {
        self.backend.get(id)
    }
    /// Exclusive typed access for an owning feature's prevalidated commit.
    pub fn get_mut<C: Component>(&mut self, id: StableId) -> Option<&mut C> {
        self.backend.get_mut(id)
    }
    pub fn insert<C: Component>(&mut self, id: StableId, component: C) -> Result<(), Error> {
        self.backend.insert(id, component)
    }
    /// Validate first, then attach the component, creating its entity if needed.
    /// A rejected component or invalid ID leaves both data and entity membership unchanged.
    pub fn insert_or_spawn<C: Component>(
        &mut self,
        id: StableId,
        component: C,
    ) -> Result<(), Error> {
        self.backend.insert_or_spawn(id, component)
    }
    /// Attach a pair atomically, including validation and optional entity creation.
    pub fn insert_pair_or_spawn<A: Component, B: Component>(
        &mut self,
        id: StableId,
        first: A,
        second: B,
    ) -> Result<(), Error> {
        self.backend.insert_pair_or_spawn(id, first, second)
    }
    pub fn remove<C: Component>(&mut self, id: StableId) -> Result<Option<C>, Error> {
        self.backend.remove(id)
    }
    pub fn iter<C: Component>(&self) -> impl Iterator<Item = (StableId, &C)> {
        self.backend.iter()
    }
    pub fn query2<A: Component, B: Component>(&mut self) -> Query2<A, B> {
        Query2 {
            backend: backend::Query2::new(&mut self.backend),
        }
    }
    pub fn query1<A: Component>(&mut self) -> Query1<A> {
        Query1 {
            backend: backend::Query1::new(&mut self.backend),
        }
    }
    pub fn query3<A: Component, B: Component, C: Component>(&mut self) -> Query3<A, B, C> {
        Query3 {
            backend: backend::Query3::new(&mut self.backend),
        }
    }
    /// Match both required components, retaining rows without the optional one.
    pub fn query2_optional<A: Component, B: Component, C: Component>(
        &mut self,
    ) -> Query2Optional<A, B, C> {
        Query2Optional {
            backend: backend::Query2Optional::new(&mut self.backend),
        }
    }
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        self.backend.snapshot()
    }
    /// Builds a separate validated store; a failed restore cannot touch live state.
    pub fn restore(schema: Schema, snapshot: Snapshot) -> Result<Self, Error> {
        Ok(Self {
            backend: backend::Storage::restore(schema, snapshot)?,
        })
    }
}
impl Clone for Store {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
        }
    }
}
impl PartialEq for Store {
    fn eq(&self, other: &Self) -> bool {
        self.backend == other.backend
    }
}
impl Eq for Store {}
impl fmt::Debug for Store {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Store")
            .field("entities", &self.ids().collect::<Vec<_>>())
            .field("schema", self.schema())
            .finish()
    }
}
/// A cached join. Refresh after adding/removing components or entities. Row order
/// is unspecified; authoritative ordering uses stable IDs, never engine storage.
pub struct Query1<A: Component> {
    backend: backend::Query1<A>,
}
impl<A: Component> Query1<A> {
    pub fn refresh(&mut self, store: &Store) -> Result<(), Error> {
        self.backend.refresh(&store.backend)
    }
    pub fn iter<'a>(
        &'a self,
        store: &'a Store,
    ) -> Result<impl Iterator<Item = (StableId, &'a A)> + 'a, Error> {
        self.backend.iter(&store.backend)
    }
}

pub struct Query2<A: Component, B: Component> {
    backend: backend::Query2<A, B>,
}
impl<A: Component, B: Component> Query2<A, B> {
    pub fn refresh(&mut self, store: &Store) -> Result<(), Error> {
        self.backend.refresh(&store.backend)
    }
    pub fn iter<'a>(
        &'a self,
        store: &'a Store,
    ) -> Result<impl Iterator<Item = (StableId, &'a A, &'a B)> + 'a, Error> {
        self.backend.iter(&store.backend)
    }
}
pub struct Query3<A: Component, B: Component, C: Component> {
    backend: backend::Query3<A, B, C>,
}
impl<A: Component, B: Component, C: Component> Query3<A, B, C> {
    pub fn refresh(&mut self, store: &Store) -> Result<(), Error> {
        self.backend.refresh(&store.backend)
    }
    pub fn iter<'a>(
        &'a self,
        store: &'a Store,
    ) -> Result<impl Iterator<Item = (StableId, &'a A, &'a B, &'a C)> + 'a, Error> {
        self.backend.iter(&store.backend)
    }
}

/// A cached join with two required components and one optional component.
/// Structural changes require refresh; row order is unspecified.
pub struct Query2Optional<A: Component, B: Component, C: Component> {
    backend: backend::Query2Optional<A, B, C>,
}
impl<A: Component, B: Component, C: Component> Query2Optional<A, B, C> {
    pub fn refresh(&mut self, store: &Store) -> Result<(), Error> {
        self.backend.refresh(&store.backend)
    }
    pub fn iter<'a>(
        &'a self,
        store: &'a Store,
    ) -> Result<impl Iterator<Item = (StableId, &'a A, &'a B, Option<&'a C>)> + 'a, Error> {
        self.backend.iter(&store.backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Count(u64);
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Name(String);
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Wide(u128);
    fn schema() -> Schema {
        let mut schema = Schema::new();
        schema
            .register::<Count>("fixture/count", |value| {
                if value.0 == 0 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        schema.register::<Name>("fixture/name", |_| Ok(())).unwrap();
        schema.register::<Wide>("fixture/wide", |_| Ok(())).unwrap();
        schema
    }
    fn fixture(reverse: bool) -> Store {
        let mut store = Store::with_schema(schema());
        let mut ids = vec![StableId(1), StableId(2), StableId(3)];
        if reverse {
            ids.reverse();
        }
        for id in ids {
            store.spawn(id).unwrap();
            store.insert(id, Count(u64::MAX - id.0)).unwrap();
        }
        store.insert(StableId(2), Name("Ada".into())).unwrap();
        store.insert(StableId(2), Wide(55)).unwrap();
        store
    }
    #[test]
    fn plain_components_join_without_engine_derives_or_native_handles() {
        let mut store = fixture(true);
        let query1 = store.query1::<Count>();
        assert_eq!(query1.iter(&store).unwrap().count(), 3);
        let query = store.query3::<Count, Name, Wide>();
        let rows: Vec<_> = query
            .iter(&store)
            .unwrap()
            .map(|(id, count, name, wide)| (id, count.0, name.0.clone(), wide.0))
            .collect();
        assert_eq!(rows, vec![(StableId(2), u64::MAX - 2, "Ada".into(), 55)]);
        let query2 = store.query2::<Count, Name>();
        assert_eq!(query2.iter(&store).unwrap().count(), 1);
    }
    #[test]
    fn optional_join_keeps_required_rows_and_tracks_optional_presence() {
        let mut store = fixture(true);
        store.insert(StableId(1), Name("Bo".into())).unwrap();
        let mut query = store.query2_optional::<Count, Name, Wide>();
        let rows = |query: &Query2Optional<Count, Name, Wide>, store: &Store| {
            let mut rows: Vec<_> = query
                .iter(store)
                .unwrap()
                .map(|(id, _, name, wide)| (id, name.0.clone(), wide.map(|v| v.0)))
                .collect();
            rows.sort_by_key(|row| row.0);
            rows
        };
        assert_eq!(
            rows(&query, &store),
            vec![
                (StableId(1), "Bo".into(), None),
                (StableId(2), "Ada".into(), Some(55))
            ]
        );
        store.insert(StableId(1), Wide(90)).unwrap();
        assert!(query.iter(&store).is_err());
        query.refresh(&store).unwrap();
        assert_eq!(rows(&query, &store)[0].2, Some(90));
        store.insert(StableId(1), Wide(91)).unwrap();
        assert_eq!(rows(&query, &store)[0].2, Some(91));
        store.remove::<Wide>(StableId(2)).unwrap();
        assert!(query.iter(&store).is_err());
        query.refresh(&store).unwrap();
        assert_eq!(rows(&query, &store)[1].2, None);
        store.remove::<Name>(StableId(1)).unwrap();
        assert!(query.iter(&store).is_err());
        query.refresh(&store).unwrap();
        assert_eq!(
            rows(&query, &store),
            vec![(StableId(2), "Ada".into(), None)]
        );
        let mut restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
        assert!(query.iter(&restored).is_err());
        assert!(query.refresh(&restored).is_err());
        let rebound = restored.query2_optional::<Count, Name, Wide>();
        assert_eq!(rows(&rebound, &restored), rows(&query, &store));
    }
    #[test]
    fn cached_queries_reject_structural_changes_until_explicit_refresh() {
        let mut store = fixture(false);
        let mut query = store.query2::<Count, Name>();
        store.insert(StableId(1), Name("Bo".into())).unwrap();
        assert!(query.iter(&store).is_err());
        query.refresh(&store).unwrap();
        assert_eq!(query.iter(&store).unwrap().count(), 2);
        store.remove::<Name>(StableId(2)).unwrap();
        assert!(query.iter(&store).is_err());
        query.refresh(&store).unwrap();
        assert_eq!(query.iter(&store).unwrap().count(), 1);
        store.spawn(StableId(8)).unwrap();
        assert!(query.iter(&store).is_err());
        query.refresh(&store).unwrap();
        assert_eq!(query.iter(&store).unwrap().count(), 1);
    }
    #[test]
    fn cached_queries_reject_other_stores_without_panicking() {
        let mut first = fixture(false);
        let second = first.clone();
        let mut query = first.query3::<Count, Name, Wide>();
        assert!(query.iter(&second).is_err());
        assert!(query.refresh(&second).is_err());
        assert_eq!(query.iter(&first).unwrap().count(), 1);
    }
    #[test]
    fn value_replacement_and_prevalidated_writes_preserve_query_cache() {
        let mut store = fixture(false);
        let query = store.query2::<Count, Name>();
        store.insert(StableId(2), Count(50)).unwrap();
        store.get_mut::<Name>(StableId(2)).unwrap().0 = "Cam".into();
        let (_, count, name) = query.iter(&store).unwrap().next().unwrap();
        assert_eq!(count.0, 50);
        assert_eq!(name.0, "Cam");
        assert_eq!(
            store
                .iter::<Count>()
                .map(|(id, count)| (id.0, count.0))
                .collect::<Vec<_>>(),
            vec![(1, u64::MAX - 1), (2, 50), (3, u64::MAX - 3)]
        );
    }
    #[test]
    fn failed_insert_and_registration_are_inert() {
        let mut store = fixture(false);
        let before = store.snapshot().unwrap();
        assert_eq!(store.insert(StableId(1), Count(0)), Err(Error::Unavailable));
        assert_eq!(
            store.insert(StableId(99), Count(5)),
            Err(Error::Unavailable)
        );
        assert_eq!(store.spawn(StableId(1)), Err(Error::SequenceConflict));
        assert!(store.spawn(StableId(0)).is_err());
        assert!(store.insert(StableId(1), StableId(2)).is_err());
        assert!(store.remove::<StableId>(StableId(1)).is_err());
        assert!(store.get_mut::<StableId>(StableId(1)).is_none());
        assert_eq!(store.snapshot().unwrap(), before);
        let mut schema = schema();
        assert!(schema.is_registered::<Count>());
        assert!(!schema.is_registered::<StableId>());
        assert!(
            schema
                .register::<Count>("different/count", |_| Ok(()))
                .is_err()
        );
        assert!(
            schema
                .register::<StableId>("internal/id", |_| Ok(()))
                .is_err()
        );
        assert_eq!(schema.keys().count(), 3);
    }
    #[test]
    fn creating_a_component_entity_validates_before_any_structural_change() {
        let mut store = fixture(false);
        let query = store.query2::<Count, Name>();
        let before = store.snapshot().unwrap();
        assert_eq!(
            store.insert_or_spawn(StableId(8), Count(0)),
            Err(Error::Unavailable)
        );
        assert!(store.insert_or_spawn(StableId(0), Count(10)).is_err());
        assert!(store.insert_or_spawn(StableId(8), StableId(9)).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(query.iter(&store).unwrap().count(), 1);
        store.insert_or_spawn(StableId(8), Count(10)).unwrap();
        assert!(query.iter(&store).is_err());
        assert_eq!(store.get::<Count>(StableId(8)), Some(&Count(10)));
        store.insert_or_spawn(StableId(2), Count(80)).unwrap();
        assert_eq!(store.get::<Count>(StableId(2)), Some(&Count(80)));
    }
    #[test]
    fn paired_components_reject_both_new_and_existing_changes_atomically() {
        let mut schema = schema();
        schema.components.remove("fixture/name");
        schema
            .register::<Name>("fixture/name", |name| {
                if name.0.is_empty() {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        store
            .insert_pair_or_spawn(StableId(1), Count(5), Name("Ada".into()))
            .unwrap();
        let before = store.snapshot().unwrap();
        let query = store.query2::<Count, Name>();
        for id in [StableId(1), StableId(2)] {
            assert_eq!(
                store.insert_pair_or_spawn(id, Count(9), Name(String::new())),
                Err(Error::Unavailable)
            );
            assert_eq!(store.snapshot().unwrap(), before);
            assert_eq!(query.iter(&store).unwrap().count(), 1);
        }
        assert!(
            store
                .insert_pair_or_spawn(StableId(2), Count(5), Count(6))
                .is_err()
        );
        assert!(
            store
                .insert_pair_or_spawn(StableId(0), Count(5), Name("Bo".into()))
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        store
            .insert_pair_or_spawn(StableId(1), Count(9), Name("Bo".into()))
            .unwrap();
        let (_, count, name) = query.iter(&store).unwrap().next().unwrap();
        assert_eq!(count.0, 9);
        assert_eq!(name.0, "Bo");
    }
    #[test]
    fn snapshot_and_clone_ignore_native_allocation_and_preserve_large_integers() {
        let first = fixture(false);
        let second = fixture(true);
        assert_eq!(first, second);
        assert_eq!(first, first.clone());
        let json = serde_json::to_string(&first.snapshot().unwrap()).unwrap();
        assert!(json.contains("18446744073709551614"));
        let restored = Store::restore(schema(), serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(
            restored.get::<Count>(StableId(1)),
            Some(&Count(u64::MAX - 1))
        );
        assert_eq!(restored, first);
    }
    #[test]
    fn sparse_presence_and_empty_entities_survive_restore() {
        let mut store = fixture(false);
        store.spawn(StableId(8)).unwrap();
        store.remove::<Count>(StableId(1)).unwrap();
        let restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
        assert_eq!(store, restored);
        assert!(restored.contains(StableId(8)));
        assert!(restored.get::<Count>(StableId(1)).is_none());
    }
    #[test]
    fn raw_component_columns_preserve_u128_without_float_conversion() {
        let mut store = fixture(false);
        store.insert(StableId(1), Wide(u128::MAX - 7)).unwrap();
        let json = serde_json::to_string(&store.snapshot().unwrap()).unwrap();
        assert!(json.contains("340282366920938463463374607431768211448"));
        let restored = Store::restore(schema(), serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(
            restored.get::<Wide>(StableId(1)),
            Some(&Wide(u128::MAX - 7))
        );
        assert_eq!(restored, store);
    }
    #[test]
    fn clone_copies_exact_values_without_validation_or_serialization() {
        let mut store = fixture(false);
        store.get_mut::<Count>(StableId(1)).unwrap().0 = 0;
        let cloned = store.clone();
        assert_eq!(cloned.get::<Count>(StableId(1)), Some(&Count(0)));
        assert_eq!(cloned, store);
        assert!(Store::restore(schema(), cloned.snapshot().unwrap()).is_err());
    }
    #[test]
    fn malformed_or_unknown_snapshot_is_rejected_without_touching_live_state() {
        let live = fixture(false);
        let before = live.snapshot().unwrap();
        let mutations: [fn(&mut Snapshot); 8] = [
            |snapshot| snapshot.version = 2,
            |snapshot| snapshot.entities.push(1),
            |snapshot| snapshot.entities[0] = 0,
            |snapshot| {
                snapshot.columns.insert("unknown/column".into(), vec![]);
            },
            |snapshot| {
                snapshot.columns.remove("fixture/name");
            },
            |snapshot| snapshot.columns.get_mut("fixture/count").unwrap()[0].entity = 99,
            |snapshot| {
                let row = snapshot.columns["fixture/count"][0].clone();
                snapshot.columns.get_mut("fixture/count").unwrap().push(row);
            },
            |snapshot| {
                snapshot.columns.get_mut("fixture/count").unwrap()[0].value =
                    RawValue::from_string("0".into()).unwrap()
            },
        ];
        for mutate in mutations {
            let mut snapshot = before.clone();
            mutate(&mut snapshot);
            assert!(matches!(
                Store::restore(schema(), snapshot),
                Err(Error::InvalidSave(_))
            ));
        }
        assert_eq!(live.snapshot().unwrap(), before);
    }
}
