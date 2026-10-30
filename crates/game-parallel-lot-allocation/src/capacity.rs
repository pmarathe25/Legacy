//! Capacity claims grouped by physical action, crediting only its own outflow.
use game_parallel_lot_model::Container;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
const CHUNK: usize = 2048;
#[derive(Clone, Copy)]
pub struct Destination {
    pub index: usize,
    pub place: usize,
    pub rank: u64,
    pub transaction: u64,
    pub id: u64,
    pub quantity: u64,
}
fn prefixes(values: &[u128]) -> Vec<u128> {
    let totals: Vec<u128> = values
        .par_chunks(CHUNK)
        .map(|chunk| chunk.iter().sum())
        .collect();
    let mut offset = 0u128;
    let offsets: Vec<_> = totals
        .into_iter()
        .map(|sum| {
            let previous = offset;
            offset += sum;
            previous
        })
        .collect();
    let mut result = vec![0; values.len()];
    result
        .par_chunks_mut(CHUNK)
        .zip(values.par_chunks(CHUNK))
        .zip(offsets.par_iter())
        .for_each(|((output, chunk), &offset)| {
            let mut sum = offset;
            for (slot, &value) in output.iter_mut().zip(chunk) {
                *slot = sum;
                sum += value;
            }
        });
    result
}
/// Each action can replace its own outgoing goods, but cannot spend another action's outflow.
pub fn grant_destinations(
    mut rows: Vec<Destination>,
    outgoing: &[(u64, usize, u128)],
    containers: &[&Container],
    used: &[u64],
    accepted: &[AtomicBool],
) {
    rows.par_sort_unstable_by_key(|row| (row.place, row.rank, row.transaction, row.id));
    let groups: Vec<_> = rows
        .par_chunk_by(|a, b| (a.place, a.transaction) == (b.place, b.transaction))
        .collect();
    groups
        .par_chunk_by(|a, b| a[0].place == b[0].place)
        .for_each(|groups| {
            let place = groups[0][0].place;
            let available = u128::from(containers[place].capacity - used[place]);
            let needs: Vec<_> = groups
                .par_iter()
                .map(|group| {
                    let incoming: u128 = group.iter().map(|row| u128::from(row.quantity)).sum();
                    let key = (group[0].transaction, place);
                    let own = outgoing
                        .binary_search_by_key(&key, |row| (row.0, row.1))
                        .map_or(0, |index| outgoing[index].2);
                    incoming.saturating_sub(own)
                })
                .collect();
            let offsets = prefixes(&needs);
            groups
                .par_iter()
                .zip(needs.par_iter())
                .zip(offsets.par_iter())
                .for_each(|((group, &need), &before)| {
                    if before + need <= available {
                        for row in *group {
                            accepted[row.index].store(true, Ordering::Relaxed);
                        }
                    }
                });
        });
}
