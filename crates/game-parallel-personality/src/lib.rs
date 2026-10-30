//! Stable personal dispositions influencing voluntary choices.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
const SCALE: u32 = 1000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub person: u64,
    pub sociability: u16,
    pub diligence: u16,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personality {
    pub social_urgency_per_isolation: u16,
    /// Copy the mean of recorded parents to actual newborns.
    #[serde(default)]
    pub inherit_births: bool,
    pub people: Vec<Person>,
}
impl Clone for Personality {
    fn clone(&self) -> Self {
        Self {
            social_urgency_per_isolation: self.social_urgency_per_isolation,
            inherit_births: self.inherit_births,
            people: self.people.par_iter().cloned().collect(),
        }
    }
}
pub struct Index<'a> {
    people: Vec<&'a Person>,
    urgency: u16,
}
impl Personality {
    pub fn validate(&self, known: &HashSet<u64>) -> Result<(), &'static str> {
        let mut ids: Vec<_> = self.people.par_iter().map(|row| row.person).collect();
        ids.par_sort_unstable();
        if self.social_urgency_per_isolation > 1000
            || ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.people.par_iter().any(|row| {
                !known.contains(&row.person) || row.sociability > 1000 || row.diligence > 1000
            })
        {
            return Err("invalid personality state");
        }
        Ok(())
    }
    pub fn index(&self) -> Index<'_> {
        let mut people: Vec<_> = self.people.par_iter().collect();
        if people
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            people.par_sort_unstable_by_key(|row| row.person);
        }
        Index {
            people,
            urgency: self.social_urgency_per_isolation,
        }
    }
    /// Enroll newborns with at least one recorded parent disposition.
    pub fn enroll_births(&mut self, births: &[(u64, [u64; 2], usize)]) {
        if !self.inherit_births || births.is_empty() {
            return;
        }
        let index = self.index();
        let rows: Vec<_> = births
            .par_iter()
            .filter_map(|&(person, parents, count)| {
                let (count, sociability, diligence) = parents
                    .iter()
                    .take(count)
                    .filter_map(|id| index.person(*id))
                    .fold((0u32, 0u32, 0u32), |(n, social, work), row| {
                        (
                            n + 1,
                            social + u32::from(row.sociability),
                            work + u32::from(row.diligence),
                        )
                    });
                (count > 0).then(|| Person {
                    person,
                    sociability: (sociability / count) as u16,
                    diligence: (diligence / count) as u16,
                })
            })
            .collect();
        self.people.extend(rows);
    }
    pub fn work_refusals(&self, seed: u64, tick: u64) -> Vec<u64> {
        self.people
            .par_iter()
            .filter(|row| sample(seed ^ 0x776f726b, row.person, tick) >= u32::from(row.diligence))
            .map(|row| row.person)
            .collect()
    }
}
fn sample(seed: u64, person: u64, tick: u64) -> u32 {
    (game_parallel_genetics::random(seed, person, tick) % u64::from(SCALE)) as u32
}
impl Index<'_> {
    fn person(&self, id: u64) -> Option<&Person> {
        self.people
            .binary_search_by_key(&id, |row| row.person)
            .ok()
            .map(|i| self.people[i])
    }
    pub fn social(&self, person: u64, isolation: u32, seed: u64, tick: u64) -> bool {
        let Some(personality) = self.person(person) else {
            return true;
        };
        let threshold = u32::from(personality.sociability)
            .saturating_add(isolation.saturating_mul(u32::from(self.urgency)))
            .min(SCALE);
        sample(seed ^ 0x736f6369616c, person, tick) < threshold
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolation_can_overcome_low_sociability_and_manual_absence_is_unrestricted() {
        let state = Personality {
            social_urgency_per_isolation: 500,
            inherit_births: false,
            people: vec![
                Person {
                    person: 1,
                    sociability: 0,
                    diligence: 0,
                },
                Person {
                    person: 2,
                    sociability: 1000,
                    diligence: 1000,
                },
            ],
        };
        assert!(!state.index().social(1, 0, 1, 1));
        assert!(state.index().social(1, 2, 1, 1));
        assert!(state.index().social(2, 0, 1, 1));
        assert!(state.index().social(3, 0, 1, 1));
        assert_eq!(state.work_refusals(1, 1), vec![1]);
    }
    #[test]
    fn choices_are_worker_invariant() {
        let state = Personality {
            social_urgency_per_isolation: 10,
            inherit_births: false,
            people: (1..=10000)
                .map(|person| Person {
                    person,
                    sociability: 350,
                    diligence: 700,
                })
                .collect(),
        };
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let index = state.index();
                    let social: Vec<_> = (1..=10000)
                        .into_par_iter()
                        .filter(|&id| index.social(id, 3, 7, 9))
                        .collect();
                    (social, state.work_refusals(7, 9))
                })
        };
        assert_eq!(run(1), run(4));
    }
}
