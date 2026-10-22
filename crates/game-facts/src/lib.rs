//! Shared immutable observations, with character-local evidence and cold history.
use game_core::*;
use game_storage::Table;
use serde::{Deserialize, Serialize, ser::SerializeMap};
use std::{ops::Bound, sync::Arc};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact<V> {
    pub value: Arc<V>,
    pub source: PersonId,
    pub observed: SimTime,
    pub revision: u64,
    pub confidence: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub source: PersonId,
    pub observed: SimTime,
    pub revision: u64,
    pub confidence: u8,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct FactRef<'a, V> {
    pub value: &'a Arc<V>,
    pub source: PersonId,
    pub observed: SimTime,
    pub revision: u64,
    pub confidence: u8,
}
impl<V> FactRef<'_, V> {
    pub fn to_owned(&self) -> Fact<V> {
        Fact {
            value: self.value.clone(),
            source: self.source,
            observed: self.observed,
            revision: self.revision,
            confidence: self.confidence,
        }
    }
}
impl<V> Fact<V> {
    fn view(&self) -> FactRef<'_, V> {
        FactRef {
            value: &self.value,
            source: self.source,
            observed: self.observed,
            revision: self.revision,
            confidence: self.confidence,
        }
    }
}
/// One view of visible values. Equal key layouts share their immutable key array.
#[derive(Clone, Debug)]
pub struct Scene<V> {
    keys: Arc<[Arc<str>]>,
    values: Box<[Arc<V>]>,
}
impl<V> Scene<V> {
    pub fn new(mut entries: Vec<(Arc<str>, Arc<V>)>, previous: Option<&Self>) -> Result<Self>
    where
        V: PartialEq,
    {
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        require(
            entries.windows(2).all(|w| w[0].0 < w[1].0),
            "duplicate scene key",
        )?;
        if let Some(old) = previous {
            for (key, value) in &mut entries {
                if let Some(i) = old.find(key)
                    && old.values[i].as_ref() == value.as_ref()
                {
                    *value = old.values[i].clone();
                }
            }
        }
        let (keys, values): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
        let keys = previous
            .filter(|old| old.keys.as_ref() == keys.as_slice())
            .map(|old| old.keys.clone())
            .unwrap_or_else(|| keys.into());
        Ok(Self {
            keys,
            values: values.into_boxed_slice(),
        })
    }
    pub fn entries(&self) -> impl Iterator<Item = (&Arc<str>, &Arc<V>)> {
        self.keys.iter().zip(self.values.iter())
    }
    fn find(&self, key: &str) -> Option<usize> {
        if self.keys.first().is_none_or(|k| key < k.as_ref())
            || self.keys.last().is_some_and(|k| key > k.as_ref())
        {
            return None;
        }
        self.keys.binary_search_by(|k| k.as_ref().cmp(key)).ok()
    }
}
#[derive(Clone, Debug)]
struct Observed<V> {
    scene: Arc<Scene<V>>,
    stamp: Stamp,
}
impl<V> Observed<V> {
    fn fact(&self, i: usize) -> FactRef<'_, V> {
        FactRef {
            value: &self.scene.values[i],
            source: self.stamp.source,
            observed: self.stamp.observed,
            revision: self.stamp.revision,
            confidence: self.stamp.confidence,
        }
    }
}
/// Current scene plus sparse explicit facts. Neither view mutates shared history.
#[derive(Clone, Debug)]
pub struct Facts<V> {
    cold: Table<Arc<str>, Fact<V>>,
    local: Option<Observed<V>>,
    overrides: usize,
    payload_version: Arc<()>,
    namespaces: Table<u8, Arc<()>>,
}
impl<V> Default for Facts<V> {
    fn default() -> Self {
        Self {
            cold: Table::new(),
            local: None,
            overrides: 0,
            payload_version: Arc::new(()),
            namespaces: Table::new(),
        }
    }
}
impl<V: Clone> Facts<V> {
    pub fn insert(&mut self, key: Arc<str>, fact: Fact<V>) -> Option<Fact<V>> {
        if self
            .get(&key)
            .is_none_or(|old| !Arc::ptr_eq(old.value, &fact.value))
        {
            self.payload_version = Arc::new(());
            self.touch(1u64 << namespace(&key));
        }
        let local = self
            .local
            .as_ref()
            .and_then(|o| o.scene.find(&key).map(|i| o.fact(i).to_owned()));
        let old = self.cold.insert(key, fact);
        if local.is_some() && old.is_none() {
            self.overrides += 1;
        }
        old.or(local)
    }
    /// Forget a personal entry without changing a scene shared with another observer.
    pub fn remove(&mut self, key: &str) -> Option<Fact<V>> {
        let old = self.get(key)?.to_owned();
        if self
            .local
            .as_ref()
            .is_some_and(|o| o.scene.find(key).is_some())
        {
            let local = self.local.take().unwrap();
            for (i, k) in local.scene.keys.iter().enumerate() {
                if k.as_ref() != key && !self.cold.contains_key(k.as_ref()) {
                    self.cold.insert(k.clone(), local.fact(i).to_owned());
                }
            }
            self.overrides = 0;
        }
        self.cold.remove(key);
        self.payload_version = Arc::new(());
        self.touch(1u64 << namespace(key));
        Some(old)
    }
    /// Refresh an entire unchanged scene by replacing only one personal stamp.
    pub fn observe(&mut self, scene: Arc<Scene<V>>, stamp: Stamp) {
        if self
            .local
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(&old.scene, &scene))
            || self.overrides > 0
        {
            let mask = scene
                .keys
                .iter()
                .zip(scene.values.iter())
                .filter(|(key, value)| {
                    self.get(key)
                        .is_none_or(|old| !Arc::ptr_eq(old.value, value))
                })
                .fold(0, |mask, (key, _)| mask | (1u64 << namespace(key)));
            if mask != 0 {
                self.payload_version = Arc::new(());
                self.touch(mask);
            }
        }
        let same = self
            .local
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(&old.scene.keys, &scene.keys));
        if !same && let Some(old) = self.local.take() {
            for (i, key) in old.scene.keys.iter().enumerate() {
                if !self.cold.contains_key(key.as_ref()) {
                    self.cold.insert(key.clone(), old.fact(i).to_owned());
                }
            }
        }
        if !same || self.overrides > 0 {
            for key in scene.keys.iter() {
                self.cold.remove(key.as_ref());
            }
        }
        self.local = Some(Observed { scene, stamp });
        self.overrides = 0;
    }
    pub fn valid_confidence(&self, previous: Option<&Self>) -> bool {
        let local = self
            .local
            .as_ref()
            .is_none_or(|o| o.stamp.confidence <= 100);
        local
            && match previous {
                Some(old) => self
                    .cold
                    .changed(&old.cold)
                    .all(|(_, f)| f.confidence <= 100),
                None => self.cold.values().all(|f| f.confidence <= 100),
            }
    }
}
fn namespace(key: &str) -> u8 {
    (key.bytes()
        .take_while(|b| *b != b':')
        .fold(0u64, |n, b| n.wrapping_mul(31).wrapping_add(u64::from(b)))
        % 64) as u8
}
impl<V> Facts<V> {
    fn touch(&mut self, mut mask: u64) {
        while mask != 0 {
            let slot = mask.trailing_zeros() as u8;
            self.namespaces.insert(slot, Arc::new(()));
            mask &= mask - 1;
        }
    }
    /// Hash collisions conservatively invalidate additional namespaces.
    pub fn namespace_version(&self, prefix: &str) -> std::sync::Weak<()> {
        self.namespaces
            .get(&namespace(prefix))
            .map(Arc::downgrade)
            .unwrap_or_default()
    }
    pub fn same_namespace(&self, old: &Self, prefix: &str) -> bool {
        let slot = namespace(prefix);
        self.namespaces.get(&slot).map(Arc::as_ptr) == old.namespaces.get(&slot).map(Arc::as_ptr)
    }
    pub fn valid_at(&self, at: SimTime, revision: u64) -> bool {
        self.values()
            .all(|f| f.observed <= at && f.revision <= revision)
    }
    /// Visit changed payloads/removals, not metadata-only refreshes; equal copies may be visited.
    pub fn changed_values(
        &self,
        previous: &Self,
        mut visit: impl FnMut(&Arc<str>, Option<&Arc<V>>),
    ) {
        if Arc::ptr_eq(&self.payload_version, &previous.payload_version) {
            return;
        }
        let mut keys: Vec<_> = self
            .cold
            .changed_pairs(&previous.cold)
            .filter(|(_, value, old)| old.is_none_or(|old| !Arc::ptr_eq(&value.value, &old.value)))
            .map(|(k, _, _)| k.clone())
            .chain(
                previous
                    .cold
                    .changed_pairs(&self.cold)
                    .filter(|(_, _, current)| current.is_none())
                    .map(|(k, _, _)| k.clone()),
            )
            .collect();
        let same_scene = match (&self.local, &previous.local) {
            (Some(a), Some(b)) => Arc::ptr_eq(&a.scene, &b.scene),
            (None, None) => true,
            _ => false,
        };
        if !same_scene {
            for local in [&self.local, &previous.local].into_iter().flatten() {
                keys.extend(local.scene.keys.iter().cloned());
            }
        }
        keys.sort_unstable();
        keys.dedup();
        for key in keys {
            let new = self.get(&key).map(|f| f.value);
            let old = previous.get(&key).map(|f| f.value);
            let same = match (new, old) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                visit(&key, new);
            }
        }
    }
    pub fn get(&self, key: &str) -> Option<FactRef<'_, V>> {
        if let Some(local) = &self.local
            && let Some(i) = local.scene.find(key)
        {
            if self.overrides > 0
                && let Some(fact) = self.cold.get(key)
            {
                return Some(fact.view());
            }
            return Some(local.fact(i));
        }
        self.cold.get(key).map(Fact::view)
    }
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    pub fn len(&self) -> usize {
        self.cold.len() + self.local.as_ref().map_or(0, |o| o.scene.keys.len()) - self.overrides
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn iter(&self) -> impl Iterator<Item = (&Arc<str>, FactRef<'_, V>)> {
        self.range((Bound::Unbounded, Bound::Unbounded))
    }
    pub fn values(&self) -> impl Iterator<Item = FactRef<'_, V>> {
        self.iter().map(|(_, f)| f)
    }
    pub fn keys(&self) -> impl Iterator<Item = &Arc<str>> {
        self.iter().map(|(k, _)| k)
    }
    pub fn range<'a>(
        &'a self,
        bounds: (Bound<&'a str>, Bound<&'a str>),
    ) -> impl Iterator<Item = (&'a Arc<str>, FactRef<'a, V>)> {
        let mut cold = self.cold.range(bounds).peekable();
        let local = self.local.as_ref();
        let mut at = local.map_or(0, |o| match bounds.0 {
            Bound::Unbounded => 0,
            Bound::Included(k) => o.scene.keys.partition_point(|s| s.as_ref() < k),
            Bound::Excluded(k) => o.scene.keys.partition_point(|s| s.as_ref() <= k),
        });
        std::iter::from_fn(move || {
            let next = local.filter(|o| at < o.scene.keys.len()).and_then(|o| {
                let key = &o.scene.keys[at];
                let included = match bounds.1 {
                    Bound::Unbounded => true,
                    Bound::Included(k) => key.as_ref() <= k,
                    Bound::Excluded(k) => key.as_ref() < k,
                };
                included.then(|| (key, o.fact(at)))
            });
            match (cold.peek(), next) {
                (Some((k, _)), Some((key, f))) if *k > key => {
                    at += 1;
                    Some((key, f))
                }
                (Some((k, _)), Some((key, _))) => {
                    if *k == key {
                        at += 1;
                    }
                    cold.next().map(|(k, f)| (k, f.view()))
                }
                (None, Some((key, f))) => {
                    at += 1;
                    Some((key, f))
                }
                (_, None) => cold.next().map(|(k, f)| (k, f.view())),
            }
        })
    }
}
/// Seek directly to a key namespace without visiting unrelated memories.
pub fn with_prefix<'a, V>(
    facts: &'a Facts<V>,
    prefix: &'a str,
) -> impl Iterator<Item = (&'a Arc<str>, FactRef<'a, V>)> {
    facts
        .range((Bound::Included(prefix), Bound::Unbounded))
        .take_while(move |(key, _)| key.starts_with(prefix))
}
/// Decode the typed payload first, borrowing its JSON form when already available.
pub fn decode<T: serde::de::DeserializeOwned + Clone + 'static>(
    typed: &dyn std::any::Any,
    wire: &impl Serialize,
    json: Option<&serde_json::Value>,
) -> Option<T> {
    if let Some(value) = typed.downcast_ref::<T>() {
        return Some(value.clone());
    }
    match json {
        Some(value) => T::deserialize(value).ok(),
        None => T::deserialize(&serde_json::to_value(wire).ok()?).ok(),
    }
}
impl<V: PartialEq> PartialEq for Facts<V> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}
impl<V: Eq> Eq for Facts<V> {}
impl<V: Serialize> Serialize for Facts<V> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (key, fact) in self.iter() {
            map.serialize_entry(key, &fact)?;
        }
        map.end()
    }
}
impl<V: PartialEq> PartialEq<Fact<V>> for FactRef<'_, V> {
    fn eq(&self, other: &Fact<V>) -> bool {
        self.value == &other.value
            && self.source == other.source
            && self.observed == other.observed
            && self.revision == other.revision
            && self.confidence == other.confidence
    }
}
/// Compare logical payloads even when an owner retains a typed form beside opaque saved JSON.
pub fn same_value<V: Eq + Serialize>(a: &V, b: &V) -> bool {
    a == b || matches!((serde_json::to_value(a), serde_json::to_value(b)), (Ok(a), Ok(b)) if a == b)
}

