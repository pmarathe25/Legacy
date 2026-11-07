//! Resolve independent, moderately sized shared sources in parallel.
use crate::independent_transfer::after;
use game_lot_model::{
    Container, Lot, Movement, OutcomeBatch, OutcomeChunk, Place, SimpleTransfer, Spoilage, State,
    TransactionOutcome,
};
use game_lot_planning::Draft;
use rayon::prelude::*;

// A single huge source has its own parallel prefix path. Bounding each group
// here prevents one worker from serializing a concentrated workload.
const MAX_GROUP: usize = 2048;

#[derive(Clone, Copy)]
struct Row {
    source: usize,
    destination: usize,
    action: u64,
    movement: Movement,
    rank: u64,
}

struct Partial {
    chunk: OutcomeChunk,
    outcomes: Vec<TransactionOutcome>,
    changes: Vec<(usize, Container)>,
}

/// The flat planner has already checked action identities and movement shape.
pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transfers.len() < 256 || transfers[0].movement.to == transfers[1].movement.to {
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
    let mut phase = game_diagnostics::span("physical.grouped_sources.locate");
    let people_start = state
        .containers
        .partition_point(|row| row.place < Place::Person(1));
    let find = |place| {
        if let Place::Person(id) = place
            && let Ok(offset) = usize::try_from(id - 1)
            && let Some(index) = people_start.checked_add(offset)
            && state
                .containers
                .get(index)
                .is_some_and(|row| row.place == place)
        {
            return Some(index);
        }
        state
            .containers
            .binary_search_by_key(&place, |row| row.place)
            .ok()
    };
    let rows: Option<Vec<_>> = transfers
        .par_iter()
        .map(|transfer| {
            let movement = transfer.movement;
            Some(Row {
                source: find(movement.from)?,
                destination: find(movement.to?)?,
                action: transfer.action,
                movement,
                rank: game_genetics::random(seed, transfer.action, next),
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
    if destinations.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    let mut ids: Vec<_> = rows.par_iter().map(|row| row.movement.id).collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    rows.par_sort_unstable_by_key(|row| (row.source, row.rank, row.movement.id));
    let starts: Vec<_> = rows
        .par_iter()
        .enumerate()
        .filter_map(|(index, row)| {
            (index == 0 || rows[index - 1].source != row.source).then_some(index)
        })
        .collect();
    if starts.len() < 2
        || starts.par_iter().enumerate().any(|(index, &start)| {
            let end = starts.get(index + 1).copied().unwrap_or(rows.len());
            end - start > MAX_GROUP
                || destinations.binary_search(&rows[start].source).is_ok()
                || rows[start..end]
                    .iter()
                    .any(|row| row.movement.good != rows[start].movement.good)
        })
    {
        return Ok(None);
    }
    let sources: Vec<_> = starts.par_iter().map(|&start| rows[start].source).collect();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.grouped_sources.resolve");
    }
    let parts: Vec<_> = starts
        .par_iter()
        .enumerate()
        .map(|(group, &start)| {
            let end = starts.get(group + 1).copied().unwrap_or(rows.len());
            let batch = &rows[start..end];
            let source = &state.containers[batch[0].source];
            let good = batch[0].movement.good;
            let lots: Vec<_> = source
                .lots
                .iter()
                .filter(|lot| lot.good == good && lot.expires_at.is_none_or(|at| at > next))
                .collect();
            let available: u128 = lots.iter().map(|lot| u128::from(lot.quantity)).sum();
            let mut prefix = 0u128;
            let mut consumed = 0u64;
            let mut lot_index = 0usize;
            let mut lot_used = 0u64;
            let mut arena = Vec::with_capacity(batch.len());
            let mut headers = Vec::with_capacity(batch.len());
            let mut outcomes = Vec::with_capacity(batch.len());
            let mut changes = Vec::with_capacity(batch.len() + 1);
            for row in batch {
                let destination = &state.containers[row.destination];
                let used = if destination.next_expiry.is_none_or(|at| at > next) {
                    destination.used
                } else {
                    destination
                        .lots
                        .iter()
                        .filter(|lot| lot.expires_at.is_none_or(|at| at > next))
                        .map(|lot| lot.quantity)
                        .sum()
                };
                prefix += u128::from(row.movement.quantity);
                let accepted =
                    prefix <= available && row.movement.quantity <= destination.capacity - used;
                let begin = arena.len();
                if accepted {
                    let mut remaining = row.movement.quantity;
                    while remaining > 0 {
                        let lot = lots[lot_index];
                        let quantity = remaining.min(lot.quantity - lot_used);
                        arena.push(Lot {
                            good,
                            quantity,
                            expires_at: lot.expires_at,
                        });
                        remaining -= quantity;
                        lot_used += quantity;
                        if lot_used == lot.quantity {
                            lot_index += 1;
                            lot_used = 0;
                        }
                    }
                    consumed += row.movement.quantity;
                }
                headers.push((row.movement.id, accepted, begin..arena.len()));
                outcomes.push(TransactionOutcome {
                    id: row.action,
                    accepted,
                });
                if accepted || destination.next_expiry.is_some_and(|at| at <= next) {
                    changes.push((
                        row.destination,
                        after(destination, next, 0, 0, &arena[begin..]),
                    ));
                }
            }
            if consumed > 0 || source.next_expiry.is_some_and(|at| at <= next) {
                changes.push((batch[0].source, after(source, next, good, consumed, &[])));
            }
            Partial {
                chunk: OutcomeChunk {
                    headers,
                    lots: arena,
                },
                outcomes,
                changes,
            }
        })
        .collect();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.grouped_sources.publish");
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
    let mut chunks = Vec::with_capacity(parts.len());
    let mut outcomes = Vec::with_capacity(rows.len());
    let mut changes = Vec::with_capacity(rows.len() + sources.len());
    for part in parts {
        chunks.push(part.chunk);
        outcomes.extend(part.outcomes);
        changes.extend(part.changes);
    }
    if !spoiled.is_empty() {
        changes.par_extend(state.containers.par_iter().enumerate().filter_map(
            |(index, container)| {
                (container.next_expiry.is_some_and(|at| at <= next)
                    && sources.binary_search(&index).is_err()
                    && destinations.binary_search(&index).is_err())
                .then(|| (index, after(container, next, 0, 0, &[])))
            },
        ));
    }
    changes.par_sort_unstable_by_key(|row| row.0);
    outcomes.par_sort_unstable_by_key(|row| row.id);
    let mut receipts = OutcomeBatch::from_chunks(chunks);
    receipts.sort_by_id();
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
#[path = "../tests/support/grouped_sources.rs"]
mod tests;
