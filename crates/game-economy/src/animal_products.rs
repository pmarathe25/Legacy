//! Personal collection of bounded renewable animal products.
use game_economy_person::Person;
use game_fauna::{Fauna, handling};
use game_lot_content::AnimalProduct;
use game_lot_runtime::Events;
use game_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub species: u32,
    pub good: u32,
    pub quantity: u64,
    pub grow_ticks: u64,
    pub min_age: u64,
    pub min_health: u32,
    pub max_hunger: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub animal: u64,
    pub growth: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub animal: u64,
    pub willing: bool,
    #[serde(default)]
    pub force_if_held: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Collected,
    Refused,
    Unavailable,
    NoCapacity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub animal: u64,
    pub outcome: Outcome,
    pub forced: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimalProducts {
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub progress: Vec<Progress>,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    rules: Vec<RuleConfig>,
    #[serde(default)]
    policies: Vec<Policy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleConfig {
    species: u32,
    good: String,
    quantity: u64,
    grow_ticks: u64,
    min_age: u64,
    min_health: u32,
    max_hunger: u32,
}
impl Config {
    pub fn build(self, lookup: impl Fn(&str) -> Option<u32>) -> Result<AnimalProducts, String> {
        let rules = self
            .rules
            .into_iter()
            .map(|r| {
                Ok(Rule {
                    species: r.species,
                    good: lookup(&r.good)
                        .ok_or_else(|| format!("unknown animal product {}", r.good))?,
                    quantity: r.quantity,
                    grow_ticks: r.grow_ticks,
                    min_age: r.min_age,
                    min_health: r.min_health,
                    max_hunger: r.max_hunger,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(AnimalProducts {
            rules,
            progress: vec![],
            policies: self.policies,
            events: vec![],
        })
    }
}
fn index_by<T>(rows: &[T], id: u64, key: impl Fn(&T) -> u64) -> Option<usize> {
    rows.first()
        .and_then(|first| id.checked_sub(key(first)))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&i| rows.get(i).is_some_and(|r| key(r) == id))
        .or_else(|| rows.binary_search_by_key(&id, key).ok())
}
impl AnimalProducts {
    pub fn validate(
        &mut self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        fauna: Option<&Fauna>,
        tick: u64,
    ) -> Result<(), String> {
        let fauna = fauna.ok_or("animal products require wildlife")?;
        self.rules.par_sort_unstable_by_key(|r| r.species);
        self.progress.par_sort_unstable_by_key(|r| r.animal);
        self.policies.par_sort_unstable_by_key(|p| p.person);
        self.events.par_sort_unstable_by_key(|e| e.animal);
        let species: HashSet<_> = fauna.species.par_iter().map(|s| s.id).collect();
        let animals: std::collections::HashMap<_, _> = fauna
            .animals
            .par_iter()
            .map(|a| (a.id, a.species))
            .collect();
        if self
            .rules
            .par_windows(2)
            .any(|w| w[0].species >= w[1].species)
            || self.rules.par_iter().any(|r| {
                !species.contains(&r.species)
                    || !goods.contains(&r.good)
                    || r.quantity == 0
                    || r.grow_ticks == 0
                    || r.min_health == 0
            })
            || self
                .progress
                .par_windows(2)
                .any(|w| w[0].animal >= w[1].animal)
            || self.progress.par_iter().any(|p| {
                animals
                    .get(&p.animal)
                    .and_then(|species| {
                        self.rules.binary_search_by_key(species, |r| r.species).ok()
                    })
                    .is_none_or(|i| p.growth > self.rules[i].grow_ticks)
            })
            || self
                .policies
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !animals.contains_key(&p.animal)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
            })
            || self
                .events
                .par_windows(2)
                .any(|w| w[0].animal >= w[1].animal)
            || self.events.par_iter().any(|e| {
                e.tick > tick || !people.contains(&e.person) || !animals.contains_key(&e.animal)
            })
        {
            return Err("invalid animal product state".into());
        }
        Ok(())
    }
    pub fn advance(&self, fauna: &Fauna, events: Vec<Event>) -> AnimalProducts {
        let mut collected: Vec<_> = events
            .par_iter()
            .filter(|e| e.outcome == Outcome::Collected)
            .map(|e| e.animal)
            .collect();
        if collected.par_windows(2).any(|w| w[0] > w[1]) {
            collected.par_sort_unstable();
        }
        let mut progress: Vec<_> = fauna
            .animals
            .par_iter()
            .filter_map(|animal| {
                let rule = self
                    .rules
                    .binary_search_by_key(&animal.species, |r| r.species)
                    .ok()
                    .map(|i| &self.rules[i])?;
                let old = index_by(&self.progress, animal.id, |p| p.animal)
                    .map_or(0, |i| self.progress[i].growth);
                let growth = if index_by(&collected, animal.id, |id| *id).is_some() {
                    0
                } else if animal.health >= rule.min_health
                    && animal.age >= rule.min_age
                    && animal.hunger <= rule.max_hunger
                {
                    old.saturating_add(1).min(rule.grow_ticks)
                } else {
                    old
                };
                Some(Progress {
                    animal: animal.id,
                    growth,
                })
            })
            .collect();
        if progress.par_windows(2).any(|w| w[0].animal > w[1].animal) {
            progress.par_sort_unstable_by_key(|p| p.animal);
        }
        AnimalProducts {
            rules: self.rules.clone(),
            progress,
            policies: self.policies.par_iter().cloned().collect(),
            events,
        }
    }
}
#[derive(Clone, Copy)]
struct Candidate {
    animal: u64,
    person: u64,
    force: bool,
    manual: bool,
    priority: u64,
}
pub struct Plan {
    pub actors: Vec<u64>,
    pub reserved: Vec<u64>,
    pub yields: Vec<AnimalProduct>,
    events: Vec<Event>,
    pub handling: Vec<handling::Event>,
}
pub struct Context<'a> {
    pub owner: Option<&'a AnimalProducts>,
    pub fauna: Option<&'a Fauna>,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub holds: Option<&'a game_animal_restraint::Restraints>,
    pub busy: &'a [u64],
    pub blocked: &'a [u64],
    pub max_hunger: u32,
    pub manual: Option<(u64, u64, bool)>,
    pub seed: u64,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let Some(owner) = self.owner else {
            if self.manual.is_some() {
                return Err("animal products are disabled".into());
            }
            return Ok(Plan {
                actors: vec![],
                reserved: vec![],
                yields: vec![],
                events: vec![],
                handling: vec![],
            });
        };
        if owner.policies.is_empty() && self.manual.is_none() {
            return Ok(Plan {
                actors: vec![],
                reserved: vec![],
                yields: vec![],
                events: vec![],
                handling: vec![],
            });
        }
        let fauna = self.fauna.ok_or("animal products require wildlife")?;
        let mut animals: Vec<_> = fauna.animals.par_iter().collect();
        if animals.par_windows(2).any(|w| w[0].id > w[1].id) {
            animals.par_sort_unstable_by_key(|a| a.id);
        }
        let find_animal = |id| index_by(&animals, id, |a| a.id).map(|i| animals[i]);
        let opinion = self
            .opinions
            .filter(|_| owner.policies.par_iter().any(|p| p.pressured_by.is_some()))
            .map(Opinions::index);
        let mut candidates: Vec<_> = owner
            .policies
            .par_iter()
            .filter_map(|p| {
                let person = &self.people[index_by(self.people, p.person, |p| p.id)?];
                let animal = find_animal(p.animal)?;
                let rule = owner
                    .rules
                    .binary_search_by_key(&animal.species, |r| r.species)
                    .ok()
                    .map(|i| &owner.rules[i])?;
                let growth = index_by(&owner.progress, animal.id, |r| r.animal)
                    .map_or(0, |i| owner.progress[i].growth);
                if growth < rule.grow_ticks
                    || animal.age < rule.min_age
                    || animal.health < rule.min_health
                    || animal.hunger > rule.max_hunger
                {
                    return None;
                }
                let pressured =
                    p.pressured_by
                        .zip(p.pressure_threshold)
                        .is_some_and(|(other, threshold)| {
                            opinion.as_ref().is_some_and(|o| {
                                let (fear, obligation) = o.feelings(p.person, other);
                                fear.saturating_add(obligation) >= threshold
                            })
                        });
                if person.controlled
                    || person.health == 0
                    || person.hunger > self.max_hunger
                    || person.region != Some(animal.region)
                    || animal.health == 0
                    || self.busy.binary_search(&p.person).is_ok()
                    || self.blocked.binary_search(&p.animal).is_ok()
                    || (!p.willing && !pressured)
                {
                    return None;
                }
                let mut priority = self.seed ^ self.tick.rotate_left(17) ^ p.person;
                priority = (priority ^ (priority >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                priority = (priority ^ (priority >> 27)).wrapping_mul(0x94d049bb133111eb);
                priority ^= priority >> 31;
                Some(Candidate {
                    animal: p.animal,
                    person: p.person,
                    force: p.force_if_held,
                    manual: false,
                    priority,
                })
            })
            .collect();
        if let Some((person, animal, force)) = self.manual {
            let p = self
                .people
                .get(index_by(self.people, person, |p| p.id).ok_or("unknown person")?)
                .expect("person index");
            let a = find_animal(animal).ok_or("unknown animal")?;
            if !p.controlled || p.health == 0 || a.health == 0 || p.region != Some(a.region) {
                return Err(
                    "collection requires a living controlled person and local animal".into(),
                );
            }
            if self.blocked.binary_search(&animal).is_err() {
                candidates.push(Candidate {
                    animal,
                    person,
                    force,
                    manual: true,
                    priority: 0,
                });
            }
        }
        let mut actors: Vec<_> = candidates.par_iter().map(|c| c.person).collect();
        actors.extend(self.manual.map(|m| m.0));
        if actors.par_windows(2).any(|w| w[0] > w[1]) {
            actors.par_sort_unstable();
        }
        actors.dedup();
        let order = |c: &Candidate| (c.animal, !c.manual, c.priority, c.person);
        if candidates
            .par_windows(2)
            .any(|w| order(&w[0]) > order(&w[1]))
        {
            candidates.par_sort_unstable_by_key(order);
        }
        let chosen: Vec<_> = candidates
            .par_chunk_by(|a, b| a.animal == b.animal)
            .map(|g| g[0])
            .collect();
        let decisions: Vec<_> = chosen
            .into_par_iter()
            .map(|c| {
                let animal = find_animal(c.animal).expect("validated animal");
                let rule = owner
                    .rules
                    .binary_search_by_key(&animal.species, |r| r.species)
                    .ok()
                    .map(|i| &owner.rules[i]);
                let progress = index_by(&owner.progress, c.animal, |p| p.animal)
                    .map_or(0, |i| owner.progress[i].growth);
                let ready = rule.is_some_and(|r| {
                    progress >= r.grow_ticks
                        && animal.age >= r.min_age
                        && animal.health >= r.min_health
                        && animal.hunger <= r.max_hunger
                });
                if !ready {
                    return (
                        Event {
                            tick: self.tick,
                            person: c.person,
                            animal: c.animal,
                            outcome: Outcome::Unavailable,
                            forced: false,
                        },
                        None,
                        None,
                    );
                }
                let response = handling::respond(animal, c.person);
                let handling_event = handling::Event {
                    tick: self.tick,
                    person: c.person,
                    animal: c.animal,
                    outcome: response,
                };
                let forced = response != handling::Outcome::Accepted
                    && c.force
                    && self.holds.is_some_and(|holds| {
                        holds
                            .holds
                            .binary_search_by_key(&c.animal, |h| h.animal)
                            .ok()
                            .is_some_and(|i| holds.holds[i].handler == c.person)
                    });
                if response != handling::Outcome::Accepted && !forced {
                    return (
                        Event {
                            tick: self.tick,
                            person: c.person,
                            animal: c.animal,
                            outcome: Outcome::Refused,
                            forced: false,
                        },
                        Some(handling_event),
                        None,
                    );
                }
                let rule = rule.expect("ready rule");
                (
                    Event {
                        tick: self.tick,
                        person: c.person,
                        animal: c.animal,
                        outcome: Outcome::NoCapacity,
                        forced,
                    },
                    Some(handling_event),
                    Some((rule.good, rule.quantity)),
                )
            })
            .collect();
        let products: Vec<_> = decisions
            .par_iter()
            .filter_map(|(event, _, product)| {
                product.map(|(good, quantity)| (event.animal, event.person, good, quantity))
            })
            .collect();
        base.checked_add(products.len() as u64)
            .ok_or("animal product action overflow")?;
        let yields = products
            .par_iter()
            .enumerate()
            .map(|(i, &(animal, collector, good, quantity))| AnimalProduct {
                action: base + i as u64,
                animal,
                collector,
                good,
                quantity,
            })
            .collect();
        let handling_events = decisions
            .par_iter()
            .filter_map(|(_, event, _)| event.clone())
            .collect();
        let events: Vec<_> = decisions
            .into_par_iter()
            .map(|(event, _, _)| event)
            .collect();
        let reserved = events
            .par_iter()
            .filter(|e| e.outcome != Outcome::Unavailable)
            .map(|e| e.animal)
            .collect();
        Ok(Plan {
            actors,
            reserved,
            yields,
            events,
            handling: handling_events,
        })
    }
}
impl Plan {
    pub fn publish(mut self, events: &Events) -> (Vec<Event>, Vec<handling::Event>) {
        if self.yields.is_empty() {
            return (self.events, self.handling);
        }
        let first = self.yields[0].action;
        if let Some(start) = events
            .physical_actions
            .par_iter()
            .position_any(|e| e.id == first)
            && let Some(end) = start.checked_add(self.yields.len())
            && let Some(receipts) = events.physical_actions.get(start..end)
            && receipts
                .par_iter()
                .zip(self.yields.par_iter())
                .all(|(receipt, yield_)| receipt.id == yield_.action)
        {
            self.events.par_iter_mut().for_each(|event| {
                if index_by(&self.yields, event.animal, |yield_| yield_.animal)
                    .is_some_and(|i| receipts[i].accepted)
                {
                    event.outcome = Outcome::Collected;
                }
            });
            return (self.events, self.handling);
        }
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|e| e.accepted)
            .map(|e| e.id)
            .collect();
        self.events.par_iter_mut().for_each(|e| {
            if index_by(&self.yields, e.animal, |y| y.animal)
                .is_some_and(|i| accepted.contains(&self.yields[i].action))
            {
                e.outcome = Outcome::Collected;
            }
        });
        (self.events, self.handling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_lots::TransactionOutcome;

    fn plan() -> Plan {
        Plan {
            actors: vec![1, 2],
            reserved: vec![1, 2],
            yields: vec![
                AnimalProduct {
                    action: 10,
                    animal: 1,
                    collector: 1,
                    good: 2,
                    quantity: 1,
                },
                AnimalProduct {
                    action: 11,
                    animal: 2,
                    collector: 2,
                    good: 2,
                    quantity: 1,
                },
            ],
            events: vec![
                Event {
                    tick: 1,
                    person: 1,
                    animal: 1,
                    outcome: Outcome::NoCapacity,
                    forced: false,
                },
                Event {
                    tick: 1,
                    person: 2,
                    animal: 2,
                    outcome: Outcome::NoCapacity,
                    forced: false,
                },
            ],
            handling: vec![],
        }
    }
    fn events(physical_actions: Vec<TransactionOutcome>) -> Events {
        Events {
            tick: 1,
            started: vec![],
            start_denied: vec![],
            abandoned: vec![],
            suspended: vec![],
            adopted: vec![],
            finished: vec![],
            delivered: vec![],
            output_lost: vec![],
            interrupted: vec![],
            returned: vec![],
            empty_returns: vec![],
            spoiled: vec![],
            physical_actions,
            physical_movements: Default::default(),
        }
    }
    #[test]
    fn product_receipts_handle_ordered_shuffled_and_missing_actions() {
        let r = |id, accepted| TransactionOutcome { id, accepted };
        let expected = vec![
            Event {
                tick: 1,
                person: 1,
                animal: 1,
                outcome: Outcome::Collected,
                forced: false,
            },
            Event {
                tick: 1,
                person: 2,
                animal: 2,
                outcome: Outcome::NoCapacity,
                forced: false,
            },
        ];
        assert_eq!(
            plan()
                .publish(&events(vec![r(2, true), r(10, true), r(11, false)]))
                .0,
            expected
        );
        assert_eq!(
            plan()
                .publish(&events(vec![r(11, false), r(2, true), r(10, true)]))
                .0,
            expected
        );
        assert_eq!(plan().publish(&events(vec![r(10, true)])).0, expected);
    }
}
