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
    /// Elapsed duration represented by the authored fatigue rates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_period_millis: Option<u64>,
    /// Nonnegative fractional numerators over the rate period, sorted by person.
    #[serde(default)]
    pub remainders: Vec<(u64, u64)>,
    /// Presence selects elapsed fatigue; absence retains legacy boundary settlement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_damage_remainders: Option<Vec<(u64, u64)>>,
}

pub struct TimedDraft {
    pub state: TimedFatigue,
    pub consequences: Vec<Consequence>,
}

impl TimedFatigue {
    pub fn validate(&self, people: &HashSet<u64>, quantum: u64) -> Result<(), String> {
        self.fatigue.validate(people)?;
        let period = self.rate_period_millis.unwrap_or(quantum);
        if quantum == 0
            || period == 0
            || (self.elapsed_damage_remainders.is_some() && self.rate_period_millis.is_none())
            || self
                .remainders
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
            || self
                .remainders
                .par_iter()
                .any(|&(id, value)| !people.contains(&id) || value == 0 || value >= period)
        {
            return Err("invalid fractional fatigue".into());
        }
        if let Some(elapsed) = self.elapsed_owner() {
            elapsed.validate(people)?;
        }
        Ok(())
    }

    /// Construct the elapsed owner only for saves explicitly using that mode.
    pub fn elapsed_owner(&self) -> Option<super::ElapsedFatigue> {
        Some(super::ElapsedFatigue {
            fatigue: self.fatigue.clone(),
            rate_period_millis: self.rate_period_millis?,
            remainders: self.remainders.clone(),
            damage_remainders: self.elapsed_damage_remainders.clone()?,
        })
    }

    pub fn set_elapsed_owner(&mut self, elapsed: super::ElapsedFatigue) {
        self.fatigue = elapsed.fatigue;
        self.rate_period_millis = Some(elapsed.rate_period_millis);
        self.remainders = elapsed.remainders;
        self.elapsed_damage_remainders = Some(elapsed.damage_remainders);
    }

    /// Apply one boundary-weighted update. The input sums durations before
    /// rounding, so splitting an interval does not multiply fatigue effects.
    pub fn prepare(
        &self,
        activity: &[TimedActivity],
        quantum: u64,
        alive: impl Fn(u64) -> bool + Sync,
        _controlled: impl Fn(u64) -> bool + Sync,
    ) -> Result<TimedDraft, String> {
        let period = self.rate_period_millis.unwrap_or(quantum);
        if quantum == 0
            || period == 0
            || self
                .remainders
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
            || self
                .remainders
                .par_iter()
                .any(|&(_, value)| value == 0 || value >= period)
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
                    u128::from(person.fatigue) * u128::from(period) + u128::from(remainder);
                let gain = u128::from(rules.baseline_gain) * u128::from(row.alive_millis)
                    + u128::from(rules.work_gain) * u128::from(row.work_millis)
                    + u128::from(rules.travel_gain) * u128::from(row.travel_millis);
                let recovery = u128::from(rules.rest_recovery) * u128::from(row.rest_millis)
                    + u128::from(rules.idle_recovery) * u128::from(row.idle_millis);
                exact = exact
                    .saturating_add(gain)
                    .saturating_sub(recovery)
                    .min(u128::from(rules.capacity) * u128::from(period));
                person.fatigue = (exact / u128::from(period)) as u32;
                let fraction = (exact % u128::from(period)) as u64;
                let consequence = (alive(row.id)
                    && exact >= u128::from(rules.exhaustion_at) * u128::from(period)
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
                rate_period_millis: self.rate_period_millis,
                remainders,
                elapsed_damage_remainders: self.elapsed_damage_remainders.clone(),
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
            rate_period_millis: None,
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
            elapsed_damage_remainders: None,
        }
    }

    #[test]
    fn explicit_fatigue_period_preserves_fraction_across_boundaries_and_resume() {
        fn run(quantum: u64, workers: usize) -> TimedFatigue {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut state = owner();
                    state.rate_period_millis = Some(1000);
                    let mut elapsed = 0;
                    while elapsed < 500 {
                        let duration = quantum.min(500 - elapsed);
                        let activity = [TimedActivity {
                            id: 1,
                            alive_millis: duration,
                            work_millis: duration,
                            ..Default::default()
                        }];
                        state = state
                            .prepare(&activity, quantum, |_| true, |_| true)
                            .unwrap()
                            .state;
                        elapsed += duration;
                        if elapsed == 200 {
                            state = serde_json::from_str(&serde_json::to_string(&state).unwrap())
                                .unwrap();
                        }
                    }
                    state
                })
        }
        let whole = run(1000, 1);
        assert_eq!(whole.fatigue.people[0].fatigue, 1);
        assert_eq!(whole.remainders, vec![(1, 500)]);
        assert_eq!(run(100, 4), whole);
        let mut invalid = whole;
        invalid.rate_period_millis = Some(0);
        let activity = [TimedActivity {
            id: 1,
            alive_millis: 100,
            work_millis: 100,
            ..Default::default()
        }];
        assert!(invalid.validate(&HashSet::from([1]), 100).is_err());
        assert!(invalid.prepare(&activity, 100, |_| true, |_| true).is_err());
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
            vec![(3, false), (3, false)]
        );
        state.fatigue = state
            .fatigue
            .choose_rest(
                &[crate::RestChoice {
                    person: 1,
                    resting: true,
                }],
                &HashSet::from([1, 2]),
            )
            .unwrap();
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
        assert_eq!(draft.state.fatigue.people.len(), 2);
        assert!(draft.state.fatigue.people[0].resting);
        let after_choice = draft
            .state
            .fatigue
            .choose_rest(
                &[crate::RestChoice {
                    person: 1,
                    resting: false,
                }],
                &HashSet::from([1, 2]),
            )
            .unwrap();
        assert_eq!(after_choice.people.len(), 1);
        assert_eq!(
            (after_choice.people[0].id, after_choice.people[0].fatigue),
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
