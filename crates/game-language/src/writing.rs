//! Personal associations between audible forms and visible marks.
//!
//! This owner does not create physical pages or imply that another person
//! understands a mark. Hosts must ground observations in actual encounters.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Association {
    pub sounds: Vec<u8>,
    pub marks: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    #[serde(default)]
    pub associations: Vec<Association>,
}

/// Each person's own choice to show a carried page and/or adopt an explanation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharingPolicy {
    pub person: u64,
    #[serde(default)]
    pub show: bool,
    #[serde(default)]
    pub adopt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_fear_at_least: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_obligation_at_least: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopt_fear_at_least: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopt_obligation_at_least: Option<u64>,
    /// Optional bounded retention of this person's observations, independent of adoption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remember_capacity: Option<usize>,
    /// An optional deliberately stated sound, which need not match their own mapping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_sounds: Option<Vec<u8>>,
    /// The receiver may deliberately adopt a different visible form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopted_marks: Option<Vec<u32>>,
}

/// A physical page was shown during an actual meeting; intended meaning is private.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Explanation {
    pub tick: u64,
    pub speaker: u64,
    pub listener: u64,
    pub page: u64,
    pub marks: Vec<u32>,
    pub spoken: Vec<u8>,
    pub heard: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopted: Option<Association>,
}

/// A person's retained observation, not the speaker's private intended meaning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub person: u64,
    pub page: u64,
    pub speaker: u64,
    pub marks: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heard: Option<Vec<u8>>,
    pub first_seen_at: u64,
    pub seen_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Writing {
    pub capacity: usize,
    #[serde(default)]
    pub minds: Vec<Mind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sharing: Vec<SharingPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explanations: Vec<Explanation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<Observation>,
}

