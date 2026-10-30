//! Snapshot-based provision allocation for a batch of departures.
use game_parallel::{Holding, Move, Region, Route, Rules};
use game_parallel_cargo::Cargo;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::HashMap;
const CHUNK: usize = 2048;
struct Demand {
    region: usize,
    person: u64,
    amount: u32,
    granted: u32,
}
pub struct PackDraft {
    pub cargo: Cargo,
    pub food: Vec<u32>,
}
pub fn prepare_pack(
    cargo: &Cargo,
    regions: &[Region],
    food: &[u32],
    people: &[PersonSnapshot],
    routes: &[Route],
    rules: &Rules,
    moves: &[Move],
) -> Result<Option<PackDraft>, String> {
    if !cargo.auto_pack_travel {
        return Ok(None);
    }
    cargo.validate_index(people)?;
    rules.validate()?;
    if regions.len() != food.len() {
        return Err("invalid food index".into());
    }
    let mut ids: Vec<_> = moves.par_iter().map(|m| m.person).collect();
    ids.par_sort_unstable();
    if ids.par_windows(2).any(|p| p[0] == p[1]) {
        return Err("duplicate departure person".into());
    }
    let areas: HashMap<_, _> = regions.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    let routes: HashMap<_, _> = routes
        .iter()
        .map(|r| ((r.source, r.destination), r.ticks))
        .collect();
    let held: HashMap<_, _> = cargo.holdings.iter().map(|h| (h.person, h.food)).collect();
    let demands: Vec<Option<Demand>> = moves
        .par_iter()
        .map(|request| -> Result<_, String> {
            let index = person_index(people, request.person).ok_or("unknown person")?;
            let (_, region, health, hunger) = people[index];
            let source = region.ok_or("person must be local")?;
            if health == 0 {
                return Err("deceased person cannot travel".into());
            }
            let ticks = *routes
                .get(&(source, request.destination))
                .ok_or("no route")?;
            let region = *areas.get(&source).ok_or("unknown region")?;
            let projected = u64::from(hunger) + u64::from(ticks) * u64::from(rules.hunger_per_tick);
            let meals = projected.div_ceil(u64::from(rules.hunger_satisfied));
            let target = (u128::from(meals) * u128::from(rules.food_per_meal))
                .min(u128::from(cargo.capacity)) as u32;
            let amount = target.saturating_sub(held.get(&request.person).copied().unwrap_or(0));
            Ok((amount > 0).then_some(Demand {
                region,
                person: request.person,
                amount,
                granted: 0,
            }))
        })
        .collect::<Result<_, _>>()?;
    let mut demands: Vec<_> = demands.into_par_iter().flatten().collect();
    demands.par_sort_unstable_by_key(|d| (d.region, d.person));
    let totals: Vec<_> = demands
        .par_chunk_by_mut(|a, b| a.region == b.region)
        .map(|group| {
            let region = group[0].region;
            let available = u64::from(food[region]);
            let counts: Vec<u64> = group
                .par_chunks(CHUNK)
                .map(|chunk| chunk.iter().map(|d| u64::from(d.amount)).sum())
                .collect();
            let mut total = 0u64;
            let offsets: Vec<_> = counts
                .into_iter()
                .map(|count| {
                    let start = total;
                    total = total.saturating_add(count).min(available);
                    start
                })
                .collect();
            group
                .par_chunks_mut(CHUNK)
                .zip(offsets.into_par_iter())
                .for_each(|(chunk, mut used)| {
                    for demand in chunk {
                        let granted = u64::from(demand.amount).min(available.saturating_sub(used));
                        demand.granted = granted as u32;
                        used += granted;
                    }
                });
            (region, total as u32)
        })
        .collect();
    let additions: Vec<_> = demands
        .into_par_iter()
        .filter(|d| d.granted > 0)
        .map(|d| Holding {
            person: d.person,
            food: d.granted,
        })
        .collect();
    let mut next = cargo.clone();
    game_parallel_packing::commit(&mut next, &[additions]);
    let mut next_food = food.to_vec();
    for (region, amount) in totals {
        next_food[region] -= amount;
    }
    Ok(Some(PackDraft {
        cargo: next,
        food: next_food,
    }))
}
