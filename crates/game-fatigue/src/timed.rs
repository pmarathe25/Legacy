//! Boundary-weighted fatigue from accumulated elapsed activity.
use super::{Consequence, Fatigue, Person};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Activity durations for one person in the current quantum.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedActivity {
    pub id: u64,
    pub alive_millis: u64,
    pub work_millis: u64,
    pub travel_millis: u64,
    pub rest_millis: u64,
    pub idle_millis: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedFatigue {
    pub fatigue: Fatigue,
    /// Nonnegative fractions, in units of one quantum, sorted by person.
    #[serde(default)]
    pub remainders: Vec<(u64, u64)>,
}

pub struct TimedDraft {
    pub state: TimedFatigue,
    pub consequences: Vec<Consequence>,
}

impl TimedFatigue {
    pub fn validate(&self, people: &HashSet<u64>, quantum: u64) -> Result<(), String> {
        self.fatigue.validate(people)?;
        if quantum == 0
            || self
                .remainders
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
            || self
                .remainders
                .par_iter()
                .any(|&(id, value)| !people.contains(&id) || value == 0 || value >= quantum)
        {
            return Err("invalid fractional fatigue".into());
        }
        Ok(())
    }

    /// Apply one boundary-weighted update. The input sums durations before
    /// rounding, so splitting an interval does not multiply fatigue effects.
    pub fn prepare(
        &self,
        activity: &[TimedActivity],
        quantum: u64,
        alive: impl Fn(u64) -> bool + Sync,
        controlled: impl Fn(u64) -> bool + Sync,
    ) -> Result<TimedDraft, String> {
        if quantum == 0
            || activity.par_windows(2).any(|rows| rows[0].id >= rows[1].id)
            || activity.par_iter().any(|row| {
                row.alive_millis > quantum
                    || u128::from(row.work_millis)
                        + u128::from(row.travel_millis)
                        + u128::from(row.rest_millis)
                        + u128::from(row.idle_millis)
                        > u128::from(row.alive_millis)
            })
        {
            return Err("invalid timed fatigue activity".into());
        }
        let mut old: Vec<_> = self.fatigue.people.par_iter().collect();
        if old.par_windows(2).any(|rows| rows[0].id > rows[1].id) {
            old.par_sort_unstable_by_key(|row| row.id);
        }
        if old
            .par_iter()
            .any(|row| activity.binary_search_by_key(&row.id, |a| a.id).is_err())
        {
            return Err("missing timed fatigue person".into());
        }
        let rules = &self.fatigue.rules;
        let rows: Vec<_> = activity
            .par_iter()
            .map(|row| {
                let prior = old
                    .binary_search_by_key(&row.id, |person| person.id)
                    .ok()
                    .map(|index| old[index]);
                let mut person = prior.cloned().unwrap_or(Person {
                    id: row.id,
                    fatigue: 0,
                    resting: false,
                });
                let remainder = self
                    .remainders
                    .binary_search_by_key(&row.id, |entry| entry.0)
                    .ok()
                    .map_or(0, |index| self.remainders[index].1);
                let mut exact =
                    u128::from(person.fatigue) * u128::from(quantum) + u128::from(remainder);
                let gain = u128::from(rules.baseline_gain) * u128::from(row.alive_millis)
                    + u128::from(rules.work_gain) * u128::from(row.work_millis)
                    + u128::from(rules.travel_gain) * u128::from(row.travel_millis);
                let recovery = u128::from(rules.rest_recovery) * u128::from(row.rest_millis)
                    + u128::from(rules.idle_recovery) * u128::from(row.idle_millis);
                exact = exact
                    .saturating_add(gain)
                    .saturating_sub(recovery)
                    .min(u128::from(rules.capacity) * u128::from(quantum));
                person.fatigue = (exact / u128::from(quantum)) as u32;
                let fraction = (exact % u128::from(quantum)) as u64;
                if !controlled(row.id) && row.alive_millis > 0 {
                    if person.resting
                        && row.rest_millis > 0
                        && exact <= u128::from(rules.resume_at) * u128::from(quantum)
                    {
                        person.resting = false;
                    }
                    if exact >= u128::from(rules.rest_at) * u128::from(quantum) {
                        person.resting = true;
                    }
                }
                let consequence = (alive(row.id)
                    && exact >= u128::from(rules.exhaustion_at) * u128::from(quantum)
                    && rules.exhaustion_damage > 0)
                    .then_some(Consequence {
                        id: row.id,
                        damage: rules.exhaustion_damage,
                    });
                (person, fraction, consequence)
            })
            .collect();
        let people = rows
            .par_iter()
            .filter(|row| row.0.fatigue != 0 || row.0.resting || row.1 != 0)
            .map(|row| row.0.clone())
            .collect();
        let remainders = rows
            .par_iter()
            .filter(|row| row.1 != 0)
            .map(|row| (row.0.id, row.1))
            .collect();
        let consequences = rows.par_iter().filter_map(|row| row.2).collect();
        Ok(TimedDraft {
            state: TimedFatigue {
                fatigue: Fatigue {
                    rules: rules.clone(),
                    people,
                },
                remainders,
            },
            consequences,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rules;

    fn owner() -> TimedFatigue {
        TimedFatigue {
            fatigue: Fatigue {
                rules: Rules {
                    capacity: 10,
                    baseline_gain: 0,
                    work_gain: 3,
                    travel_gain: 2,
                    idle_recovery: 1,
                    rest_recovery: 4,
                    rest_at: 3,
                    resume_at: 1,
                    exhaustion_at: 9,
                    exhaustion_damage: 2,
                },
                people: vec![],
            },
            remainders: vec![],
        }
    }
    #[test]
    fn fractional_fatigue_accumulates_and_rest_is_personal() {
        let mut state = owner();
        state.validate(&HashSet::from([1, 2]), 1000).unwrap();
        let work = |id| TimedActivity {
            id,
            alive_millis: 1000,
            work_millis: 500,
            ..Default::default()
        };
        state = state
            .prepare(&[work(1), work(2)], 1000, |_| true, |id| id == 2)
            .unwrap()
            .state;
        assert_eq!(
            state
                .fatigue
                .people
                .iter()
                .map(|p| p.fatigue)
                .collect::<Vec<_>>(),
            vec![1, 1]
        );
        assert_eq!(state.remainders, vec![(1, 500), (2, 500)]);
        state = state
            .prepare(&[work(1), work(2)], 1000, |_| true, |id| id == 2)
            .unwrap()
            .state;
        assert_eq!(
            state
                .fatigue
                .people
                .iter()
                .map(|p| (p.fatigue, p.resting))
                .collect::<Vec<_>>(),
            vec![(3, true), (3, false)]
        );
        let rest = TimedActivity {
            id: 1,
            alive_millis: 1000,
            rest_millis: 1000,
            ..Default::default()
        };
        let idle = TimedActivity {
            id: 2,
            alive_millis: 1000,
            idle_millis: 1000,
            ..Default::default()
        };
        let draft = state
            .prepare(&[rest, idle], 1000, |_| true, |id| id == 2)
            .unwrap();
        assert_eq!(draft.state.fatigue.people.len(), 1);
        assert_eq!(
            (
                draft.state.fatigue.people[0].id,
                draft.state.fatigue.people[0].fatigue
            ),
            (2, 2)
        );
    }
    #[test]
    fn rejects_overlapping_or_unsorted_activity() {
        let state = owner();
        let overlapping = TimedActivity {
            id: 1,
            alive_millis: 1000,
            work_millis: 600,
            travel_millis: 500,
            ..Default::default()
        };
        assert!(
            state
                .prepare(&[overlapping], 1000, |_| true, |_| false)
                .is_err()
        );
        assert!(
            state
                .prepare(
                    &[
                        TimedActivity {
                            id: 2,
                            ..Default::default()
                        },
                        TimedActivity {
                            id: 1,
                            ..Default::default()
                        }
                    ],
                    1000,
                    |_| true,
                    |_| false
                )
                .is_err()
        );
    }
}
