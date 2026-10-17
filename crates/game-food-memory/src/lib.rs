//! Derived indexes of personal evidence. Queries cannot inspect actual stocks.
pub mod assessment;
mod remembered;
use game_core::*;
use game_knowledge::{Facts, StoreLocation, known, location_key, store_key};
pub use remembered::remembered_meal;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Weak},
};

#[derive(Clone, Debug)]
pub struct Store {
    pub id: InventoryId,
    pub location: Option<StoreLocation>,
}
#[derive(Default)]
pub struct Stores {
    all: BTreeMap<Arc<str>, Arc<Store>>,
    keys: BTreeMap<InventoryId, BTreeSet<Arc<str>>>,
    meals: BTreeMap<Arc<str>, Arc<Store>>,
    carried: BTreeMap<PersonId, BTreeMap<Arc<str>, Arc<Store>>>,
}
impl Stores {
    pub fn all(&self) -> impl Iterator<Item = &Store> {
        self.all.values().map(Arc::as_ref)
    }
    pub fn meals(&self) -> impl Iterator<Item = &Store> {
        self.meals.values().map(Arc::as_ref)
    }
    pub fn meal(&self, id: InventoryId) -> Option<&Store> {
        self.meals.get(store_key(id).as_str()).map(Arc::as_ref)
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
            let meal = facts
                .get(&store_key(id))
                .is_some_and(|f| remembered_meal(objects, f.value));
            for key in keys {
                self.remove(&key);
                let store = Arc::new(Store {
                    id,
                    location: location.clone(),
                });
                if meal {
                    self.meals.insert(key.clone(), store.clone());
                }
                if let Some(carrier) = location.as_ref().and_then(|l| l.carrier) {
                    self.carried
                        .entry(carrier)
                        .or_default()
                        .insert(key.clone(), store.clone());
                }
                self.all.insert(key, store);
            }
        }
    }
}
#[derive(Default)]
struct Entry {
    facts: Facts,
    stores: Stores,
    rules: Weak<BTreeMap<GoodId, game_objects::Food>>,
}
#[derive(Default)]
struct Cache {
    entries: BTreeMap<PersonId, (u64, Entry)>,
    clock: u64,
    active_limit: usize,
}
thread_local! {static CACHE: RefCell<Cache> = RefCell::new(Cache::default());}
/// Keep projections for the current living population. A fixed actor limit
/// otherwise causes every review to rebuild indexes once the population grows.
pub fn retain_people(people: impl Iterator<Item = PersonId>) {
    let people: BTreeSet<_> = people.collect();
    CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        cache.entries.retain(|p, _| people.contains(p));
        cache.active_limit = people.len().max(1024);
    });
}
/// The index ignores evidence metadata because only payloads determine these
/// physical possibilities. Assessment of confidence or age must read the facts.
pub fn with_stores<T>(
    person: PersonId,
    facts: &Facts,
    objects: &game_objects::Objects,
    query: impl FnOnce(&Stores) -> T,
) -> T {
    let (mut entry, used) = CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        cache.clock = cache.clock.saturating_add(1);
        let used = cache.clock;
        if let Some((_, entry)) = cache.entries.remove(&person) {
            return (entry, used);
        }
        (Entry::default(), used)
    });
    if entry.rules.as_ptr() != Arc::as_ptr(objects.food_rules()) {
        entry = Entry {
            rules: Arc::downgrade(objects.food_rules()),
            ..Default::default()
        };
    }
    let mut changed = vec![];
    facts.changed_values(&entry.facts, |key, _| {
        if key.starts_with("store:") || key.starts_with("store-location:") {
            changed.push(key.clone());
        }
    });
    entry.stores.refresh(facts, objects, changed);
    entry.facts = facts.clone();
    let result = query(&entry.stores);
    CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        if cache.entries.len() >= cache.active_limit.max(1024)
            && !cache.entries.contains_key(&person)
        {
            let oldest = cache
                .entries
                .iter()
                .min_by_key(|(id, (used, _))| (*used, **id))
                .map(|(id, _)| *id)
                .unwrap();
            cache.entries.remove(&oldest);
        }
        cache.entries.insert(person, (used, entry));
    });
    result
}
