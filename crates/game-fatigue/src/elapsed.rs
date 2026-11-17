//! Analytic, independently staged fatigue for constant-activity segments.
use super::{Consequence, Fatigue, Person};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Activity remains constant for the segment. Inactive people do not accrue fatigue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exertion {
    Work,
    Travel,
    Rest,
    Idle,
    Other,
    Inactive,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rules;

    fn owner(initial: u32) -> ElapsedFatigue {
        ElapsedFatigue {
            fatigue: Fatigue {
                rules: Rules {
                    capacity: 10,
                    baseline_gain: 0,
                    work_gain: 4,
                    travel_gain: 2,
                    idle_recovery: 1,
                    rest_recovery: 4,
                    rest_at: 7,
                    resume_at: 2,
                    exhaustion_at: 8,
                    exhaustion_damage: 3,
                },
                people: vec![Person {
                    id: 1,
                    fatigue: initial,
                    resting: false,
                }],
            },
            rate_period_millis: 1000,
            remainders: vec![],
            damage_remainders: vec![],
        }
    }

    fn run(
        initial: u32,
        segments: &[(Exertion, u64)],
        partition: u64,
        workers: usize,
    ) -> (ElapsedFatigue, u32) {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let people = HashSet::from([1]);
                let mut state = owner(initial);
                let mut damage = 0;
                for &(exertion, duration) in segments {
                    let mut elapsed = 0;
                    while elapsed < duration {
                        let step = partition.min(duration - elapsed);
                        let draft = state
                            .prepare(&[ElapsedActivity { id: 1, exertion }], step, &people)
                            .unwrap();
                        damage += draft
                            .consequences
                            .iter()
                            .map(|event| event.damage)
                            .sum::<u32>();
                        state = serde_json::from_str(&serde_json::to_string(&draft.state).unwrap())
                            .unwrap();
                        elapsed += step;
                    }
                }
                (state, damage)
            })
    }

    #[test]
    fn saturation_preserves_activity_order_and_partitioning() {
        let segments = [(Exertion::Work, 1000), (Exertion::Rest, 1000)];
        let whole = run(9, &segments, 1000, 1);
        assert_eq!(whole.0.fatigue.people[0].fatigue, 6);
        assert!(!whole.0.fatigue.people[0].resting);
        assert_eq!(run(9, &segments, 17, 4), whole);
        let reverse = [(Exertion::Rest, 1000), (Exertion::Work, 1000)];
        assert_eq!(run(0, &reverse, 19, 4).0.fatigue.people[0].fatigue, 4);
    }

    #[test]
    fn exposure_and_fraction_survive_threshold_crossings_and_save() {
        let segments = [
            (Exertion::Work, 625),
            (Exertion::Rest, 375),
            (Exertion::Work, 625),
            (Exertion::Inactive, 1000),
        ];
        let whole = run(7, &segments, 1000, 1);
        assert!(whole.1 > 0);
        assert!(!whole.0.damage_remainders.is_empty());
        assert_eq!(run(7, &segments, 1, 4), whole);
        assert_eq!(run(7, &segments, 137, 1), whole);
    }

    #[test]
    fn rejects_bad_fractions_and_missing_activity() {
        let people = HashSet::from([1]);
        let mut state = owner(1);
        assert!(state.prepare(&[], 1, &people).is_err());
        state.remainders.push((1, 1000));
        assert!(state.validate(&people).is_err());
        state.remainders[0].1 = 1;
        state.fatigue.people[0].fatigue = 10;
        assert!(
            state
                .prepare(
                    &[ElapsedActivity {
                        id: 1,
                        exertion: Exertion::Work
                    }],
                    1,
                    &people
                )
                .is_err()
        );
    }

    #[test]
    fn predicts_first_damage_across_threshold_and_interrupted_exposure() {
        let people = HashSet::from([1]);
        let work = [ElapsedActivity {
            id: 1,
            exertion: Exertion::Work,
        }];
        let rest = [ElapsedActivity {
            id: 1,
            exertion: Exertion::Rest,
        }];
        let state = owner(7);
        assert_eq!(
            state.next_damage_deadline(&work, &people).unwrap(),
            Some(583)
        );
        let before = state.prepare(&work, 582, &people).unwrap();
        assert!(before.consequences.is_empty());
        assert_eq!(
            before.state.next_damage_deadline(&work, &people).unwrap(),
            Some(1)
        );
        assert_eq!(
            before
                .state
                .prepare(&work, 1, &people)
                .unwrap()
                .consequences[0]
                .damage,
            1
        );
        assert_eq!(owner(8).next_damage_deadline(&rest, &people).unwrap(), None);
        let mut almost = owner(9);
        almost.damage_remainders = vec![(1, 999)];
        assert_eq!(
            almost.next_damage_deadline(&rest, &people).unwrap(),
            Some(1)
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElapsedActivity {
    pub id: u64,
    pub exertion: Exertion,
}

/// Saved exact fractions use the same explicit denominator as the authored rates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElapsedFatigue {
    pub fatigue: Fatigue,
    pub rate_period_millis: u64,
    #[serde(default)]
    pub remainders: Vec<(u64, u64)>,
    #[serde(default)]
    pub damage_remainders: Vec<(u64, u64)>,
}

pub struct ElapsedFatigueDraft {
    pub state: ElapsedFatigue,
    pub consequences: Vec<Consequence>,
}

impl ElapsedFatigue {
    pub fn validate(&self, people: &HashSet<u64>) -> Result<(), String> {
        self.fatigue.validate(people)?;
        let valid = |rows: &[(u64, u64)]| {
            !rows.par_windows(2).any(|pair| pair[0].0 >= pair[1].0)
                && !rows.par_iter().any(|&(id, value)| {
                    !people.contains(&id) || value == 0 || value >= self.rate_period_millis
                })
        };
        if self.rate_period_millis == 0
            || !valid(&self.remainders)
            || !valid(&self.damage_remainders)
            || self.fatigue.people.par_iter().any(|person| {
                person.fatigue == self.fatigue.rules.capacity
                    && self
                        .remainders
                        .binary_search_by_key(&person.id, |row| row.0)
                        .is_ok()
            })
        {
            return Err("invalid elapsed fatigue fractions".into());
        }
        Ok(())
    }

    fn validate_activity(
        &self,
        activity: &[ElapsedActivity],
        people: &HashSet<u64>,
    ) -> Result<(), String> {
        self.validate(people)?;
        if activity.par_windows(2).any(|pair| pair[0].id >= pair[1].id)
            || activity.par_iter().any(|row| !people.contains(&row.id))
            || self.fatigue.people.par_iter().any(|person| {
                activity
                    .binary_search_by_key(&person.id, |row| row.id)
                    .is_err()
            })
            || self
                .remainders
                .par_iter()
                .chain(self.damage_remainders.par_iter())
                .any(|row| {
                    activity
                        .binary_search_by_key(&row.0, |entry| entry.id)
                        .is_err()
                })
        {
            return Err("invalid elapsed fatigue activity".into());
        }
        Ok(())
    }

    /// Predict the first damage event if these activities remain unchanged.
    /// The host must also stop at earlier activity or health transitions.
    pub fn next_damage_deadline(
        &self,
        activity: &[ElapsedActivity],
        people: &HashSet<u64>,
    ) -> Result<Option<u128>, String> {
        self.validate_activity(activity, people)?;
        let rules = &self.fatigue.rules;
        if rules.exhaustion_damage == 0 {
            return Ok(None);
        }
        let mut old: Vec<_> = self.fatigue.people.par_iter().collect();
        old.par_sort_unstable_by_key(|person| person.id);
        let period = u128::from(self.rate_period_millis);
        let threshold = u128::from(rules.exhaustion_at) * period;
        let cap = u128::from(rules.capacity) * period;
        let deadlines: Result<Vec<_>, String> = activity
            .par_iter()
            .map(|row| {
                let fraction = |entries: &[(u64, u64)]| {
                    entries
                        .binary_search_by_key(&row.id, |entry| entry.0)
                        .ok()
                        .map_or(0, |index| entries[index].1)
                };
                let fatigue = old
                    .binary_search_by_key(&row.id, |person| person.id)
                    .ok()
                    .map_or(0, |index| old[index].fatigue);
                let initial = u128::from(fatigue) * period + u128::from(fraction(&self.remainders));
                if initial > cap {
                    return Err("fatigue exceeds capacity".into());
                }
                if row.exertion == Exertion::Inactive {
                    return Ok(None);
                }
                let (gain, recovery) = rates(rules, row.exertion);
                let required = (period - u128::from(fraction(&self.damage_remainders)))
                    .div_ceil(u128::from(rules.exhaustion_damage));
                let deadline = if gain >= recovery {
                    let rate = u128::from(gain - recovery);
                    let first = if initial >= threshold {
                        1
                    } else if rate == 0 {
                        return Ok(None);
                    } else {
                        (threshold - initial).div_ceil(rate).max(1)
                    };
                    Some(first + required - 1)
                } else {
                    let rate = u128::from(recovery - gain);
                    if initial < threshold || (initial - threshold) / rate < required {
                        None
                    } else {
                        Some(required)
                    }
                };
                Ok(deadline)
            })
            .collect();
        Ok(deadlines?.into_par_iter().flatten().min())
    }

    /// Stage one half-open segment. Exhaustion exposure is sampled at the end
    /// of each elapsed millisecond; fractions survive interrupted exposure.
    /// Rest is an input to this calculation, never an automatic decision.
    pub fn prepare(
        &self,
        activity: &[ElapsedActivity],
        millis: u64,
        people: &HashSet<u64>,
    ) -> Result<ElapsedFatigueDraft, String> {
        self.validate_activity(activity, people)?;
        let mut old: Vec<_> = self.fatigue.people.par_iter().collect();
        old.par_sort_unstable_by_key(|person| person.id);
        let rules = &self.fatigue.rules;
        let period = u128::from(self.rate_period_millis);
        let threshold = u128::from(rules.exhaustion_at) * period;
        let cap = u128::from(rules.capacity) * period;
        let rows: Result<Vec<_>, String> = activity
            .par_iter()
            .map(|row| {
                let mut person = old
                    .binary_search_by_key(&row.id, |person| person.id)
                    .ok()
                    .map(|index| old[index].clone())
                    .unwrap_or(Person {
                        id: row.id,
                        fatigue: 0,
                        resting: false,
                    });
                let fraction = |entries: &[(u64, u64)]| {
                    entries
                        .binary_search_by_key(&row.id, |entry| entry.0)
                        .ok()
                        .map_or(0, |index| entries[index].1)
                };
                let initial =
                    u128::from(person.fatigue) * period + u128::from(fraction(&self.remainders));
                if initial > cap {
                    return Err("fatigue exceeds capacity".into());
                }
                let (gain, recovery) = rates(rules, row.exertion);
                let duration = u128::from(millis);
                let (exact, exposed) = if row.exertion == Exertion::Inactive {
                    (initial, 0)
                } else if gain >= recovery {
                    let rate = u128::from(gain - recovery);
                    let exact = (initial + rate * duration).min(cap);
                    let exposed = if initial >= threshold {
                        duration
                    } else if rate == 0 {
                        0
                    } else {
                        let crossing = (threshold - initial).div_ceil(rate);
                        if crossing > duration {
                            0
                        } else {
                            duration - crossing + 1
                        }
                    };
                    (exact, exposed)
                } else {
                    let rate = u128::from(recovery - gain);
                    let exact = initial.saturating_sub(rate * duration);
                    let exposed = if initial < threshold {
                        0
                    } else {
                        ((initial - threshold) / rate).min(duration)
                    };
                    (exact, exposed)
                };
                person.fatigue = (exact / period) as u32;
                let fatigue_fraction = (exact % period) as u64;
                let damage_exact = u128::from(fraction(&self.damage_remainders))
                    + exposed * u128::from(rules.exhaustion_damage);
                let damage = u32::try_from(damage_exact / period)
                    .map_err(|_| "elapsed exhaustion damage overflow".to_string())?;
                Ok((
                    person,
                    fatigue_fraction,
                    (damage_exact % period) as u64,
                    damage,
                ))
            })
            .collect();
        let rows = rows?;
        Ok(ElapsedFatigueDraft {
            state: Self {
                fatigue: Fatigue {
                    rules: rules.clone(),
                    people: rows
                        .par_iter()
                        .filter(|row| row.0.fatigue != 0 || row.0.resting || row.1 != 0)
                        .map(|row| row.0.clone())
                        .collect(),
                },
                rate_period_millis: self.rate_period_millis,
                remainders: rows
                    .par_iter()
                    .filter(|row| row.1 != 0)
                    .map(|row| (row.0.id, row.1))
                    .collect(),
                damage_remainders: rows
                    .par_iter()
                    .filter(|row| row.2 != 0)
                    .map(|row| (row.0.id, row.2))
                    .collect(),
            },
            consequences: rows
                .par_iter()
                .filter(|row| row.3 != 0)
                .map(|row| Consequence {
                    id: row.0.id,
                    damage: row.3,
                })
                .collect(),
        })
    }
}

fn rates(rules: &super::Rules, activity: Exertion) -> (u64, u64) {
    match activity {
        Exertion::Work => (
            u64::from(rules.baseline_gain) + u64::from(rules.work_gain),
            0,
        ),
        Exertion::Travel => (
            u64::from(rules.baseline_gain) + u64::from(rules.travel_gain),
            0,
        ),
        Exertion::Rest => (
            u64::from(rules.baseline_gain),
            u64::from(rules.rest_recovery),
        ),
        Exertion::Idle => (
            u64::from(rules.baseline_gain),
            u64::from(rules.idle_recovery),
        ),
        Exertion::Other => (u64::from(rules.baseline_gain), 0),
        Exertion::Inactive => (0, 0),
    }
}
