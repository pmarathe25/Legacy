//! Personal recovery of actual material from previously destroyed sites.
use crate::Material;
use game_parallel_economy_claims::Index as Claims;
use game_parallel_economy_person::Person;
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
    pub site: u64,
    pub quantity: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub disregard_claims: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub site: u64,
    pub good: u32,
    pub quantity: u64,
    pub recovered: bool,
}
pub(crate) fn valid(
    policies: &[Policy],
    events: &[Event],
    people: &HashSet<u64>,
    tick: u64,
) -> bool {
    !policies.par_windows(2).any(|w| w[0].person >= w[1].person)
        && !policies.par_iter().any(|p| {
            !people.contains(&p.person)
                || p.site == 0
                || p.quantity == 0
                || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                || p.pressure_threshold == Some(0)
                || p.pressured_by
                    .is_some_and(|id| id == p.person || !people.contains(&id))
        })
        && !events.par_iter().any(|e| {
            e.tick > tick
                || !people.contains(&e.person)
                || e.site == 0
                || e.good == 0
                || e.quantity == 0
        })
}
#[derive(Clone, Copy)]
pub struct Manual {
    pub person: u64,
    pub site: u64,
    pub good: u32,
    pub quantity: u64,
}
struct Choice {
    person: u64,
    site: u64,
    good: u32,
    quantity: u64,
}
pub struct Plan {
    pub actions: Vec<Transaction>,
    pub actors: Vec<u64>,
    rows: Vec<Choice>,
}
pub struct Context<'a> {
    pub policies: &'a [Policy],
    pub materials: &'a [Material],
    pub people: &'a [Person],
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub manual: Option<Manual>,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if (self.policies.is_empty() || self.materials.is_empty()) && self.manual.is_none() {
            return Ok(Plan {
                actions: Vec::new(),
                actors: Vec::new(),
                rows: Vec::new(),
            });
        }
        let opinions = self.opinions.map(Opinions::index);
        let mut choices: Vec<_> = self
            .policies
            .par_iter()
            .filter_map(|policy| {
                if self.busy.binary_search(&policy.person).is_ok() {
                    return None;
                }
                let person = self
                    .people
                    .binary_search_by_key(&policy.person, |row| row.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let material = self
                    .materials
                    .binary_search_by_key(&policy.site, |row| row.site)
                    .ok()
                    .map(|i| &self.materials[i])?;
                if !material.debris
                    || person.controlled
                    || person.health == 0
                    || person.hunger > self.max_hunger
                    || person.region != Some(material.region)
                    || (!policy.disregard_claims
                        && self
                            .claims
                            .is_some_and(|claims| !claims.allows_site(person.id, material.site)))
                {
                    return None;
                }
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(person.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                if !policy.willing && !pressured {
                    return None;
                }
                let debris = self.view.container(Place::Site(material.site))?;
                let holder = self.view.container(Place::Person(person.id))?;
                let good = debris.goods.first()?;
                let quantity = policy
                    .quantity
                    .min(good.quantity)
                    .min(holder.capacity.saturating_sub(holder.used));
                (quantity > 0).then_some(Choice {
                    person: person.id,
                    site: material.site,
                    good: good.good,
                    quantity,
                })
            })
            .collect();
        if let Some(manual) = self.manual {
            let person = self
                .people
                .binary_search_by_key(&manual.person, |row| row.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let material = self
                .materials
                .binary_search_by_key(&manual.site, |row| row.site)
                .ok()
                .map(|i| &self.materials[i])
                .ok_or("unknown site debris")?;
            if !person.controlled
                || person.health == 0
                || person.region != Some(material.region)
                || !material.debris
                || manual.good == 0
                || manual.quantity == 0
                || self.view.container(Place::Person(person.id)).is_none()
            {
                return Err("salvage requires a living controlled person at site debris".into());
            }
            choices.push(Choice {
                person: manual.person,
                site: manual.site,
                good: manual.good,
                quantity: manual.quantity,
            });
        }
        choices.par_sort_unstable_by_key(|choice| choice.person);
        base.checked_add(choices.len() as u64)
            .ok_or("salvage action overflow")?;
        let actions = choices
            .par_iter()
            .enumerate()
            .map(|(i, choice)| {
                let id = base + i as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: Place::Site(choice.site),
                        to: Some(Place::Person(choice.person)),
                        good: choice.good,
                        quantity: choice.quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        Ok(Plan {
            actors: choices.par_iter().map(|row| row.person).collect(),
            actions,
            rows: choices,
        })
    }
}
impl Plan {
    pub fn publish(self, tick: u64, events: &Events) -> Vec<Event> {
        if self.rows.is_empty() {
            return Vec::new();
        }
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|row| row.accepted)
            .map(|row| row.id)
            .collect();
        self.rows
            .into_par_iter()
            .zip(self.actions.into_par_iter())
            .map(|(row, action)| Event {
                tick,
                person: row.person,
                site: row.site,
                good: row.good,
                quantity: row.quantity,
                recovered: accepted.contains(&action.id),
            })
            .collect()
    }
}

/// Previous-boundary debris whose live contents have all expired or actually moved.
pub fn exhausted(materials: &[Material], view: &View, events: &[Event]) -> Vec<Place> {
    let mut recovered: Vec<_> = events
        .par_iter()
        .filter(|event| event.recovered)
        .map(|event| (event.site, event.quantity))
        .collect();
    recovered.par_sort_unstable_by_key(|row| row.0);
    let totals: Vec<_> = recovered
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| {
            (
                group[0].0,
                group.iter().map(|row| u128::from(row.1)).sum::<u128>(),
            )
        })
        .collect();
    materials
        .par_iter()
        .filter(|row| row.debris)
        .filter_map(|row| {
            let used = view.container(Place::Site(row.site))?.used;
            let moved = totals
                .binary_search_by_key(&row.site, |total| total.0)
                .ok()
                .map_or(0, |i| totals[i].1);
            (u128::from(used) == moved).then_some(Place::Site(row.site))
        })
        .collect()
}
