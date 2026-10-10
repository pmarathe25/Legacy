//! Synthetic access patterns shared by the storage experiment and retained kernel
//! benchmark. These choices and lifecycle operations are not campaign rules.
use crate::{compute, transactions};
use game_api::{InventoryId, PersonId};
use rayon::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    pub units: u64,
    pub remainder: u64,
    pub score: u64,
    pub area: u64,
    pub relations: [u64; 2],
}

pub fn rows(population: usize) -> Vec<Row> {
    (1..=population as u64)
        .map(|id| Row {
            id,
            units: 0,
            remainder: 0,
            score: 0,
            area: id % 16,
            relations: [id % population as u64 + 1, (id + 7) % population as u64 + 1],
        })
        .collect()
}

/// Seeded independent branch scoring and two sparse lookups, followed by the
/// production CPU progress kernel. All candidates use these exact operations.
pub fn evaluate(pool: &rayon::ThreadPool, input: &[Row], tick: u64, reverse: bool) -> Vec<Row> {
    let work = |p: &Row| {
        let related = p
            .relations
            .iter()
            .filter_map(|id| input.binary_search_by_key(id, |r| r.id).ok())
            .map(|i| input[i].score)
            .fold(0, u64::wrapping_add);
        let mut best = 0;
        for option in 0..64 {
            let mut x = p.id ^ tick.rotate_left(11) ^ option ^ related;
            for _ in 0..32 {
                x = x.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(13);
                if x & 7 == 0 {
                    x ^= related;
                } else {
                    x = x.wrapping_add(option);
                }
            }
            best = best.max(x % 1_000_000);
        }
        let out = compute::progress(compute::ProgressInput {
            units: p.units,
            remainder: p.remainder,
            units_per_minute: 1_000 + best % 17,
            elapsed_ms: 60_000,
        })
        .unwrap();
        Row {
            units: out.units,
            remainder: out.remainder,
            score: best,
            ..p.clone()
        }
    };
    pool.install(|| {
        if pool.current_num_threads() == 1 {
            input.iter().map(work).collect()
        } else if reverse {
            let mut result: Vec<_> = input.par_iter().with_min_len(256).rev().map(work).collect();
            result.reverse();
            result
        } else {
            input.par_iter().with_min_len(256).map(work).collect()
        }
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ledger {
    pub balances: BTreeMap<InventoryId, u64>,
    pub cursors: BTreeMap<InventoryId, PersonId>,
    pub grants: Vec<(u64, u64, u64)>,
    pub next_id: u64,
}
impl Ledger {
    pub fn new(population: usize) -> Self {
        Self {
            balances: (0..16).map(|id| (InventoryId(id), 1_000_000)).collect(),
            cursors: BTreeMap::new(),
            grants: Vec::new(),
            next_id: population as u64 + 1,
        }
    }
    pub fn resolve(&mut self, rows: &[Row], tick: u64, contested: bool) {
        let balances = self
            .balances
            .iter()
            .map(|(id, stock)| {
                (
                    *id,
                    transactions::Balance {
                        stock: *stock,
                        capacity: u64::MAX,
                        last_granted: self.cursors.get(id).copied(),
                    },
                )
            })
            .collect();
        let requests = rows
            .iter()
            .filter(|r| r.score % 4 == 0)
            .map(|r| transactions::Request {
                actor: PersonId(r.id),
                source: InventoryId(if contested { 0 } else { r.area }),
                destination: InventoryId((r.area + 1) % 16),
                amount: 1_000,
                eligible: true,
            })
            .collect();
        for (r, granted) in transactions::arbitrate(&balances, requests) {
            if granted {
                *self.balances.get_mut(&r.source).unwrap() -= r.amount;
                *self.balances.get_mut(&r.destination).unwrap() += r.amount;
                self.cursors.insert(r.source, r.actor);
                self.grants.push((tick, r.actor.0, r.destination.0));
            }
        }
        assert_eq!(self.balances.values().sum::<u64>(), 16_000_000);
    }
    pub fn replacement(&mut self, removed: &Row) -> Row {
        let id = self.next_id;
        self.next_id += 1;
        Row {
            id,
            units: 0,
            remainder: 0,
            score: 0,
            area: removed.area,
            relations: removed.relations,
        }
    }
}

pub fn custom_run(
    population: usize,
    pool: &rayon::ThreadPool,
    contested: bool,
    reverse: bool,
    ticks: u64,
) -> (Vec<Row>, Ledger) {
    let mut rows = rows(population);
    let mut ledger = Ledger::new(population);
    for tick in 0..ticks {
        rows = evaluate(pool, &rows, tick, reverse);
        ledger.resolve(&rows, tick, contested);
        let removed = rows.remove(0);
        rows.push(ledger.replacement(&removed));
        std::hint::black_box(
            rows.iter()
                .map(|r| (r.id, r.units, r.area))
                .collect::<Vec<_>>(),
        );
    }
    (rows, ledger)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branching_sparse_relations_arbitration_and_lifecycle_match_across_workers() {
        for contested in [false, true] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build()
                .unwrap();
            let expected = custom_run(1_000, &pool, contested, false, 3);
            for workers in [2, 4] {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap();
                assert_eq!(expected, custom_run(1_000, &pool, contested, true, 3));
            }
        }
    }
}
