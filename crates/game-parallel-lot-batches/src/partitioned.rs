//! Split actions only where all physical endpoints are exclusive.
use game_parallel_lot_model::{Container, Place, SimpleTransfer, State, Transaction};
use game_parallel_lot_planning::Draft;
use rayon::prelude::*;

/// Isolate simple transactions only when no other physical leg touches their
/// custody. Multi-leg actions remain together in the general arbiter.
pub fn prepare_transactions(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    compact: &[u64],
    additions: &[Container],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 256 {
        return Ok(None);
    }
    if transactions
        .par_iter()
        .filter(|row| {
            row.movements.len() == 1
                && row.movements[0].to.is_some()
                && row.creations.is_empty()
                && compact.binary_search(&row.id).is_err()
        })
        .count()
        < 128
    {
        return Ok(None);
    }
    // Movement and creation identities share a namespace, including across
    // the two partitions. Let the general planner report malformed input.
    let mut ids: Vec<_> = transactions
        .par_iter()
        .flat_map_iter(|row| {
            row.movements
                .iter()
                .map(|movement| movement.id)
                .chain(row.creations.iter().map(|creation| creation.id))
        })
        .collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    let mut endpoints: Vec<_> = transactions
        .par_iter()
        .enumerate()
        .flat_map_iter(|(index, row)| {
            row.movements
                .iter()
                .flat_map(move |movement| {
                    std::iter::once((movement.from, index))
                        .chain(movement.to.map(|place| (place, index)))
                })
                .chain(
                    row.creations
                        .iter()
                        .map(move |creation| (creation.to, index)),
                )
        })
        .collect();
    endpoints.par_sort_unstable();
    let mut contested: Vec<_> = endpoints
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter(|group| group.len() > 1)
        .flat_map_iter(|group| group.iter().map(|row| row.1))
        .collect();
    contested.par_sort_unstable();
    let contested: Vec<_> = contested
        .par_iter()
        .enumerate()
        .filter_map(|(index, &value)| {
            (index == 0 || contested[index - 1] != value).then_some(value)
        })
        .collect();
    let eligible: Vec<_> = transactions
        .par_iter()
        .enumerate()
        .map(|(index, row)| {
            if contested.binary_search(&index).is_ok()
                || compact.binary_search(&row.id).is_ok()
                || row.movements.len() != 1
                || !row.creations.is_empty()
            {
                return false;
            }
            let movement = row.movements[0];
            movement.id != 0
                && movement.quantity != 0
                && movement.from.accepts(movement.good)
                && movement
                    .to
                    .is_some_and(|to| to != movement.from && to.accepts(movement.good))
                && (additions.is_empty()
                    || [movement.from, movement.to.unwrap()].iter().all(|place| {
                        state
                            .containers
                            .binary_search_by_key(place, |row| row.place)
                            .is_ok()
                    }))
        })
        .collect();
    let transfers: Vec<_> = transactions
        .par_iter()
        .zip(eligible.par_iter())
        .filter_map(|(row, &eligible)| {
            eligible.then(|| SimpleTransfer {
                action: row.id,
                movement: row.movements[0],
            })
        })
        .collect();
    if transfers.len() < 128 || transfers.len() == transactions.len() {
        return Ok(None);
    }
    let remaining: Vec<_> = transactions
        .par_iter()
        .zip(eligible.par_iter())
        .filter(|(_, eligible)| !**eligible)
        .map(|(row, _)| row.clone())
        .collect();
    let (fast, general) = rayon::join(
        || super::independent_transfer::prepare_flat(state, instance, &transfers, advance),
        || {
            super::prepare_unpartitioned(
                state,
                instance,
                seed,
                &remaining,
                advance,
                additions.to_vec(),
                compact,
            )
        },
    );
    let Some(fast) = fast? else { return Ok(None) };
    let (general, _) = general?;
    Ok(Some(merge(state, additions, &transfers, fast, general)))
}

