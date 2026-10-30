//! Atomic multi-output storage arbitration for completed jobs.
use game_parallel_production_model::Job;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
const CHUNK: usize = 1024;
type StockUpdates = HashMap<(u64, u32), u64>;
type Arbitration = (Option<Vec<AtomicBool>>, StockUpdates);
pub struct Demand {
    region: u64,
    good: u32,
    job: usize,
    id: u64,
    quantity: u64,
    fits: bool,
}
impl Demand {
    pub fn new(region: u64, good: u32, job: usize, id: u64, quantity: u64) -> Self {
        Self {
            region,
            good,
            job,
            id,
            quantity,
            fits: false,
        }
    }
}
pub struct Progress {
    pub remaining: u64,
    pub finished: bool,
    pub waiting: bool,
}
/// Use block-local reductions when every due output fits. Contested resources
/// retain the conservative multi-output prefix arbitration below.
pub fn arbitrate(
    jobs: &[Job],
    progress: &[Progress],
    capacity: &HashMap<(u64, u32), u64>,
) -> Arbitration {
    let totals: HashMap<(u64, u32), u128> = jobs
        .par_iter()
        .zip(progress.par_iter())
        .filter(|(_, state)| state.waiting)
        .fold(HashMap::new, |mut totals, (job, _)| {
            for item in &job.outputs {
                *totals.entry((job.region, item.good)).or_default() += u128::from(item.quantity);
            }
            totals
        })
        .reduce(HashMap::new, |mut left, right| {
            for (key, amount) in right {
                *left.entry(key).or_default() += amount;
            }
            left
        });
    if totals
        .iter()
        .all(|(key, amount)| *amount <= u128::from(capacity[key]))
    {
        return (
            None,
            totals
                .into_iter()
                .map(|(key, amount)| (key, amount as u64))
                .collect(),
        );
    }
    let demands: Vec<_> = jobs
        .par_iter()
        .enumerate()
        .filter(|(index, _)| progress[*index].waiting)
        .flat_map_iter(|(index, job)| {
            job.outputs
                .iter()
                .map(move |a| Demand::new(job.region, a.good, index, job.id, a.quantity))
        })
        .collect();
    resolve_claims(demands, jobs.len(), capacity)
}
/// Resolve prebuilt claims without materializing serialized job records.
pub fn resolve_claims(
    mut demands: Vec<Demand>,
    job_count: usize,
    capacity: &HashMap<(u64, u32), u64>,
) -> Arbitration {
    if demands.par_windows(2).any(|pair| {
        (pair[0].region, pair[0].good, pair[0].id) > (pair[1].region, pair[1].good, pair[1].id)
    }) {
        demands.par_sort_unstable_by_key(|d| (d.region, d.good, d.id));
    }
    demands
        .par_chunk_by_mut(|a, b| (a.region, a.good) == (b.region, b.good))
        .for_each(|group| {
            let free = u128::from(capacity[&(group[0].region, group[0].good)]);
            let sums: Vec<u128> = group
                .par_chunks(CHUNK)
                .map(|chunk| chunk.iter().map(|d| u128::from(d.quantity)).sum())
                .collect();
            let mut total = 0u128;
            let offsets: Vec<_> = sums
                .into_iter()
                .map(|sum| {
                    let offset = total;
                    total += sum;
                    offset
                })
                .collect();
            group
                .par_chunks_mut(CHUNK)
                .zip(offsets.into_par_iter())
                .for_each(|(chunk, mut prefix)| {
                    for demand in chunk {
                        prefix += u128::from(demand.quantity);
                        demand.fits = prefix <= free;
                    }
                });
        });
    // Every claim in an atomic job must fit. Rejected claims retain prefix priority for this boundary.
    let denied: Vec<_> = (0..job_count).map(|_| AtomicBool::new(false)).collect();
    demands
        .par_iter()
        .filter(|d| !d.fits)
        .for_each(|d| denied[d.job].store(true, Ordering::Relaxed));
    let updates: HashMap<_, u64> = demands
        .par_chunk_by(|a, b| (a.region, a.good) == (b.region, b.good))
        .map(|group| {
            let amount: u64 = group
                .par_iter()
                .filter(|d| !denied[d.job].load(Ordering::Relaxed))
                .map(|d| d.quantity)
                .sum();
            ((group[0].region, group[0].good), amount)
        })
        .collect();
    (Some(denied), updates)
}
