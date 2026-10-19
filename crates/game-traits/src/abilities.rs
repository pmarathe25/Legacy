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
        let values = if let Some(source) = traits.people.get(&person) {
            let compiled = self.people.get(&person)?;
            if !std::ptr::eq(source, compiled.source.as_ref()) {
                return None;
            }
            &compiled.values
        } else {
            &self.defaults
        };
        values.get(slot).copied()
    }
}
