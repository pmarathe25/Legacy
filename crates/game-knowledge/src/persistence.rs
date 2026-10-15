use super::*;
use serde::de::{DeserializeSeed, Error, MapAccess, Visitor};
use std::{collections::HashSet, fmt};
/// Rebuild shared keys and immutable payloads while streaming the save. Hash
/// collisions still compare complete values; source/time/confidence stay local.
#[derive(Default)]
struct Shared {
    keys: HashSet<Arc<str>>,
    values: HashSet<Arc<FactValue>>,
}
impl<'de> Deserialize<'de> for Knowledge {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct People;
        impl<'de> Visitor<'de> for People {
            type Value = Knowledge;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("personal knowledge")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Knowledge, A::Error> {
                let mut shared = Shared::default();
                let mut knowledge = Knowledge::default();
                while let Some(person) = map.next_key()? {
                    let facts = map.next_value_seed(Memories(&mut shared))?;
                    if knowledge.0.insert(person, facts).is_some() {
                        return Err(A::Error::custom("duplicate person"));
                    }
                }
                Ok(knowledge)
            }
        }
        deserializer.deserialize_map(People)
    }
}
struct Memories<'a>(&'a mut Shared);
impl<'de> DeserializeSeed<'de> for Memories<'_> {
    type Value = Facts;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<Facts, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Memories<'_> {
    type Value = Facts;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("unique facts")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Facts, A::Error> {
        let mut facts = Facts::default();
        while let Some((key, mut fact)) = map.next_entry::<Arc<str>, Fact>()? {
            let key = if let Some(old) = self.0.keys.get(&key) {
                old.clone()
            } else {
                self.0.keys.insert(key.clone());
                key
            };
            fact.value = FactValue::promote(fact.value, &key);
            if let Some(value) = self.0.values.get(&fact.value) {
                fact.value = value.clone();
            } else {
                self.0.values.insert(fact.value.clone());
            }
            if facts.insert(key, fact).is_some() {
                return Err(A::Error::custom("duplicate fact key"));
            }
        }
        Ok(facts)
    }
}

impl<'de> Deserialize<'de> for Facts {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Memories(&mut Shared::default()).deserialize(deserializer)
    }
}
