//! Individual need for agency: time without successful chosen work accumulates deprivation.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub max: u32,
    pub per_tick: u32,
    pub blocked_increase: u32,
    pub completion_relief: u32,
    pub damage_threshold: u32,
    pub damage: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub person: u64,
    pub deprivation: u32,
    #[serde(default)]
    pub eligible_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Purpose {
    pub rules: Rules,
    pub people: Vec<Person>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Blocked,
    Progress,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub person: u64,
    pub outcome: Outcome,
}
impl Purpose {
    pub fn validate(&self, people: &HashSet<u64>) -> Result<(), &'static str> {
        if self.rules.max == 0
            || self.rules.damage_threshold == 0
            || self.rules.damage_threshold > self.rules.max
            || self.rules.completion_relief == 0
        {
            return Err("invalid purpose rules");
        }
        let mut ids: Vec<_> = self.people.par_iter().map(|row| row.person).collect();
        ids.par_sort_unstable();
        if ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || self
                .people
                .par_iter()
                .any(|row| !people.contains(&row.person) || row.deprivation > self.rules.max)
        {
            return Err("invalid purpose state");
        }
        Ok(())
    }
    /// Outcomes refer only to this generation, and progress overrides a simultaneous setback.
    pub fn prepare(&self, events: &[Event], tick: u64, alive: impl Fn(u64) -> bool + Sync) -> Self {
        let mut events = events.to_vec();
        if events
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            events.par_sort_unstable_by_key(|event| event.person);
        }
        let dense = events.first().and_then(|first| {
            let last = events.last()?.person;
            (last.checked_sub(first.person) == Some((events.len() - 1) as u64)
                && events
                    .par_windows(2)
                    .all(|pair| pair[0].person.checked_add(1) == Some(pair[1].person)))
            .then_some(first.person)
        });
        let people = self
            .people
            .par_iter()
            .map(|row| {
                if tick < row.eligible_at || !alive(row.person) {
                    return row.clone();
                }
                let outcomes = if let Some(first) = dense {
                    row.person
                        .checked_sub(first)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .and_then(|index| events.get(index))
                        .map(std::slice::from_ref)
                        .unwrap_or(&[])
                } else {
                    let start = events.partition_point(|event| event.person < row.person);
                    let end = events.partition_point(|event| event.person <= row.person);
                    &events[start..end]
                };
                let value = row
                    .deprivation
                    .saturating_add(self.rules.per_tick)
                    .min(self.rules.max);
                let value = if outcomes
                    .iter()
                    .any(|event| event.outcome == Outcome::Progress)
                {
                    value.saturating_sub(self.rules.completion_relief)
                } else if outcomes
                    .iter()
                    .any(|event| event.outcome == Outcome::Blocked)
                {
                    value
                        .saturating_add(self.rules.blocked_increase)
                        .min(self.rules.max)
                } else {
                    value
                };
                Person {
                    person: row.person,
                    deprivation: value,
                    eligible_at: row.eligible_at,
                }
            })
            .collect();
        Self {
            rules: self.rules.clone(),
            people,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_relieves_deprivation_but_blocking_worsens_it_and_death_freezes_it() {
        let state = Purpose {
            rules: Rules {
                max: 20,
                per_tick: 2,
                blocked_increase: 3,
                completion_relief: 5,
                damage_threshold: 10,
                damage: 1,
            },
            people: vec![
                Person {
                    person: 1,
                    deprivation: 8,
                    eligible_at: 0,
                },
                Person {
                    person: 2,
                    deprivation: 8,
                    eligible_at: 0,
                },
                Person {
                    person: 3,
                    deprivation: 8,
                    eligible_at: 0,
                },
            ],
        };
        let next = state.prepare(
            &[
                Event {
                    person: 1,
                    outcome: Outcome::Progress,
                },
                Event {
                    person: 2,
                    outcome: Outcome::Blocked,
                },
            ],
            1,
            |id| id != 3,
        );
        assert_eq!(
            next.people
                .iter()
                .map(|p| p.deprivation)
                .collect::<Vec<_>>(),
            vec![5, 13, 8]
        );
        assert_eq!(state.people[0].deprivation, 8);
    }
    #[test]
    fn mixed_sparse_and_duplicate_outcomes_match_independent_serial_expectations() {
        let state = Purpose {
            rules: Rules {
                max: 50,
                per_tick: 1,
                blocked_increase: 3,
                completion_relief: 4,
                damage_threshold: 40,
                damage: 1,
            },
            people: (0..10_000)
                .map(|person| Person {
                    person,
                    deprivation: 10,
                    eligible_at: 0,
                })
                .collect(),
        };
        let events: Vec<_> = (0..10_000)
            .filter(|id| id % 3 == 0)
            .flat_map(|person| {
                [
                    Event {
                        person,
                        outcome: Outcome::Blocked,
                    },
                    Event {
                        person,
                        outcome: Outcome::Progress,
                    },
                ]
            })
            .rev()
            .collect();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| state.prepare(&events, 1, |_| true))
        };
        let one = run(1);
        let many = run(4);
        assert_eq!(one, many);
        for row in &one.people {
            assert_eq!(row.deprivation, if row.person % 3 == 0 { 7 } else { 11 });
        }
    }
}
