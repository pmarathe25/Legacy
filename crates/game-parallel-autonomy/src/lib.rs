//! Individual travel decisions from bounded personal food observations.
pub mod adapter;
use game_parallel_knowledge::Mind;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{cmp::Reverse, collections::HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Autonomy {
    pub rules: Rules,
    pub actors: Vec<Actor>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub review_interval: u64,
    pub min_health: u32,
    pub min_hunger: u32,
    pub min_destination_food: u32,
    pub min_food_gain: u32,
    pub max_observation_age: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub person: u64,
    pub enabled: bool,
    #[serde(default)]
    pub next_review: u64,
    /// Extra food gain required before leaving a familiar place.
    #[serde(default)]
    pub travel_caution: u32,
    /// Reduces the hunger threshold for seeking a better location.
    #[serde(default)]
    pub hunger_urgency: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub source: u64,
    pub destination: u64,
    pub outcome: Outcome,
    pub reason: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Started,
    Unavailable,
    Rejected,
}
#[derive(Clone, Copy)]
pub struct Situation {
    pub busy: bool,
    pub person: u64,
    pub region: Option<u64>,
    pub health: u32,
    pub hunger: u32,
}
#[derive(Clone, Copy)]
pub struct Path {
    pub source: u64,
    pub destination: u64,
    pub ticks: u32,
}
#[derive(Clone, Copy)]
pub struct Decision {
    pub person: u64,
    pub source: u64,
    pub destination: u64,
}
pub struct Draft {
    pub state: Autonomy,
    pub decisions: Vec<Decision>,
}

impl Autonomy {
    /// Initialize independent generated actors and stagger their first reviews.
    pub fn generate_actors(&mut self, people: &[u64], controlled: Option<u64>, seed: u64) {
        let interval = self.rules.review_interval.max(1);
        self.actors = people
            .par_iter()
            .copied()
            .filter(|&person| Some(person) != controlled)
            .map(|person| Actor {
                person,
                enabled: true,
                next_review: person.wrapping_add(seed) % interval,
                travel_caution: 0,
                hunger_urgency: 0,
            })
            .collect();
        self.events.clear();
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        now: u64,
    ) -> Result<(), &'static str> {
        if self.rules.review_interval == 0 {
            return Err("invalid autonomy rules");
        }
        let mut actors = HashSet::new();
        for a in &self.actors {
            if !people.contains(&a.person) || !actors.insert(a.person) {
                return Err("invalid autonomous actor");
            }
        }
        if self.events.iter().any(|e| {
            e.tick > now
                || !actors.contains(&e.person)
                || e.source == e.destination
                || !regions.contains(&e.source)
                || !regions.contains(&e.destination)
        }) {
            return Err("invalid autonomy event");
        }
        Ok(())
    }
    /// Each actor evaluates only their own committed observations and local outgoing paths.
    pub fn prepare(
        &self,
        people: &[Situation],
        paths: &[Path],
        minds: &[Mind],
        regions: &HashSet<u64>,
        now: u64,
    ) -> Result<Draft, &'static str> {
        self.prepare_inner(people, paths, minds, regions, now, false)
    }
    /// The private host supplies validated actors, people and routes.
    pub fn prepare_validated(
        &self,
        people: &[Situation],
        paths: &[Path],
        minds: &[Mind],
        regions: &HashSet<u64>,
        now: u64,
    ) -> Result<Draft, &'static str> {
        self.prepare_inner(people, paths, minds, regions, now, true)
    }
    fn prepare_inner(
        &self,
        people: &[Situation],
        paths: &[Path],
        minds: &[Mind],
        regions: &HashSet<u64>,
        now: u64,
        validated: bool,
    ) -> Result<Draft, &'static str> {
        if self.rules.review_interval == 0 {
            return Err("invalid autonomy rules");
        }
        let ordered = if people
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            let mut rows = people.to_vec();
            rows.par_sort_unstable_by_key(|p| p.person);
            Some(rows)
        } else {
            None
        };
        let situations = ordered.as_deref().unwrap_or(people);
        if !validated
            && situations
                .par_windows(2)
                .any(|p| p[0].person == p[1].person)
        {
            return Err("duplicate autonomous situation");
        }
        if !validated {
            let mut actor_ids: Vec<_> = self.actors.par_iter().map(|a| a.person).collect();
            actor_ids.par_sort_unstable();
            if actor_ids.par_windows(2).any(|p| p[0] == p[1])
                || actor_ids
                    .par_iter()
                    .any(|id| situations.binary_search_by_key(id, |p| p.person).is_err())
            {
                return Err("invalid autonomous actor");
            }
            if self.events.par_iter().any(|e| {
                e.tick > now
                    || actor_ids.binary_search(&e.person).is_err()
                    || e.source == e.destination
                    || !regions.contains(&e.source)
                    || !regions.contains(&e.destination)
            }) {
                return Err("invalid autonomy event");
            }
        }
        let mut minds: Vec<_> = minds.par_iter().collect();
        if minds
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            minds.par_sort_unstable_by_key(|m| m.person);
        }
        if !validated && minds.par_windows(2).any(|p| p[0].person == p[1].person) {
            return Err("duplicate mind");
        }
        let mut paths = paths.to_vec();
        if paths.par_windows(2).any(|pair| {
            (pair[0].source, pair[0].destination) > (pair[1].source, pair[1].destination)
        }) {
            paths.par_sort_unstable_by_key(|p| (p.source, p.destination));
        }
        if !validated
            && (paths
                .par_windows(2)
                .any(|p| (p[0].source, p[0].destination) == (p[1].source, p[1].destination))
                || paths.par_iter().any(|p| {
                    p.ticks == 0
                        || p.source == p.destination
                        || !regions.contains(&p.source)
                        || !regions.contains(&p.destination)
                }))
        {
            return Err("invalid autonomous path");
        }
        let mind_base = minds.first().and_then(|first| {
            minds.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((minds.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let mind_index = |id: u64| {
            mind_base
                .and_then(|first| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < minds.len())
                .or_else(|| {
                    if mind_base.is_none() {
                        minds.binary_search_by_key(&id, |row| row.person).ok()
                    } else {
                        None
                    }
                })
        };
        self.prepare_records_validated(
            situations,
            &paths,
            |person| mind_index(person).map(|i| minds[i].records.as_slice()),
            now,
        )
    }
    /// Sorted validated situations/routes and committed personal record access.
    pub fn prepare_records_validated<'a>(
        &self,
        situations: &[Situation],
        paths: &[Path],
        records_for: impl Fn(u64) -> Option<&'a [game_parallel_knowledge::Record]> + Sync,
        now: u64,
    ) -> Result<Draft, &'static str> {
        let situation_base = situations.first().and_then(|first| {
            situations.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((situations.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let situation_index = |id: u64| {
            situation_base
                .and_then(|first| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < situations.len())
                .or_else(|| {
                    if situation_base.is_none() {
                        situations.binary_search_by_key(&id, |row| row.person).ok()
                    } else {
                        None
                    }
                })
        };
        let rows: Result<Vec<(Actor, Option<Decision>)>, &'static str> = self
            .actors
            .par_iter()
            .map(|old| {
                let mut actor = old.clone();
                let situation = situations[situation_index(actor.person).expect("validated actor")];
                let Some(source) = situation.region else {
                    return Ok((actor, None));
                };
                if !actor.enabled
                    || situation.busy
                    || situation.health == 0
                    || situation.health < self.rules.min_health
                    || actor.next_review > now
                {
                    return Ok((actor, None));
                }
                actor.next_review = now
                    .checked_add(self.rules.review_interval)
                    .ok_or("autonomy review overflow")?;
                if situation.hunger < self.rules.min_hunger.saturating_sub(actor.hunger_urgency) {
                    return Ok((actor, None));
                }
                let Some(records) = records_for(actor.person) else {
                    return Ok((actor, None));
                };
                let valid = |r: &&game_parallel_knowledge::Record| {
                    r.observed_at <= now && now - r.observed_at <= self.rules.max_observation_age
                };
                let Some(local) = records.iter().filter(valid).find(|r| r.region == source) else {
                    return Ok((actor, None));
                };
                let start = paths.partition_point(|p| p.source < source);
                let end = paths.partition_point(|p| p.source <= source);
                let best = paths[start..end]
                    .iter()
                    .filter_map(|path| {
                        let record = records
                            .iter()
                            .filter(valid)
                            .find(|r| r.region == path.destination)?;
                        (record.food >= self.rules.min_destination_food
                            && u64::from(record.food)
                                >= u64::from(local.food)
                                    + u64::from(self.rules.min_food_gain)
                                    + u64::from(actor.travel_caution))
                        .then_some((
                            record.food,
                            Reverse(path.ticks),
                            Reverse(path.destination),
                            path.destination,
                        ))
                    })
                    .max();
                let decision = best.map(|(_, _, _, destination)| Decision {
                    person: actor.person,
                    source,
                    destination,
                });
                Ok((actor, decision))
            })
            .collect();
        let (actors, choices): (Vec<_>, Vec<_>) = rows?.into_par_iter().unzip();
        let mut decisions: Vec<_> = choices.into_par_iter().flatten().collect();
        if decisions
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            decisions.par_sort_unstable_by_key(|d| d.person);
        }
        Ok(Draft {
            state: Autonomy {
                rules: self.rules.clone(),
                actors,
                events: Vec::new(),
            },
            decisions,
        })
    }
}
