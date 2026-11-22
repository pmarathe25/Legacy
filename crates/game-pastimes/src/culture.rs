//! Chosen attention to a real local performance.
use super::{Event, Outcome};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub type Venue = (u64, (u64, u64));

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub person: u64,
    pub activity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heard {
    pub listener: u64,
    pub performer: u64,
    pub activity: u32,
    pub at_millis: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interest {
    pub activity: u32,
    pub value: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heard: Option<Heard>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    pub person: u64,
    #[serde(default)]
    pub idle: i64,
    pub interests: Vec<Interest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Culture {
    pub presentations: Vec<Presentation>,
    pub listeners: Vec<Listener>,
}

impl Culture {
    pub fn sort(&mut self) {
        self.presentations.sort_unstable();
        self.listeners.sort_unstable_by_key(|row| row.person);
        for listener in &mut self.listeners {
            listener.interests.sort_unstable_by_key(|row| row.activity);
        }
    }

    pub(crate) fn valid(
        &self,
        people: &HashSet<u64>,
        activity_exists: impl Fn(u32) -> bool + Sync,
        now: u128,
    ) -> bool {
        self.presentations.par_windows(2).all(|p| p[0] < p[1])
            && self
                .presentations
                .par_iter()
                .all(|row| people.contains(&row.person) && activity_exists(row.activity))
            && self
                .listeners
                .par_windows(2)
                .all(|p| p[0].person < p[1].person)
            && self.listeners.par_iter().all(|row| {
                people.contains(&row.person)
                    && row.interests.len() <= 16
                    && row
                        .interests
                        .windows(2)
                        .all(|p| p[0].activity < p[1].activity)
                    && row.interests.iter().all(|interest| {
                        activity_exists(interest.activity)
                            && interest.last_heard.as_ref().is_none_or(|heard| {
                                heard.listener == row.person
                                    && heard.activity == interest.activity
                                    && heard.performer != row.person
                                    && people.contains(&heard.performer)
                                    && heard.at_millis <= now
                            })
                    })
            })
    }

    pub fn remembered(&self, person: u64) -> impl Iterator<Item = Heard> + '_ {
        self.listeners
            .binary_search_by_key(&person, |row| row.person)
            .ok()
            .into_iter()
            .flat_map(|index| {
                self.listeners[index]
                    .interests
                    .iter()
                    .filter_map(|row| row.last_heard)
            })
    }

    pub fn set_interest(&mut self, person: u64, activity: u32, value: i64) -> bool {
        let listener = match self
            .listeners
            .binary_search_by_key(&person, |row| row.person)
        {
            Ok(index) => &mut self.listeners[index],
            Err(index) => {
                self.listeners.insert(
                    index,
                    Listener {
                        person,
                        idle: 0,
                        interests: Vec::new(),
                    },
                );
                &mut self.listeners[index]
            }
        };
        match listener
            .interests
            .binary_search_by_key(&activity, |row| row.activity)
        {
            Ok(index) => listener.interests[index].value = value,
            Err(index) if listener.interests.len() < 16 => listener.interests.insert(
                index,
                Interest {
                    activity,
                    value,
                    last_heard: None,
                },
            ),
            Err(_) => return false,
        }
        true
    }

    pub fn set_presentation(&mut self, presentation: Presentation, enabled: bool) {
        match self.presentations.binary_search(&presentation) {
            Ok(index) if !enabled => {
                self.presentations.remove(index);
            }
            Err(index) if enabled => self.presentations.insert(index, presentation),
            _ => (),
        }
    }

    /// The final passage is an opportunity. A listener can give it attention
    /// or keep doing something they value more. No full-session attendance is inferred.
    pub fn hear(
        &self,
        events: &[Event],
        venue: impl Fn(u64) -> Option<Venue> + Sync,
        available: impl Fn(u64) -> bool + Sync,
        competing_value: impl Fn(u64) -> Option<i64> + Sync,
    ) -> Vec<Heard> {
        if events.is_empty() || self.presentations.is_empty() || self.listeners.is_empty() {
            return Vec::new();
        }
        let mut openings: Vec<_> = events
            .par_iter()
            .filter_map(|event| {
                (event.outcome == Outcome::Completed
                    && self
                        .presentations
                        .binary_search(&Presentation {
                            person: event.person,
                            activity: event.activity,
                        })
                        .is_ok())
                .then_some(())?;
                Some((
                    venue(event.person)?,
                    event.activity,
                    event.person,
                    event.at_millis,
                ))
            })
            .collect();
        if openings.is_empty() {
            return Vec::new();
        }
        openings.par_sort_unstable();
        self.listeners
            .par_iter()
            .filter_map(|listener| {
                if !available(listener.person) {
                    return None;
                }
                let venue = venue(listener.person)?;
                let floor = competing_value(listener.person)
                    .map_or(listener.idle, |value| value.max(listener.idle));
                let (index, performer, at_millis) = listener
                    .interests
                    .iter()
                    .enumerate()
                    .filter_map(|(index, interest)| {
                        if interest.value <= floor {
                            return None;
                        }
                        let first = openings
                            .partition_point(|row| (row.0, row.1) < (venue, interest.activity));
                        let performer = openings[first..]
                            .iter()
                            .take_while(|row| (row.0, row.1) == (venue, interest.activity))
                            .find(|row| row.2 != listener.person)?;
                        Some((index, performer.2, performer.3))
                    })
                    .max_by_key(|(index, performer, _)| {
                        let interest = &listener.interests[*index];
                        (
                            interest.value,
                            std::cmp::Reverse(interest.activity),
                            std::cmp::Reverse(*performer),
                        )
                    })?;
                Some(Heard {
                    listener: listener.person,
                    activity: listener.interests[index].activity,
                    performer,
                    at_millis,
                })
            })
            .collect()
    }

    pub fn remember(&mut self, hearings: &[Heard]) {
        if hearings.is_empty() {
            return;
        }
        self.listeners.par_iter_mut().for_each(|listener| {
            if let Ok(index) = hearings.binary_search_by_key(&listener.person, |row| row.listener) {
                let hearing = hearings[index];
                let interest = listener
                    .interests
                    .binary_search_by_key(&hearing.activity, |row| row.activity)
                    .expect("chosen interest");
                listener.interests[interest].last_heard = Some(hearing);
            }
        });
    }
}
