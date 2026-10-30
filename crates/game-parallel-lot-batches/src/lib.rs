//! Flatten transactions without changing their ownership or physical semantics.
use game_parallel_lot_model::{Container, State, Transaction};
use game_parallel_lot_planning::{Batch, Draft, prefixes};
use rayon::prelude::*;

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
    mut additions: Vec<Container>,
    compact: &[u64],
) -> Result<(Draft, Vec<Container>), String> {
    additions.par_sort_unstable_by_key(|row| row.place);
    if additions
        .par_windows(2)
        .any(|pair| pair[0].place == pair[1].place)
        || additions.par_iter().any(|row| {
            row.validate().is_err()
                || row.used != 0
                || state
                    .containers
                    .binary_search_by_key(&row.place, |old| old.place)
                    .is_ok()
        })
    {
        return Err("invalid or existing new custody".into());
    }
    let mut ids: Vec<_> = transactions.par_iter().map(|row| row.id).collect();
    ids.par_sort_unstable();
    if ids.first() == Some(&0) || ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("invalid or duplicate transaction id".into());
    }
    let mut compact = compact.to_vec();
    compact.par_sort_unstable();
    if compact.par_windows(2).any(|pair| pair[0] == pair[1])
        || compact.par_iter().any(|id| ids.binary_search(id).is_err())
        || transactions.par_iter().any(|row| {
            compact.binary_search(&row.id).is_ok()
                && (row.movements.len() != 1
                    || row.movements[0].to.is_some()
                    || !row.creations.is_empty())
        })
    {
        return Err("invalid compact consumption".into());
    }
    if transactions
        .par_iter()
        .any(|row| row.movements.is_empty() && row.creations.is_empty())
    {
        return Err("empty physical transaction".into());
    }
    let moves: Vec<_> = transactions
        .par_iter()
        .flat_map_iter(|row| row.movements.iter().copied())
        .collect();
    let creations: Vec<_> = transactions
        .par_iter()
        .flat_map_iter(|row| row.creations.iter().cloned())
        .collect();
    let move_counts: Vec<_> = transactions
        .par_iter()
        .map(|row| row.movements.len() as u64)
        .collect();
    let creation_counts: Vec<_> = transactions
        .par_iter()
        .map(|row| row.creations.len() as u64)
        .collect();
    let move_offsets = prefixes(&move_counts);
    let creation_offsets = prefixes(&creation_counts);
    let groups: Vec<_> = transactions
        .par_iter()
        .enumerate()
        .map(|(index, row)| {
            let a = move_offsets[index] as usize;
            let b = creation_offsets[index] as usize;
            (
                row.id,
                a..a + row.movements.len(),
                b..b + row.creations.len(),
            )
        })
        .collect();
    let move_owners: Vec<_> = transactions
        .par_iter()
        .flat_map_iter(|row| std::iter::repeat_n(row.id, row.movements.len()))
        .collect();
    let creation_owners: Vec<_> = transactions
        .par_iter()
        .flat_map_iter(|row| std::iter::repeat_n(row.id, row.creations.len()))
        .collect();
    let draft = game_parallel_lot_planning::prepare(
        state,
        instance,
        seed,
        &moves,
        &creations,
        Some(Batch {
            groups: &groups,
            move_owners: &move_owners,
            creation_owners: &creation_owners,
            additions: &additions,
            compact: &compact,
        }),
        advance,
    )?;
    Ok((draft, additions))
}
