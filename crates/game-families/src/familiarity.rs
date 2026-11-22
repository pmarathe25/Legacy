//! A person's familiarity with caregivers learned from actual childhood visits.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

const CONTACT_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Learning {
    pub before_age: u64,
    pub visits: NonZeroU32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    /// An explicit absent learning policy overrides a scenario default.
    pub learning: Option<Learning>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Familiarity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_learning: Option<Learning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<Preference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub learners: Vec<Learner>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Learner {
    pub person: u64,
    pub contacts: Vec<Contact>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    pub caregiver: u64,
    pub visits: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognized_at: Option<u128>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    pub child: u64,
    pub age: u64,
    pub caregiver: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recognition {
    pub child: u64,
    pub caregiver: u64,
    pub at_millis: u128,
}

impl Familiarity {
    pub fn normalize(&mut self) {
        self.preferences.par_sort_unstable_by_key(|row| row.person);
        self.learners.par_sort_unstable_by_key(|row| row.person);
        self.learners.par_iter_mut().for_each(|row| {
            row.contacts
                .sort_unstable_by_key(|contact| contact.caregiver);
        });
    }

    fn policy(&self, child: u64) -> Option<Learning> {
        match self
            .preferences
            .binary_search_by_key(&child, |row| row.person)
        {
            Ok(index) => self.preferences[index].learning,
            Err(_) => self.default_learning,
        }
    }

    pub fn recognizes(&self, person: u64, caregiver: u64) -> bool {
        let Some(learner) = self.learner(person) else {
            return false;
        };
        learner
            .contacts
            .binary_search_by_key(&caregiver, |row| row.caregiver)
            .ok()
            .is_some_and(|index| learner.contacts[index].recognized_at.is_some())
    }

    fn learner(&self, person: u64) -> Option<&Learner> {
        let index = self
            .learners
            .binary_search_by_key(&person, |row| row.person)
            .ok()?;
        Some(&self.learners[index])
    }

    pub fn validate(
        &self,
        adult_age: u64,
        now: u128,
        known_learner: impl Fn(u64) -> bool + Sync,
        known_caregiver: impl Fn(u64) -> bool + Sync,
    ) -> Result<(), &'static str> {
        let age_valid = |policy: Learning| policy.before_age > 0 && policy.before_age <= adult_age;
        let valid = self.default_learning.is_none_or(age_valid)
            && self
                .preferences
                .par_windows(2)
                .all(|pair| pair[0].person < pair[1].person)
            && self
                .preferences
                .par_iter()
                .all(|row| known_learner(row.person) && row.learning.is_none_or(age_valid))
            && self
                .learners
                .par_windows(2)
                .all(|pair| pair[0].person < pair[1].person)
            && self.learners.par_iter().all(|row| {
                known_learner(row.person)
                    && (1..=CONTACT_LIMIT).contains(&row.contacts.len())
                    && row
                        .contacts
                        .windows(2)
                        .all(|pair| pair[0].caregiver < pair[1].caregiver)
                    && self.policy(row.person).is_some_and(|policy| {
                        row.contacts.iter().all(|contact| {
                            contact.caregiver != row.person
                                && known_caregiver(contact.caregiver)
                                && (1..=policy.visits.get()).contains(&contact.visits)
                                && (contact.visits == policy.visits.get())
                                    == contact.recognized_at.is_some()
                                && contact.recognized_at.is_none_or(|at| at <= now)
                        })
                    })
            });
        valid.then_some(()).ok_or("invalid childhood familiarity")
    }

    pub fn observe_care(&mut self, experiences: &[Experience], now: u128) -> Vec<Recognition> {
        // The care owner publishes at most one visit per child at an opportunity.
        let mut changes: Vec<_> = experiences
            .par_iter()
            .filter_map(|experience| {
                let policy = self.policy(experience.child)?;
                (experience.child != experience.caregiver && experience.age < policy.before_age)
                    .then_some(())?;
                let previous = self.learner(experience.child);
                let lookup = previous.map_or(Err(0), |row| {
                    row.contacts
                        .binary_search_by_key(&experience.caregiver, |contact| contact.caregiver)
                });
                match (lookup, previous) {
                    (Ok(index), Some(row)) if row.contacts[index].recognized_at.is_some() => {
                        return None;
                    }
                    (Err(_), Some(row)) if row.contacts.len() == CONTACT_LIMIT => return None,
                    _ => {}
                }
                let mut next = previous.cloned().unwrap_or_else(|| Learner {
                    person: experience.child,
                    contacts: Vec::new(),
                });
                let index = match lookup {
                    Ok(index) => index,
                    Err(index) => {
                        next.contacts.insert(
                            index,
                            Contact {
                                caregiver: experience.caregiver,
                                visits: 0,
                                recognized_at: None,
                            },
                        );
                        index
                    }
                };
                let contact = &mut next.contacts[index];
                contact.visits += 1;
                let recognition = (contact.visits == policy.visits.get()).then(|| {
                    contact.recognized_at = Some(now);
                    Recognition {
                        child: experience.child,
                        caregiver: experience.caregiver,
                        at_millis: now,
                    }
                });
                Some((next, previous.is_some(), recognition))
            })
            .collect();
        if changes.is_empty() {
            return Vec::new();
        }
        changes.par_sort_unstable_by_key(|row| row.0.person);
        self.learners.par_chunks_mut(512).for_each(|chunk| {
            let start = changes.partition_point(|row| row.0.person < chunk[0].person);
            for (learner, existing, _) in &changes[start..] {
                if learner.person > chunk[chunk.len() - 1].person {
                    break;
                }
                if *existing {
                    let slot = chunk
                        .binary_search_by_key(&learner.person, |row| row.person)
                        .expect("prepared existing learner");
                    chunk[slot].contacts.clone_from(&learner.contacts);
                }
            }
        });
        let recognized = changes.par_iter().filter_map(|row| row.2).collect();
        let additions: Vec<_> = changes
            .into_par_iter()
            .filter_map(|(learner, existing, _)| (!existing).then_some(learner))
            .collect();
        if !additions.is_empty() {
            self.learners.par_extend(additions);
            self.learners.par_sort_unstable_by_key(|row| row.person);
        }
        recognized
    }
}
