//! Stable output reservations using job-ordered segment summaries.
use game_parallel_fused_metadata::Store;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
type Key = (u64, u32);
struct Summary {
    key: Key,
    block: usize,
    amount: u128,
    prefix: u128,
}
pub fn resolve(
    jobs: &Store,
    flags: &[AtomicU8],
    capacity: &HashMap<Key, u64>,
    totals: &HashMap<Key, u128>,
) -> HashMap<Key, u64> {
    let contested: HashSet<_> = totals
        .iter()
        .filter(|(key, amount)| **amount > u128::from(capacity[key]))
        .map(|(&key, _)| key)
        .collect();
    let segments = jobs.chunks();
    let uniform: Vec<_> = segments
        .par_iter()
        .map(|rows| {
            rows.first().is_some_and(|first| {
                rows.iter()
                    .all(|row| row.region == first.region && row.bundle == first.bundle)
            })
        })
        .collect();
    let mut summaries: Vec<Summary> = segments
        .par_iter()
        .zip(flags.par_chunks(4096))
        .enumerate()
        .flat_map_iter(|(block, (rows, flags))| {
            let mut amounts = HashMap::<Key, u128>::new();
            if uniform[block] {
                let row = &rows[0];
                let count = flags
                    .iter()
                    .filter(|flag| flag.load(Ordering::Relaxed) != 0)
                    .count() as u128;
                for item in &jobs.bundles[row.bundle as usize].outputs {
                    let key = (row.region, item.good);
                    if contested.contains(&key) {
                        amounts.insert(key, u128::from(item.quantity) * count);
                    }
                }
            } else {
                for (row, flag) in rows.iter().zip(flags) {
                    if flag.load(Ordering::Relaxed) == 0 {
                        continue;
                    }
                    for item in &jobs.bundles[row.bundle as usize].outputs {
                        let key = (row.region, item.good);
                        if contested.contains(&key) {
                            *amounts.entry(key).or_default() += u128::from(item.quantity);
                        }
                    }
                }
            }
            amounts.into_iter().map(move |(key, amount)| Summary {
                key,
                block,
                amount,
                prefix: 0,
            })
        })
        .collect();
    summaries.par_sort_unstable_by_key(|row| (row.key, row.block));
    summaries
        .par_chunk_by_mut(|a, b| a.key == b.key)
        .for_each(|group| {
            let mut prefix = 0u128;
            for row in group {
                row.prefix = prefix;
                prefix += row.amount;
            }
        });
    summaries.par_sort_unstable_by_key(|row| (row.block, row.key));
    segments
        .par_iter()
        .zip(flags.par_chunks(4096))
        .enumerate()
        .map(|(block, (rows, flags))| {
            let start = summaries.partition_point(|row| row.block < block);
            let end = summaries.partition_point(|row| row.block <= block);
            let mut prefixes: HashMap<_, _> = summaries[start..end]
                .iter()
                .map(|row| (row.key, row.prefix))
                .collect();
            let mut updates = HashMap::<Key, u64>::new();
            if uniform[block] {
                let row = &rows[0];
                let outputs = &jobs.bundles[row.bundle as usize].outputs;
                let limit = outputs
                    .iter()
                    .filter_map(|item| {
                        let key = (row.region, item.good);
                        prefixes.get(&key).map(|prefix| {
                            u128::from(capacity[&key])
                                .saturating_sub(*prefix)
                                .checked_div(u128::from(item.quantity))
                                .unwrap_or(u128::MAX)
                        })
                    })
                    .min()
                    .unwrap_or(u128::MAX);
                let mut rank = 0u128;
                for flag in flags {
                    let state = flag.load(Ordering::Relaxed);
                    if state == 0 {
                        continue;
                    }
                    let accepted = rank < limit;
                    rank += 1;
                    flag.store((state & 1) | (u8::from(accepted) << 1), Ordering::Relaxed);
                }
                let accepted = rank.min(limit) as u64;
                for item in outputs {
                    updates.insert((row.region, item.good), item.quantity * accepted);
                }
                return updates;
            }
            for (row, flag) in rows.iter().zip(flags) {
                let state = flag.load(Ordering::Relaxed);
                if state == 0 {
                    continue;
                }
                let outputs = &jobs.bundles[row.bundle as usize].outputs;
                let mut denied = false;
                for item in outputs {
                    let key = (row.region, item.good);
                    if let Some(prefix) = prefixes.get_mut(&key) {
                        *prefix += u128::from(item.quantity);
                        denied |= *prefix > u128::from(capacity[&key]);
                    }
                }
                if !denied {
                    for item in outputs {
                        *updates.entry((row.region, item.good)).or_default() += item.quantity;
                    }
                }
                flag.store((state & 1) | (u8::from(!denied) << 1), Ordering::Relaxed);
            }
            updates
        })
        .reduce(HashMap::new, |mut a, b| {
            for (key, value) in b {
                *a.entry(key).or_default() += value;
            }
            a
        })
}
