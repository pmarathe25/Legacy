//! An animal's own responses to capture and release.
use crate::{Animal, AnimalOpinion, Fauna};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

fn default_resistance() -> u64 {
    1
}

/// Individual physical resistance and subjective reactions; none grants ownership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    #[serde(default)]
    pub yield_willingly: bool,
    #[serde(default)]
    pub yield_affinity_at_least: Option<i32>,
    #[serde(default)]
    pub yield_affinity_at_most: Option<i32>,
    #[serde(default = "default_resistance")]
    pub resistance: u64,
    #[serde(default)]
    pub escape_willing: bool,
    #[serde(default)]
    pub escape_effort: u64,
    #[serde(default)]
    pub on_yielded: i32,
    #[serde(default)]
    pub on_forced: i32,
    #[serde(default)]
    pub on_resisted: i32,
    #[serde(default)]
    pub on_released: i32,
}

/// Yielding is an animal's own inclination, not a prerequisite for physical capture.
pub fn yields(animal: &Animal, person: u64) -> bool {
    let Some(response) = &animal.restraint_response else {
        return false;
    };
    let affinity = animal
        .opinions
        .binary_search_by_key(&person, |opinion| opinion.person)
        .ok()
        .map_or(0, |index| animal.opinions[index].affinity);
    response.yield_willingly
        || response
            .yield_affinity_at_least
            .is_some_and(|min| affinity >= min)
        || response
            .yield_affinity_at_most
            .is_some_and(|max| affinity <= max)
}

pub fn resistance(animal: &Animal) -> u64 {
    animal
        .restraint_response
        .as_ref()
        .map_or(1, |response| response.resistance)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reaction {
    pub animal: u64,
    pub person: u64,
    pub change: i32,
}

/// Apply caller-attributed, signed experiences to bounded individual memories.
pub fn apply_reactions(fauna: &mut Fauna, reactions: &[Reaction]) {
    if reactions.is_empty() {
        return;
    }
    let dense = reactions
        .first()
        .zip(reactions.last())
        .and_then(|(first, last)| {
            (last.animal.checked_sub(first.animal) == Some((reactions.len() - 1) as u64)
                && reactions
                    .par_windows(2)
                    .all(|pair| pair[0].animal < pair[1].animal))
            .then_some(first.animal)
        });
    fauna.animals.par_iter_mut().for_each(|animal| {
        let Some(social) = &animal.social_response else {
            return;
        };
        let (start, end) = if let Some(first) = dense {
            let Some(index) = animal
                .id
                .checked_sub(first)
                .and_then(|index| usize::try_from(index).ok())
                .filter(|&index| index < reactions.len())
            else {
                return;
            };
            (index, index + 1)
        } else {
            (
                reactions.partition_point(|reaction| reaction.animal < animal.id),
                reactions.partition_point(|reaction| reaction.animal <= animal.id),
            )
        };
        for reaction in &reactions[start..end] {
            if reaction.change == 0 {
                continue;
            }
            let adjust = |value: i32| {
                (i128::from(value) + i128::from(reaction.change))
                    .clamp(i128::from(social.minimum), i128::from(social.maximum))
                    as i32
            };
            match animal
                .opinions
                .binary_search_by_key(&reaction.person, |opinion| opinion.person)
            {
                Ok(index) => {
                    animal.opinions[index].affinity = adjust(animal.opinions[index].affinity)
                }
                Err(index) if animal.opinions.len() < social.capacity => animal.opinions.insert(
                    index,
                    AnimalOpinion {
                        person: reaction.person,
                        affinity: adjust(0),
                    },
                ),
                Err(_) => {}
            }
        }
    });
}
