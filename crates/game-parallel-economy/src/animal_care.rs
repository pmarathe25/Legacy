//! Personal preferences for offering carried goods to locally observed animals.
use game_parallel_economy_person::Person;
use game_parallel_fauna::{Fauna, Feeding};
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    #[serde(default)]
    pub animal: Option<u64>,
    pub good: u32,
    pub quantity: u64,
    pub willing: bool,
    pub health_at_most: u32,
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
pub struct AnimalCare {
    #[serde(default)]
    pub inherit_births: bool,
    pub policies: Vec<Policy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub inherit_births: bool,
    pub policies: Vec<PolicyConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    person: u64,
    #[serde(default)]
    animal: Option<u64>,
    good: String,
    quantity: u64,
    willing: bool,
    health_at_most: u32,
    #[serde(default)]
    affinity_at_least: Option<i32>,
    #[serde(default)]
    affinity_at_most: Option<i32>,
    #[serde(default)]
    pressured_by: Option<u64>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
}

impl Config {
    pub fn build(self, lookup: impl Fn(&str) -> Option<u32>) -> Result<AnimalCare, String> {
        let policies = self
            .policies
            .into_iter()
            .map(|p| {
                Ok(Policy {
                    person: p.person,
                    animal: p.animal,
                    good: lookup(&p.good)
                        .ok_or_else(|| format!("unknown animal care good {}", p.good))?,
                    quantity: p.quantity,
                    willing: p.willing,
                    health_at_most: p.health_at_most,
                    affinity_at_least: p.affinity_at_least,
                    affinity_at_most: p.affinity_at_most,
                    pressured_by: p.pressured_by,
                    pressure_threshold: p.pressure_threshold,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(AnimalCare {
            inherit_births: self.inherit_births,
            policies,
        })
    }
}

/// A newborn may acquire a general inclination, without inheriting a target or pressure.
pub fn enroll(
    owner: Option<&AnimalCare>,
    family: Option<&crate::Families>,
    tick: u64,
) -> Vec<Policy> {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return Vec::new();
    };
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|member| member.born == Some(tick))
        .collect();
    if newborns.is_empty() {
        return Vec::new();
    }
    let mut policies: Vec<_> = owner.policies.par_iter().collect();
    if policies.par_windows(2).any(|p| p[0].person > p[1].person) {
        policies.par_sort_unstable_by_key(|p| p.person);
    }
    newborns
        .par_iter()
        .filter_map(|member| {
            if policies
                .binary_search_by_key(&member.id, |p| p.person)
                .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                policies
                    .binary_search_by_key(id, |p| p.person)
                    .ok()
                    .map(|i| policies[i])
            })?;
            Some(Policy {
                person: member.id,
                animal: None,
                good: parent.good,
                quantity: parent.quantity,
                willing: parent.willing,
                health_at_most: parent.health_at_most,
                affinity_at_least: parent.affinity_at_least,
                affinity_at_most: parent.affinity_at_most,
                pressured_by: None,
                pressure_threshold: None,
            })
        })
        .collect()
}

impl AnimalCare {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        fauna: Option<&Fauna>,
    ) -> Result<(), String> {
        let animals: HashSet<_> = fauna
            .into_iter()
            .flat_map(|f| f.animals.par_iter().map(|a| a.id).collect::<Vec<_>>())
            .collect();
        let mut actors: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        actors.par_sort_unstable();
        if (fauna.is_none() && !self.policies.is_empty())
            || actors.par_windows(2).any(|p| p[0] == p[1])
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || p.animal.is_some_and(|id| !animals.contains(&id))
                    || !goods.contains(&p.good)
                    || p.quantity == 0
                    || p.affinity_at_least
                        .zip(p.affinity_at_most)
                        .is_some_and(|(a, b)| a > b)
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
            })
        {
            return Err("invalid personal animal care preference".into());
        }
        Ok(())
    }
}

pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
    records: Vec<(u64, Feeding)>,
}

pub struct Context<'a> {
    pub owner: Option<&'a AnimalCare>,
    pub fauna: Option<&'a Fauna>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub animal_feelings: Option<&'a crate::AnimalFeelings>,
    pub view: &'a View,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub seed: u64,
    pub tick: u64,
}

impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        #[cfg(feature = "profile")]
        let _profile = game_diagnostics::span("economy.animal_care.plan");
        let policies = self
            .owner
            .map_or(&[][..], |owner| owner.policies.as_slice());
        if policies.is_empty() {
            return Ok(Plan {
                actors: Vec::new(),
                actions: Vec::new(),
                records: Vec::new(),
            });
        }
        let mut animals: Vec<_> = self
            .fauna
            .into_iter()
            .flat_map(|f| f.animals.par_iter().collect::<Vec<_>>())
            .collect();
        animals.par_sort_unstable_by_key(|a| a.id);
        let mut visible: Vec<_> = if policies.par_iter().any(|p| p.animal.is_none()) {
            animals
                .par_iter()
                .filter(|a| a.health > 0)
                .map(|a| (a.region, a.id))
                .collect()
        } else {
            Vec::new()
        };
        visible.par_sort_unstable();
        let opinions = self.opinions.map(Opinions::index);
        let mut choices: Vec<_> = policies
            .par_iter()
            .filter_map(|p| {
                let actor = self
                    .people
                    .binary_search_by_key(&p.person, |a| a.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let target = p.animal.or_else(|| {
                    let region = actor.region?;
                    let start = visible.partition_point(|row| row.0 < region);
                    let end = visible.partition_point(|row| row.0 <= region);
                    if start == end {
                        return None;
                    }
                    let offset = (game_parallel_genetics::random(
                        self.seed ^ 0x63617265,
                        actor.id,
                        self.tick,
                    ) % (end - start) as u64) as usize;
                    Some(visible[start + offset].1)
                })?;
                let animal = animals
                    .binary_search_by_key(&target, |a| a.id)
                    .ok()
                    .map(|i| animals[i])?;
                let pressured =
                    p.pressured_by
                        .zip(p.pressure_threshold)
                        .is_some_and(|(other, threshold)| {
                            opinions.as_ref().is_some_and(|index| {
                                let (fear, obligation) = index.feelings(actor.id, other);
                                fear.saturating_add(obligation) >= threshold
                            })
                        });
                let affinity = || {
                    self.animal_feelings
                        .and_then(|f| f.affinity(actor.id, animal.id))
                        .unwrap_or(0)
                };
                (!actor.controlled
                    && actor.health > 0
                    && actor.hunger <= self.max_hunger
                    && animal.health > 0
                    && actor.region == Some(animal.region)
                    && self.busy.binary_search(&actor.id).is_err()
                    && (pressured
                        || (p.willing
                            && animal.health <= p.health_at_most
                            && p.affinity_at_least.is_none_or(|min| affinity() >= min)
                            && p.affinity_at_most.is_none_or(|max| affinity() <= max)))
                    && self
                        .view
                        .container(Place::Person(actor.id))
                        .is_some_and(|c| c.quantity(p.good) >= p.quantity))
                .then_some((
                    actor.id,
                    Feeding {
                        animal: animal.id,
                        person: Some(actor.id),
                        good: p.good,
                        quantity: p.quantity,
                    },
                ))
            })
            .collect();
        choices.par_sort_unstable_by_key(|row| row.0);
        base.checked_add(choices.len() as u64)
            .ok_or("animal care action overflow")?;
        let actors = choices.par_iter().map(|row| row.0).collect();
        let records: Vec<_> = choices
            .par_iter()
            .enumerate()
            .map(|(i, row)| (base + i as u64, row.1))
            .collect();
        let actions = choices
            .par_iter()
            .enumerate()
            .map(|(i, (actor, food))| {
                let id = base + i as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: Place::Person(*actor),
                        to: None,
                        good: food.good,
                        quantity: food.quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        Ok(Plan {
            actors,
            actions,
            records,
        })
    }
}

impl Plan {
    pub fn targets(&self) -> Vec<u64> {
        let mut targets: Vec<_> = self
            .records
            .par_iter()
            .map(|(_, food)| food.animal)
            .collect();
        targets.par_sort_unstable();
        targets.dedup();
        targets
    }
    pub fn accepted(&self, events: &Events) -> Vec<Feeding> {
        #[cfg(feature = "profile")]
        let _profile = game_diagnostics::span("economy.animal_care.receipts");
        if self.records.is_empty() {
            return Vec::new();
        }
        let mut accepted: Vec<_> = events
            .physical_actions
            .par_iter()
            .filter(|e| e.accepted)
            .map(|e| e.id)
            .collect();
        accepted.par_sort_unstable();
        self.records
            .par_iter()
            .filter_map(|(id, food)| accepted.binary_search(id).is_ok().then_some(*food))
            .collect()
    }
}