fn merge(
    state: &State,
    additions: &[Container],
    transfers: &[SimpleTransfer],
    mut fast: Draft,
    mut general: Draft,
) -> Draft {
    let mut fast_changes = std::mem::take(&mut fast.changes).into_flat();
    let general_changes = std::mem::take(&mut general.changes).into_flat();
    if !additions.is_empty() {
        fast_changes.par_iter_mut().for_each(|row| {
            row.0 += additions.partition_point(|addition| addition.place < row.1.place);
        });
    }
    let mut changes = if fast.spoiled.is_empty() {
        // Without boundary-wide expiry, each planner changes only its own
        // exclusive endpoints, so no ownership filtering is necessary.
        let mut changes = fast_changes;
        changes.par_extend(general_changes);
        changes
    } else {
        let mut independent_endpoints: Vec<_> = transfers
            .par_iter()
            .flat_map_iter(|row| [row.movement.from, row.movement.to.unwrap()])
            .map(|place| {
                let index = state
                    .containers
                    .binary_search_by_key(&place, |row| row.place)
                    .expect("the fast planner checked custody");
                index + additions.partition_point(|addition| addition.place < place)
            })
            .collect();
        independent_endpoints.par_sort_unstable();
        let mut changes: Vec<_> = fast_changes
            .into_par_iter()
            .filter(|row| independent_endpoints.binary_search(&row.0).is_ok())
            .collect();
        changes.par_extend(
            general_changes
                .into_par_iter()
                .filter(|row| independent_endpoints.binary_search(&row.0).is_err()),
        );
        changes
    };
    changes.par_sort_unstable_by_key(|row| row.0);
    general.changes = changes.into();
    general.outcomes.append(fast.outcomes);
    general.outcomes.sort_by_id();
    general.transactions.par_extend(fast.transactions);
    general.transactions.par_sort_unstable_by_key(|row| row.id);
    general
}

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if let Some(draft) =
        super::independent_transfer::prepare_flat(state, instance, transfers, advance)?
    {
        return Ok(Some(draft));
    }
    if transfers.len() < 256
        || transfers.par_iter().any(|row| {
            let m = row.movement;
            m.id == 0
                || m.quantity == 0
                || !m.from.accepts(m.good)
                || m.to.is_none_or(|to| to == m.from || !to.accepts(m.good))
        })
    {
        return Ok(None);
    }
    if let Some(draft) = super::fan_out::prepare_flat(state, instance, seed, transfers, advance)? {
        return Ok(Some(draft));
    }
    if let Some(draft) = super::grouped_sources::prepare(state, instance, seed, transfers, advance)?
    {
        return Ok(Some(draft));
    }
    if let Some(draft) = super::fan_in::prepare_flat(state, instance, seed, transfers, advance)? {
        return Ok(Some(draft));
    }
    let mut movement_ids: Vec<_> = transfers.par_iter().map(|row| row.movement.id).collect();
    if movement_ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        movement_ids.par_sort_unstable();
    }
    if movement_ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    let mut endpoints: Vec<(Place, usize)> = transfers
        .par_iter()
        .enumerate()
        .flat_map_iter(|(index, row)| {
            [
                (row.movement.from, index),
                (row.movement.to.expect("validated destination"), index),
            ]
        })
        .collect();
    endpoints.par_sort_unstable();
    let mut contested: Vec<_> = endpoints
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter(|group| group.len() > 1)
        .flat_map_iter(|group| group.iter().map(|row| row.1))
        .collect();
    contested.par_sort_unstable();
    let contested: Vec<_> = contested
        .par_iter()
        .enumerate()
        .filter_map(|(index, &value)| {
            (index == 0 || contested[index - 1] != value).then_some(value)
        })
        .collect();
    if contested.is_empty() || transfers.len().saturating_sub(contested.len()) < 128 {
        return Ok(None);
    }
    let independent: Vec<_> = transfers
        .par_iter()
        .enumerate()
        .filter(|(index, _)| contested.binary_search(index).is_err())
        .map(|(_, row)| *row)
        .collect();
    let conflicted: Vec<_> = contested
        .par_iter()
        .map(|&index| {
            let row = transfers[index];
            Transaction {
                id: row.action,
                movements: vec![row.movement],
                creations: Vec::new(),
            }
        })
        .collect();
    let (fast, general) = rayon::join(
        || super::independent_transfer::prepare_flat(state, instance, &independent, advance),
        || super::prepare(state, instance, seed, &conflicted, advance, Vec::new(), &[]),
    );
    let Some(fast) = fast? else { return Ok(None) };
    let (general, additions) = general?;
    debug_assert!(additions.is_empty());
    Ok(Some(merge(state, &[], &independent, fast, general)))
}
