//! Numeric projections check the exact definition revision and immutable source profile.
use super::*;
use std::sync::Arc;
#[derive(Clone, Debug)]
struct Profile {
    source: Arc<BTreeMap<String, i32>>,
    values: Box<[(u64, i64)]>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Index {
    version: Option<game_storage::Version<String, Arc<Trait>>>,
    keys: rustc_hash::FxHashMap<String, usize>,
    defaults: Box<[(u64, i64)]>,
    people: Records<PersonId, Profile>,
}
fn values(
    definitions: &Records<String, Trait>,
    profile: Option<&BTreeMap<String, i32>>,
) -> Box<[(u64, i64)]> {
    definitions
        .iter()
        .map(|(key, t)| {
            let value = profile
                .and_then(|p| p.get(key))
                .copied()
                .unwrap_or(t.default);
            (
                value.max(0) as u64,
                (i64::from(value) - i64::from(t.minimum)) * 1000
                    / (i64::from(t.maximum) - i64::from(t.minimum)).max(1),
            )
        })
        .collect()
}
impl Index {
    pub fn new(traits: &Traits) -> Self {
        let mut index = Self {
            version: Some(traits.definitions.version()),
            keys: traits
                .definitions
                .keys()
                .enumerate()
                .map(|(i, key)| (key.clone(), i))
                .collect(),
            defaults: values(&traits.definitions, None),
            people: Records::new(),
        };
        for p in traits.people.keys() {
            index.set(&traits.definitions, *p, traits.people.shared(p).unwrap());
        }
        index
    }
    pub fn set(
        &mut self,
        definitions: &Records<String, Trait>,
        person: PersonId,
        source: Arc<BTreeMap<String, i32>>,
    ) {
        self.people.insert(
            person,
            Profile {
                values: values(definitions, Some(&source)),
                source,
            },
        );
    }
    pub fn get(&self, traits: &Traits, person: PersonId, key: &str) -> Option<(u64, i64)> {
        if !self
            .version
            .as_ref()
            .is_some_and(|v| traits.definitions.matches(v))
        {
            return None;
        }
        let slot = *self.keys.get(key)?;
        RECENT.with(|cache| {
            let mut cache = cache.borrow_mut();
            let entry = &mut cache[person.0 as usize % CACHE_SLOTS];
            if let Some(old) = entry
                && old.person == person
                && self.people.matches(&old.compiled)
                && traits.people.matches(&old.sources)
            {
                return old
                    .profile
                    .as_ref()
                    .map_or(&self.defaults, |p| &p.values)
                    .get(slot)
                    .copied();
            }
            let profile = if let Some(source) = traits.people.get(&person) {
                let compiled = self.people.shared(&person)?;
                if !std::ptr::eq(source, compiled.source.as_ref()) {
                    return None;
                }
                Some(compiled)
            } else {
                None
            };
            let result = profile
                .as_ref()
                .map_or(&self.defaults, |p| &p.values)
                .get(slot)
                .copied();
            *entry = Some(Recent {
                person,
                compiled: self.people.version(),
                sources: traits.people.version(),
                profile,
            });
            result
        })
    }
}
// Repeated attributes in one review share the exact checked personal projection.
// Weak table versions invalidate on direct edits, rollback, loading and world switches.
struct Recent {
    person: PersonId,
    compiled: game_storage::Version<PersonId, Arc<Profile>>,
    sources: game_storage::Version<PersonId, Arc<BTreeMap<String, i32>>>,
    profile: Option<Arc<Profile>>,
}
const CACHE_SLOTS: usize = 128;
thread_local! {
    static RECENT: std::cell::RefCell<[Option<Recent>; CACHE_SLOTS]> =
        const { std::cell::RefCell::new([const { None }; CACHE_SLOTS]) };
}
