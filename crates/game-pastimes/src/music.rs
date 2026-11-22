//! Personal musical phrases from real instrumental or vocal practice.
use super::{Event, Heard, Outcome, Session};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::num::NonZeroU64;

pub type Phrase = [u8; 4];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub person: u64,
    pub activity: u32,
    pub palette: Vec<u8>,
    #[serde(default)]
    pub idle: i64,
    pub compose_value: Option<i64>,
    pub replay_value: Option<i64>,
    pub adopt_value: Option<i64>,
    pub vary_value: Option<i64>,
    pub listen_millis: Option<NonZeroU64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Composed,
    Replayed,
    Adopted,
    Varied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub person: u64,
    pub activity: u32,
    pub phrase: Phrase,
    pub at_millis: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u64>,
    pub choice: Choice,
}

impl Record {
    fn key(&self) -> (u64, u32) {
        (self.person, self.activity)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Playing {
    concert: Record,
    started_at: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Music {
    pub profiles: Vec<Profile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known: Vec<Record>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    playing: Vec<Playing>,
}

impl Music {
    pub fn normalize(&mut self) {
        self.profiles
            .par_sort_unstable_by_key(|row| (row.person, row.activity));
        self.profiles
            .par_iter_mut()
            .for_each(|row| row.palette.sort_unstable());
        self.known.par_sort_unstable_by_key(Record::key);
        self.playing
            .par_sort_unstable_by_key(|row| row.concert.person);
    }

    fn profile(&self, person: u64, activity: u32) -> Option<&Profile> {
        let index = self
            .profiles
            .binary_search_by_key(&(person, activity), |row| (row.person, row.activity))
            .ok()?;
        Some(&self.profiles[index])
    }

    pub fn remembered(&self, person: u64) -> &[Record] {
        let first = self.known.partition_point(|row| row.person < person);
        let end = self.known.partition_point(|row| row.person <= person);
        &self.known[first..end]
    }

    pub(crate) fn valid(
        &self,
        people: &HashSet<u64>,
        active: &[Session],
        activity: &(impl Fn(u32) -> bool + Sync),
        now: u128,
    ) -> bool {
        let valid_record = |row: &Record| {
            self.profile(row.person, row.activity).is_some()
                && row
                    .from
                    .is_none_or(|from| row.person != from && people.contains(&from))
        };
        self.profiles
            .par_windows(2)
            .all(|rows| (rows[0].person, rows[0].activity) < (rows[1].person, rows[1].activity))
            && self.profiles.par_iter().all(|row| {
                people.contains(&row.person)
                    && activity(row.activity)
                    && !row.palette.is_empty()
                    && row.palette.windows(2).all(|notes| notes[0] < notes[1])
            })
            && self
                .known
                .par_windows(2)
                .all(|rows| rows[0].key() < rows[1].key())
            && self.known.par_iter().all(|row| {
                valid_record(row)
                    && row.at_millis <= now
                    && row.choice != Choice::Replayed
                    && (row.from.is_none() == (row.choice == Choice::Composed))
            })
            && self
                .playing
                .par_windows(2)
                .all(|rows| rows[0].concert.person < rows[1].concert.person)
            && self.playing.par_iter().all(|row| {
                let concert = &row.concert;
                valid_record(concert)
                    && matches!(concert.choice, Choice::Composed | Choice::Replayed)
                    && (concert.choice != Choice::Composed || concert.from.is_none())
                    && self
                        .profile(concert.person, concert.activity)
                        .is_some_and(|profile| {
                            concert
                                .phrase
                                .iter()
                                .all(|note| profile.palette.binary_search(note).is_ok())
                        })
                    && active
                        .binary_search_by_key(&concert.person, |session| session.person)
                        .ok()
                        .is_some_and(|index| {
                            let session = active[index];
                            session.activity == concert.activity
                                && session.started_at == row.started_at
                                && session.ends_at == concert.at_millis
                        })
            })
    }

    pub fn open(&mut self, sessions: &[Session], seed: u64) {
        let prepared: Vec<_> = sessions
            .par_iter()
            .filter_map(|session| {
                let profile = self.profile(session.person, session.activity)?;
                let composed = profile.compose_value.filter(|&value| value > profile.idle);
                let repeated = profile
                    .replay_value
                    .filter(|&value| value > profile.idle)
                    .and_then(|value| {
                        let known = self
                            .known
                            .binary_search_by_key(&(session.person, session.activity), Record::key)
                            .ok()?;
                        let known = self.known[known];
                        known
                            .phrase
                            .iter()
                            .all(|note| profile.palette.binary_search(note).is_ok())
                            .then_some((value, known))
                    });
                let (phrase, from, choice) = match repeated {
                    Some((value, known)) if composed.is_none_or(|score| value >= score) => {
                        (known.phrase, known.from, Choice::Replayed)
                    }
                    _ => {
                        composed?;
                        let key = time_key(session.started_at);
                        let phrase = std::array::from_fn(|index| {
                            let note = game_genetics::random(
                                seed ^ u64::from(session.activity),
                                session.person,
                                key ^ index as u64,
                            );
                            profile.palette[(note % profile.palette.len() as u64) as usize]
                        });
                        (phrase, None, Choice::Composed)
                    }
                };
                Some(Playing {
                    concert: Record {
                        person: session.person,
                        activity: session.activity,
                        phrase,
                        at_millis: session.ends_at,
                        from,
                        choice,
                    },
                    started_at: session.started_at,
                })
            })
            .collect();
        if !prepared.is_empty() {
            self.playing.par_extend(prepared);
            self.playing
                .par_sort_unstable_by_key(|row| row.concert.person);
        }
    }

    pub(crate) fn stop(&mut self, person: u64) {
        if let Ok(index) = self
            .playing
            .binary_search_by_key(&person, |row| row.concert.person)
        {
            self.playing.remove(index);
        }
    }

    pub fn settle(&mut self, events: &[Event], heard: &[Heard], seed: u64) -> Vec<Record> {
        if events.is_empty() || self.playing.is_empty() {
            return Vec::new();
        }
        let event_for = |person| {
            events
                .binary_search_by_key(&person, |row| row.person)
                .ok()
                .map(|index| events[index])
        };
        let mut records: Vec<_> = self
            .playing
            .par_iter()
            .filter_map(|row| {
                let event = event_for(row.concert.person)?;
                (event.outcome == Outcome::Completed).then_some(row.concert)
            })
            .collect();
        let replies: Vec<_> = heard
            .par_iter()
            .filter_map(|hearing| {
                let index = records
                    .binary_search_by_key(&hearing.performer, |row| row.person)
                    .ok()?;
                let original = records[index];
                let profile = self.profile(hearing.listener, hearing.activity)?;
                if profile
                    .listen_millis
                    .is_some_and(|min| hearing.attended_millis.unwrap_or(0) < min.get())
                {
                    return None;
                }
                let adopt = profile.adopt_value.filter(|&value| value > profile.idle);
                let vary = profile
                    .vary_value
                    .filter(|&value| value > profile.idle)
                    .and_then(|value| {
                        let key = time_key(hearing.at_millis);
                        let draw =
                            game_genetics::random(seed ^ hearing.performer, hearing.listener, key);
                        let position = (draw % original.phrase.len() as u64) as usize;
                        let old = original.phrase[position];
                        let options = profile.palette.iter().copied().filter(|&note| note != old);
                        let note = options.max_by_key(|&note| {
                            game_genetics::random(draw, u64::from(note), key)
                        })?;
                        let mut phrase = original.phrase;
                        phrase[position] = note;
                        Some((value, phrase))
                    });
                let (phrase, choice) = match vary {
                    Some((value, phrase)) if adopt.is_none_or(|score| value > score) => {
                        (phrase, Choice::Varied)
                    }
                    _ => {
                        adopt?;
                        (original.phrase, Choice::Adopted)
                    }
                };
                Some(Record {
                    person: hearing.listener,
                    activity: hearing.activity,
                    phrase,
                    at_millis: hearing.at_millis,
                    from: Some(hearing.performer),
                    choice,
                })
            })
            .collect();
        self.playing
            .retain(|row| event_for(row.concert.person).is_none());
        records.extend(replies);
        records.par_sort_unstable_by_key(|row| (row.key(), row.from.is_some(), row.from));
        let mut changed: Vec<_> = records
            .par_iter()
            .rev()
            .filter(|row| row.choice != Choice::Replayed)
            .copied()
            .collect();
        changed.dedup_by_key(|row| row.key());
        changed.reverse();
        if !changed.is_empty() {
            let mut known: Vec<_> = self
                .known
                .par_iter()
                .filter(|row| {
                    changed
                        .binary_search_by_key(&row.key(), Record::key)
                        .is_err()
                })
                .copied()
                .collect();
            known.append(&mut changed);
            known.par_sort_unstable_by_key(Record::key);
            self.known = known;
        }
        records
    }
}

fn time_key(now: u128) -> u64 {
    now as u64 ^ ((now >> 64) as u64).rotate_left(17)
}
