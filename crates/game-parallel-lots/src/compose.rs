//! Compose independently generated physical actions into one identity space.
use crate::Transaction;
use rayon::prelude::*;

pub struct Composed {
    pub transactions: Vec<Transaction>,
    /// At position `i`, the original owner key corresponding to transaction `i + 1`.
    pub keys: Vec<(u32, u64)>,
}
impl Composed {
    /// Translate owner-local compact consumption IDs into this batch's IDs.
    pub fn compact_ids(&self, owner: u32, ids: &[u64]) -> Result<Vec<u64>, String> {
        let mut ids = ids.to_vec();
        ids.par_sort_unstable();
        if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate compact consumption".into());
        }
        let found: Vec<_> = self
            .keys
            .par_iter()
            .enumerate()
            .filter_map(|(i, &(key_owner, id))| {
                (key_owner == owner && ids.binary_search(&id).is_ok()).then_some(i as u64 + 1)
            })
            .collect();
        if found.len() != ids.len() {
            return Err("unknown compact consumption".into());
        }
        Ok(found)
    }
}
pub fn compose(mut actions: Vec<(u32, Transaction)>) -> Result<Composed, String> {
    game_parallel_lot_allocation::order_actions(&mut actions);
    if actions
        .par_iter()
        .any(|(_, row)| row.id == 0 || (row.movements.is_empty() && row.creations.is_empty()))
        || actions
            .par_windows(2)
            .any(|pair| (pair[0].0, pair[0].1.id) == (pair[1].0, pair[1].1.id))
    {
        return Err("invalid or duplicate physical action key".into());
    }
    let counts: Vec<_> = actions
        .par_iter()
        .map(|(_, row)| (row.movements.len() + row.creations.len()) as u64)
        .collect();
    let offsets = game_parallel_lot_planning::prefixes(&counts);
    if offsets
        .last()
        .zip(counts.last())
        .is_some_and(|(&start, &count)| start + u128::from(count) > u128::from(u64::MAX))
        || u64::try_from(actions.len()).is_err()
    {
        return Err("physical action identity overflow".into());
    }
    let keys = actions
        .par_iter()
        .map(|(owner, row)| (*owner, row.id))
        .collect();
    let transactions = actions
        .into_par_iter()
        .enumerate()
        .map(|(index, (_, mut row))| {
            row.id = index as u64 + 1;
            let base = offsets[index] as u64;
            let movement_count = row.movements.len();
            for (n, item) in row.movements.iter_mut().enumerate() {
                item.id = base + n as u64 + 1;
            }
            for (n, item) in row.creations.iter_mut().enumerate() {
                item.id = base + movement_count as u64 + n as u64 + 1;
            }
            row
        })
        .collect();
    Ok(Composed { transactions, keys })
}
