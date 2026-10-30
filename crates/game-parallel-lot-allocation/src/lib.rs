//! Parallel conservative prefixes for a single contested source or destination.
pub mod additions;
mod capacity;
pub use capacity::{Destination, grant_destinations};
use rayon::prelude::*;
pub fn order_actions(actions: &mut [(u32, game_parallel_lot_model::Transaction)]) {
    if actions
        .par_windows(2)
        .any(|p| (p[0].0, p[0].1.id) > (p[1].0, p[1].1.id))
    {
        actions.par_sort_unstable_by_key(|(owner, row)| (*owner, row.id));
    }
}
const CHUNK: usize = 2048;

/// Prefix before each quantity. Large groups do bounded work in parallel.
pub fn prefixes(values: &[u64]) -> Vec<u128> {
    if values.len() <= CHUNK {
        let mut total = 0u128;
        return values
            .iter()
            .map(|&value| {
                let before = total;
                total += u128::from(value);
                before
            })
            .collect();
    }
    let totals: Vec<u128> = values
        .par_chunks(CHUNK)
        .map(|chunk| chunk.iter().map(|&v| u128::from(v)).sum())
        .collect();
    let mut offsets = Vec::with_capacity(totals.len());
    let mut total = 0u128;
    for value in totals {
        offsets.push(total);
        total += value;
    }
    let mut result = vec![0u128; values.len()];
    result
        .par_chunks_mut(CHUNK)
        .zip(values.par_chunks(CHUNK))
        .zip(offsets.par_iter())
        .for_each(|((output, chunk), &offset)| {
            let mut total = offset;
            for (slot, &value) in output.iter_mut().zip(chunk) {
                *slot = total;
                total += u128::from(value);
            }
        });
    result
}

pub fn choose(values: &[u64], available: u64) -> Vec<bool> {
    prefixes(values)
        .into_par_iter()
        .zip(values.par_iter())
        .map(|(before, &amount)| before + u128::from(amount) <= u128::from(available))
        .collect()
}
