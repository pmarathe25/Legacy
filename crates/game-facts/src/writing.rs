//! Stream shared evidence without materializing every personal metadata row.
use crate::*;
use serde::ser::{SerializeMap, SerializeSeq, SerializeStruct};
use std::collections::HashMap;
pub struct View<'a, V>(Vec<(PersonId, &'a Facts<V>)>);
impl<'a, V> View<'a, V> {
    pub fn new(people: impl Iterator<Item = (PersonId, &'a Facts<V>)>) -> Self {
        let mut people: Vec<_> = people.collect();
        people.sort_by_key(|(p, _)| *p);
        Self(people)
    }
}
fn key_pointer(key: &Arc<str>) -> usize {
    Arc::as_ptr(key) as *const u8 as usize
}
impl<V: Serialize + Eq + std::hash::Hash> Serialize for View<'_, V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut keys = Vec::new();
        let mut values = Vec::new();
        let mut key_ids = HashMap::new();
        let mut value_ids = HashMap::new();
        let mut key_pointers = HashMap::new();
        let mut value_pointers = HashMap::new();
        for (_, facts) in &self.0 {
            for (key, fact) in facts.iter() {
                key_pointers.entry(key_pointer(key)).or_insert_with(|| {
                    *key_ids.entry(key.clone()).or_insert_with(|| {
                        keys.push(key.clone());
                        keys.len() - 1
                    })
                });
                value_pointers
                    .entry(Arc::as_ptr(fact.value) as usize)
                    .or_insert_with(|| {
                        *value_ids.entry(fact.value.clone()).or_insert_with(|| {
                            values.push(fact.value.clone());
                            values.len() - 1
                        })
                    });
            }
        }
        let mut archive = s.serialize_struct("Archive", 4)?;
        archive.serialize_field("version", &1u32)?;
        archive.serialize_field("keys", &keys)?;
        archive.serialize_field("values", &values)?;
        archive.serialize_field(
            "people",
            &People {
                people: &self.0,
                keys: &key_pointers,
                values: &value_pointers,
            },
        )?;
        archive.end()
    }
}
struct People<'a, 'b, V> {
    people: &'a [(PersonId, &'b Facts<V>)],
    keys: &'a HashMap<usize, usize>,
    values: &'a HashMap<usize, usize>,
}
impl<V> Serialize for People<'_, '_, V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut people = s.serialize_map(Some(self.people.len()))?;
        for (p, facts) in self.people {
            people.serialize_entry(
                p,
                &Rows {
                    facts,
                    keys: self.keys,
                    values: self.values,
                },
            )?;
        }
        people.end()
    }
}
struct Rows<'a, V> {
    facts: &'a Facts<V>,
    keys: &'a HashMap<usize, usize>,
    values: &'a HashMap<usize, usize>,
}
impl<V> Serialize for Rows<'_, V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut rows = s.serialize_seq(Some(self.facts.len()))?;
        for (key, f) in self.facts.iter() {
            rows.serialize_element(&(
                self.keys[&key_pointer(key)],
                self.values[&(Arc::as_ptr(f.value) as usize)],
                f.source,
                f.observed,
                f.revision,
                f.confidence,
            ))?;
        }
        rows.end()
    }
}
