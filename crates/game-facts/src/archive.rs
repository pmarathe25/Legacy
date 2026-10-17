//! Lossless shared dictionaries for collections of personal evidence.
use crate::*;
use game_storage::Table;
use std::collections::HashMap;
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
        self,
        mut promote: impl FnMut(&str, Arc<V>) -> Arc<V>,
    ) -> Result<Vec<(PersonId, Facts<V>)>> {
        require(self.version == 1, "unknown evidence archive version")?;
        let mut promoted = HashMap::new();
        let mut people = vec![];
        for (person, rows) in &self.people {
            let person = *person;
            let mut facts = Facts::default();
            for &(k, v, source, observed, revision, confidence) in rows {
                let key = self
                    .keys
                    .get(k)
                    .ok_or_else(|| Error::Rejected("invalid archived fact key".into()))?;
                let value = self
                    .values
                    .get(v)
                    .ok_or_else(|| Error::Rejected("invalid archived fact value".into()))?;
                // Promotion depends on the namespace as well as the payload.
                let value = promoted
                    .entry((k, v))
                    .or_insert_with(|| promote(key, value.clone()))
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
