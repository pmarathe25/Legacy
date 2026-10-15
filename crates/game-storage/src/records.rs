use crate::*;
use serde::{Deserialize, Serialize};
/// Sparse or heap-owning records share each value until that record changes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    transparent,
    bound(
        serialize = "K: Ord + Serialize, V: Serialize",
        deserialize = "K: Ord + Clone + Deserialize<'de>, V: Deserialize<'de>"
    )
)]
pub struct Records<K, V>(Table<K, Arc<V>>);
impl<K, V> Default for Records<K, V> {
    fn default() -> Self {
        Self(Table::default())
    }
}
impl<K: Ord + Clone, V: Clone> Records<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.get(key).map(AsRef::as_ref)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.0.get_mut(key).map(Arc::make_mut)
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0
            .insert(key, Arc::new(value))
            .map(Arc::unwrap_or_clone)
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.0.remove(key).map(Arc::unwrap_or_clone)
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&K, &V)> {
        self.0.iter().map(|(k, v)| (k, v.as_ref()))
    }
    pub fn scan_after<'a>(&'a self, key: Option<&'a K>) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.0.scan_after(key).map(|(k, v)| (k, v.as_ref()))
    }
    pub fn changed<'a>(&'a self, previous: &'a Self) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.0
            .changed(&previous.0)
            .filter(move |(k, v)| previous.0.get(k).is_none_or(|old| !Arc::ptr_eq(old, v)))
            .map(|(k, v)| (k, v.as_ref()))
    }
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> {
        self.0.keys()
    }
    pub fn values(&self) -> impl ExactSizeIterator<Item = &V> {
        self.0.values().map(AsRef::as_ref)
    }
    pub fn entry(&mut self, key: K) -> RecordEntry<'_, K, V> {
        RecordEntry { records: self, key }
    }
}
pub struct RecordEntry<'a, K, V> {
    records: &'a mut Records<K, V>,
    key: K,
}
impl<'a, K: Ord + Clone, V: Clone> RecordEntry<'a, K, V> {
    pub fn or_insert_with(self, f: impl FnOnce() -> V) -> &'a mut V {
        if !self.records.contains_key(&self.key) {
            self.records.insert(self.key.clone(), f());
        }
        self.records.get_mut(&self.key).expect("inserted")
    }
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}
impl<K: Ord + Clone, V: Clone> Index<&K> for Records<K, V> {
    type Output = V;
    fn index(&self, key: &K) -> &V {
        self.get(key).expect("missing record key")
    }
}
impl<K: Ord + Clone, V: Clone> FromIterator<(K, V)> for Records<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self(iter.into_iter().map(|(k, v)| (k, Arc::new(v))).collect())
    }
}
impl<K: Ord + Clone, V: Clone, const N: usize> From<[(K, V); N]> for Records<K, V> {
    fn from(rows: [(K, V); N]) -> Self {
        rows.into_iter().collect()
    }
}
impl<'a, K: Ord + Clone, V: Clone> IntoIterator for &'a Records<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::iter::Map<Iter<'a, K, Arc<V>>, fn((&'a K, &'a Arc<V>)) -> (&'a K, &'a V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, v)| (k, v.as_ref()))
    }
}
impl<K: Ord + Clone, V: Clone + PartialEq> PartialEq for Records<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<K: Ord + Clone, V: Clone + Eq> Eq for Records<K, V> {}

impl<K: Ord + Clone, V: Clone> From<std::collections::BTreeMap<K, V>> for Records<K, V> {
    fn from(rows: std::collections::BTreeMap<K, V>) -> Self {
        rows.into_iter().collect()
    }
}
