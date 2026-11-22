//! A person's actual completed practice across activities using the same skill.
use super::{Activity, Event, Outcome};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Practice {
    pub person: u64,
    pub skill: String,
    pub millis: u128,
    pub completed: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Training {
    pub records: Vec<Practice>,
}

impl Training {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn for_person(&self, person: u64) -> &[Practice] {
        let start = self.records.partition_point(|row| row.person < person);
        let end = self.records.partition_point(|row| row.person <= person);
        &self.records[start..end]
    }

    pub(crate) fn validate(&self, people: &HashSet<u64>, activities: &[Activity]) -> bool {
        if self.records.is_empty() {
            return true;
        }
        let skills: HashSet<_> = activities.iter().map(|row| row.skill.as_str()).collect();
        self.records.par_windows(2).all(|rows| {
            (rows[0].person, rows[0].skill.as_str()) < (rows[1].person, rows[1].skill.as_str())
        }) && self.records.par_iter().all(|row| {
            people.contains(&row.person)
                && skills.contains(row.skill.as_str())
                && row.millis > 0
                && row.completed > 0
        })
    }

    pub(crate) fn practice_hours(&self, person: u64, skill: &str) -> i64 {
        let records = self.for_person(person);
        records
            .binary_search_by(|row| row.skill.as_str().cmp(skill))
            .ok()
            .map_or(0, |index| {
                (records[index].millis / 3_600_000).min(i64::MAX as u128) as i64
            })
    }

    pub(crate) fn credit(&mut self, events: &[Event], activities: &[Activity]) {
        let earned: Vec<_> = events
            .par_iter()
            .filter_map(|event| {
                if event.outcome != Outcome::Completed {
                    return None;
                }
                let index = activities
                    .binary_search_by_key(&event.activity, |row| row.id)
                    .ok()?;
                let activity = &activities[index];
                Some((
                    event.person,
                    activity.skill.as_str(),
                    u128::from(activity.duration_millis),
                ))
            })
            .collect();
        if earned.is_empty() {
            return;
        }
        self.records.par_iter_mut().for_each(|row| {
            if let Ok(index) = earned.binary_search_by_key(&row.person, |row| row.0)
                && earned[index].1 == row.skill
            {
                row.millis = row.millis.saturating_add(earned[index].2);
                row.completed = row.completed.saturating_add(1);
            }
        });
        let new: Vec<_> = earned
            .par_iter()
            .filter_map(|&(person, skill, millis)| {
                self.records
                    .binary_search_by(|row| (row.person, row.skill.as_str()).cmp(&(person, skill)))
                    .is_err()
                    .then(|| Practice {
                        person,
                        skill: skill.to_owned(),
                        millis,
                        completed: 1,
                    })
            })
            .collect();
        if !new.is_empty() {
            self.records.par_extend(new);
            self.records.par_sort_unstable_by(|a, b| {
                (a.person, a.skill.as_str()).cmp(&(b.person, b.skill.as_str()))
            });
        }
    }
}
