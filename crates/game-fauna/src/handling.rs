//! Individual animal responses to a person's handling attempt.
use crate::{Animal, AnimalOpinion, Fauna};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// An animal's own response, including its private interpretation of the attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub willing: bool,
    #[serde(default)]
    pub affinity_at_least: Option<i32>,
    #[serde(default)]
    pub affinity_at_most: Option<i32>,
    #[serde(default)]
    pub on_accept: i32,
    #[serde(default)]
    pub on_refuse: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Accepted,
    Refused,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub animal: u64,
    pub outcome: Outcome,
}

/// An absent response does not invent willingness or a relationship.
pub fn respond(animal: &Animal, person: u64) -> Outcome {
    let Some(response) = &animal.handling_response else {
        return Outcome::Refused;
    };
    let affinity = animal
        .opinions
        .binary_search_by_key(&person, |opinion| opinion.person)
        .ok()
        .map_or(0, |index| animal.opinions[index].affinity);
    if response.willing
        || response
            .affinity_at_least
            .is_some_and(|min| affinity >= min)
        || response.affinity_at_most.is_some_and(|max| affinity <= max)
    {
        Outcome::Accepted
    } else {
        Outcome::Refused
    }
}

/// Publish only the animal's configured signed reactions to actual encounters.
/// Events must be sorted by animal, with at most one actual encounter per animal.
pub fn apply(fauna: &mut Fauna, events: &[Event]) {
    if events.is_empty() {
        return;
    }
    let dense = events.first().zip(events.last()).and_then(|(first, last)| {
        (last.animal.checked_sub(first.animal) == Some((events.len() - 1) as u64))
            .then_some(first.animal)
    });
    fauna.animals.par_iter_mut().for_each(|animal| {
        let index = if let Some(first) = dense {
            animal
                .id
                .checked_sub(first)
                .and_then(|index| usize::try_from(index).ok())
                .filter(|&index| index < events.len())
        } else {
            events
                .binary_search_by_key(&animal.id, |event| event.animal)
                .ok()
        };
        let Some(index) = index else {
            return;
        };
        let event = &events[index];
        let Some((response, social)) = animal
            .handling_response
            .as_ref()
            .zip(animal.social_response.as_ref())
        else {
            return;
        };
        let change = match event.outcome {
            Outcome::Accepted => response.on_accept,
            Outcome::Refused => response.on_refuse,
            Outcome::Unavailable => 0,
        };
        if change == 0 {
            return;
        }
        let adjust = |value: i32| {
            (i128::from(value) + i128::from(change))
                .clamp(i128::from(social.minimum), i128::from(social.maximum)) as i32
        };
        match animal
            .opinions
            .binary_search_by_key(&event.person, |opinion| opinion.person)
        {
            Ok(index) => animal.opinions[index].affinity = adjust(animal.opinions[index].affinity),
            Err(index) if animal.opinions.len() < social.capacity => animal.opinions.insert(
                index,
                AnimalOpinion {
                    person: event.person,
                    affinity: adjust(0),
                },
            ),
            Err(_) => {}
        }
    });
}
