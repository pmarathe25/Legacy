//! Persistent, compact job blocks with interned exact material bundles.
mod admission;
pub use admission::{StartDraft, apply_needs};
use game_parallel_packed_jobs_model::{Block, Bundle, Row};
use game_parallel_production_contended as contended;
use game_parallel_production_model::{
    Amount, Event, Inventory, Job, JobPhase, PersonSnapshot, person_index,
};
use game_parallel_production_output::Progress;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::HashMap;
const BLOCK: usize = 4096;
type Key<'a> = (&'a [Amount], &'a [Amount]);
pub struct PackedJobs {
    blocks: Vec<Block>,
    receipts: game_parallel_production_receipts::Receipts,
}
pub struct Draft {
    progress: Vec<Vec<Progress>>,
    denied: Option<Vec<Vec<bool>>>,
    updates: HashMap<(u64, u32), u64>,
    receipts: game_parallel_production_receipts::Receipts,
}
impl PackedJobs {
    pub fn from_jobs(jobs: &[Job]) -> Self {
        let mut ordered: Vec<_> = jobs.par_iter().collect();
        if ordered.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            ordered.par_sort_unstable_by_key(|job| job.id);
        }
        let blocks = ordered
            .par_chunks(BLOCK)
            .map(|chunk| {
                let mut index: HashMap<Key<'_>, u32> = HashMap::new();
                let mut bundles: Vec<Bundle> = Vec::new();
                let mut rows = Vec::with_capacity(chunk.len());
                for job in chunk {
                    let key = (job.reserved.as_slice(), job.outputs.as_slice());
                    let bundle = *index.entry(key).or_insert_with(|| {
                        let id = bundles.len() as u32;
                        bundles.push(Bundle {
                            reserved: job.reserved.clone(),
                            outputs: job.outputs.clone(),
                        });
                        id
                    });
                    rows.push(Row {
                        id: job.id,
                        worker: job.worker,
                        region: job.region,
                        remaining: job.remaining,
                        recipe: job.recipe,
                        bundle,
                        waiting: job.phase == JobPhase::Waiting,
                    });
                }
                Block { rows, bundles }
            })
            .collect();
        Self {
            blocks,
            receipts: game_parallel_production_receipts::Receipts::default(),
        }
    }
    pub fn events(&self) -> Vec<Event> {
        self.receipts.expand()
    }
    pub fn set_events(&mut self, events: Vec<Event>) {
        self.receipts = game_parallel_production_receipts::Receipts::from_events(events);
    }
    pub fn len(&self) -> usize {
        self.blocks.iter().map(|block| block.rows.len()).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|block| block.rows.is_empty())
    }
    pub fn max_job_id(&self) -> Option<u64> {
        self.blocks
            .iter()
            .rev()
            .find_map(|block| block.rows.last().map(|row| row.id))
    }
    pub fn worker_for_job(&self, id: u64) -> Option<u64> {
        self.blocks.iter().find_map(|block| {
            block
                .rows
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|index| block.rows[index].worker)
        })
    }
    pub fn busy_people(&self) -> Vec<u64> {
        self.blocks
            .par_iter()
            .flat_map_iter(|block| {
                block
                    .rows
                    .iter()
                    .filter(|row| !row.waiting)
                    .map(|row| row.worker)
            })
            .collect()
    }
    pub fn jobs(&self) -> Vec<Job> {
        self.blocks
            .par_iter()
            .flat_map_iter(|block| {
                block.rows.iter().map(|row| {
                    let bundle = &block.bundles[row.bundle as usize];
                    Job {
                        id: row.id,
                        worker: row.worker,
                        region: row.region,
                        recipe: row.recipe,
                        remaining: row.remaining,
                        phase: if row.waiting {
                            JobPhase::Waiting
                        } else {
                            JobPhase::Working
                        },
                        reserved: if row.waiting {
                            SmallVec::new()
                        } else {
                            bundle.reserved.clone()
                        },
                        outputs: bundle.outputs.clone(),
                    }
                })
            })
            .collect()
    }
    pub fn prepare(
        &self,
        people: &[PersonSnapshot],
        inventories: &[Inventory],
        max_work_hunger: u32,
        tick: u64,
    ) -> Draft {
        let capacity: HashMap<_, _> = inventories
            .iter()
            .flat_map(|inventory| {
                inventory.stocks.iter().map(move |stock| {
                    (
                        (inventory.region, stock.good),
                        stock.capacity - stock.quantity,
                    )
                })
            })
            .collect();
        let progress: Vec<Vec<Progress>> = self
            .blocks
            .par_iter()
            .map(|block| {
                block
                    .rows
                    .iter()
                    .map(|row| {
                        let active = !row.waiting
                            && person_index(people, row.worker).is_some_and(|i| {
                                let (_, region, health, hunger) = people[i];
                                region == Some(row.region)
                                    && health > 0
                                    && hunger <= max_work_hunger
                            });
                        let remaining = row.remaining - u64::from(active);
                        let finished = active && remaining == 0;
                        Progress {
                            remaining,
                            finished,
                            waiting: row.waiting || finished,
                        }
                    })
                    .collect()
            })
            .collect();
        let totals: HashMap<(u64, u32), u128> = self
            .blocks
            .par_iter()
            .zip(progress.par_iter())
            .map(|(block, progress)| {
                let mut totals = HashMap::new();
                if let Some(first) = block.rows.first()
                    && block
                        .rows
                        .iter()
                        .all(|row| row.region == first.region && row.bundle == first.bundle)
                {
                    let count = progress.iter().filter(|state| state.waiting).count() as u128;
                    for item in &block.bundles[first.bundle as usize].outputs {
                        totals.insert((first.region, item.good), u128::from(item.quantity) * count);
                    }
                    return totals;
                }
                for (row, progress) in block.rows.iter().zip(progress) {
                    if progress.waiting {
                        for item in &block.bundles[row.bundle as usize].outputs {
                            *totals.entry((row.region, item.good)).or_insert(0) +=
                                u128::from(item.quantity);
                        }
                    }
                }
                totals
            })
            .reduce(HashMap::new, |mut left, right| {
                for (key, amount) in right {
                    *left.entry(key).or_insert(0) += amount;
                }
                left
            });
        let (denied, updates) = if totals
            .iter()
            .all(|(key, amount)| *amount <= u128::from(capacity[key]))
        {
            (
                None,
                totals
                    .into_iter()
                    .map(|(key, value)| (key, value as u64))
                    .collect(),
            )
        } else {
            let (denied, updates) = contended::resolve(&self.blocks, &progress, &capacity, &totals);
            (Some(denied), updates)
        };
        let receipts = game_parallel_production_receipts::prepare(
            &self.blocks,
            &progress,
            denied.as_deref(),
            tick,
        );
        Draft {
            progress,
            denied,
            updates,
            receipts,
        }
    }
}
impl Draft {
    pub fn commit(self, jobs: &mut PackedJobs, inventories: &mut [Inventory]) {
        inventories.par_iter_mut().for_each(|inventory| {
            for stock in &mut inventory.stocks {
                stock.quantity += self
                    .updates
                    .get(&(inventory.region, stock.good))
                    .copied()
                    .unwrap_or(0);
            }
        });
        jobs.blocks
            .par_iter_mut()
            .zip(self.progress.par_iter())
            .enumerate()
            .for_each(|(block_index, (block, progress))| {
                let mut index = 0usize;
                block.rows.retain_mut(|row| {
                    let p = &progress[index];
                    let delivered = p.waiting
                        && self
                            .denied
                            .as_ref()
                            .is_none_or(|denied| !denied[block_index][index]);
                    index += 1;
                    row.remaining = p.remaining;
                    if p.finished {
                        row.waiting = true;
                    }
                    !delivered
                });
            });
        // Finished batches release both their rows and interned bundles; long
        // simulations must not retain one empty block per historical cohort.
        jobs.blocks.retain(|block| !block.rows.is_empty());
        jobs.receipts = self.receipts;
    }
}

#[cfg(test)]
#[path = "../tests/support/equivalence.rs"]
mod equivalence;
