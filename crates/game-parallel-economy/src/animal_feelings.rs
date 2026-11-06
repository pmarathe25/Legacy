//! Private, individually interpreted experiences with animals.
use game_parallel_fauna::Fauna;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opinion {
    pub animal: u64,
    pub affinity: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    pub minimum: i32,
    pub maximum: i32,
    pub capacity: usize,
    #[serde(default)]
    pub on_feed: i32,
    #[serde(default)]
    pub on_hunt: i32,
    #[serde(default)]
    pub on_handle_accepted: i32,
    #[serde(default)]
    pub on_handle_refused: i32,
    #[serde(default)]
    pub on_capture_yielded: i32,
    #[serde(default)]
    pub on_capture_forced: i32,
    #[serde(default)]
    pub on_capture_resisted: i32,
    #[serde(default)]
    pub on_animal_released: i32,
    #[serde(default)]
    pub on_animal_release_blocked: i32,
    #[serde(default)]
    pub on_defense_succeeded: i32,
    #[serde(default)]
    pub on_defense_overcome: i32,
    #[serde(default)]
    pub opinions: Vec<Opinion>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimalFeelings {
    pub minds: Vec<Mind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Experience {
    Feed,
    Hunt,
    HandleAccepted,
    HandleRefused,
    CaptureYielded,
    CaptureForced,
    CaptureResisted,
    Released,
    ReleaseBlocked,
    DefenseSucceeded,
    DefenseOvercome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Event {
    pub person: u64,
    pub animal: u64,
    pub kind: Experience,
}

impl AnimalFeelings {
    pub fn normalize_and_validate(
        &mut self,
        people: &HashSet<u64>,
        fauna: Option<&Fauna>,
    ) -> Result<(), String> {
        let animals: HashSet<_> = fauna
            .into_iter()
            .flat_map(|f| f.animals.iter().map(|a| a.id))
            .collect();
        self.minds.par_sort_unstable_by_key(|m| m.person);
        self.minds
            .par_iter_mut()
            .for_each(|mind| mind.opinions.sort_unstable_by_key(|o| o.animal));
        if self
            .minds
            .par_windows(2)
            .any(|p| p[0].person == p[1].person)
            || self.minds.par_iter().any(|mind| {
                !people.contains(&mind.person)
                    || mind.minimum > 0
                    || mind.maximum < 0
                    || mind.capacity > 64
                    || mind.opinions.len() > mind.capacity
                    || mind.opinions.windows(2).any(|p| p[0].animal == p[1].animal)
                    || mind.opinions.iter().any(|opinion| {
                        !animals.contains(&opinion.animal)
                            || opinion.affinity < mind.minimum
                            || opinion.affinity > mind.maximum
                    })
            })
        {
            return Err("invalid personal animal feelings".into());
        }
        Ok(())
    }
    /// Neutral is the default for an unremembered animal; absence of a mind remains distinct.
    pub fn affinity(&self, person: u64, animal: u64) -> Option<i32> {
        let mind = self
            .minds
            .binary_search_by_key(&person, |m| m.person)
            .ok()
            .map(|i| &self.minds[i])?;
        Some(
            mind.opinions
                .binary_search_by_key(&animal, |o| o.animal)
                .ok()
                .map_or(0, |i| mind.opinions[i].affinity),
        )
    }
    pub fn relevant(&self, events: Vec<Event>) -> Vec<Event> {
        let dense = self
            .minds
            .first()
            .zip(self.minds.last())
            .and_then(|(first, last)| {
                (last.person.checked_sub(first.person) == Some((self.minds.len() - 1) as u64))
                    .then_some(first.person)
            });
        events
            .into_par_iter()
            .filter(|event| {
                let index = if let Some(first) = dense {
                    event
                        .person
                        .checked_sub(first)
                        .and_then(|i| usize::try_from(i).ok())
                        .filter(|&i| i < self.minds.len())
                } else {
                    self.minds
                        .binary_search_by_key(&event.person, |mind| mind.person)
                        .ok()
                };
                index.is_some_and(|i| {
                    let mind = &self.minds[i];
                    mind.capacity > 0
                        && match event.kind {
                            Experience::Feed => mind.on_feed != 0,
                            Experience::Hunt => mind.on_hunt != 0,
                            Experience::HandleAccepted => mind.on_handle_accepted != 0,
                            Experience::HandleRefused => mind.on_handle_refused != 0,
                            Experience::CaptureYielded => mind.on_capture_yielded != 0,
                            Experience::CaptureForced => mind.on_capture_forced != 0,
                            Experience::CaptureResisted => mind.on_capture_resisted != 0,
                            Experience::Released => mind.on_animal_released != 0,
                            Experience::ReleaseBlocked => mind.on_animal_release_blocked != 0,
                            Experience::DefenseSucceeded => mind.on_defense_succeeded != 0,
                            Experience::DefenseOvercome => mind.on_defense_overcome != 0,
                        }
                })
            })
            .collect()
    }
    pub fn advance(&self, events: &[Event]) -> Self {
        let mut events = events.to_vec();
        if events.par_windows(2).any(|pair| pair[0] > pair[1]) {
            events.par_sort_unstable();
        }
        let chunks: Vec<Vec<Mind>> = self
            .minds
            .par_chunks(1024)
            .map(|chunk| {
                let mut cursor = events.partition_point(|event| event.person < chunk[0].person);
                chunk
                    .iter()
                    .map(|old| {
                        while cursor < events.len() && events[cursor].person < old.person {
                            cursor += 1;
                        }
                        let start = cursor;
                        while cursor < events.len() && events[cursor].person == old.person {
                            cursor += 1;
                        }
                        let mut mind = old.clone();
                        for event in &events[start..cursor] {
                            let delta = match event.kind {
                                Experience::Feed => mind.on_feed,
                                Experience::Hunt => mind.on_hunt,
                                Experience::HandleAccepted => mind.on_handle_accepted,
                                Experience::HandleRefused => mind.on_handle_refused,
                                Experience::CaptureYielded => mind.on_capture_yielded,
                                Experience::CaptureForced => mind.on_capture_forced,
                                Experience::CaptureResisted => mind.on_capture_resisted,
                                Experience::Released => mind.on_animal_released,
                                Experience::ReleaseBlocked => mind.on_animal_release_blocked,
                                Experience::DefenseSucceeded => mind.on_defense_succeeded,
                                Experience::DefenseOvercome => mind.on_defense_overcome,
                            };
                            if delta == 0 {
                                continue;
                            }
                            match mind
                                .opinions
                                .binary_search_by_key(&event.animal, |o| o.animal)
                            {
                                Ok(i) => {
                                    mind.opinions[i].affinity = mind.opinions[i]
                                        .affinity
                                        .saturating_add(delta)
                                        .clamp(mind.minimum, mind.maximum)
                                }
                                Err(i) if mind.opinions.len() < mind.capacity => {
                                    mind.opinions.insert(
                                        i,
                                        Opinion {
                                            animal: event.animal,
                                            affinity: delta.clamp(mind.minimum, mind.maximum),
                                        },
                                    )
                                }
                                Err(_) => {}
                            }
                        }
                        mind
                    })
                    .collect()
            })
            .collect();
        let minds = chunks.into_par_iter().flatten().collect();
        Self { minds }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_and_dense_people_keep_their_own_experiences_across_chunks() {
        for stride in [1, 3] {
            let feelings = AnimalFeelings {
                minds: (0..4097)
                    .map(|i| Mind {
                        person: 1 + i * stride,
                        minimum: -5,
                        maximum: 5,
                        capacity: 1,
                        on_feed: 2,
                        on_hunt: -1,
                        on_handle_accepted: 0,
                        on_handle_refused: 0,
                        on_capture_yielded: 0,
                        on_capture_forced: 0,
                        on_capture_resisted: 0,
                        on_animal_released: 0,
                        on_animal_release_blocked: 0,
                        on_defense_succeeded: 0,
                        on_defense_overcome: 0,
                        opinions: vec![],
                    })
                    .collect(),
            };
            let mut events: Vec<_> = (0..4097)
                .filter(|i| i % 2 == 0)
                .map(|i| Event {
                    person: 1 + i * stride,
                    animal: 7,
                    kind: Experience::Feed,
                })
                .collect();
            events.reverse();
            events.push(Event {
                person: 999_999,
                animal: 7,
                kind: Experience::Feed,
            });
            let relevant = feelings.relevant(events);
            let updated = feelings.advance(&relevant);
            assert_eq!(updated.minds.len(), 4097);
            for (i, mind) in updated.minds.iter().enumerate() {
                assert_eq!(mind.person, 1 + i as u64 * stride);
                if i % 2 == 0 {
                    assert_eq!(
                        mind.opinions,
                        vec![Opinion {
                            animal: 7,
                            affinity: 2
                        }]
                    );
                } else {
                    assert!(mind.opinions.is_empty());
                }
            }
        }
    }
}