/// Prefer a lossless compact schema while retaining every other JSON value unchanged.
pub fn deserialize_preferred<'de, D, T, R>(
    d: D,
    specific: impl FnOnce(T) -> R,
    other: impl FnOnce(serde_json::Value) -> R,
) -> std::result::Result<R, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Preferred<T> {
        Specific(T),
        Other(serde_json::Value),
    }
    Preferred::deserialize(d).map(|value| match value {
        Preferred::Specific(v) => specific(v),
        Preferred::Other(v) => other(v),
    })
}

impl<V: Serialize> Serialize for Scene<V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.entries().collect::<Vec<_>>().serialize(s)
    }
}
impl<'de, V: Deserialize<'de> + PartialEq> Deserialize<'de> for Scene<V> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Self::new(Vec::<(Arc<str>, Arc<V>)>::deserialize(d)?, None)
            .map_err(serde::de::Error::custom)
    }
}

/// Payload-specific compact interpretation, independent of cognitive retention rules.
pub trait Interpretation:
    Clone
    + std::fmt::Debug
    + Eq
    + std::hash::Hash
    + Serialize
    + serde::de::DeserializeOwned
    + Send
    + Sync
    + 'static
{
    fn promote(value: Arc<Self>, key: &str) -> Arc<Self>;
}
#[derive(Clone, Debug)]
pub struct Interpreted<V: Interpretation>(Facts<V>);
impl<V: Interpretation> Default for Interpreted<V> {
    fn default() -> Self {
        Self(Default::default())
    }
}
impl<V: Interpretation> Interpreted<V> {
    pub fn from_facts(facts: Facts<V>) -> Self {
        Self(facts)
    }
    pub fn as_facts(&self) -> &Facts<V> {
        &self.0
    }
    pub fn observe(&mut self, scene: Arc<Scene<V>>, stamp: Stamp) {
        self.0.observe(scene, stamp);
    }
    pub fn remove(&mut self, key: &str) -> Option<Fact<V>> {
        self.0.remove(key)
    }

    pub fn insert(&mut self, key: Arc<str>, mut fact: Fact<V>) -> Option<Fact<V>> {
        fact.value = V::promote(fact.value, &key);
        self.0.insert(key, fact)
    }
}
impl<V: Interpretation> std::ops::Deref for Interpreted<V> {
    type Target = Facts<V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<V: Interpretation> FromIterator<(Arc<str>, Fact<V>)> for Interpreted<V> {
    fn from_iter<I: IntoIterator<Item = (Arc<str>, Fact<V>)>>(rows: I) -> Self {
        let mut facts = Self::default();
        for (k, f) in rows {
            facts.insert(k, f);
        }
        facts
    }
}
impl<V: Interpretation> PartialEq for Interpreted<V> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<V: Interpretation> Eq for Interpreted<V> {}

impl<V: Interpretation> Serialize for Interpreted<V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de, V: Interpretation> Deserialize<'de> for Interpreted<V> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(
            std::collections::BTreeMap::<Arc<str>, Fact<V>>::deserialize(d)?
                .into_iter()
                .collect(),
        )
    }
}
