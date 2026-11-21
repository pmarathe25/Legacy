//! Bounded individual fatigue, automatic rest, and exhaustion consequences.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
mod elapsed;
mod timed;
pub use elapsed::{
    ElapsedActivity, ElapsedFatigue, ElapsedFatigueDraft, ElapsedFatigueView, Exertion,
};
pub use timed::{TimedActivity, TimedDraft, TimedFatigue};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub capacity: u32,
    #[serde(default)]
    pub baseline_gain: u32,
    pub work_gain: u32,
    #[serde(default)]
    pub travel_gain: u32,
    pub idle_recovery: u32,
    pub rest_recovery: u32,
    pub rest_at: u32,
    pub resume_at: u32,
    pub exhaustion_at: u32,
    pub exhaustion_damage: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: u64,
    pub fatigue: u32,
    #[serde(default)]
    pub resting: bool,
}
/// A person's own requested rest state; the host validates their circumstances.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestChoice {
    pub person: u64,
    pub resting: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fatigue {
    pub rules: Rules,
    #[serde(default)]
    pub people: Vec<Person>,
}
/// Activity is sampled before work and health consequences are applied.
#[derive(Clone, Copy, Debug)]
pub struct Activity {
    pub id: u64,
    pub alive: bool,
    pub working: bool,
    pub traveling: bool,
    /// Recovery requires a suitable stationary opportunity to rest.
    pub can_rest: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Consequence {
    pub id: u64,
    pub damage: u32,
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub state: Fatigue,
    pub consequences: Vec<Consequence>,
}
impl Fatigue {
    /// Stage independent personal rest choices without deciding for anyone.
    pub fn choose_rest(
        &self,
        choices: &[RestChoice],
        people: &HashSet<u64>,
    ) -> Result<Self, String> {
        if choices
            .par_windows(2)
            .any(|rows| rows[0].person >= rows[1].person)
            || choices
                .par_iter()
                .any(|choice| choice.person == 0 || !people.contains(&choice.person))
        {
            return Err("invalid personal rest choices".into());
        }
        let mut old: Vec<_> = self.people.par_iter().cloned().collect();
        old.par_sort_unstable_by_key(|person| person.id);
        if old.par_windows(2).any(|rows| rows[0].id == rows[1].id)
            || old.par_iter().any(|person| {
                person.id == 0
                    || !people.contains(&person.id)
                    || person.fatigue > self.rules.capacity
            })
        {
            return Err("duplicate fatigue person".into());
        }
        let mut next: Vec<_> = old
            .par_iter()
            .filter(|person| {
                choices
                    .binary_search_by_key(&person.id, |choice| choice.person)
                    .is_err()
            })
            .cloned()
            .collect();
        next.par_extend(
            choices
                .par_iter()
                .map(|choice| {
                    let mut person = old
                        .binary_search_by_key(&choice.person, |person| person.id)
                        .ok()
                        .map(|index| old[index].clone())
                        .unwrap_or(Person {
                            id: choice.person,
                            fatigue: 0,
                            resting: false,
                        });
                    person.resting = choice.resting;
                    person
                })
                .filter(|person| person.fatigue != 0 || person.resting),
        );
        next.par_sort_unstable_by_key(|person| person.id);
        Ok(Self {
            rules: self.rules.clone(),
            people: next,
        })
    }
    pub fn validate(&self, people: &HashSet<u64>) -> Result<(), String> {
        let r = &self.rules;
        if r.capacity == 0
            || r.work_gain == 0
            || r.rest_recovery == 0
            || r.resume_at >= r.rest_at
            || r.rest_at > r.capacity
            || r.exhaustion_at > r.capacity
            || r.exhaustion_at == 0
        {
            return Err("invalid fatigue rules".into());
        }
        let mut ids = HashSet::new();
        if self
            .people
            .iter()
            .any(|p| !people.contains(&p.id) || !ids.insert(p.id) || p.fatigue > r.capacity)
        {
            return Err("invalid fatigue person".into());
        }
        Ok(())
    }
    /// Build an immutable sorted index once per generation for work admission.
    pub fn resting(&self) -> Vec<u64> {
        let mut ids: Vec<_> = self
            .people
            .par_iter()
            .filter(|p| p.resting)
            .map(|p| p.id)
            .collect();
        ids.par_sort_unstable();
        ids
    }
    /// Calculate a private next generation from unique, identity-sorted activity.
    /// The host supplies one activity for each living or deceased person. Missing
    /// records start at zero; deceased records are retained without changing.
    pub fn prepare(&self, activity: &[Activity]) -> Result<Draft, &'static str> {
        self.prepare_with(activity, |_| None)
    }
    /// Explicitly controlled people choose rest themselves; others use thresholds.
    pub fn prepare_with(
        &self,
        activity: &[Activity],
        control: impl Fn(u64) -> Option<bool> + Sync,
    ) -> Result<Draft, &'static str> {
        let input_owned = if activity.par_windows(2).any(|w| w[0].id > w[1].id) {
            let mut rows = activity.to_vec();
            rows.par_sort_unstable_by_key(|a| a.id);
            Some(rows)
        } else {
            None
        };
        let input = input_owned.as_deref().unwrap_or(activity);
        if input.par_windows(2).any(|w| w[0].id == w[1].id) {
            return Err("duplicate fatigue activity");
        }
        let old_owned = if self.people.par_windows(2).any(|w| w[0].id > w[1].id) {
            let mut rows: Vec<_> = self.people.par_iter().cloned().collect();
            rows.par_sort_unstable_by_key(|p| p.id);
            Some(rows)
        } else {
            None
        };
        let old = old_owned.as_deref().unwrap_or(&self.people);
        if old.par_windows(2).any(|w| w[0].id == w[1].id)
            || old.par_chunks(4096).any(|chunk| {
                let mut index = input.partition_point(|a| a.id < chunk[0].id);
                chunk.iter().any(|p| {
                    while index < input.len() && input[index].id < p.id {
                        index += 1;
                    }
                    input.get(index).is_none_or(|a| a.id != p.id)
                })
            })
        {
            return Err("invalid fatigue activity");
        }
        let rules = &self.rules;
        let chunks: Vec<_> = input
            .par_chunks(4096)
            .map(|chunk| {
                let mut index = old.partition_point(|p| p.id < chunk[0].id);
                let mut people = Vec::new();
                let mut consequences = Vec::new();
                for a in chunk {
                    while index < old.len() && old[index].id < a.id {
                        index += 1;
                    }
                    let prior = old.get(index).filter(|p| p.id == a.id);
                    let mut next = prior.cloned().unwrap_or(Person {
                        id: a.id,
                        fatigue: 0,
                        resting: false,
                    });
                    if a.alive {
                        let chosen = control(a.id);
                        if let Some(resting) = chosen {
                            next.resting = resting;
                        }
                        next.fatigue = next
                            .fatigue
                            .saturating_add(rules.baseline_gain)
                            .min(rules.capacity);
                        if a.working {
                            next.fatigue = next
                                .fatigue
                                .saturating_add(rules.work_gain)
                                .min(rules.capacity);
                        } else if next.resting {
                            if a.can_rest {
                                next.fatigue = next.fatigue.saturating_sub(rules.rest_recovery);
                                if next.fatigue <= rules.resume_at {
                                    next.resting = false;
                                }
                            }
                        } else if a.can_rest {
                            next.fatigue = next.fatigue.saturating_sub(rules.idle_recovery);
                        }
                        if a.traveling {
                            next.fatigue = next
                                .fatigue
                                .saturating_add(rules.travel_gain)
                                .min(rules.capacity);
                        }
                        if chosen.is_none() && next.fatigue >= rules.rest_at {
                            next.resting = true;
                        }
                        if next.fatigue >= rules.exhaustion_at && rules.exhaustion_damage > 0 {
                            consequences.push(Consequence {
                                id: a.id,
                                damage: rules.exhaustion_damage,
                            });
                        }
                    }
                    if next.fatigue != 0 || next.resting {
                        people.push(next);
                    }
                }
                (people, consequences)
            })
            .collect();
        let (people, consequences): (Vec<Vec<_>>, Vec<Vec<_>>) = chunks.into_par_iter().unzip();
        let people = people.into_par_iter().flatten().collect();
        let consequences = consequences.into_par_iter().flatten().collect();
        Ok(Draft {
            state: Self {
                rules: self.rules.clone(),
                people,
            },
            consequences,
        })
    }
}

#[cfg(test)]
mod tests;
