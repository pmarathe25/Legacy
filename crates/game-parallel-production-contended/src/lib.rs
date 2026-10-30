//! Parallel per-block prefix arbitration for contested workshop outputs.
use game_parallel_packed_jobs_model::Block;
use game_parallel_production_output::Progress;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
type Stock = (u64, u32);
type Totals = HashMap<Stock, u64>;
struct Summary {
    key: Stock,
    block: usize,
    amount: u128,
    prefix: u128,
}
pub fn resolve(
    blocks: &[Block],
    progress: &[Vec<Progress>],
    capacity: &HashMap<Stock, u64>,
    totals: &HashMap<Stock, u128>,
) -> (Vec<Vec<bool>>, Totals) {
    let contested: HashSet<_> = totals
        .iter()
        .filter(|(key, amount)| **amount > u128::from(capacity[key]))
        .map(|(&key, _)| key)
        .collect();
    let uniform: Vec<_> = blocks
        .par_iter()
        .map(|block| {
            block.rows.first().is_some_and(|first| {
                block
                    .rows
                    .iter()
                    .all(|row| row.region == first.region && row.bundle == first.bundle)
            })
        })
        .collect();
    let mut summaries: Vec<Summary> = blocks
        .par_iter()
        .zip(progress.par_iter())
        .enumerate()
        .flat_map_iter(|(block_index, (block, states))| {
            let mut amounts: HashMap<Stock, u128> = HashMap::new();
            if uniform[block_index] {
                let row = &block.rows[0];
                let count = states.iter().filter(|state| state.waiting).count() as u128;
                for item in &block.bundles[row.bundle as usize].outputs {
                    let key = (row.region, item.good);
                    if contested.contains(&key) {
                        amounts.insert(key, u128::from(item.quantity) * count);
                    }
                }
            } else {
                for (row, state) in block.rows.iter().zip(states) {
                    if state.waiting {
                        for item in &block.bundles[row.bundle as usize].outputs {
                            let key = (row.region, item.good);
                            if contested.contains(&key) {
                                *amounts.entry(key).or_default() += u128::from(item.quantity);
                            }
                        }
                    }
                }
            }
            amounts.into_iter().map(move |(key, amount)| Summary {
                key,
                block: block_index,
                amount,
                prefix: 0,
            })
        })
        .collect();
    summaries.par_sort_unstable_by_key(|row| (row.key, row.block));
    summaries
        .par_chunk_by_mut(|a, b| a.key == b.key)
        .for_each(|group| {
            let mut total = 0u128;
            for row in group {
                row.prefix = total;
                total += row.amount;
            }
        });
    summaries.par_sort_unstable_by_key(|row| (row.block, row.key));
    let bases: Vec<HashMap<Stock, u128>> = (0..blocks.len())
        .into_par_iter()
        .map(|block| {
            let start = summaries.partition_point(|row| row.block < block);
            let end = summaries.partition_point(|row| row.block <= block);
            summaries[start..end]
                .iter()
                .map(|row| (row.key, row.prefix))
                .collect()
        })
        .collect();
    let denied: Vec<Vec<bool>> = blocks
        .par_iter()
        .zip(progress.par_iter())
        .zip(bases.into_par_iter())
        .enumerate()
        .map(|(block_index, ((block, states), mut prefixes))| {
            if uniform[block_index] {
                let row = &block.rows[0];
                let mut limits: Vec<_> = block.bundles[row.bundle as usize]
                    .outputs
                    .iter()
                    .filter_map(|item| {
                        let key = (row.region, item.good);
                        prefixes.get(&key).map(|prefix| {
                            (
                                *prefix,
                                u128::from(item.quantity),
                                u128::from(capacity[&key]),
                            )
                        })
                    })
                    .collect();
                return states
                    .iter()
                    .map(|state| {
                        if !state.waiting {
                            return false;
                        }
                        let mut denied = false;
                        for (prefix, quantity, capacity) in &mut limits {
                            *prefix += *quantity;
                            denied |= *prefix > *capacity;
                        }
                        denied
                    })
                    .collect();
            }
            block
                .rows
                .iter()
                .zip(states)
                .map(|(row, state)| {
                    let mut denied = false;
                    if state.waiting {
                        for item in &block.bundles[row.bundle as usize].outputs {
                            let key = (row.region, item.good);
                            if let Some(prefix) = prefixes.get_mut(&key) {
                                *prefix += u128::from(item.quantity);
                                denied |= *prefix > u128::from(capacity[&key]);
                            }
                        }
                    }
                    denied
                })
                .collect()
        })
        .collect();
    let updates: Totals = blocks
        .par_iter()
        .zip(progress.par_iter())
        .zip(denied.par_iter())
        .enumerate()
        .map(|(block_index, ((block, states), denied))| {
            let mut result = HashMap::new();
            if uniform[block_index] {
                let row = &block.rows[0];
                let count = states
                    .iter()
                    .zip(denied)
                    .filter(|(state, denied)| state.waiting && !**denied)
                    .count() as u64;
                for item in &block.bundles[row.bundle as usize].outputs {
                    result.insert((row.region, item.good), item.quantity * count);
                }
                return result;
            }
            for ((row, state), denied) in block.rows.iter().zip(states).zip(denied) {
                if state.waiting && !denied {
                    for item in &block.bundles[row.bundle as usize].outputs {
                        *result.entry((row.region, item.good)).or_insert(0u64) += item.quantity;
                    }
                }
            }
            result
        })
        .reduce(HashMap::new, |mut left, right| {
            for (key, value) in right {
                *left.entry(key).or_insert(0) += value;
            }
            left
        });
    (denied, updates)
}
