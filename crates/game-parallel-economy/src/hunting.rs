//! Individual pursuit; accepted yields and animal deaths commit together.
use game_parallel_economy_person::Person;
use game_parallel_fauna::Fauna;
use game_parallel_lot_content::AnimalYield;
use game_parallel_lot_runtime::Events;
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub animal: u64,
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hunting {
    pub policies: Vec<Policy>,
}

impl Hunting {
    pub fn validate(&self, people: &HashSet<u64>, fauna: Option<&Fauna>) -> Result<(), String> {
        let animals: HashSet<_> = fauna
            .into_iter()
            .flat_map(|f| f.animals.iter().map(|a| a.id))
            .collect();
        let mut actors: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        actors.par_sort_unstable();
        if actors.par_windows(2).any(|p| p[0] == p[1])
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !animals.contains(&p.animal)
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
            })
        {
            return Err("invalid personal hunting preference".into());
        }
        Ok(())
    }
}

pub struct Plan {
    pub actors: Vec<u64>,
    pub yields: Vec<AnimalYield>,
}

pub struct Context<'a> {
    pub owner: Option<&'a Hunting>,
    pub fauna: Option<&'a Fauna>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub busy: &'a [u64],
    pub feed_targets: &'a [u64],
    pub max_hunger: u32,
    pub seed: u64,
    pub tick: u64,
    pub manual: Option<(u64, u64)>,
}

impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let policies = self
            .owner
            .map_or(&[][..], |owner| owner.policies.as_slice());
        if policies.is_empty() && self.manual.is_none() {
            return Ok(Plan {
                actors: vec![],
                yields: vec![],
            });
        }
        let fauna = self.fauna.ok_or("wildlife is disabled")?;
        let mut animals: Vec<_> = fauna.animals.par_iter().collect();
        if animals.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            animals.par_sort_unstable_by_key(|a| a.id);
        }
        let animal = |id| {
            animals
                .binary_search_by_key(&id, |a| a.id)
                .ok()
                .map(|i| animals[i])
        };
        let mut species_index: Vec<_> = fauna.species.par_iter().collect();
        if species_index
            .par_windows(2)
            .any(|pair| pair[0].id > pair[1].id)
        {
            species_index.par_sort_unstable_by_key(|species| species.id);
        }
        let species = |id| {
            species_index
                .binary_search_by_key(&id, |species| species.id)
                .ok()
                .map(|i| species_index[i])
        };
        let person = |id| {
            self.people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
        };
        let opinions = self.opinions.map(Opinions::index);
        let mut candidates: Vec<_> = policies
            .par_iter()
            .filter_map(|policy| {
                let actor = person(policy.person)?;
                let target = animal(policy.animal)?;
                let yield_ = species(target.species)?.hunting.as_ref();
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(actor.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                (!actor.controlled
                    && actor.health > 0
                    && actor.hunger <= self.max_hunger
                    && target.health > 0
                    && actor.region == Some(target.region)
                    && self.busy.binary_search(&actor.id).is_err()
                    && self.feed_targets.binary_search(&target.id).is_err()
                    && (policy.willing || pressured))
                    .then_some((
                        target.id,
                        game_parallel_genetics::random(self.seed ^ target.id, actor.id, self.tick),
                        actor.id,
                        yield_,
                    ))
            })
            .collect();
        if let Some((actor_id, animal_id)) = self.manual {
            let actor = person(actor_id).ok_or("unknown hunter")?;
            let target = animal(animal_id).ok_or("unknown animal")?;
            if !actor.controlled
                || actor.health == 0
                || target.health == 0
                || actor.region != Some(target.region)
            {
                return Err(
                    "hunting requires a living controlled person and a local animal".into(),
                );
            }
            if self.feed_targets.binary_search(&animal_id).is_err() {
                candidates.push((
                    target.id,
                    game_parallel_genetics::random(self.seed ^ target.id, actor.id, self.tick),
                    actor.id,
                    species(target.species).and_then(|s| s.hunting.as_ref()),
                ));
            }
        }
        let mut actors: Vec<_> = candidates.par_iter().map(|row| row.2).collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        if candidates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            candidates.par_sort_unstable_by_key(|row| row.0);
        }
        let selected: Vec<_> = candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .filter_map(|group| {
                let row = *group
                    .par_iter()
                    .min_by_key(|row| (row.1, row.2))
                    .expect("nonempty target group");
                let yield_ = row.3?;
                let roll = game_parallel_genetics::random(
                    self.seed ^ 0x68756e74 ^ row.0,
                    row.2,
                    self.tick,
                ) % 1000;
                (roll < u64::from(yield_.success_per_thousand))
                    .then_some((row.0, row.1, row.2, yield_))
            })
            .collect();
        base.checked_add(selected.len() as u64)
            .ok_or("hunting action overflow")?;
        let yields = selected
            .par_iter()
            .enumerate()
            .map(|(i, row)| AnimalYield {
                action: base + i as u64,
                animal: row.0,
                hunter: row.2,
                good: row.3.good,
                quantity: row.3.quantity,
            })
            .collect();
        Ok(Plan { actors, yields })
    }
}

impl Plan {
    pub fn accepted(&self, events: &Events) -> Vec<u64> {
        if self.yields.is_empty() {
            return Vec::new();
        }
        let mut accepted: Vec<_> = events
            .physical_actions
            .par_iter()
            .filter(|event| event.accepted)
            .map(|event| event.id)
            .collect();
        accepted.par_sort_unstable();
        let mut animals: Vec<_> = self
            .yields
            .par_iter()
            .filter_map(|yield_| {
                accepted
                    .binary_search(&yield_.action)
                    .is_ok()
                    .then_some(yield_.animal)
            })
            .collect();
        animals.par_sort_unstable();
        animals
    }
    pub fn caught(&self, hunter: u64, animals: &[u64]) -> bool {
        self.yields
            .iter()
            .any(|yield_| yield_.hunter == hunter && animals.binary_search(&yield_.animal).is_ok())
    }
}
