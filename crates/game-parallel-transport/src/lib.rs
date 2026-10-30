//! Saved travel state and parallel traveler advancement.
pub mod routing;
use game_parallel_cargo::Cargo;
use game_parallel_people::{Person, Region, Rules};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Move {
    pub person: u64,
    pub destination: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub source: u64,
    pub destination: u64,
    pub ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Traveler {
    pub person: Person,
    pub source: u64,
    pub destination: u64,
    pub ticks_remaining: u32,
}

pub fn valid_destinations(regions: &[Region], travelers: &[Traveler]) -> bool {
    if travelers.is_empty() {
        return true;
    }
    let destinations: HashSet<_> = regions.iter().map(|r| r.id).collect();
    destinations.len() == regions.len()
        && travelers
            .par_iter()
            .all(|t| t.ticks_remaining > 0 && destinations.contains(&t.destination))
}
pub fn advance(
    regions: &mut [Region],
    travelers: &mut Vec<Traveler>,
    rules: &Rules,
    cargo: Option<&mut Cargo>,
) {
    if travelers.is_empty() {
        return;
    }
    let mut eaten: Vec<u64> = if let Some(cargo) = cargo.as_ref().filter(|c| c.auto_eat_travel) {
        let mut eligible: Vec<u64> = cargo
            .holdings
            .par_iter()
            .filter(|h| h.food >= rules.food_per_meal)
            .map(|h| h.person)
            .collect();
        eligible.par_sort_unstable();
        travelers
            .par_iter()
            .filter(|t| {
                t.person.health > 0
                    && t.person.hunger.saturating_add(rules.hunger_per_tick)
                        >= rules.hunger_satisfied
                    && eligible.binary_search(&t.person.id).is_ok()
            })
            .map(|t| t.person.id)
            .collect()
    } else {
        Vec::new()
    };
    eaten.par_sort_unstable();
    travelers.par_iter_mut().for_each(|traveler| {
        let p = &mut traveler.person;
        if p.health > 0 {
            p.hunger = p.hunger.saturating_add(rules.hunger_per_tick);
            if eaten.binary_search(&p.id).is_ok() {
                p.hunger = p.hunger.saturating_sub(rules.hunger_satisfied);
            }
            if p.hunger >= rules.starvation_threshold {
                p.health = p.health.saturating_sub(rules.starvation_damage);
            } else {
                p.health = p
                    .health
                    .saturating_add(rules.recovery)
                    .min(rules.max_health);
            }
        }
        traveler.ticks_remaining = traveler.ticks_remaining.saturating_sub(1);
    });
    if !eaten.is_empty() {
        let cargo = cargo.expect("eating requires cargo");
        cargo.holdings.par_iter_mut().for_each(|holding| {
            if eaten.binary_search(&holding.person).is_ok() {
                holding.food -= rules.food_per_meal;
            }
        });
        if cargo.holdings.par_iter().any(|h| h.food == 0) {
            cargo.holdings = cargo
                .holdings
                .par_drain(..)
                .filter(|h| h.food > 0)
                .collect();
        }
    }
    if !travelers
        .par_iter()
        .any(|traveler| traveler.ticks_remaining == 0)
    {
        return;
    }
    // Collect arrivals independently, then use destination and stable person ID
    // as the routing key. Region owners append disjoint slices at the barrier.
    let mut arrivals: Vec<_> = travelers
        .par_iter()
        .filter(|t| t.ticks_remaining == 0)
        .map(|t| (t.destination, t.person.clone()))
        .collect();
    if arrivals
        .par_windows(2)
        .any(|pair| (pair[0].0, pair[0].1.id) > (pair[1].0, pair[1].1.id))
    {
        arrivals.par_sort_unstable_by_key(|(destination, person)| (*destination, person.id));
    }
    let remaining: Vec<_> = travelers
        .par_iter()
        .filter(|t| t.ticks_remaining != 0)
        .cloned()
        .collect();
    regions.par_iter_mut().for_each(|region| {
        let start = arrivals.partition_point(|(destination, _)| *destination < region.id);
        let end = arrivals.partition_point(|(destination, _)| *destination <= region.id);
        region.people.par_extend(
            arrivals[start..end]
                .par_iter()
                .map(|(_, person)| person.clone()),
        );
    });
    *travelers = remaining;
}
#[cfg(test)]
#[path = "../tests/support/routing.rs"]
mod routing_tests;
