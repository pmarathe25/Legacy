//! Personally chosen elapsed leisure and creative practice.
mod culture;
mod training;
pub use culture::{Culture, Heard, Interest as ListeningInterest, Listener, Presentation, Venue};
use game_production_model::Amount;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
pub use training::{Practice, Training};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub id: u32,
    pub name: String,
    pub skill: String,
    pub duration_millis: u64,
    pub satisfaction_millis: u64,
    pub materials: Vec<Amount>,
    pub tools: Vec<Amount>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interest {
    pub activity: u32,
    pub value: i64,
    /// Signed change to desire during the activity's satisfaction window.
    #[serde(default)]
    pub recent_value: i64,
    /// How a practiced hour in this skill changes their interest in this activity.
    #[serde(default)]
    pub practice_hour_value: i64,
    /// Signed response to remembering another person's performance.
    #[serde(default)]
    pub heard_value: i64,
    #[serde(default)]
    pub completions: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_completion: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    #[serde(default)]
    pub idle: i64,
    pub interests: Vec<Interest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub person: u64,
    pub activity: u32,
    pub region: u64,
    pub started_at: u128,
    pub ends_at: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Started,
    Completed,
    Interrupted,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub person: u64,
    pub activity: u32,
    pub at_millis: u128,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pastimes {
    pub activities: Vec<Activity>,
    #[serde(default)]
    pub minds: Vec<Mind>,
    #[serde(default)]
    pub active: Vec<Session>,
    #[serde(default, skip_serializing_if = "Training::is_empty")]
    pub training: Training,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub culture: Option<Box<Culture>>,
}

impl Pastimes {
    pub fn activity(&self, id: u32) -> Option<&Activity> {
        self.activities
            .binary_search_by_key(&id, |row| row.id)
            .ok()
            .map(|slot| &self.activities[slot])
    }

    pub fn session(&self, person: u64) -> Option<&Session> {
        self.active
            .binary_search_by_key(&person, |row| row.person)
            .ok()
            .map(|slot| &self.active[slot])
    }

    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        goods: &HashSet<u32>,
        now: u128,
    ) -> Result<(), String> {
        let valid_amounts = |rows: &[Amount]| {
            rows.par_windows(2).all(|pair| pair[0].good < pair[1].good)
                && rows
                    .par_iter()
                    .all(|row| row.quantity > 0 && goods.contains(&row.good))
        };
        if !self.training.validate(people, &self.activities)
            || self.culture.as_ref().is_some_and(|culture| {
                !culture.valid(people, |id| self.activity(id).is_some(), now)
            })
            || self
                .activities
                .par_windows(2)
                .any(|pair| pair[0].id >= pair[1].id)
            || self.activities.par_iter().any(|row| {
                row.id == 0
                    || row.name.trim().is_empty()
                    || row.skill.trim().is_empty()
                    || row.duration_millis == 0
                    || !valid_amounts(&row.materials)
                    || !valid_amounts(&row.tools)
                    || row.tools.par_iter().any(|tool| {
                        row.materials
                            .binary_search_by_key(&tool.good, |input| input.good)
                            .is_ok()
                    })
            })
            || self
                .minds
                .par_windows(2)
                .any(|pair| pair[0].person >= pair[1].person)
            || self.minds.par_iter().any(|mind| {
                !people.contains(&mind.person)
                    || mind.interests.len() > 16
                    || mind
                        .interests
                        .par_windows(2)
                        .any(|pair| pair[0].activity >= pair[1].activity)
                    || mind.interests.par_iter().any(|interest| {
                        self.activity(interest.activity).is_none()
                            || (interest.completions == 0) != interest.last_completion.is_none()
                            || interest.last_completion.is_some_and(|at| at > now)
                    })
            })
            || self
                .active
                .par_windows(2)
                .any(|pair| pair[0].person >= pair[1].person)
            || self.active.par_iter().any(|row| {
                !people.contains(&row.person)
                    || !regions.contains(&row.region)
                    || row.started_at > now
                    || self.activity(row.activity).is_none_or(|kind| {
                        row.started_at.checked_add(u128::from(kind.duration_millis))
                            != Some(row.ends_at)
                    })
            })
        {
            return Err("invalid pastime state".into());
        }
        Ok(())
    }

    /// Consider a bounded set of activities for each individual.
    pub fn choose(
        &self,
        now: u128,
        willing_actor: impl Fn(u64) -> Option<u64> + Sync,
        usable: impl Fn(u64, &Activity) -> bool + Sync,
    ) -> Vec<Session> {
        self.minds
            .par_iter()
            .filter_map(|mind| {
                let region = willing_actor(mind.person)?;
                if self.session(mind.person).is_some() {
                    return None;
                }
                let (_, activity) =
                    self.preferred(mind, now, |activity| usable(mind.person, activity))?;
                self.contemplate(mind.person, region, activity.id, now)
            })
            .collect()
    }

    fn preferred(
        &self,
        mind: &Mind,
        now: u128,
        usable: impl Fn(&Activity) -> bool,
    ) -> Option<(i64, &Activity)> {
        mind.interests
            .iter()
            .filter_map(|interest| {
                let activity = self.activity(interest.activity)?;
                let recent = interest.last_completion.is_some_and(|at| {
                    now.saturating_sub(at) < u128::from(activity.satisfaction_millis)
                });
                let value =
                    interest
                        .value
                        .saturating_add(if recent { interest.recent_value } else { 0 });
                let value =
                    if interest.practice_hour_value == 0 {
                        value
                    } else {
                        value.saturating_add(interest.practice_hour_value.saturating_mul(
                            self.training.practice_hours(mind.person, &activity.skill),
                        ))
                    };
                let value = if interest.heard_value != 0
                    && self.culture.as_ref().is_some_and(|culture| {
                        culture
                            .remembered(mind.person)
                            .any(|heard| heard.activity == activity.id)
                    }) {
                    value.saturating_add(interest.heard_value)
                } else {
                    value
                };
                (value > mind.idle && usable(activity)).then_some((value, activity))
            })
            .max_by_key(|(value, activity)| (*value, std::cmp::Reverse(activity.id)))
    }

    pub fn preferred_value(
        &self,
        person: u64,
        now: u128,
        usable: impl Fn(&Activity) -> bool,
    ) -> Option<i64> {
        let index = self
            .minds
            .binary_search_by_key(&person, |row| row.person)
            .ok()?;
        self.preferred(&self.minds[index], now, usable)
            .map(|row| row.0)
    }

    pub fn contemplate(
        &self,
        person: u64,
        region: u64,
        activity: u32,
        now: u128,
    ) -> Option<Session> {
        let ends_at = now.checked_add(u128::from(self.activity(activity)?.duration_millis))?;
        self.session(person).is_none().then_some(Session {
            person,
            activity,
            region,
            started_at: now,
            ends_at,
        })
    }

    pub fn next_finish(&self, now: u128) -> Option<u64> {
        let deadline = self.active.par_iter().map(|row| row.ends_at).min()?;
        u64::try_from(deadline.saturating_sub(now)).ok()
    }

    /// All selected rows have passed physical custody, including consumable inputs.
    pub fn start(&mut self, selected: Vec<Session>) -> Vec<Event> {
        let events = selected
            .par_iter()
            .map(|row| Event {
                person: row.person,
                activity: row.activity,
                at_millis: row.started_at,
                outcome: Outcome::Started,
            })
            .collect();
        self.active.par_extend(selected);
        self.active.par_sort_unstable_by_key(|row| row.person);
        events
    }

    pub fn stop(&mut self, person: u64, now: u128) -> Option<Event> {
        let slot = self
            .active
            .binary_search_by_key(&person, |row| row.person)
            .ok()?;
        let row = self.active.remove(slot);
        Some(Event {
            person,
            activity: row.activity,
            at_millis: now,
            outcome: Outcome::Stopped,
        })
    }

    pub fn settle(
        &mut self,
        now: u128,
        can_continue: impl Fn(&Session, &Activity) -> bool + Sync,
    ) -> Vec<Event> {
        let events: Vec<_> = self
            .active
            .par_iter()
            .filter_map(|session| {
                let usable = can_continue(
                    session,
                    self.activity(session.activity).expect("known pastime"),
                );
                let outcome = if !usable {
                    Outcome::Interrupted
                } else if session.ends_at <= now {
                    Outcome::Completed
                } else {
                    return None;
                };
                Some(Event {
                    person: session.person,
                    activity: session.activity,
                    at_millis: now,
                    outcome,
                })
            })
            .collect();
        if events.is_empty() {
            return events;
        }
        self.training.credit(&events, &self.activities);
        self.active.retain(|row| {
            events
                .binary_search_by_key(&row.person, |event| event.person)
                .is_err()
        });
        self.minds.par_iter_mut().for_each(|mind| {
            if let Ok(slot) = events.binary_search_by_key(&mind.person, |row| row.person) {
                let event = events[slot];
                if event.outcome == Outcome::Completed
                    && let Ok(index) = mind
                        .interests
                        .binary_search_by_key(&event.activity, |row| row.activity)
                {
                    let interest = &mut mind.interests[index];
                    interest.completions = interest.completions.saturating_add(1);
                    interest.last_completion = Some(now);
                }
            }
        });
        events
    }
}
