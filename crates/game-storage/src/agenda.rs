use crate::*;
use serde::{Deserialize, Deserializer, Serialize};
/// One deadline per key, indexed by time and then stable key. The time index is
/// derived; snapshots and updates retain the table's bounded copying contract.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent, bound(serialize = "K: Ord + Serialize, T: Serialize"))]
pub struct Agenda<K, T> {
    deadlines: Table<K, T>,
    #[serde(skip)]
    times: Table<T, Table<K, ()>>,
}
impl<K, T> Default for Agenda<K, T> {
    fn default() -> Self {
        Self {
            deadlines: Table::default(),
            times: Table::default(),
        }
    }
}
impl<K: Ord + Clone, T: Ord + Clone> Agenda<K, T> {
    pub fn get(&self, key: &K) -> Option<&T> {
        self.deadlines.get(key)
    }
    pub fn set(&mut self, key: K, at: Option<T>) {
        if self.get(&key) == at.as_ref() {
            return;
        }
        if let Some(old) = self.deadlines.get(&key).cloned() {
            let keys = self.times.get_mut(&old).expect("indexed deadline");
            keys.remove(&key);
            if keys.is_empty() {
                self.times.remove(&old);
            }
        }
        if let Some(at) = at {
            self.deadlines.insert(key.clone(), at.clone());
            self.times.entry(at).or_default().insert(key, ());
        } else {
            self.deadlines.remove(&key);
        }
    }
    pub fn due<'a>(&'a self, at: &'a T) -> impl Iterator<Item = &'a K> {
        self.times
            .range((Bound::Unbounded, Bound::Included(at)))
            .flat_map(|(_, keys)| keys.keys())
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &T)> {
        self.deadlines.iter()
    }
    pub fn valid(&self) -> bool {
        let mut expected = Self::default();
        for (key, at) in &self.deadlines {
            expected.set(key.clone(), Some(at.clone()));
        }
        expected.times == self.times
    }
}
impl<'de, K: Ord + Clone + Deserialize<'de>, T: Ord + Clone + Deserialize<'de>> Deserialize<'de>
    for Agenda<K, T>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let deadlines = Table::<K, T>::deserialize(deserializer)?;
        let mut agenda = Self::default();
        for (key, at) in &deadlines {
            agenda.set(key.clone(), Some(at.clone()));
        }
        Ok(agenda)
    }
}

impl<K: Ord, T: PartialEq> PartialEq for Agenda<K, T> {
    fn eq(&self, other: &Self) -> bool {
        self.deadlines == other.deadlines
    }
}
impl<K: Ord, T: Eq> Eq for Agenda<K, T> {}
