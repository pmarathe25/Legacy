//! Individual social connection needs, derived from committed contact and encounters.
use crate::{Contact, Encounter, Participant};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Needs {
    pub rules: NeedRules,
    #[serde(default)]
    pub people: Vec<Need>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedRules {
    pub capacity: u32,
    pub isolation_gain: u32,
    pub encounter_relief: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Need {
    pub person: u64,
    pub isolation: u32,
}
impl Needs {
    pub fn validate(&self, participants: &HashSet<u64>) -> Result<(), &'static str> {
        if self.rules.capacity == 0
            || self.rules.isolation_gain == 0
            || self.rules.encounter_relief == 0
        {
            return Err("invalid social need rules");
        }
        let mut people = HashSet::new();
        if self.people.iter().any(|row| {
            !participants.contains(&row.person)
                || row.isolation > self.rules.capacity
                || !people.insert(row.person)
        }) {
            return Err("invalid social need");
        }
        Ok(())
    }
    pub fn validate_sorted(&self, participants: &[u64]) -> Result<(), &'static str> {
        if self.rules.capacity == 0
            || self.rules.isolation_gain == 0
            || self.rules.encounter_relief == 0
        {
            return Err("invalid social need rules");
        }
        let rows = self.index();
        if rows.par_windows(2).any(|pair| pair[0].0 == pair[1].0)
            || rows.par_iter().any(|&(id, isolation)| {
                isolation > self.rules.capacity || participants.binary_search(&id).is_err()
            })
        {
            return Err("invalid social need");
        }
        Ok(())
    }
    pub fn index(&self) -> Vec<(u64, u32)> {
        let mut rows: Vec<_> = self
            .people
            .par_iter()
            .map(|row| (row.person, row.isolation))
            .collect();
        if rows.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            rows.par_sort_unstable_by_key(|row| row.0);
        }
        rows
    }
    /// Tick-start needs rise while alive, then the committed encounter relieves them.
    pub fn advance(
        &self,
        participants: &[Participant],
        contacts: &[Contact],
        events: &[Encounter],
    ) -> Self {
        let mut ids: Vec<_> = participants
            .par_iter()
            .map(|person| person.person)
            .collect();
        if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            ids.par_sort_unstable();
        }
        let mut people = std::borrow::Cow::Borrowed(contacts);
        if people
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            people.to_mut().par_sort_unstable_by_key(|p| p.person);
        }
        let old = self.index();
        let mut met: Vec<_> = events
            .par_iter()
            .flat_map_iter(|event| event.unpressured())
            .collect();
        if met.par_windows(2).any(|pair| pair[0] > pair[1]) {
            met.par_sort_unstable();
        }
        let rules = &self.rules;
        let people = &people;
        let old = &old;
        let met = &met;
        let rows: Vec<_> = ids
            .par_chunks(4096)
            .flat_map_iter(|chunk| {
                let mut a = people.partition_point(|p| p.person < chunk[0]);
                let mut b = old.partition_point(|p| p.0 < chunk[0]);
                let mut c = met.partition_point(|p| *p < chunk[0]);
                chunk.iter().filter_map(move |&id| {
                    while a < people.len() && people[a].person < id {
                        a += 1;
                    }
                    while b < old.len() && old[b].0 < id {
                        b += 1;
                    }
                    while c < met.len() && met[c] < id {
                        c += 1;
                    }
                    let mut isolation = old.get(b).filter(|row| row.0 == id).map_or(0, |row| row.1);
                    if people
                        .get(a)
                        .is_some_and(|person| person.person == id && person.health > 0)
                    {
                        isolation = isolation
                            .saturating_add(rules.isolation_gain)
                            .min(rules.capacity);
                        if met.get(c) == Some(&id) {
                            isolation = isolation.saturating_sub(rules.encounter_relief);
                        }
                    }
                    (isolation > 0).then_some(Need {
                        person: id,
                        isolation,
                    })
                })
            })
            .collect();
        Self {
            rules: self.rules.clone(),
            people: rows,
        }
    }
}
