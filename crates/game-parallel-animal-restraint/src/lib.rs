//! Physical holds on individual living animals, independent of ownership.
use game_parallel_economy_person::Person;
use game_parallel_fauna::{Animal, Fauna, restraint};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn one() -> u64 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    #[serde(default)]
    pub animal: Option<u64>,
    pub willing: bool,
    #[serde(default)]
    pub affinity_at_least: Option<i32>,
    #[serde(default)]
    pub affinity_at_most: Option<i32>,
    #[serde(default = "one")]
    pub effort: u64,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}

/// A person's independent inclination to release a physically held animal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePolicy {
    pub person: u64,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
    pub animal: u64,
    pub handler: u64,
    /// Remaining physical holding effort, reduced by escape attempts.
    pub integrity: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Yielded,
    Forced,
    Resisted,
    Unavailable,
    Struggled,
    Escaped,
    Released,
    LetGo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub animal: u64,
    pub actor: Option<u64>,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Restraints {
    #[serde(default = "one")]
    pub default_effort: u64,
    #[serde(default = "one")]
    pub capacity_per_handler: u64,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub release_policies: Vec<ReleasePolicy>,
    #[serde(default)]
    pub holds: Vec<Hold>,
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Default for Restraints {
    fn default() -> Self {
        Self {
            default_effort: 1,
            capacity_per_handler: 1,
            policies: Vec::new(),
            release_policies: Vec::new(),
            holds: Vec::new(),
            events: Vec::new(),
        }
    }
}

fn indexed(fauna: &Fauna) -> Vec<&Animal> {
    let mut animals: Vec<_> = fauna.animals.par_iter().collect();
    if animals.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        animals.par_sort_unstable_by_key(|animal| animal.id);
    }
    animals
}

impl Restraints {
    pub fn held(&self, animal: u64) -> bool {
        self.holds
            .binary_search_by_key(&animal, |hold| hold.animal)
            .is_ok()
    }

