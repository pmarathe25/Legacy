//! Independently chosen approaches to living animals.
use game_parallel_economy_person::Person;
use game_parallel_fauna::{
    Fauna,
    handling::{Event, Outcome, respond},
};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    /// Absent chooses one locally observed animal.
    #[serde(default)]
    pub animal: Option<u64>,
    pub willing: bool,
    #[serde(default)]
    pub affinity_at_least: Option<i32>,
    #[serde(default)]
    pub affinity_at_most: Option<i32>,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handling {
    #[serde(default)]
    pub inherit_births: bool,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub events: Vec<Event>,
}

/// A newborn may acquire an inclination, never another person's target or pressure.
pub fn enroll(
    owner: Option<&Handling>,
    family: Option<&crate::Families>,
    tick: u64,
) -> Vec<Policy> {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return Vec::new();
    };
    family
        .members
        .par_iter()
        .filter(|member| member.born == Some(tick))
        .filter_map(|member| {
            if owner
                .policies
                .binary_search_by_key(&member.id, |policy| policy.person)
                .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                owner
                    .policies
                    .binary_search_by_key(id, |policy| policy.person)
                    .ok()
                    .map(|index| &owner.policies[index])
            })?;
            Some(Policy {
                person: member.id,
                animal: None,
                willing: parent.willing,
                affinity_at_least: parent.affinity_at_least,
                affinity_at_most: parent.affinity_at_most,
                pressured_by: None,
                pressure_threshold: None,
            })
        })
        .collect()
}

impl Handling {
    pub fn validate(
        &mut self,
        people: &HashSet<u64>,
        fauna: Option<&Fauna>,
        tick: u64,
    ) -> Result<(), String> {
        let animals: HashSet<_> = fauna
            .ok_or("animal handling requires wildlife")?
            .animals
            .par_iter()
            .map(|animal| animal.id)
            .collect();
        self.policies
            .par_sort_unstable_by_key(|policy| policy.person);
        self.events
            .par_sort_unstable_by_key(|event| (event.animal, event.person));
        if self
            .policies
            .par_windows(2)
            .any(|pair| pair[0].person == pair[1].person)
            || self
                .events
                .par_windows(2)
                .any(|pair| pair[0].animal == pair[1].animal)
            || self.policies.par_iter().any(|policy| {
                !people.contains(&policy.person)
                    || policy.animal.is_some_and(|id| !animals.contains(&id))
                    || policy.pressured_by.is_some() != policy.pressure_threshold.is_some()
                    || policy.pressure_threshold == Some(0)
                    || policy
                        .pressured_by
                        .is_some_and(|id| id == policy.person || !people.contains(&id))
            })
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || !people.contains(&event.person)
                    || !animals.contains(&event.animal)
            })
        {
            return Err("invalid personal animal handling".into());
        }
        Ok(())
    }
}

pub struct Plan {
    pub actors: Vec<u64>,
    /// Sorted by animal; at most one event per animal.
    pub events: Vec<Event>,
}

pub struct Context<'a> {
    pub owner: Option<&'a Handling>,
    pub fauna: Option<&'a Fauna>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub feelings: Option<&'a crate::AnimalFeelings>,
    pub busy: &'a [u64],
    /// Animals already reserved for feeding or an explicit hunt.
    pub blocked: &'a [u64],
    pub max_hunger: u32,
    pub seed: u64,
    pub tick: u64,
    pub manual: Option<(u64, u64)>,
}

