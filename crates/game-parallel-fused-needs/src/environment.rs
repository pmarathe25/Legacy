//! Exposure and identity-stable shelter allocation after basic needs.
use game_parallel_environment::{Environment, Hazard};
use game_parallel_fused_data::Block;
use game_parallel_people::Region;
use game_parallel_production_model::Inventory;
use rayon::prelude::*;
use std::collections::HashMap;
pub fn apply(
    blocks: &mut [Block],
    regions: &[Region],
    environment: Option<&Environment>,
    inventories: &[Inventory],
    tick: u64,
) {
    let Some(environment) = environment else {
        return;
    };
    let season = environment.season(tick);
    let hazards: HashMap<_, _> = environment
        .regions
        .iter()
        .map(|hazard| (hazard.region, hazard))
        .collect();
    let by_region: Vec<Option<&Hazard>> = regions
        .iter()
        .map(|region| hazards.get(&region.id).copied())
        .collect();
    let counts: Vec<usize> = blocks
        .par_iter()
        .map(|block| {
            if block.journey.is_some() || by_region[block.region].is_none() {
                0
            } else {
                block
                    .next_health
                    .iter()
                    .filter(|&&health| health > 0)
                    .count()
            }
        })
        .collect();
    let mut totals = vec![0usize; regions.len()];
    let mut prefix = vec![0usize; blocks.len()];
    let mut overlap = vec![false; regions.len()];
    let mut last = vec![None; regions.len()];
    for (index, block) in blocks.iter().enumerate() {
        let region = block.region;
        prefix[index] = totals[region];
        totals[region] += counts[index];
        if block.journey.is_none() && counts[index] > 0 {
            if last[region].is_some_and(|id| id > block.ids[0]) {
                overlap[region] = true;
            }
            last[region] = Some(last[region].map_or(*block.ids.last().unwrap(), |id: u64| {
                id.max(*block.ids.last().unwrap())
            }));
        }
    }
    let capacity: HashMap<_, u64> = inventories
        .iter()
        .flat_map(|inventory| {
            inventory
                .stocks
                .iter()
                .map(move |stock| ((inventory.region, stock.good), stock.quantity))
        })
        .collect();
    let limit: Vec<_> = totals
        .iter()
        .enumerate()
        .map(|(region, &total)| {
            by_region[region].map_or(0, |hazard| {
                let goods = hazard
                    .shelter_good
                    .and_then(|good| capacity.get(&(hazard.region, good)).copied())
                    .unwrap_or(0);
                total.min(
                    usize::try_from(u64::from(hazard.shelter).saturating_add(goods))
                        .unwrap_or(usize::MAX),
                )
            })
        })
        .collect();
    let mut cutoffs = vec![None; regions.len()];
    if overlap
        .iter()
        .enumerate()
        .any(|(region, &mixed)| mixed && limit[region] > 0 && limit[region] < totals[region])
    {
        let mut candidates: Vec<_> = blocks
            .par_iter()
            .flat_map_iter(|block| {
                let region = block.region;
                let active = block.journey.is_none()
                    && overlap[region]
                    && limit[region] > 0
                    && limit[region] < totals[region];
                block
                    .ids
                    .iter()
                    .zip(&block.next_health)
                    .filter_map(move |(&id, &health)| {
                        (active && health > 0).then_some((region, id))
                    })
            })
            .collect();
        candidates.par_sort_unstable();
        for (region, cutoff) in cutoffs.iter_mut().enumerate() {
            if overlap[region] && limit[region] > 0 && limit[region] < totals[region] {
                let start = candidates.partition_point(|row| row.0 < region);
                *cutoff = Some(candidates[start + limit[region] - 1].1);
            }
        }
    }
    blocks
        .par_iter_mut()
        .enumerate()
        .for_each(|(block_index, block)| {
            let Some(hazard) = by_region[block.region] else {
                return;
            };
            let mut rank = prefix[block_index];
            for i in 0..block.ids.len() {
                if block.next_health[i] == 0 {
                    continue;
                }
                let sheltered = block.journey.is_none()
                    && cutoffs[block.region]
                        .map_or(rank < limit[block.region], |cutoff| block.ids[i] <= cutoff);
                if block.journey.is_none() {
                    rank += 1;
                }
                let damage = hazard.damage.saturating_sub(if sheltered {
                    hazard.shelter_reduction
                } else {
                    0
                });
                let damage = season.map_or(damage, |season| season.exposure(damage));
                block.next_health[i] = block.next_health[i].saturating_sub(damage);
            }
        });
}
