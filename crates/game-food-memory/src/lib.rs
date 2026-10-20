//! Derived indexes of personal evidence. Queries cannot inspect actual stocks.
pub mod assessment;
mod projection;
mod remembered;
use game_core::*;
use game_knowledge::{Facts, StoreLocation, known, location_key, store_key};
pub use remembered::remembered_meal;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, MutexGuard, OnceLock, Weak},
};

#[derive(Clone, Debug)]
pub struct Store {
    pub id: InventoryId,
    pub location: Option<StoreLocation>,
    pub stock: Option<game_knowledge::Stock>,
    canonical_meal: bool,
}
#[derive(Clone, Default)]
pub struct Stores {
    all: BTreeMap<Arc<str>, Arc<Store>>,
    indexed: BTreeMap<InventoryId, Arc<Store>>,
    keys: BTreeMap<InventoryId, BTreeSet<Arc<str>>>,
    meals: BTreeMap<Arc<str>, Arc<Store>>,
    carried: BTreeMap<PersonId, BTreeMap<Arc<str>, Arc<Store>>>,
}
impl Stores {
    pub fn all(&self) -> impl Iterator<Item = &Store> {
        self.all.values().map(Arc::as_ref)
    }
    pub fn get(&self, id: InventoryId) -> Option<&Store> {
        self.indexed.get(&id).map(Arc::as_ref)
    }
    pub fn meals(&self) -> impl Iterator<Item = &Store> {
        self.meals.values().map(Arc::as_ref)
    }
    pub fn meal(&self, id: InventoryId) -> Option<&Store> {
        self.indexed
            .get(&id)
            .filter(|s| s.canonical_meal)
            .map(Arc::as_ref)
    }
    /// A deterministic recall window, drawn only from this person's remembered stores.
    pub fn meals_from(&self, cue: u64) -> impl Iterator<Item = &Store> {
        let maximum = self.keys.last_key_value().map_or(0, |(id, _)| id.0);
        let anchor: Arc<str> =
            store_key(InventoryId(cue % maximum.saturating_add(1).max(1))).into();
        self.meals
            .range((
                std::ops::Bound::Included(anchor),
                std::ops::Bound::Unbounded,
            ))
            .chain(self.meals.iter())
            .take(self.meals.len())
            .map(|(_, s)| s.as_ref())
    }
    pub fn carried(&self, person: PersonId) -> impl Iterator<Item = &Store> {
        self.carried
            .get(&person)
            .into_iter()
            .flat_map(|rows| rows.values())
            .map(Arc::as_ref)
    }
    /// Match a caller's physical candidates against known inventory keys,
    /// preserving the original evidence ordering and aliases.
    pub fn among(&self, candidates: impl IntoIterator<Item = InventoryId>) -> Vec<InventoryId> {
        let mut rows: Vec<_> = candidates
            .into_iter()
            .flat_map(|id| {
                self.keys
                    .get(&id)
                    .into_iter()
                    .flatten()
                    .map(move |key| (key, id))
            })
            .collect();
        rows.sort_unstable_by(|a, b| a.0.cmp(b.0));
        rows.into_iter().map(|(_, id)| id).collect()
    }
    fn remove(&mut self, key: &Arc<str>) {
        self.meals.remove(key);
        if let Some(old) = self.all.remove(key)
            && let Some(carrier) = old.location.as_ref().and_then(|l| l.carrier)
        {
            let keys = self.carried.get_mut(&carrier).unwrap();
            keys.remove(key);
            if keys.is_empty() {
                self.carried.remove(&carrier);
            }
        }
    }
    fn refresh(&mut self, facts: &Facts, objects: &game_objects::Objects, changed: Vec<Arc<str>>) {
        let mut ids = BTreeSet::new();
        for key in changed {
            if let Some(id) = key
                .strip_prefix("store:")
                .and_then(|s| s.parse::<u64>().ok())
            {
                let id = InventoryId(id);
                ids.insert(id);
                if facts.contains_key(&key) {
                    self.keys.entry(id).or_default().insert(key);
                } else {
                    self.remove(&key);
                    if let Some(keys) = self.keys.get_mut(&id) {
                        keys.remove(&key);
                        if keys.is_empty() {
                            self.keys.remove(&id);
                        }
                    }
                }
            } else if let Some(id) = key
                .strip_prefix("store-location:")
                .and_then(|s| s.parse::<u64>().ok())
            {
                ids.insert(InventoryId(id));
            }
        }
        for id in ids {
            self.indexed.remove(&id);
            let keys: Vec<_> = self.keys.get(&id).into_iter().flatten().cloned().collect();
            let absent = facts.get(&store_key(id)).is_some_and(|f| {
                matches!(f.value.as_ref(), game_knowledge::FactValue::Other(v)
                    if v.get("absent_from").is_some())
            });
            let location: Option<StoreLocation> = if absent {
                None
            } else {
                known(facts, &location_key(id))
            };
            // Noncanonical stock aliases use the canonical stock's evidence, as
            // acquisition always did; an alias alone cannot invent a meal.
            let stock = known::<game_knowledge::Stock>(facts, &store_key(id));
            let meal = stock.as_ref().is_some_and(|s| objects.has_meal(s));
            let canonical_meal = meal && keys.iter().any(|k| k.as_ref() == store_key(id));
            let store = Arc::new(Store {
                id,
                location: location.clone(),
                stock,
                canonical_meal,
            });
            if !keys.is_empty() {
                self.indexed.insert(id, store.clone());
            }
            for key in keys {
                self.remove(&key);
                if meal {
                    self.meals.insert(key.clone(), store.clone());
                }
                if let Some(carrier) = location.as_ref().and_then(|l| l.carrier) {
                    self.carried
                        .entry(carrier)
                        .or_default()
                        .insert(key.clone(), store.clone());
                }
                self.all.insert(key, store.clone());
            }
        }
    }
}
#[derive(Default)]
struct Entry {
    facts: Option<Facts>,
    versions: [Weak<()>; 2],
    payloads: Vec<(Arc<str>, Weak<game_knowledge::FactValue>)>,
    stores: Arc<Stores>,
    rules: Weak<BTreeMap<GoodId, game_objects::Food>>,
}
#[derive(Default)]
struct Cache {
    entries: BTreeMap<PersonId, (u64, Entry)>,
    clock: u64,
    active_limit: usize,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn cache() -> MutexGuard<'static, Cache> {
    let slot = CACHE.get_or_init(Default::default);
    slot.lock().unwrap_or_else(|poisoned| {
        let mut value = poisoned.into_inner();
        *value = Cache::default();
        slot.clear_poison();
        value
    })
}
/// Keep projections for the current living population. A fixed actor limit
/// otherwise causes every review to rebuild indexes once the population grows.
pub fn retain_people(people: impl Iterator<Item = PersonId>) {
    let people: BTreeSet<_> = people.collect();
    let mut cache = cache();
    cache.entries.retain(|p, _| people.contains(p));
    cache.active_limit = people.len().max(1024);
}
/// The index ignores evidence metadata because only payloads determine these
/// physical possibilities. Assessment of confidence or age must read the facts.
pub fn with_stores<T>(
    person: PersonId,
    facts: &Facts,
    objects: &game_objects::Objects,
    query: impl FnOnce(&Stores) -> T,
) -> T {
    let mut cache = cache();
    cache.clock = cache.clock.saturating_add(1);
    let used = cache.clock;
    if cache.entries.len() >= cache.active_limit.max(1024) && !cache.entries.contains_key(&person) {
        let oldest = cache
            .entries
            .iter()
            .min_by_key(|(id, (used, _))| (*used, **id))
            .map(|(id, _)| *id)
            .unwrap();
        cache.entries.remove(&oldest);
    }
    let (_, entry) = cache.entries.entry(person).or_default();
    if entry.rules.as_ptr() != Arc::as_ptr(objects.food_rules()) {
        *entry = Entry {
            rules: Arc::downgrade(objects.food_rules()),
            ..Default::default()
        };
    }
    let versions = [
        facts.namespace_version("store"),
        facts.namespace_version("store-location"),
    ];
    if versions
        .iter()
        .zip(&entry.versions)
        .any(|(new, old)| !new.ptr_eq(old))
    {
        let changed = projection::changed(entry, facts);
        Arc::make_mut(&mut entry.stores).refresh(facts, objects, changed);
        entry.versions = versions;
    }
    if entry.facts.is_some() {
        entry.facts = Some(facts.clone());
    }
    let stores = entry.stores.clone();
    cache.entries.get_mut(&person).unwrap().0 = used;
    drop(cache);
    query(&stores)
}
