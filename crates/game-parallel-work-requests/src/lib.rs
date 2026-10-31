//! Addressed requests for physical work; no agreement or organization is required.
mod experience;
pub use experience::{Memory, WorkLink};
use game_parallel_opinions::Opinions;
use game_parallel_production::Production;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub next_id: u64,
    /// Enroll newborns with general preferences, never another person's experiences.
    #[serde(default)]
    pub inherit_births: bool,
    /// Maximum personal work memories per observer; zero disables learning.
    #[serde(default)]
    pub memory_limit: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work_links: Vec<WorkLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memories: Vec<Memory>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub askers: Vec<Asker>,
    #[serde(default)]
    pub pending: Vec<Request>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Clone for Requests {
    fn clone(&self) -> Self {
        Self {
            next_id: self.next_id,
            inherit_births: self.inherit_births,
            memory_limit: self.memory_limit,
            work_links: self.work_links.par_iter().cloned().collect(),
            memories: self.memories.par_iter().cloned().collect(),
            preferences: self.preferences.par_iter().cloned().collect(),
            askers: self.askers.par_iter().cloned().collect(),
            pending: self.pending.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub effort: u64,
    #[serde(default)]
    pub help_value: u64,
    /// This person's signed motivation per unit of their own directed affinity.
    #[serde(default)]
    pub affinity_weight: i64,
    #[serde(default)]
    pub disregard_claims: bool,
}
impl Preference {
    /// No agreement or feeling forces compliance; this is the person's present evaluation.
    pub fn motivated(&self, fear: u64, obligation: u64, affinity: i32) -> bool {
        let value = i128::from(self.help_value)
            + i128::from(fear)
            + i128::from(obligation)
            + i128::from(affinity) * i128::from(self.affinity_weight);
        value >= i128::from(self.effort)
    }
}
/// Periodically ask a local person to help with an active personal goal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asker {
    pub person: u64,
    pub interval: u64,
    pub lifetime: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: u64,
    pub requester: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub created_at: u64,
    pub expires: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub request: u64,
    pub worker: u64,
    pub outcome: Outcome,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Started { job: u64 },
    Refused,
    Expired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub requester: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub expires: u64,
}
pub struct Decision {
    pub request: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub help: bool,
    pub disregard_claims: bool,
}
pub struct Draft {
    pub decisions: Vec<Decision>,
    pub expired: Vec<Event>,
}

impl Requests {
    /// Copy only personal inclinations for actual newborns; no pending requests or memories.
    pub fn enroll(&mut self, family: &game_parallel_families::Families, tick: u64) {
        if !self.inherit_births || !family.rules.enroll_births {
            return;
        }
        let newborns: Vec<_> = family
            .members
            .par_iter()
            .filter(|member| member.born == Some(tick))
            .collect();
        if newborns.is_empty() {
            return;
        }
        let mut preferences: Vec<_> = self.preferences.par_iter().collect();
        preferences.par_sort_unstable_by_key(|row| row.person);
        let mut askers: Vec<_> = self.askers.par_iter().collect();
        askers.par_sort_unstable_by_key(|row| row.person);
        let inherited_preferences: Vec<_> = newborns
            .par_iter()
            .filter_map(|member| {
                if preferences
                    .binary_search_by_key(&member.id, |row| row.person)
                    .is_ok()
                {
                    return None;
                }
                let parent = member.parents.iter().find_map(|id| {
                    preferences
                        .binary_search_by_key(id, |row| row.person)
                        .ok()
                        .map(|i| preferences[i])
                })?;
                let mut preference = parent.clone();
                preference.person = member.id;
                Some(preference)
            })
            .collect();
        let inherited_askers: Vec<_> = newborns
            .into_par_iter()
            .filter_map(|member| {
                if askers
                    .binary_search_by_key(&member.id, |row| row.person)
                    .is_ok()
                {
                    return None;
                }
                let parent = member.parents.iter().find_map(|id| {
                    askers
                        .binary_search_by_key(id, |row| row.person)
                        .ok()
                        .map(|i| askers[i])
                })?;
                let mut asker = parent.clone();
                asker.person = member.id;
                Some(asker)
            })
            .collect();
        self.preferences.par_extend(inherited_preferences);
        self.askers.par_extend(inherited_askers);
    }

    pub fn validate(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
    ) -> Result<(), String> {
        let mut preferences: Vec<_> = self.preferences.par_iter().map(|row| row.person).collect();
        preferences.par_sort_unstable();
        let mut askers: Vec<_> = self.askers.par_iter().map(|row| row.person).collect();
        askers.par_sort_unstable();
        let mut ids: Vec<_> = self.pending.par_iter().map(|row| row.id).collect();
        ids.par_sort_unstable();
        let recipes: HashSet<_> = production.recipes.iter().map(|row| row.id).collect();
        let regions: HashSet<_> = production
            .inventories
            .iter()
            .map(|row| row.region)
            .collect();
        if !self.valid_experience(people, &recipes, &regions)
            || self.next_id == 0
            || askers.par_windows(2).any(|row| row[0] == row[1])
            || self.askers.par_iter().any(|row| {
                row.interval == 0 || row.lifetime == 0 || person_index(people, row.person).is_none()
            })
            || preferences.par_windows(2).any(|w| w[0] == w[1])
            || preferences
                .par_iter()
                .any(|&person| person_index(people, person).is_none())
            || ids.par_windows(2).any(|w| w[0] == w[1])
            || self.pending.par_iter().any(|row| {
                row.id == 0
                    || row.id >= self.next_id
                    || row.requester == row.worker
                    || person_index(people, row.requester).is_none()
                    || person_index(people, row.worker).is_none()
                    || !recipes.contains(&row.recipe)
                    || !regions.contains(&row.region)
                    || row.expires < row.created_at
            })
            || self.events.par_iter().any(|row| {
                row.request == 0
                    || row.request >= self.next_id
                    || person_index(people, row.worker).is_none()
            })
        {
            return Err("invalid work requests".into());
        }
        Ok(())
    }
    /// Send an addressed request; receiving a demand does not imply approval.
    pub fn open(
        &mut self,
        proposal: Proposal,
        people: &[PersonSnapshot],
        production: &Production,
        now: u64,
    ) -> Result<u64, String> {
        Ok(self.open_batch(&[proposal], people, production, now)?[0])
    }

    /// Validate the entire batch before allocating IDs or changing any inbox.
    pub fn open_batch(
        &mut self,
        proposals: &[Proposal],
        people: &[PersonSnapshot],
        production: &Production,
        now: u64,
    ) -> Result<Vec<u64>, String> {
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        let count = u64::try_from(proposals.len()).map_err(|_| "request id overflow")?;
        let next = self
            .next_id
            .checked_add(count)
            .ok_or("request id overflow")?;
        let recipes: HashSet<_> = production.recipes.iter().map(|row| row.id).collect();
        let regions: HashSet<_> = production
            .inventories
            .iter()
            .map(|row| row.region)
            .collect();
        if proposals.par_iter().any(|proposal| {
            let a = person_index(people, proposal.requester).map(|i| people[i]);
            let b = person_index(people, proposal.worker).map(|i| people[i]);
            proposal.requester == proposal.worker
                || proposal.expires <= now
                || !matches!((a,b), (Some((_,Some(ar),ah,_)),Some((_,Some(br),bh,_))) if ar == proposal.region && br == ar && ah > 0 && bh > 0)
                || !recipes.contains(&proposal.recipe)
                || !regions.contains(&proposal.region)
        }) {
            return Err("invalid work request".into());
        }
        let mut existing: Vec<_> = self.pending.par_iter().map(|row| row.worker).collect();
        if existing.par_windows(2).any(|pair| pair[0] > pair[1]) {
            existing.par_sort_unstable();
        }
        let mut recipients: Vec<_> = proposals.par_iter().map(|row| row.worker).collect();
        if recipients.par_windows(2).any(|pair| pair[0] > pair[1]) {
            recipients.par_sort_unstable();
        }
        if recipients.par_chunk_by(|a, b| a == b).any(|rows| {
            let start = existing.partition_point(|id| *id < rows[0]);
            let end = existing.partition_point(|id| *id <= rows[0]);
            rows.len() + end - start > 4
        }) {
            return Err("recipient request inbox is full".into());
        }
        let first = self.next_id;
        let rows: Vec<_> = proposals
            .par_iter()
            .enumerate()
            .map(|(i, proposal)| Request {
                id: first + i as u64,
                requester: proposal.requester,
                worker: proposal.worker,
                region: proposal.region,
                recipe: proposal.recipe,
                created_at: now,
                expires: proposal.expires,
            })
            .collect();
        self.pending.par_extend(rows);
        self.next_id = next;
        Ok((first..next).collect())
    }
    pub fn prepare(
        &self,
        people: &[PersonSnapshot],
        opinions: Option<&Opinions>,
        busy: &[u64],
        tick: u64,
    ) -> Draft {
        let mut busy = busy.to_vec();
        busy.par_sort_unstable();
        let mut prefs: Vec<_> = self.preferences.par_iter().collect();
        if prefs
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            prefs.par_sort_unstable_by_key(|row| row.person);
        }
        let dense = prefs.first().and_then(|first| {
            prefs.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((prefs.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let preference = |person: u64| {
            dense
                .and_then(|first| person.checked_sub(first))
                .and_then(|index| usize::try_from(index).ok())
                .filter(|&index| index < prefs.len())
                .or_else(|| {
                    if dense.is_none() {
                        prefs.binary_search_by_key(&person, |p| p.person).ok()
                    } else {
                        None
                    }
                })
                .map(|index| prefs[index])
        };
        let index = opinions
            .filter(|opinions| !opinions.records.is_empty())
            .map(Opinions::index);
        let mut eligible: Vec<_> = self
            .pending
            .par_iter()
            .filter(|row| {
                row.expires >= tick
                    && busy.binary_search(&row.worker).is_err()
                    && person_index(people, row.worker)
                        .is_some_and(|i| people[i].1 == Some(row.region) && people[i].2 > 0)
                    && preference(row.worker).is_some()
            })
            .collect();
        if eligible
            .par_windows(2)
            .any(|pair| (pair[0].worker, pair[0].id) > (pair[1].worker, pair[1].id))
        {
            eligible.par_sort_unstable_by_key(|row| (row.worker, row.id));
        }
        let decide = |row: &Request| {
            let preference = preference(row.worker).expect("eligible preference");
            let (fear, obligation, affinity) = index.as_ref().map_or((0, 0, 0), |index| {
                (
                    index.fear(row.worker, row.requester),
                    index.obligation(row.worker, row.requester),
                    index.affinity(row.worker, row.requester),
                )
            });
            Decision {
                request: row.id,
                worker: row.worker,
                region: row.region,
                recipe: row.recipe,
                help: preference.motivated(fear, obligation, affinity),
                disregard_claims: preference.disregard_claims,
            }
        };
        let decisions = if eligible
            .par_windows(2)
            .any(|pair| pair[0].worker == pair[1].worker)
        {
            eligible
                .par_chunk_by(|a, b| a.worker == b.worker)
                .map(|rows| decide(rows[0]))
                .collect()
        } else {
            eligible.par_iter().map(|row| decide(row)).collect()
        };
        let expired = self
            .pending
            .par_iter()
            .filter(|row| row.expires < tick)
            .map(|row| Event {
                tick,
                request: row.id,
                worker: row.worker,
                outcome: Outcome::Expired,
            })
            .collect();
        Draft { decisions, expired }
    }
}
#[cfg(test)]
extern crate self as game_parallel_work_requests;
#[cfg(test)]
mod preference_tests {
    use super::Preference;

    #[test]
    fn signed_personal_affinity_can_raise_or_lower_the_value_of_help() {
        let mut preference = Preference {
            person: 1,
            effort: 5,
            help_value: 0,
            affinity_weight: 3,
            disregard_claims: false,
        };
        assert!(preference.motivated(0, 0, 2));
        assert!(!preference.motivated(0, 0, -2));
        preference.affinity_weight = -3;
        assert!(preference.motivated(0, 0, -2));
        assert!(!preference.motivated(0, 0, 2));
        assert!(preference.motivated(u64::MAX, u64::MAX, i32::MIN));
        preference.affinity_weight = 0;
        assert!(!preference.motivated(0, 0, i32::MAX));
    }
}
#[cfg(test)]
#[path = "../tests/support/experience.rs"]
mod experience_tests;
