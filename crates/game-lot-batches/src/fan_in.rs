//! Resolve single-leg transfers with exclusive sources and shared destinations.
use crate::independent_transfer::after;
use game_lot_model::{
    Container, Lot, Movement, OutcomeBatch, OutcomeChunk, Place, SimpleTransfer, Spoilage, State,
    Transaction, TransactionOutcome, key,
};
use game_lot_planning::Draft;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
struct Row {
    source: usize,
    destination: usize,
    transaction: u64,
    movement: Movement,
}

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128
        || transactions.par_iter().any(|transaction| {
            transaction.movements.len() != 1 || !transaction.creations.is_empty() || {
                let movement = transaction.movements[0];
                movement.id == 0
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
    prepare_rows(
        state,
        instance,
        seed,
        transactions.len(),
        |index| {
            let transaction = &transactions[index];
            (transaction.id, transaction.movements[0])
        },
        advance,
    )
}

/// Action and movement identities have already been validated by the flat planner.
pub fn prepare_flat(
    state: &State,
    instance: u64,
    seed: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transfers.len() < 128 {
        return Ok(None);
    }
    prepare_rows(
        state,
        instance,
        seed,
        transfers.len(),
        |index| {
            let row = transfers[index];
            (row.action, row.movement)
        },
        advance,
    )
}

fn prepare_rows(
    state: &State,
    instance: u64,
    seed: u64,
    len: usize,
    row_at: impl Fn(usize) -> (u64, Movement) + Sync,
    advance: bool,
) -> Result<Option<Draft>, String> {
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
    let mut phase = game_diagnostics::span("physical.fan_in.locate");
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
    let rows: Option<Vec<_>> = (0..len)
        .into_par_iter()
        .map(|index| {
            let (transaction, movement) = row_at(index);
            Some(Row {
                source: find(movement.from)?,
                destination: find(movement.to?)?,
                transaction,
                movement,
            })
        })
        .collect();
    let Some(rows) = rows else { return Ok(None) };
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.fan_in.eligibility");
    }
    let mut sources: Vec<_> = rows.par_iter().map(|row| row.source).collect();
    if sources.par_windows(2).any(|pair| pair[0] > pair[1]) {
        sources.par_sort_unstable();
    }
    if sources.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    let mut destinations: Vec<_> = rows.par_iter().map(|row| row.destination).collect();
    if destinations.par_windows(2).any(|pair| pair[0] > pair[1]) {
        destinations.par_sort_unstable();
    }
    // Independent transfers already handle distinct destinations more cheaply.
    if destinations.par_windows(2).all(|pair| pair[0] != pair[1]) {
        return Ok(None);
    }
    let separated = sources
        .last()
        .zip(destinations.first())
        .is_some_and(|(source, destination)| source < destination)
        || destinations
            .last()
            .zip(sources.first())
            .is_some_and(|(destination, source)| destination < source);
    if !separated
        && destinations
            .par_chunk_by(|a, b| a == b)
            .any(|group| sources.binary_search(&group[0]).is_ok())
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
        phase = game_diagnostics::span("physical.fan_in.arbitrate");
    }
    let source_valid: Vec<_> = rows
        .par_iter()
        .map(|row| {
            let available: u128 = state.containers[row.source]
                .lots
                .iter()
                .filter(|lot| {
                    lot.good == row.movement.good && lot.expires_at.is_none_or(|at| at > next)
                })
                .map(|lot| u128::from(lot.quantity))
                .sum();
            available >= u128::from(row.movement.quantity)
        })
        .collect();
    let mut amounts: Vec<_> = rows
        .par_iter()
        .zip(source_valid.par_iter())
        .filter_map(|(row, &valid)| valid.then_some((row.destination, row.movement.quantity)))
        .collect();
    if amounts.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        amounts.par_sort_unstable_by_key(|row| row.0);
    }
    let contested = amounts.par_chunk_by(|a, b| a.0 == b.0).any(|group| {
        let container = &state.containers[group[0].0];
        let used: u128 = if container.next_expiry.is_none_or(|at| at > next) {
            u128::from(container.used)
        } else {
            container
                .lots
                .par_iter()
                .filter(|lot| lot.expires_at.is_none_or(|at| at > next))
                .map(|lot| u128::from(lot.quantity))
                .sum()
        };
        let incoming: u128 = group.par_iter().map(|row| u128::from(row.1)).sum();
        used + incoming > u128::from(container.capacity)
    });
    let accepted: Vec<bool> = if contested {
        let containers: Vec<_> = state.containers.par_iter().collect();
        let used: Vec<_> = state
            .containers
            .par_iter()
            .map(|container| {
                if container.next_expiry.is_none_or(|at| at > next) {
                    container.used
                } else {
                    container
                        .lots
                        .par_iter()
                        .filter(|lot| lot.expires_at.is_none_or(|at| at > next))
                        .map(|lot| lot.quantity)
                        .sum()
                }
            })
            .collect();
        let flags: Vec<_> = (0..rows.len())
            .into_par_iter()
            .map(|_| AtomicBool::new(false))
            .collect();
        let candidates: Vec<_> = rows
            .par_iter()
            .zip(source_valid.par_iter())
            .enumerate()
            .filter_map(|(index, (row, &valid))| {
                valid.then(|| game_lot_allocation::Destination {
                    index,
                    place: row.destination,
                    rank: game_genetics::random(seed, row.transaction, next),
                    transaction: row.transaction,
                    id: row.movement.id,
                    quantity: row.movement.quantity,
                })
            })
            .collect();
        // Sources never overlap destinations in this path, so no action has an
        // outgoing credit at a contested destination.
        game_lot_allocation::grant_destinations(candidates, &[], &containers, &used, &flags);
        flags
            .into_par_iter()
            .map(|flag| flag.load(Ordering::Relaxed))
            .collect()
    } else {
        source_valid
    };
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.fan_in.materialize");
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
        .map(|(chunk, decisions)| {
            let mut lots = Vec::with_capacity(chunk.len());
            let mut headers = Vec::with_capacity(chunk.len());
            let mut outcomes = Vec::with_capacity(chunk.len());
            let mut changes = Vec::with_capacity(chunk.len());
            let mut incoming = Vec::with_capacity(chunk.len());
            for (row, &accepted) in chunk.iter().zip(decisions) {
                let source = &state.containers[row.source];
                let begin = lots.len();
                let mut remaining = if accepted { row.movement.quantity } else { 0 };
                for lot in &source.lots {
                    if remaining == 0 {
                        break;
                    }
                    if lot.good == row.movement.good && lot.expires_at.is_none_or(|at| at > next) {
                        let quantity = remaining.min(lot.quantity);
                        let part = Lot {
                            item: lot.item,
                            good: lot.good,
                            quantity,
                            expires_at: lot.expires_at,
                        };
                        incoming.push((row.destination, part.clone()));
                        lots.push(part);
                        remaining -= quantity;
                    }
                }
                debug_assert_eq!(remaining, 0);
                headers.push((row.movement.id, accepted, begin..lots.len()));
                outcomes.push(TransactionOutcome {
                    id: row.transaction,
                    accepted,
                });
                if accepted || source.next_expiry.is_some_and(|at| at <= next) {
                    changes.push((
                        row.source,
                        after(
                            source,
                            next,
                            row.movement.good,
                            if accepted { row.movement.quantity } else { 0 },
                            &[],
                        ),
                    ));
                }
            }
            (OutcomeChunk { headers, lots }, outcomes, changes, incoming)
        })
        .collect();
    let mut chunks = Vec::with_capacity(parts.len());
    let mut transactions = Vec::with_capacity(rows.len());
    let mut changes = Vec::with_capacity(rows.len());
    let mut incoming = Vec::new();
    for (chunk, outcomes, source_changes, additions) in parts {
        chunks.push(chunk);
        transactions.extend(outcomes);
        changes.extend(source_changes);
        incoming.extend(additions);
    }
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.fan_in.coalesce");
    }
    if incoming
        .par_windows(2)
        .any(|pair| (pair[0].0, key(&pair[0].1)) > (pair[1].0, key(&pair[1].1)))
    {
        incoming.par_sort_unstable_by_key(|row| (row.0, key(&row.1)));
    }
    let cohorts: Vec<_> = incoming
        .par_chunk_by(|a, b| a.0 == b.0 && key(&a.1) == key(&b.1))
        .map(|group| {
            let mut lot = group[0].1.clone();
            lot.quantity = group.par_iter().map(|row| row.1.quantity).sum();
            (group[0].0, lot)
        })
        .collect();
    let destination_changes: Vec<_> = cohorts
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| {
            let index = group[0].0;
            let container = &state.containers[index];
            let mut lots: Vec<_> = container
                .lots
                .par_iter()
                .filter(|lot| lot.expires_at.is_none_or(|at| at > next))
                .cloned()
                .chain(group.par_iter().map(|row| row.1.clone()))
                .collect();
            if lots
                .par_windows(2)
                .any(|pair| key(&pair[0]) > key(&pair[1]))
            {
                lots.par_sort_unstable_by_key(key);
            }
            if lots
                .par_windows(2)
                .any(|pair| key(&pair[0]) == key(&pair[1]))
            {
                lots = lots
                    .par_chunk_by(|a, b| key(a) == key(b))
                    .map(|group| {
                        let mut lot = group[0].clone();
                        lot.quantity = group.par_iter().map(|lot| lot.quantity).sum();
                        lot
                    })
                    .collect();
            }
            let used = lots.par_iter().map(|lot| lot.quantity).sum();
            let next_expiry = lots.par_iter().filter_map(|lot| lot.expires_at).min();
            (
                index,
                Container {
                    place: container.place,
                    capacity: container.capacity,
                    lots,
                    used,
                    next_expiry,
                },
            )
        })
        .collect();
    let changed_destinations: Vec<_> = if spoiled.is_empty() {
        Vec::new()
    } else {
        destination_changes.par_iter().map(|row| row.0).collect()
    };
    changes.par_extend(destination_changes);
    if !spoiled.is_empty() {
        changes.par_extend(state.containers.par_iter().enumerate().filter_map(
            |(index, container)| {
                (container.next_expiry.is_some_and(|at| at <= next)
                    && sources.binary_search(&index).is_err()
                    && changed_destinations.binary_search(&index).is_err())
                .then(|| (index, after(container, next, 0, 0, &[])))
            },
        ));
    }
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.fan_in.finalize");
    }
    changes.par_sort_unstable_by_key(|row| row.0);
    let mut outcomes = OutcomeBatch::from_chunks(chunks);
    outcomes.sort_by_id();
    if transactions
        .par_windows(2)
        .any(|pair| pair[0].id > pair[1].id)
    {
        transactions.par_sort_unstable_by_key(|row| row.id);
    }
    #[cfg(feature = "profile")]
    drop(phase);
    Ok(Some(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes: changes.into(),
        outcomes,
        spoiled,
        creations: Vec::new(),
        transactions,
    }))
}

#[cfg(test)]
#[path = "../tests/support/fan_in.rs"]
mod tests;
