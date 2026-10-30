//! Read-only preparation for deterministic, contested travel provisions.
use game_parallel_cargo::{Cargo, Holding};
use game_parallel_people::{Region, Rules};
use game_parallel_transport::Traveler;
use rayon::prelude::*;
use std::collections::HashMap;

pub type Departure = (u64, u64, u64, u32);
type Departures = [Departure];

pub fn prepare(
    regions: &[Region],
    travelers: &[Traveler],
    cargo: Option<&Cargo>,
    rules: &Rules,
    departures: &Departures,
) -> Result<Vec<Vec<Holding>>, String> {
    let Some(cargo) = cargo.filter(|c| c.auto_pack_travel) else {
        return Ok(vec![Vec::new(); regions.len()]);
    };
    rules.validate()?;
    let mut people: Vec<u64> = regions
        .par_iter()
        .flat_map(|r| r.people.par_iter().map(|p| p.id))
        .chain(travelers.par_iter().map(|t| t.person.id))
        .collect();
    people.par_sort_unstable();
    if people.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate person id".into());
    }
    cargo.validate_sorted(&people)?;
    let existing: HashMap<u64, u32> = cargo.holdings.iter().map(|h| (h.person, h.food)).collect();
    Ok(regions
        .par_iter()
        .map(|region| {
            let mut demands: Vec<_> = region
                .people
                .par_iter()
                .filter_map(|person| {
                    let i = departures
                        .binary_search_by_key(&person.id, |row| row.0)
                        .ok()?;
                    let (_, _, _, ticks) = departures[i];
                    let held = existing.get(&person.id).copied().unwrap_or(0);
                    let projected = u64::from(person.hunger)
                        + u64::from(ticks) * u64::from(rules.hunger_per_tick);
                    let meals = projected.div_ceil(u64::from(rules.hunger_satisfied));
                    let target = (u128::from(meals) * u128::from(rules.food_per_meal))
                        .min(u128::from(cargo.capacity)) as u32;
                    let demand = target.saturating_sub(held);
                    (demand > 0).then_some((person.id, demand))
                })
                .collect();
            demands.par_sort_unstable_by_key(|(person, _)| *person);
            let counts: Vec<u64> = demands
                .par_chunks(2048)
                .map(|chunk| chunk.iter().map(|(_, amount)| u64::from(*amount)).sum())
                .collect();
            let mut prefix = 0u64;
            let starts: Vec<_> = counts
                .into_iter()
                .map(|count| {
                    let start = prefix;
                    prefix = prefix.saturating_add(count).min(u64::from(region.food));
                    start
                })
                .collect();
            demands
                .par_chunks(2048)
                .zip(starts.into_par_iter())
                .flat_map_iter(|(chunk, mut used)| {
                    chunk.iter().filter_map(move |&(person, demand)| {
                        let amount = u64::from(demand)
                            .min(u64::from(region.food).saturating_sub(used))
                            as u32;
                        used += u64::from(amount);
                        (amount > 0).then_some(Holding {
                            person,
                            food: amount,
                        })
                    })
                })
                .collect()
        })
        .collect())
}

pub fn commit(cargo: &mut Cargo, allocations: &[Vec<Holding>]) {
    let mut additions: Vec<Holding> = allocations
        .par_iter()
        .flat_map(|group| group.par_iter().cloned())
        .collect();
    if additions.is_empty() {
        return;
    }
    additions.par_sort_unstable_by_key(|h| h.person);
    let mut existing: Vec<u64> = cargo.holdings.par_iter().map(|h| h.person).collect();
    existing.par_sort_unstable();
    cargo.holdings.par_iter_mut().for_each(|h| {
        if let Ok(i) = additions.binary_search_by_key(&h.person, |a| a.person) {
            h.food += additions[i].food;
        }
    });
    cargo.holdings.par_extend(
        additions
            .into_par_iter()
            .filter(|h| existing.binary_search(&h.person).is_err()),
    );
    cargo.holdings.par_sort_unstable_by_key(|h| h.person);
}
