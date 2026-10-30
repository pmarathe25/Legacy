//! Deterministic regional food arbitration with optional rotating individual priority.
use game_parallel_fused_data::Block;
use game_parallel_people::Rules;
use rayon::prelude::*;
pub struct Allocation {
    pub meals: Vec<usize>,
    pub cutoffs: Vec<Option<Cutoff>>,
}
#[derive(Clone, Copy)]
pub struct Cutoff {
    pivot: u64,
    end: u64,
    wraps: bool,
}
impl Cutoff {
    pub fn allows(self, id: u64) -> bool {
        if self.wraps {
            id >= self.pivot || id <= self.end
        } else {
            id >= self.pivot && id <= self.end
        }
    }
}
pub fn allocate(
    blocks: &mut [Block],
    grown: &[u32],
    rules: &Rules,
    rotation: Option<(u64, u64)>,
) -> Allocation {
    let mut claims = vec![0usize; grown.len()];
    let mut overlap = vec![false; grown.len()];
    let mut last = vec![None; grown.len()];
    for block in blocks.iter_mut() {
        block.prefix = claims[block.region];
        claims[block.region] += block.claims;
        if block.journey.is_none() {
            if last[block.region].is_some_and(|id| id > block.ids[0]) {
                overlap[block.region] = true;
            }
            last[block.region] = Some(
                last[block.region].map_or(*block.ids.last().unwrap(), |id: u64| {
                    id.max(*block.ids.last().unwrap())
                }),
            );
        }
    }
    let meals: Vec<_> = claims
        .par_iter()
        .zip(grown.par_iter())
        .map(|(&claims, &food)| claims.min((food / rules.food_per_meal) as usize))
        .collect();
    let mut cutoffs = vec![None; grown.len()];
    if (rotation.is_some() || overlap.iter().any(|&mixed| mixed))
        && meals
            .par_iter()
            .zip(claims.par_iter())
            .any(|(&meals, &claims)| meals > 0 && meals < claims)
    {
        // Each region selects a rank independently, using bounded parallel radix passes.
        let selected: Vec<_> = blocks
            .par_chunk_by(|a, b| a.region == b.region)
            .filter_map(|group| {
                let region = group[0].region;
                let amount = meals[region];
                if amount == 0
                    || amount >= claims[region]
                    || (rotation.is_none() && !overlap[region])
                {
                    return None;
                }
                if let Some((seed, tick)) = rotation {
                    // Choose a claimant by rank so sparse IDs receive equal starting chances.
                    let pivot_rank = (game_parallel_genetics::random(seed, region as u64, tick)
                        % claims[region] as u64) as usize;
                    let pivot = game_parallel_food_select::cutoff(group, pivot_rank);
                    let upper = claims[region] - pivot_rank;
                    let wraps = upper < amount;
                    let end_rank = if wraps {
                        amount - upper - 1
                    } else {
                        pivot_rank + amount - 1
                    };
                    let end = game_parallel_food_select::cutoff(group, end_rank);
                    Some((region, Cutoff { pivot, end, wraps }))
                } else {
                    let end = game_parallel_food_select::cutoff(group, amount - 1);
                    Some((
                        region,
                        Cutoff {
                            pivot: 0,
                            end,
                            wraps: false,
                        },
                    ))
                }
            })
            .collect();
        for (region, cutoff) in selected {
            cutoffs[region] = Some(cutoff);
        }
    }
    Allocation { meals, cutoffs }
}

/// Persons whose physical acquisition attempt was granted from the local source.
pub fn acquired(blocks: &[Block], allocation: &Allocation) -> Vec<u64> {
    let mut ids: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            let mut rank = block.prefix;
            block
                .ids
                .iter()
                .enumerate()
                .filter_map(move |(index, &id)| {
                    if !block.food_claim[index] {
                        return None;
                    }
                    let granted = allocation.cutoffs[block.region]
                        .map_or(rank < allocation.meals[block.region], |cutoff| {
                            cutoff.allows(id)
                        });
                    rank += 1;
                    (granted && !block.care_food_claim[index]).then_some(id)
                })
        })
        .collect();
    if ids.par_windows(2).any(|p| p[0] > p[1]) {
        ids.par_sort_unstable();
    }
    ids
}
