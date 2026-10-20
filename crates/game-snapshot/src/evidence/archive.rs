//! Lossless shared dictionaries for collections of personal evidence.
use game_core::{Error, PersonId, Result, SimTime, require};
use game_facts::{Fact, Facts};
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
type Row = (usize, usize, PersonId, SimTime, u64, u8);
#[derive(Serialize, Deserialize)]
pub struct Archive<V> {
    version: u32,
    keys: Vec<Arc<str>>,
    values: Vec<Arc<V>>,
    people: Table<PersonId, Vec<Row>>,
}
impl<V: Eq + std::hash::Hash> Archive<V> {
    pub fn capture<'a>(people: impl Iterator<Item = (PersonId, &'a Facts<V>)>) -> Self
    where
        V: 'a,
    {
        let mut archive = Self {
            version: 1,
            keys: vec![],
            values: vec![],
            people: Table::new(),
        };
        let mut keys = HashMap::new();
        let mut values = HashMap::new();
        for (person, facts) in people {
            let rows = facts
                .iter()
                .map(|(key, f)| {
                    let k = *keys.entry(key.clone()).or_insert_with(|| {
                        archive.keys.push(key.clone());
                        archive.keys.len() - 1
                    });
                    let v = *values.entry(f.value.clone()).or_insert_with(|| {
                        archive.values.push(f.value.clone());
                        archive.values.len() - 1
                    });
                    (k, v, f.source, f.observed, f.revision, f.confidence)
                })
                .collect();
            archive.people.insert(person, rows);
        }
        archive
    }
}
impl<V: Clone> Archive<V> {
    pub fn restore(
        mut self,
        mut promote: impl FnMut(&str, Arc<V>) -> Arc<V>,
    ) -> Result<Vec<(PersonId, Facts<V>)>> {
        require(self.version == 1, "unknown evidence archive version")?;
        let pairs: HashSet<_> = self.people.values().flatten().map(|r| (r.0, r.1)).collect();
        let mut remaining = vec![0usize; self.values.len()];
        for (key, value) in pairs {
            require(key < self.keys.len(), "invalid archived fact key")?;
            let count = remaining
                .get_mut(value)
                .ok_or_else(|| Error::Rejected("invalid archived fact value".into()))?;
            *count += 1;
        }
        let mut values: Vec<_> = self
            .values
            .into_iter()
            .enumerate()
            .map(|(i, value)| (remaining[i] > 0).then_some(value))
            .collect();
        let mut promoted = HashMap::new();
        let mut people = vec![];
        let ids: Vec<_> = self.people.keys().copied().collect();
        for person in ids {
            let rows = self.people.remove(&person).unwrap();
            let mut facts = Facts::default();
            for (k, v, source, observed, revision, confidence) in rows {
                let key = &self.keys[k];
                // Keep each original only until every required namespace has its own projection.
                let value = promoted
                    .entry((k, v))
                    .or_insert_with(|| {
                        let raw = values[v]
                            .as_ref()
                            .expect("unpromoted archive source")
                            .clone();
                        let value = promote(key, raw);
                        remaining[v] -= 1;
                        if remaining[v] == 0 {
                            values[v] = None;
                        }
                        value
                    })
                    .clone();
                require(confidence <= 100, "invalid archived confidence")?;
                require(
                    facts
                        .insert(
                            key.clone(),
                            Fact {
                                value,
                                source,
                                observed,
                                revision,
                                confidence,
                            },
                        )
                        .is_none(),
                    "duplicate archived fact key",
                )?;
            }
            people.push((person, facts));
        }
        Ok(people)
    }
}