    pub fn normalize_and_validate(
        &mut self,
        people: &[Person],
        fauna: Option<&Fauna>,
        tick: u64,
    ) -> Result<(), String> {
        let fauna = fauna.ok_or("animal restraint requires wildlife")?;
        let animals = indexed(fauna);
        self.policies
            .par_sort_unstable_by_key(|policy| policy.person);
        self.release_policies
            .par_sort_unstable_by_key(|policy| policy.person);
        self.holds.par_sort_unstable_by_key(|hold| hold.animal);
        self.events
            .par_sort_unstable_by_key(|event| (event.animal, event.actor, event.kind));
        let ids: HashSet<_> = people.par_iter().map(|person| person.id).collect();
        let animal = |id| {
            animals
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|i| animals[i])
        };
        let person = |id| {
            people
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|i| &people[i])
        };
        let mut handlers: Vec<_> = self.holds.par_iter().map(|hold| hold.handler).collect();
        handlers.par_sort_unstable();
        if self.default_effort == 0
            || self.capacity_per_handler == 0
            || self
                .policies
                .par_windows(2)
                .any(|pair| pair[0].person == pair[1].person)
            || self
                .release_policies
                .par_windows(2)
                .any(|pair| pair[0].person == pair[1].person)
            || self
                .holds
                .par_windows(2)
                .any(|pair| pair[0].animal == pair[1].animal)
            || handlers
                .par_chunk_by(|a, b| a == b)
                .any(|group| group.len() as u64 > self.capacity_per_handler)
            || self.policies.par_iter().any(|policy| {
                !ids.contains(&policy.person)
                    || policy.effort == 0
                    || policy.animal.is_some_and(|id| animal(id).is_none())
                    || policy.pressured_by.is_some() != policy.pressure_threshold.is_some()
                    || policy.pressure_threshold == Some(0)
                    || policy
                        .pressured_by
                        .is_some_and(|id| id == policy.person || !ids.contains(&id))
            })
            || self.release_policies.par_iter().any(|policy| {
                !ids.contains(&policy.person)
                    || policy.animal.is_some_and(|id| animal(id).is_none())
                    || policy.pressured_by.is_some() != policy.pressure_threshold.is_some()
                    || policy.pressure_threshold == Some(0)
                    || policy
                        .pressured_by
                        .is_some_and(|id| id == policy.person || !ids.contains(&id))
            })
            || self.holds.par_iter().any(|hold| {
                hold.integrity == 0
                    || person(hold.handler).is_none_or(|p| {
                        p.health == 0
                            || animal(hold.animal)
                                .is_none_or(|a| a.health == 0 || p.region != Some(a.region))
                    })
            })
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || animal(event.animal).is_none()
                    || event.actor.is_some_and(|id| !ids.contains(&id))
            })
        {
            return Err("invalid physical animal restraint".into());
        }
        Ok(())
    }

    /// A physical hold ends when either participant can no longer remain together.
    pub fn reconcile(
        &mut self,
        people: &[Person],
        fauna: &Fauna,
        available: &(dyn Fn(u64) -> bool + Sync),
        tick: u64,
    ) {
        if self.holds.is_empty() {
            return;
        }
        let animals = indexed(fauna);
        let dense_people = people.first().zip(people.last()).and_then(|(first, last)| {
            (last.id.checked_sub(first.id) == Some((people.len() - 1) as u64)).then_some(first.id)
        });
        let dense_animals = animals
            .first()
            .zip(animals.last())
            .and_then(|(first, last)| {
                (last.id.checked_sub(first.id) == Some((animals.len() - 1) as u64))
                    .then_some(first.id)
            });
        let keep = |hold: &Hold| {
            let person = dense_people.map_or_else(
                || {
                    people
                        .binary_search_by_key(&hold.handler, |row| row.id)
                        .ok()
                        .map(|i| &people[i])
                },
                |first| {
                    hold.handler
                        .checked_sub(first)
                        .and_then(|i| usize::try_from(i).ok())
                        .and_then(|i| people.get(i))
                },
            );
            let animal = dense_animals.map_or_else(
                || {
                    animals
                        .binary_search_by_key(&hold.animal, |row| row.id)
                        .ok()
                        .map(|i| animals[i])
                },
                |first| {
                    hold.animal
                        .checked_sub(first)
                        .and_then(|i| usize::try_from(i).ok())
                        .and_then(|i| animals.get(i).copied())
                },
            );
            person.zip(animal).is_some_and(|(person, animal)| {
                person.health > 0
                    && animal.health > 0
                    && person.region == Some(animal.region)
                    && available(person.id)
            })
        };
        self.events.par_extend(
            self.holds
                .par_iter()
                .filter(|hold| !keep(hold))
                .map(|hold| Event {
                    tick,
                    animal: hold.animal,
                    actor: None,
                    kind: EventKind::LetGo,
                }),
        );
        self.holds = self
            .holds
            .par_iter()
            .filter(|hold| keep(hold))
            .cloned()
            .collect();
        if self.events.par_windows(2).any(|pair| {
            (pair[0].animal, pair[0].actor, pair[0].kind)
                > (pair[1].animal, pair[1].actor, pair[1].kind)
        }) {
            self.events
                .par_sort_unstable_by_key(|event| (event.animal, event.actor, event.kind));
        }
    }
}

#[derive(Clone, Copy)]
pub enum Manual {
    Capture { person: u64, animal: u64 },
    Release { person: u64, animal: u64 },
}

pub struct Plan {
    pub state: Option<Restraints>,
    pub actors: Vec<u64>,
    /// Animals that actually struggled, escaped, were released, or were approached.
    pub reserved: Vec<u64>,
    pub reactions: Vec<restraint::Reaction>,
}