impl Writing {
    /// Validate an owner's saved associations against the actual sound palette.
    pub fn validate(&self, sounds: usize, people: &[u64]) -> Result<(), &'static str> {
        if self.capacity == 0
            || self.capacity > 1024
            || people.windows(2).any(|row| row[0] >= row[1])
            || self
                .minds
                .par_windows(2)
                .any(|row| row[0].person >= row[1].person)
            || self.minds.par_iter().any(|mind| {
                people.binary_search(&mind.person).is_err()
                    || mind.associations.len() > self.capacity
                    || mind
                        .associations
                        .windows(2)
                        .any(|row| row[0].sounds >= row[1].sounds)
                    || mind.associations.iter().any(|row| !valid(row, sounds))
            })
            || self
                .sharing
                .par_windows(2)
                .any(|row| row[0].person >= row[1].person)
            || self.sharing.par_iter().any(|policy| {
                people.binary_search(&policy.person).is_err()
                    || policy
                        .remember_capacity
                        .is_some_and(|capacity| capacity == 0 || capacity > 1024)
                    || policy.claimed_sounds.as_ref().is_some_and(|form| {
                        form.is_empty()
                            || form.len() > 16
                            || form.iter().any(|sound| usize::from(*sound) >= sounds)
                    })
                    || policy.adopted_marks.as_ref().is_some_and(|marks| {
                        marks.is_empty() || marks.len() > 16 || marks.contains(&0)
                    })
            })
            || self.explanations.par_iter().any(|event| {
                event.speaker == event.listener
                    || people.binary_search(&event.speaker).is_err()
                    || people.binary_search(&event.listener).is_err()
                    || event.page == 0
                    || !valid_parts(&event.spoken, &event.marks, sounds)
                    || event
                        .adopted
                        .as_ref()
                        .is_some_and(|form| !event.heard || !valid(form, sounds))
            })
            || self
                .observations
                .par_windows(2)
                .any(|rows| (rows[0].person, rows[0].page) >= (rows[1].person, rows[1].page))
            || self.observations.par_iter().any(|row| {
                people.binary_search(&row.person).is_err()
                    || people.binary_search(&row.speaker).is_err()
                    || row.person == row.speaker
                    || row.page == 0
                    || row.first_seen_at > row.seen_at
                    || row.marks.is_empty()
                    || row.marks.len() > 16
                    || row.marks.contains(&0)
                    || row.heard.as_ref().is_some_and(|sound| {
                        sound.is_empty()
                            || sound.len() > 16
                            || sound.iter().any(|sound| usize::from(*sound) >= sounds)
                    })
            })
            || self
                .observations
                .par_chunk_by(|a, b| a.person == b.person)
                .any(|rows| {
                    rows.len() > 1024
                        || self
                            .sharing
                            .binary_search_by_key(&rows[0].person, |policy| policy.person)
                            .ok()
                            .and_then(|index| self.sharing[index].remember_capacity)
                            .is_some_and(|capacity| rows.len() > capacity)
                })
        {
            return Err("invalid personal writing associations");
        }
        Ok(())
    }

    /// Record one person's chosen mapping, replacing only their own earlier form.
    pub fn assign(
        &mut self,
        person: u64,
        association: Association,
        sounds: usize,
    ) -> Result<(), &'static str> {
        if person == 0 || !valid(&association, sounds) || self.capacity == 0 || self.capacity > 1024
        {
            return Err("invalid writing choice");
        }
        let index = match self.minds.binary_search_by_key(&person, |mind| mind.person) {
            Ok(index) => index,
            Err(index) => {
                self.minds.insert(
                    index,
                    Mind {
                        person,
                        associations: Vec::new(),
                    },
                );
                index
            }
        };
        let mind = &mut self.minds[index];
        match mind
            .associations
            .binary_search_by(|row| row.sounds.cmp(&association.sounds))
        {
            Ok(index) => mind.associations[index] = association,
            Err(index) if mind.associations.len() < self.capacity => {
                mind.associations.insert(index, association)
            }
            Err(_) => return Err("writing association capacity reached"),
        }
        Ok(())
    }

    /// Stage disjoint personal choices together; any invalid choice is inert.
    pub fn assign_batch(
        &mut self,
        choices: &[(u64, Association)],
        sounds: usize,
    ) -> Result<(), &'static str> {
        if self.capacity == 0
            || self.capacity > 1024
            || choices.par_windows(2).any(|row| row[0].0 >= row[1].0)
            || choices.par_iter().any(|(person, association)| {
                *person == 0
                    || !valid(association, sounds)
                    || self
                        .minds
                        .binary_search_by_key(person, |mind| mind.person)
                        .ok()
                        .is_some_and(|index| {
                            let mind = &self.minds[index];
                            mind.associations.len() >= self.capacity
                                && mind
                                    .associations
                                    .binary_search_by(|row| row.sounds.cmp(&association.sounds))
                                    .is_err()
                        })
            })
        {
            return Err("invalid writing choices");
        }
        let mut next: Vec<_> = self
            .minds
            .par_iter()
            .map(|mind| {
                let mut mind = mind.clone();
                if let Ok(index) = choices.binary_search_by_key(&mind.person, |choice| choice.0) {
                    let association = choices[index].1.clone();
                    match mind
                        .associations
                        .binary_search_by(|row| row.sounds.cmp(&association.sounds))
                    {
                        Ok(index) => mind.associations[index] = association,
                        Err(index) => mind.associations.insert(index, association),
                    }
                }
                mind
            })
            .collect();
        next.par_extend(
            choices
                .par_iter()
                .filter(|(person, _)| {
                    self.minds
                        .binary_search_by_key(person, |mind| mind.person)
                        .is_err()
                })
                .map(|(person, association)| Mind {
                    person: *person,
                    associations: vec![association.clone()],
                }),
        );
        next.par_sort_unstable_by_key(|mind| mind.person);
        self.minds = next;
        Ok(())
    }

    /// Retain each person's chosen observations, evicting their own oldest records.
    pub fn remember(
        &mut self,
        additions: &[Observation],
        sounds: usize,
    ) -> Result<(), &'static str> {
        if additions.par_iter().any(|row| {
            row.person == 0
                || row.speaker == 0
                || row.person == row.speaker
                || row.page == 0
                || row.first_seen_at > row.seen_at
                || row.marks.is_empty()
                || row.marks.len() > 16
                || row.marks.contains(&0)
                || row.heard.as_ref().is_some_and(|form| {
                    form.is_empty()
                        || form.len() > 16
                        || form.iter().any(|sound| usize::from(*sound) >= sounds)
                })
                || self
                    .sharing
                    .binary_search_by_key(&row.person, |policy| policy.person)
                    .ok()
                    .is_none_or(|index| self.sharing[index].remember_capacity.is_none())
        }) {
            return Err("invalid written observations");
        }
        if additions.is_empty() {
            return Ok(());
        }
        let mut records: Vec<_> = self
            .observations
            .par_iter()
            .cloned()
            .map(|row| (row, false))
            .collect();
        records.par_extend(additions.par_iter().cloned().map(|row| (row, true)));
        records.par_sort_unstable_by(|(a, a_new), (b, b_new)| {
            (
                a.person,
                a.page,
                a.seen_at,
                *a_new,
                a.speaker,
                &a.marks,
                &a.heard,
                a.first_seen_at,
            )
                .cmp(&(
                    b.person,
                    b.page,
                    b.seen_at,
                    *b_new,
                    b.speaker,
                    &b.marks,
                    &b.heard,
                    b.first_seen_at,
                ))
        });
        let mut unique: Vec<_> = records
            .par_chunk_by(|a, b| (a.0.person, a.0.page) == (b.0.person, b.0.page))
            .map(|group| {
                let mut latest = group.last().expect("nonempty observation group").0.clone();
                latest.first_seen_at = group
                    .iter()
                    .map(|row| row.0.first_seen_at)
                    .min()
                    .expect("nonempty group");
                latest
            })
            .collect();
        unique.par_sort_unstable_by_key(|row| {
            (
                row.person,
                std::cmp::Reverse(row.seen_at),
                std::cmp::Reverse(row.page),
            )
        });
        let mut retained: Vec<_> = unique
            .par_chunk_by(|a, b| a.person == b.person)
            .flat_map_iter(|group| {
                let capacity = self
                    .sharing
                    .binary_search_by_key(&group[0].person, |policy| policy.person)
                    .ok()
                    .and_then(|index| self.sharing[index].remember_capacity)
                    .unwrap_or(1024);
                group.iter().take(capacity).cloned()
            })
            .collect();
        retained.par_sort_unstable_by_key(|row| (row.person, row.page));
        self.observations = retained;
        Ok(())
    }

    /// Look up the visible form a person has chosen for a spoken form.
    pub fn marks(&self, person: u64, sounds: &[u8]) -> Option<&[u32]> {
        let index = self
            .minds
            .binary_search_by_key(&person, |mind| mind.person)
            .ok()?;
        let associations = &self.minds[index].associations;
        let index = associations
            .binary_search_by(|row| row.sounds.as_slice().cmp(sounds))
            .ok()?;
        Some(&associations[index].marks)
    }

    /// Interpret only through this reader's own associations; homographs remain ambiguous.
    pub fn interpretations(&self, person: u64, marks: &[u32]) -> Vec<Vec<u8>> {
        self.minds
            .binary_search_by_key(&person, |mind| mind.person)
            .ok()
            .map_or_else(Vec::new, |index| {
                self.minds[index]
                    .associations
                    .iter()
                    .filter(|row| row.marks == marks)
                    .map(|row| row.sounds.clone())
                    .collect()
            })
    }
}

fn valid(row: &Association, sounds: usize) -> bool {
    valid_parts(&row.sounds, &row.marks, sounds)
}

fn valid_parts(form: &[u8], marks: &[u32], sounds: usize) -> bool {
    !form.is_empty()
        && form.len() <= 16
        && form.iter().all(|&sound| usize::from(sound) < sounds)
        && !marks.is_empty()
        && marks.len() <= 16
        && !marks.contains(&0)
}
