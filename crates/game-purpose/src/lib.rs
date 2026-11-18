//! Individual appraisals of meaningful activities and deprivation over time.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub max: u32,
    /// When present, deprivation and its physical consequences apply at
    /// each multiple of this elapsed period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_period_millis: Option<u64>,
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
    /// An explicit appraisal replaces the default interpretation of work and setbacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appraisal: Option<Appraisal>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appraisal {
    /// Positive scores relieve deprivation; negative scores increase it.
    pub completed_work: i32,
    pub actions: Vec<ActionAppraisal>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionAppraisal {
    pub domain: u32,
    pub accepted: i32,
    pub denied: i32,
    pub stopped: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    Accepted,
    Denied,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    pub person: u64,
    pub domain: u32,
    pub outcome: ActionOutcome,
    pub tick: u64,
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
            || self.rules.elapsed_period_millis == Some(0)
        {
            return Err("invalid purpose rules");
        }
        let mut ids: Vec<_> = self.people.par_iter().map(|row| row.person).collect();
        ids.par_sort_unstable();
        if ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.people.par_iter().any(|row| {
                !people.contains(&row.person)
                    || row.deprivation > self.rules.max
                    || row.appraisal.as_ref().is_some_and(|appraisal| {
                        appraisal
                            .actions
                            .first()
                            .is_some_and(|action| action.domain == 0)
                            || appraisal
                                .actions
                                .windows(2)
                                .any(|pair| pair[0].domain >= pair[1].domain)
                    })
            })
        {
            return Err("invalid purpose state");
        }
        Ok(())
    }

    /// Apply outcomes of executed or physically denied personal choices.
    /// The interval host keeps person rows in identity order.
    pub fn appraise(&mut self, experiences: &[Experience]) {
        let mut changes: Vec<_> = experiences
            .par_iter()
            .enumerate()
            .filter_map(|(order, event)| {
                let index = self
                    .people
                    .binary_search_by_key(&event.person, |row| row.person)
                    .ok()?;
                let row = &self.people[index];
                if event.tick < row.eligible_at {
                    return None;
                }
                let appraisal = row.appraisal.as_ref()?;
                let action = appraisal
                    .actions
                    .binary_search_by_key(&event.domain, |action| action.domain)
                    .ok()?;
                let action = &appraisal.actions[action];
                let relief = match event.outcome {
                    ActionOutcome::Accepted => action.accepted,
                    ActionOutcome::Denied => action.denied,
                    ActionOutcome::Stopped => action.stopped,
                };
                (relief != 0).then_some((index, order, i64::from(relief)))
            })
            .collect();
        changes.par_sort_unstable();
        if changes.is_empty() {
            return;
        }
        self.people
            .par_chunks_mut(1024)
            .enumerate()
            .for_each(|(chunk, rows)| {
                let start = chunk * 1024;
                let first = changes.partition_point(|&(index, _, _)| index < start);
                for &(index, _, relief) in &changes[first..] {
                    let Some(row) = rows.get_mut(index - start) else {
                        break;
                    };
                    row.deprivation = (i64::from(row.deprivation) - relief)
                        .clamp(0, i64::from(self.rules.max))
                        as u32;
                }
            });
    }
    /// Outcomes refer only to this generation, and progress overrides a simultaneous setback.
    pub fn prepare(&self, events: &[Event], tick: u64, alive: impl Fn(u64) -> bool + Sync) -> Self {
        self.prepare_inner(events, tick, alive, self.rules.per_tick)
    }

    /// Physical time was already accounted for on the elapsed clock.
    pub fn settle_outcomes(
        &self,
        events: &[Event],
        tick: u64,
        alive: impl Fn(u64) -> bool + Sync,
    ) -> Self {
        self.prepare_inner(events, tick, alive, 0)
    }

    fn prepare_inner(
        &self,
        events: &[Event],
        tick: u64,
        alive: impl Fn(u64) -> bool + Sync,
        drift: u32,
    ) -> Self {
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
                let value = row.deprivation.saturating_add(drift).min(self.rules.max);
                let progressed = outcomes
                    .iter()
                    .any(|event| event.outcome == Outcome::Progress);
                let value = if let Some(appraisal) = &row.appraisal {
                    if progressed {
                        (i64::from(value) - i64::from(appraisal.completed_work))
                            .clamp(0, i64::from(self.rules.max)) as u32
                    } else {
                        value
                    }
                } else if progressed {
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
                    deprivation: value,
                    ..row.clone()
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
                elapsed_period_millis: None,
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
                    appraisal: None,
                },
                Person {
                    person: 2,
                    deprivation: 8,
                    eligible_at: 0,
                    appraisal: None,
                },
                Person {
                    person: 3,
                    deprivation: 8,
                    eligible_at: 0,
                    appraisal: None,
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
                elapsed_period_millis: None,
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
                    appraisal: None,
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

    #[test]
    fn identical_work_can_relieve_or_increase_deprivation() {
        let state = Purpose {
            rules: Rules {
                max: 20,
                elapsed_period_millis: None,
                per_tick: 0,
                blocked_increase: 5,
                completion_relief: 1,
                damage_threshold: 20,
                damage: 0,
            },
            people: [-3, 0, 4]
                .into_iter()
                .enumerate()
                .map(|(index, completed_work)| Person {
                    person: index as u64 + 1,
                    deprivation: 8,
                    eligible_at: 0,
                    appraisal: Some(Appraisal {
                        completed_work,
                        ..Appraisal::default()
                    }),
                })
                .collect(),
        };
        let outcomes: Vec<_> = (1..=3)
            .map(|person| Event {
                person,
                outcome: Outcome::Progress,
            })
            .collect();
        let next = state.prepare(&outcomes, 1, |_| true);
        assert_eq!(
            next.people
                .iter()
                .map(|row| row.deprivation)
                .collect::<Vec<_>>(),
            [11, 8, 4]
        );
    }
}