pub struct Context<'a> {
    pub owner: Option<&'a Restraints>,
    pub fauna: Option<&'a Fauna>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub affinity: Option<&'a (dyn Fn(u64, u64) -> i32 + Sync)>,
    pub busy: &'a [u64],
    pub blocked: &'a [u64],
    pub max_hunger: u32,
    pub seed: u64,
    pub tick: u64,
    pub manual: Option<Manual>,
}

impl Context<'_> {
    pub fn prepare(&self) -> Result<Plan, String> {
        if self.owner.is_none() && self.manual.is_none() {
            return Ok(Plan {
                state: None,
                actors: Vec::new(),
                reserved: Vec::new(),
                reactions: Vec::new(),
            });
        }
        if let Some(owner) = self.owner.filter(|owner| {
            owner.policies.is_empty()
                && owner.release_policies.is_empty()
                && owner.holds.is_empty()
                && self.manual.is_none()
        }) {
            return Ok(Plan {
                state: Some(Restraints {
                    default_effort: owner.default_effort,
                    capacity_per_handler: owner.capacity_per_handler,
                    policies: Vec::new(),
                    release_policies: Vec::new(),
                    holds: Vec::new(),
                    events: Vec::new(),
                }),
                actors: Vec::new(),
                reserved: Vec::new(),
                reactions: Vec::new(),
            });
        }
        let fauna = self.fauna.ok_or("wildlife is disabled")?;
        let animals = indexed(fauna);
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
        let animal = |id: u64| {
            if let Some(first) = dense_animals {
                animals
                    .get(usize::try_from(id.checked_sub(first)?).ok()?)
                    .copied()
            } else {
                animals
                    .binary_search_by_key(&id, |row| row.id)
                    .ok()
                    .map(|i| animals[i])
            }
        };
        let person = |id: u64| {
            if let Some(first) = dense_people {
                self.people
                    .get(usize::try_from(id.checked_sub(first)?).ok()?)
            } else {
                self.people
                    .binary_search_by_key(&id, |row| row.id)
                    .ok()
                    .map(|i| &self.people[i])
            }
        };
        let mut next = self
            .owner
            .map_or_else(Restraints::default, |owner| Restraints {
                default_effort: owner.default_effort,
                capacity_per_handler: owner.capacity_per_handler,
                policies: owner.policies.par_iter().cloned().collect(),
                release_policies: owner.release_policies.par_iter().cloned().collect(),
                holds: owner.holds.par_iter().cloned().collect(),
                events: Vec::new(),
            });
        let mut actors = Vec::new();
        let manual_release = match self.manual {
            Some(Manual::Release {
                person: actor,
                animal: target,
            }) => {
                let actor = person(actor).ok_or("unknown person")?;
                let target_animal = animal(target).ok_or("unknown animal")?;
                if !actor.controlled
                    || actor.health == 0
                    || target_animal.health == 0
                    || actor.region != Some(target_animal.region)
                    || !next.held(target)
                {
                    return Err(
                        "release requires a living controlled person and a local held animal"
                            .into(),
                    );
                }
                actors.push(actor.id);
                next.events.push(Event {
                    tick: self.tick,
                    animal: target,
                    actor: Some(actor.id),
                    kind: EventKind::Released,
                });
                Some(target)
            }
            _ => None,
        };
        let opinions = self
            .opinions
            .filter(|_| {
                next.policies
                    .par_iter()
                    .any(|policy| policy.pressure_threshold.is_some())
                    || next
                        .release_policies
                        .par_iter()
                        .any(|policy| policy.pressure_threshold.is_some())
            })
            .map(Opinions::index);
        let manual_capture = match self.manual {
            Some(Manual::Capture { animal, .. }) => Some(animal),
            _ => None,
        };
        let dense_holds = next
            .holds
            .first()
            .zip(next.holds.last())
            .and_then(|(first, last)| {
                (last.animal.checked_sub(first.animal) == Some((next.holds.len() - 1) as u64))
                    .then_some(first.animal)
            });
        let is_held = |id: u64| {
            dense_holds.map_or_else(
                || next.held(id),
                |first| {
                    id.checked_sub(first)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .is_some_and(|index| index < next.holds.len())
                },
            )
        };
        let mut held_by_region: Vec<_> = if next
            .release_policies
            .par_iter()
            .any(|policy| policy.animal.is_none())
        {
            next.holds
                .par_iter()
                .filter(|hold| {
                    self.blocked.binary_search(&hold.animal).is_err()
                        && manual_capture != Some(hold.animal)
                        && manual_release != Some(hold.animal)
                })
                .filter_map(|hold| {
                    animal(hold.animal)
                        .filter(|a| a.health > 0)
                        .map(|a| (a.region, a.id))
                })
                .collect()
        } else {
            Vec::new()
        };
        if held_by_region.par_windows(2).any(|pair| pair[0] > pair[1]) {
            held_by_region.par_sort_unstable();
        }
        let mut release_candidates: Vec<_> = next
            .release_policies
            .par_iter()
            .filter_map(|policy| {
                let actor = person(policy.person)?;
                let target = policy.animal.or_else(|| {
                    let region = actor.region?;
                    let start = held_by_region.partition_point(|row| row.0 < region);
                    let end = held_by_region.partition_point(|row| row.0 <= region);
                    (start < end).then(|| {
                        let offset = (game_parallel_genetics::random(
                            self.seed ^ region,
                            actor.id,
                            self.tick,
                        ) % (end - start) as u64) as usize;
                        held_by_region[start + offset].1
                    })
                })?;
                let target_animal = animal(target)?;
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(actor.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                let affinity = || {
                    self.affinity
                        .map_or(0, |feelings| feelings(actor.id, target))
                };
                let willing = policy.willing
                    && policy.affinity_at_least.is_none_or(|min| affinity() >= min)
                    && policy.affinity_at_most.is_none_or(|max| affinity() <= max);
                (!actor.controlled
                    && actor.health > 0
                    && actor.hunger <= self.max_hunger
                    && target_animal.health > 0
                    && actor.region == Some(target_animal.region)
                    && self.busy.binary_search(&actor.id).is_err()
                    && self.blocked.binary_search(&target).is_err()
                    && manual_capture != Some(target)
                    && manual_release != Some(target)
                    && is_held(target)
                    && (willing || pressured))
                    .then_some((
                        target,
                        game_parallel_genetics::random(self.seed ^ target, actor.id, self.tick),
                        actor.id,
                    ))
            })
            .collect();
        actors.par_extend(release_candidates.par_iter().map(|candidate| candidate.2));
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        actors.dedup();
        if release_candidates
            .par_windows(2)
            .any(|pair| pair[0].0 > pair[1].0)
        {
            release_candidates.par_sort_unstable_by_key(|row| row.0);
        }
        let released: Vec<_> = release_candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let chosen = group
                    .par_iter()
                    .min_by_key(|row| (row.1, row.2))
                    .expect("nonempty release group");
                Event {
                    tick: self.tick,
                    animal: chosen.0,
                    actor: Some(chosen.2),
                    kind: EventKind::Released,
                }
            })
            .collect();
        next.events.par_extend(released);
        let mut released_targets: Vec<_> =
            next.events.par_iter().map(|event| event.animal).collect();
        if released_targets
            .par_windows(2)
            .any(|pair| pair[0] > pair[1])
        {
            released_targets.par_sort_unstable();
        }
        let dense_released = released_targets
            .first()
            .zip(released_targets.last())
            .and_then(|(first, last)| {
                (last.checked_sub(*first) == Some((released_targets.len() - 1) as u64))
                    .then_some(*first)
            });
        let progressed: Vec<_> = next
            .holds
            .par_iter()
            .filter_map(|hold| {
                let released = dense_released.map_or_else(
                    || released_targets.binary_search(&hold.animal).is_ok(),
                    |first| {
                        hold.animal
                            .checked_sub(first)
                            .and_then(|offset| usize::try_from(offset).ok())
                            .is_some_and(|index| index < released_targets.len())
                    },
                );
                if released {
                    return None;
                }
                if self.blocked.binary_search(&hold.animal).is_ok() {
                    return Some((Some(hold.clone()), None));
                }
                let target = animal(hold.animal)?;
                let effort = target
                    .restraint_response
                    .as_ref()
                    .filter(|response| response.escape_willing)
                    .map_or(0, |response| response.escape_effort);
                if effort == 0 {
                    return Some((Some(hold.clone()), None));
                }
                let remaining = hold.integrity.saturating_sub(effort);
                let event = Event {
                    tick: self.tick,
                    animal: hold.animal,
                    actor: None,
                    kind: if remaining == 0 {
                        EventKind::Escaped
                    } else {
                        EventKind::Struggled
                    },
                };
                Some((
                    (remaining > 0).then_some(Hold {
                        integrity: remaining,
                        ..hold.clone()
                    }),
                    Some(event),
                ))
            })
            .collect();
        next.holds = progressed
            .par_iter()
            .filter_map(|row| row.0.clone())
            .collect();
        next.events
            .par_extend(progressed.into_par_iter().filter_map(|row| row.1));
        let mut unavailable: Vec<_> = next.events.par_iter().map(|event| event.animal).collect();
        unavailable.par_extend(self.blocked.par_iter().copied());
        if unavailable.par_windows(2).any(|pair| pair[0] > pair[1]) {
            unavailable.par_sort_unstable();
        }
        if unavailable.par_windows(2).any(|pair| pair[0] == pair[1]) {
            unavailable.dedup();
        }
        let mut handlers: Vec<_> = next.holds.par_iter().map(|hold| hold.handler).collect();
        if handlers.par_windows(2).any(|pair| pair[0] > pair[1]) {
            handlers.par_sort_unstable();
        }
        let capacity = next.capacity_per_handler;
        let has_capacity = |id| {
            let first = handlers.partition_point(|handler| *handler < id);
            let last = handlers.partition_point(|handler| *handler <= id);
            ((last - first) as u64) < capacity
        };
        let mut visible: Vec<_> = if next
            .policies
            .par_iter()
            .any(|policy| policy.animal.is_none())
        {
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
        let mut candidates: Vec<_> = next
            .policies
            .par_iter()
            .filter_map(|policy| {
                let actor = person(policy.person)?;
                let target = policy.animal.or_else(|| {
                    let region = actor.region?;
                    let start = visible.partition_point(|row| row.0 < region);
                    let end = visible.partition_point(|row| row.0 <= region);
                    if start == end {
                        return None;
                    }
                    let offset =
                        (game_parallel_genetics::random(self.seed ^ region, actor.id, self.tick)
                            % (end - start) as u64) as usize;
                    Some(visible[start + offset].1)
                })?;
                let target_animal = animal(target)?;
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(actor.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                let affinity = || {
                    self.affinity
                        .map_or(0, |feelings| feelings(actor.id, target))
                };
                let willing = policy.willing
                    && policy.affinity_at_least.is_none_or(|min| affinity() >= min)
                    && policy.affinity_at_most.is_none_or(|max| affinity() <= max);
                (!actor.controlled
                    && actor.health > 0
                    && actor.hunger <= self.max_hunger
                    && target_animal.health > 0
                    && actor.region == Some(target_animal.region)
                    && self.busy.binary_search(&actor.id).is_err()
                    && actors.binary_search(&actor.id).is_err()
                    && has_capacity(actor.id)
                    && next
                        .holds
                        .binary_search_by_key(&target, |hold| hold.animal)
                        .is_err()
                    && unavailable.binary_search(&target).is_err()
                    && (willing || pressured))
                    .then_some((
                        target,
                        true,
                        game_parallel_genetics::random(self.seed ^ target, actor.id, self.tick),
                        actor.id,
                        policy.effort,
                    ))
            })
            .collect();
        if let Some(Manual::Capture {
            person: actor,
            animal: target,
        }) = self.manual
        {
            let actor = person(actor).ok_or("unknown person")?;
            let target_animal = animal(target).ok_or("unknown animal")?;
            if !actor.controlled
                || actor.health == 0
                || target_animal.health == 0
                || actor.region != Some(target_animal.region)
            {
                return Err(
                    "capture requires a living controlled person and a local animal".into(),
                );
            }
            actors.push(actor.id);
            if next.held(target)
                || unavailable.binary_search(&target).is_ok()
                || !has_capacity(actor.id)
            {
                next.events.push(Event {
                    tick: self.tick,
                    animal: target,
                    actor: Some(actor.id),
                    kind: EventKind::Unavailable,
                });
            } else {
                let effort = next
                    .policies
                    .binary_search_by_key(&actor.id, |policy| policy.person)
                    .ok()
                    .map_or(next.default_effort, |index| next.policies[index].effort);
                candidates.push((target, false, 0, actor.id, effort));
            }
        }
        actors.par_extend(candidates.par_iter().map(|candidate| candidate.3));
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        actors.dedup();
        if candidates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            candidates.par_sort_unstable_by_key(|row| row.0);
        }
        let outcomes: Vec<_> = candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let chosen = *group
                    .par_iter()
                    .min_by_key(|row| (row.1, row.2, row.3))
                    .expect("nonempty capture group");
                let target = animal(chosen.0).expect("validated animal");
                let kind = if restraint::yields(target, chosen.3) {
                    EventKind::Yielded
                } else if chosen.4 > restraint::resistance(target) {
                    EventKind::Forced
                } else {
                    EventKind::Resisted
                };
                let hold = matches!(kind, EventKind::Yielded | EventKind::Forced).then_some(Hold {
                    animal: chosen.0,
                    handler: chosen.3,
                    integrity: chosen.4,
                });
                (
                    hold,
                    Event {
                        tick: self.tick,
                        animal: chosen.0,
                        actor: Some(chosen.3),
                        kind,
                    },
                )
            })
            .collect();
        next.holds
            .par_extend(outcomes.par_iter().filter_map(|row| row.0.clone()));
        if next
            .holds
            .par_windows(2)
            .any(|pair| pair[0].animal > pair[1].animal)
        {
            next.holds.par_sort_unstable_by_key(|hold| hold.animal);
        }
        next.events
            .par_extend(outcomes.into_par_iter().map(|row| row.1));
        if next.events.par_windows(2).any(|pair| {
            (pair[0].animal, pair[0].actor, pair[0].kind)
                > (pair[1].animal, pair[1].actor, pair[1].kind)
        }) {
            next.events
                .par_sort_unstable_by_key(|event| (event.animal, event.actor, event.kind));
        }
        let mut reserved: Vec<_> = next
            .events
            .par_iter()
            .filter_map(|event| (event.kind != EventKind::Unavailable).then_some(event.animal))
            .collect();
        if reserved.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            reserved.par_sort_unstable();
            reserved.dedup();
        }
        let mut reactions: Vec<_> = next
            .events
            .par_iter()
            .filter_map(|event| {
                let actor = event.actor?;
                let response = animal(event.animal)?.restraint_response.as_ref()?;
                let change = match event.kind {
                    EventKind::Yielded => response.on_yielded,
                    EventKind::Forced => response.on_forced,
                    EventKind::Resisted => response.on_resisted,
                    EventKind::Released => response.on_released,
                    _ => 0,
                };
                (change != 0).then_some(restraint::Reaction {
                    animal: event.animal,
                    person: actor,
                    change,
                })
            })
            .collect();
        if reactions
            .par_windows(2)
            .any(|pair| (pair[0].animal, pair[0].person) > (pair[1].animal, pair[1].person))
        {
            reactions.par_sort_unstable_by_key(|reaction| (reaction.animal, reaction.person));
        }
        Ok(Plan {
            state: Some(next),
            actors,
            reserved,
            reactions,
        })
    }
}
