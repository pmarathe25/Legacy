//! Stage production output claims without publishing stock or receipts.
use game_parallel::Production;
use game_parallel_fused_data::Block;
use game_parallel_fused_metadata::Store;
use game_parallel_production_receipts::Receipts;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
pub struct Draft {
    pub updates: HashMap<(u64, u32), u64>,
    pub waiting: Vec<usize>,
    pub receipts: Receipts,
    pub delivered: usize,
}
pub fn prepare(
    blocks: &[Block],
    jobs: &Store,
    waiting: &[usize],
    production: Option<&Production>,
    next: u64,
) -> Result<Draft, &'static str> {
    let Some(production) = production else {
        return Ok(Draft {
            updates: HashMap::new(),
            waiting: Vec::new(),
            receipts: Receipts::default(),
            delivered: 0,
        });
    };
    let bundles = &jobs.bundles;
    let finished: Vec<usize> = blocks
        .par_iter()
        .flat_map_iter(|block| block.finished.iter().copied())
        .collect();
    if finished.is_empty() && waiting.is_empty() {
        return Ok(Draft {
            updates: HashMap::new(),
            waiting: Vec::new(),
            receipts: Receipts::default(),
            delivered: 0,
        });
    }
    let mut totals: HashMap<(u64, u32), u128> = blocks
        .par_iter()
        .map(|block| {
            let mut totals = HashMap::new();
            if let Some((region, bundle)) = block.uniform_output {
                let count = block.finished.len() as u128;
                for output in &bundles[bundle as usize].outputs {
                    totals.insert((region, output.good), u128::from(output.quantity) * count);
                }
            } else {
                for &handle in &block.finished {
                    let job = jobs.row(handle);
                    for output in &bundles[job.bundle as usize].outputs {
                        *totals.entry((job.region, output.good)).or_insert(0) +=
                            u128::from(output.quantity);
                    }
                }
            }
            totals
        })
        .reduce(HashMap::new, |mut left, right| {
            for (key, value) in right {
                *left.entry(key).or_insert(0) += value;
            }
            left
        });
    let waiting_totals: HashMap<(u64, u32), u128> = waiting
        .par_iter()
        .fold(HashMap::new, |mut totals, &handle| {
            let job = jobs.row(handle);
            for output in &bundles[job.bundle as usize].outputs {
                *totals.entry((job.region, output.good)).or_insert(0) +=
                    u128::from(output.quantity);
            }
            totals
        })
        .reduce(HashMap::new, |mut left, right| {
            for (key, value) in right {
                *left.entry(key).or_insert(0) += value;
            }
            left
        });
    for (key, value) in waiting_totals {
        *totals.entry(key).or_insert(0) += value;
    }
    // Jobs can touch a handful of goods even when the content catalog is large.
    let inventories: HashMap<_, _> = production
        .inventories
        .par_iter()
        .map(|inventory| (inventory.region, inventory))
        .collect();
    let capacity: HashMap<_, _> = totals
        .par_iter()
        .map(|(&key, _)| {
            let stock = inventories[&key.0]
                .stocks
                .iter()
                .find(|stock| stock.good == key.1)
                .expect("validated output stock");
            (key, stock.capacity - stock.quantity)
        })
        .collect();
    let flags: Vec<_> = (0..jobs.len())
        .into_par_iter()
        .map(|_| AtomicU8::new(0))
        .collect();
    finished
        .par_iter()
        .for_each(|&handle| flags[handle].store(1, Ordering::Relaxed));
    waiting
        .par_iter()
        .for_each(|&handle| flags[handle].store(4, Ordering::Relaxed));
    let updates = if totals
        .iter()
        .all(|(key, amount)| *amount <= u128::from(capacity[key]))
    {
        flags.par_iter().for_each(|flag| {
            let state = flag.load(Ordering::Relaxed);
            if state != 0 {
                flag.store((state & 1) | 2, Ordering::Relaxed);
            }
        });
        totals
            .into_iter()
            .map(|(key, value)| (key, value as u64))
            .collect()
    } else {
        game_parallel_fused_contended::resolve(jobs, &flags, &capacity, &totals)
    };
    let receipts = if finished.is_empty() && waiting.is_empty() {
        Receipts::default()
    } else {
        game_parallel_production_receipts::prepare_flags(jobs.chunks(), &flags, next)
    };
    let accepted = |handle: usize| flags[handle].load(Ordering::Relaxed) & 2 != 0;
    let mut next_waiting: Vec<_> = waiting
        .par_iter()
        .chain(finished.par_iter())
        .copied()
        .filter(|&handle| !accepted(handle))
        .collect();
    if next_waiting
        .par_windows(2)
        .any(|pair| jobs.row(pair[0]).id > jobs.row(pair[1]).id)
    {
        next_waiting.par_sort_unstable_by_key(|&handle| jobs.row(handle).id);
    }
    let delivered = finished
        .len()
        .checked_add(waiting.len())
        .and_then(|total| total.checked_sub(next_waiting.len()))
        .ok_or("invalid live job count")?;
    Ok(Draft {
        updates,
        waiting: next_waiting,
        receipts,
        delivered,
    })
}

/// Apply prevalidated output increments after all owners prepare successfully.
pub fn commit(production: Option<&mut Production>, updates: &HashMap<(u64, u32), u64>) {
    if updates.is_empty() {
        return;
    }
    if let Some(production) = production {
        let mut changes: Vec<_> = updates
            .par_iter()
            .map(|(&key, &quantity)| (key, quantity))
            .collect();
        changes.par_sort_unstable_by_key(|row| row.0);
        production.inventories.par_iter_mut().for_each(|inventory| {
            let start = changes.partition_point(|row| row.0.0 < inventory.region);
            let end = changes.partition_point(|row| row.0.0 <= inventory.region);
            let active = &changes[start..end];
            if active.is_empty() {
                return;
            }
            if active.len() < inventory.stocks.len() / 8 {
                for &((_, good), quantity) in active {
                    inventory
                        .stocks
                        .iter_mut()
                        .find(|stock| stock.good == good)
                        .expect("validated output stock")
                        .quantity += quantity;
                }
            } else {
                for stock in &mut inventory.stocks {
                    stock.quantity += updates
                        .get(&(inventory.region, stock.good))
                        .copied()
                        .unwrap_or(0);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel::{Inventory, Stock};

    #[test]
    fn publishing_sparse_output_changes_only_targeted_physical_stocks() {
        let mut production = Production {
            max_work_hunger: 10,
            goods: vec![],
            recipes: vec![],
            recipe_categories: vec![],
            jobs: vec![],
            events: vec![],
            inventories: (1..=3)
                .map(|region| Inventory {
                    region,
                    stocks: (1..=128)
                        .map(|good| Stock {
                            good,
                            quantity: 3,
                            capacity: 100,
                        })
                        .collect(),
                })
                .collect(),
        };
        let before = production.clone();
        commit(Some(&mut production), &HashMap::new());
        assert_eq!(production, before);
        let updates = HashMap::from([((1, 1), 2), ((1, 128), 5), ((3, 64), 7)]);
        commit(Some(&mut production), &updates);
        for inventory in &production.inventories {
            for stock in &inventory.stocks {
                assert_eq!(
                    stock.quantity,
                    3 + updates
                        .get(&(inventory.region, stock.good))
                        .copied()
                        .unwrap_or(0)
                );
            }
        }
    }
}
