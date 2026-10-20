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
impl Serialize for Knowledge {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let archive = game_snapshot::evidence::View::new(self.0.iter().map(|(p, f)| (*p, &f.0)));
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry("archive", &archive)?;
        map.end()
    }
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
                while let Some(key) = map.next_key::<String>()? {
                    if key == "archive" {
                        if !knowledge.0.is_empty() {
                            return Err(A::Error::custom("mixed knowledge formats"));
                        }
                        let archive =
                            map.next_value::<game_snapshot::evidence::Archive<FactValue>>()?;
                        for (p, f) in archive
                            .restore(|key, value| FactValue::promote(value, key))
                            .map_err(A::Error::custom)?
                        {
                            knowledge.0.insert(p, Facts(f, None));
                        }
                        if map.next_key::<String>()?.is_some() {
                            return Err(A::Error::custom("mixed knowledge formats"));
                        }
                        return Ok(knowledge);
                    }
                    let person = PersonId(key.parse().map_err(A::Error::custom)?);
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
