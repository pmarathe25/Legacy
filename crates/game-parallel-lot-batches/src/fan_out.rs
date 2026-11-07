//! Resolve one contested source feeding distinct destinations in parallel.
use crate::independent_transfer::after;
use game_parallel_lot_model::{
    Lot, Movement, OutcomeBatch, OutcomeChunk, Place, Spoilage, State, Transaction,
    TransactionOutcome,
};
use game_parallel_lot_planning::Draft;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Row {
    transaction: u64,
    movement: Movement,
    destination: usize,
    rank: u64,
}

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128 {
        return Ok(None);
    }
    let Some(first) = transactions[0].movements.first().copied() else {
        return Ok(None);
    };
    if transactions[1]
        .movements
        .first()
        .is_none_or(|row| row.from != first.from || row.good != first.good)
        || transactions.par_iter().any(|transaction| {
            transaction.movements.len() != 1 || !transaction.creations.is_empty() || {
                let movement = transaction.movements[0];
                movement.from != first.from
                    || movement.good != first.good
                    || movement.id == 0
                    || movement.quantity == 0
                    || !movement.from.accepts(movement.good)
                    || movement
                        .to
                        .is_none_or(|to| to == movement.from || !to.accepts(movement.good))
            }
        })
    {
        return Ok(None);
    }
    let next = if advance {
        state.tick.checked_add(1).ok_or("lot tick overflow")?
    } else {
        state.tick
    };
    state
        .revision
        .checked_add(1)
        .ok_or("lot revision overflow")?;
    #[cfg(feature = "profile")]
    let mut phase = game_diagnostics::span("physical.fan_out.locate");
    let Some(source_index) = state
        .containers
        .binary_search_by_key(&first.from, |row| row.place)
        .ok()
    else {
        return Ok(None);
    };
    let people_start = state
        .containers
        .partition_point(|c| c.place < Place::Person(1));
    let find = |place| {
        if let Place::Person(id) = place
            && let Ok(offset) = usize::try_from(id - 1)
            && let Some(index) = people_start.checked_add(offset)
            && state
                .containers
                .get(index)
                .is_some_and(|c| c.place == place)
        {
            return Some(index);
        }
        state
            .containers
            .binary_search_by_key(&place, |c| c.place)
            .ok()
    };
    let rows: Option<Vec<_>> = transactions
        .par_iter()
        .map(|transaction| {
            let movement = transaction.movements[0];
            Some(Row {
                transaction: transaction.id,
                movement,
                destination: find(movement.to?)?,
                rank: game_parallel_genetics::random(seed, transaction.id, next),
            })
        })
        .collect();
    let Some(mut rows) = rows else {
        return Ok(None);
    };
    let mut destinations: Vec<_> = rows.par_iter().map(|row| row.destination).collect();
    if destinations.par_windows(2).any(|pair| pair[0] > pair[1]) {
        destinations.par_sort_unstable();
    }
    if destinations.par_windows(2).any(|pair| pair[0] == pair[1])
        || destinations.binary_search(&source_index).is_ok()
    {
        return Ok(None);
    }
    let mut movement_ids: Vec<_> = rows.par_iter().map(|row| row.movement.id).collect();
    if movement_ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        movement_ids.par_sort_unstable();
    }
    if movement_ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.fan_out.arbitrate");
    }
    rows.par_sort_unstable_by_key(|row| (row.rank, row.movement.id));
    let source = &state.containers[source_index];
    let lots: Vec<_> = if source.lots.len() >= 2048 {
        source
            .lots
            .par_iter()
            .filter(|lot| lot.good == first.good && lot.expires_at.is_none_or(|at| at > next))
            .collect()
    } else {
        source
            .lots
            .iter()
            .filter(|lot| lot.good == first.good && lot.expires_at.is_none_or(|at| at > next))
            .collect()
    };
    let lot_amounts: Vec<_> = lots.par_iter().map(|lot| lot.quantity).collect();
    let lot_offsets = game_parallel_lot_allocation::prefixes(&lot_amounts);
    let available: u64 = lot_amounts.par_iter().copied().sum();
    let requests: Vec<_> = rows.par_iter().map(|row| row.movement.quantity).collect();
    let source_grants = game_parallel_lot_allocation::choose(&requests, available);
    let accepted: Vec<_> = rows
        .par_iter()
        .zip(source_grants.par_iter())
        .map(|(row, &granted)| {
            let destination = &state.containers[row.destination];
            let used: u64 = if destination.next_expiry.is_none_or(|at| at > next) {
                destination.used
            } else {
                destination
                    .lots
                    .iter()
                    .filter(|lot| lot.expires_at.is_none_or(|at| at > next))
                    .map(|lot| lot.quantity)
                    .sum()
            };
            granted && row.movement.quantity <= destination.capacity - used
        })
        .collect();
    let accepted_amounts: Vec<_> = rows
        .par_iter()
        .zip(accepted.par_iter())
        .map(|(row, &accepted)| if accepted { row.movement.quantity } else { 0 })
        .collect();
    let offsets = game_parallel_lot_allocation::prefixes(&accepted_amounts);
    let consumed: u64 = accepted_amounts.par_iter().sum();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.fan_out.materialize");
    }
    let mut spoiled: Vec<_> = state
        .containers
        .par_iter()
        .filter(|container| container.next_expiry.is_some_and(|at| at <= next))
        .flat_map_iter(|container| {
            container.lots.iter().filter_map(move |lot| {
                lot.expires_at.filter(|&at| at <= next).map(|at| Spoilage {
                    place: container.place,
                    good: lot.good,
                    quantity: lot.quantity,
                    expires_at: at,
                })
            })
        })
        .collect();
    spoiled.par_sort_unstable_by_key(|row| (row.place, row.good, row.expires_at));
    let chunk_size = rows.len().div_ceil(64).clamp(16, 2048);
    let parts: Vec<_> = rows
        .par_chunks(chunk_size)
        .zip(accepted.par_chunks(chunk_size))
        .zip(offsets.par_chunks(chunk_size))
        .map(|((chunk, decisions), offsets)| {
            let mut arena = Vec::with_capacity(chunk.len());
            let mut headers = Vec::with_capacity(chunk.len());
            let mut outcomes = Vec::with_capacity(chunk.len());
            let mut changes = Vec::with_capacity(chunk.len());
            for ((row, &accepted), &offset) in chunk.iter().zip(decisions).zip(offsets) {
                let begin = arena.len();
                if accepted {
                    let end = offset + u128::from(row.movement.quantity);
                    let mut index = lot_offsets
                        .partition_point(|&start| start <= offset)
                        .saturating_sub(1);
                    while index < lots.len() && lot_offsets[index] < end {
                        let lot = lots[index];
                        let lot_end = lot_offsets[index] + u128::from(lot.quantity);
                        let quantity = (lot_end.min(end) - lot_offsets[index].max(offset)) as u64;
                        if quantity > 0 {
                            arena.push(Lot {
                                good: lot.good,
                                quantity,
                                expires_at: lot.expires_at,
                            });
                        }
                        index += 1;
                    }
                }
                headers.push((row.movement.id, accepted, begin..arena.len()));
                outcomes.push(TransactionOutcome {
                    id: row.transaction,
                    accepted,
                });
                let destination = &state.containers[row.destination];
                if accepted || destination.next_expiry.is_some_and(|at| at <= next) {
                    changes.push((
                        row.destination,
                        after(destination, next, 0, 0, &arena[begin..]),
                    ));
                }
            }
            (
                OutcomeChunk {
                    headers,
                    lots: arena,
                },
                outcomes,
                changes,
            )
        })
        .collect();
    let mut chunks = Vec::with_capacity(parts.len());
    let mut outcomes = Vec::with_capacity(rows.len());
    let mut changes = Vec::with_capacity(rows.len() + 1);
    for (chunk, transactions, destinations) in parts {
        chunks.push(chunk);
        outcomes.extend(transactions);
        changes.extend(destinations);
    }
    if consumed > 0 || source.next_expiry.is_some_and(|at| at <= next) {
        changes.push((source_index, after(source, next, first.good, consumed, &[])));
    }
    if !spoiled.is_empty() {
        changes.par_extend(state.containers.par_iter().enumerate().filter_map(
            |(index, container)| {
                (index != source_index
                    && destinations.binary_search(&index).is_err()
                    && container.next_expiry.is_some_and(|at| at <= next))
                .then(|| (index, after(container, next, 0, 0, &[])))
            },
        ));
    }
    changes.par_sort_unstable_by_key(|row| row.0);
    let mut receipts = OutcomeBatch::from_chunks(chunks);
    receipts.sort_by_id();
    if outcomes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        outcomes.par_sort_unstable_by_key(|row| row.id);
    }
    #[cfg(feature = "profile")]
    drop(phase);
    Ok(Some(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes: changes.into(),
        outcomes: receipts,
        spoiled,
        creations: Vec::new(),
        transactions: outcomes,
    }))
}

#[cfg(test)]
#[path = "../tests/support/fan_out.rs"]
mod tests;