impl Context<'_> {
    pub fn prepare(&self) -> Result<Plan, String> {
        let policies = self
            .owner
            .map_or(&[][..], |owner| owner.policies.as_slice());
        if policies.is_empty() && self.manual.is_none() {
            return Ok(Plan {
                actors: Vec::new(),
                events: Vec::new(),
            });
        }
        let fauna = self.fauna.ok_or("wildlife is disabled")?;
        let mut animals: Vec<_> = fauna.animals.par_iter().collect();
        if animals.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            animals.par_sort_unstable_by_key(|animal| animal.id);
        }
        let dense_animals = animals
            .first()
            .zip(animals.last())
            .and_then(|(first, last)| {
                (last.id.checked_sub(first.id) == Some((animals.len() - 1) as u64))
                    .then_some(first.id)
            });
        let dense_people = self
            .people
            .first()
            .zip(self.people.last())
            .and_then(|(first, last)| {
                (last.id.checked_sub(first.id) == Some((self.people.len() - 1) as u64))
                    .then_some(first.id)
            });
        let find_animal = |id: u64| {
            if let Some(first) = dense_animals {
                animals
                    .get(usize::try_from(id.checked_sub(first)?).ok()?)
                    .copied()
            } else {
                animals
                    .binary_search_by_key(&id, |animal| animal.id)
                    .ok()
                    .map(|index| animals[index])
            }
        };
        let find_person = |id: u64| {
            if let Some(first) = dense_people {
                self.people
                    .get(usize::try_from(id.checked_sub(first)?).ok()?)
            } else {
                self.people
                    .binary_search_by_key(&id, |person| person.id)
                    .ok()
                    .map(|index| &self.people[index])
            }
        };
        let mut visible: Vec<_> = if policies.par_iter().any(|policy| policy.animal.is_none()) {
            fauna
                .animals
                .par_iter()
                .filter(|animal| animal.health > 0)
                .map(|animal| (animal.region, animal.id))
                .collect()
        } else {
            Vec::new()
        };
        if visible.par_windows(2).any(|pair| pair[0] > pair[1]) {
            visible.par_sort_unstable();
        }
        let opinions = self
            .opinions
            .filter(|_| {
                policies
                    .par_iter()
                    .any(|policy| policy.pressure_threshold.is_some())
            })
            .map(Opinions::index);
        let mut candidates: Vec<_> = policies
            .par_iter()
            .filter_map(|policy| {
                let person = find_person(policy.person)?;
                let animal_id = policy.animal.or_else(|| {
                    let region = person.region?;
                    let start = visible.partition_point(|row| row.0 < region);
                    let end = visible.partition_point(|row| row.0 <= region);
                    if start == end {
                        return None;
                    }
                    let offset =
                        (game_parallel_genetics::random(self.seed ^ region, person.id, self.tick)
                            % (end - start) as u64) as usize;
                    Some(visible[start + offset].1)
                })?;
                let animal = find_animal(animal_id)?;
                let affinity = || {
                    self.feelings
                        .and_then(|feelings| feelings.affinity(person.id, animal_id))
                        .unwrap_or(0)
                };
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(person.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                let willing = policy.willing
                    && policy.affinity_at_least.is_none_or(|min| affinity() >= min)
                    && policy.affinity_at_most.is_none_or(|max| affinity() <= max);
                (!person.controlled
                    && person.health > 0
                    && person.hunger <= self.max_hunger
                    && animal.health > 0
                    && person.region == Some(animal.region)
                    && self.busy.binary_search(&person.id).is_err()
                    && self.blocked.binary_search(&animal_id).is_err()
                    && (willing || pressured))
                    .then_some((
                        animal_id,
                        true,
                        game_parallel_genetics::random(self.seed ^ animal_id, person.id, self.tick),
                        person.id,
                    ))
            })
            .collect();
        let mut events = Vec::new();
        if let Some((person_id, animal_id)) = self.manual {
            let person = find_person(person_id).ok_or("unknown person")?;
            let animal = find_animal(animal_id).ok_or("unknown animal")?;
            if !person.controlled
                || person.health == 0
                || animal.health == 0
                || person.region != Some(animal.region)
            {
                return Err(
                    "handling requires a living controlled person and a local animal".into(),
                );
            }
            if self.blocked.binary_search(&animal_id).is_ok() {
                events.push(Event {
                    tick: self.tick,
                    person: person_id,
                    animal: animal_id,
                    outcome: Outcome::Unavailable,
                });
            } else {
                candidates.push((animal_id, false, 0, person_id));
            }
        }
        let mut actors: Vec<_> = candidates.par_iter().map(|row| row.3).collect();
        actors.extend(self.manual.map(|row| row.0));
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        actors.dedup();
        if candidates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            candidates.par_sort_unstable_by_key(|candidate| candidate.0);
        }
        events.par_extend(candidates.par_chunk_by(|a, b| a.0 == b.0).map(|group| {
            let chosen = *group
                .par_iter()
                .min_by_key(|row| (row.1, row.2, row.3))
                .expect("nonempty animal contender group");
            let animal = find_animal(chosen.0).expect("validated animal");
            Event {
                tick: self.tick,
                person: chosen.3,
                animal: chosen.0,
                outcome: respond(animal, chosen.3),
            }
        }));
        if events
            .par_windows(2)
            .any(|pair| (pair[0].animal, pair[0].person) > (pair[1].animal, pair[1].person))
        {
            events.par_sort_unstable_by_key(|event| (event.animal, event.person));
        }
        Ok(Plan { actors, events })
    }
}
