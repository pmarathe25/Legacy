//! Compact consumption from one shared fungible source, with ordinary rank and prefix rules.
use game_lot_model::{
    Container, Lot, OutcomeBatch, OutcomeChunk, Spoilage, State, Transaction, TransactionOutcome,
};
use game_lot_planning::Draft;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Row {
    transaction: u64,
    movement: u64,
    quantity: u64,
    rank: u64,
}

fn after(container: &Container, tick: u64, good: u32, mut debit: u64) -> Container {
    let lots: Vec<Lot> = container
        .lots
        .iter()
        .filter_map(|lot| {
            if lot.expires_at.is_some_and(|at| at <= tick) {
                return None;
            }
            let consumed = if lot.good == good {
                debit.min(lot.quantity)
            } else {
                0
            };
            debit -= consumed;
            (consumed < lot.quantity).then(|| Lot {
                batch: lot.batch,
                good: lot.good,
                quantity: lot.quantity - consumed,
                item: lot.item,
                expires_at: lot.expires_at,
            })
        })
        .collect();
    debug_assert_eq!(debit, 0);
    Container {
        place: container.place,
        capacity: container.capacity,
        used: lots.iter().map(|lot| lot.quantity).sum(),
        next_expiry: lots.iter().filter_map(|lot| lot.expires_at).min(),
        lots,
    }
}

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    compact: &[u64],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128 || compact.len() != transactions.len() {
        return Ok(None);
    }
    let Some(first) = transactions
        .first()
        .and_then(|t| t.movements.first())
        .copied()
    else {
        return Ok(None);
    };
    if transactions
        .last()
        .and_then(|t| t.movements.first())
        .is_none_or(|movement| movement.from != first.from || movement.good != first.good)
    {
        return Ok(None);
    }
    if first.to.is_some()
        || first.item.is_some()
        || first.id == 0
        || first.quantity == 0
        || !first.from.accepts(first.good)
        || transactions.par_iter().any(|t| {
            t.movements.len() != 1 || !t.creations.is_empty() || {
                let m = t.movements[0];
                m.from != first.from
                    || m.good != first.good
                    || m.to.is_some()
                    || m.item.is_some()
                    || m.id == 0
                    || m.quantity == 0
            }
        })
    {
        return Ok(None);
    }
    let Some(source) = state
        .containers
        .binary_search_by_key(&first.from, |row| row.place)
        .ok()
    else {
        return Ok(None);
    };
    let holder = &state.containers[source];
    if holder.lots.iter().any(|lot| lot.item.is_some()) {
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
    let mut rows: Vec<_> = transactions
        .par_iter()
        .map(|transaction| {
            let movement = transaction.movements[0];
            Row {
                transaction: transaction.id,
                movement: movement.id,
                quantity: movement.quantity,
                rank: game_genetics::random(seed, transaction.id, next),
            }
        })
        .collect();
    let mut movement_ids: Vec<_> = rows.par_iter().map(|row| row.movement).collect();
    if movement_ids.par_windows(2).any(|p| p[0] > p[1]) {
        movement_ids.par_sort_unstable();
    }
    if movement_ids.par_windows(2).any(|p| p[0] == p[1]) {
        return Ok(None);
    }
    rows.par_sort_unstable_by_key(|row| (row.rank, row.movement));
    let available: u64 = holder
        .lots
        .iter()
        .filter(|lot| lot.good == first.good && lot.expires_at.is_none_or(|at| at > next))
        .map(|lot| lot.quantity)
        .sum();
    let quantities: Vec<_> = rows.par_iter().map(|row| row.quantity).collect();
    let accepted = game_lot_allocation::choose(&quantities, available);
    let debit: u64 = rows
        .par_iter()
        .zip(accepted.par_iter())
        .filter(|(_, accepted)| **accepted)
        .map(|(row, _)| row.quantity)
        .sum();
    let mut outcomes: Vec<_> = rows
        .par_iter()
        .zip(accepted.par_iter())
        .map(|(row, accepted)| (row.movement, *accepted))
        .collect();
    outcomes.par_sort_unstable_by_key(|row| row.0);
    let outcome_chunks: Vec<_> = outcomes
        .par_chunks(2048)
        .map(|chunk| OutcomeChunk {
            headers: chunk
                .iter()
                .map(|&(id, accepted)| (id, accepted, 0..0))
                .collect(),
            lots: Vec::new(),
        })
        .collect();
    let mut transaction_outcomes: Vec<_> = rows
        .par_iter()
        .zip(accepted.par_iter())
        .map(|(row, accepted)| TransactionOutcome {
            id: row.transaction,
            accepted: *accepted,
        })
        .collect();
    transaction_outcomes.par_sort_unstable_by_key(|row| row.id);
    let mut spoiled: Vec<_> = state
        .containers
        .par_iter()
        .filter(|container| container.next_expiry.is_some_and(|at| at <= next))
        .flat_map_iter(|container| {
            container.lots.iter().filter_map(move |lot| {
                lot.expires_at.filter(|at| *at <= next).map(|at| Spoilage {
                    place: container.place,
                    good: lot.good,
                    quantity: lot.quantity,
                    expires_at: at,
                })
            })
        })
        .collect();
    spoiled.par_sort_unstable_by_key(|row| (row.place, row.good, row.expires_at));
    let changes: Vec<_> = state
        .containers
        .par_iter()
        .enumerate()
        .filter_map(|(index, container)| {
            if index == source {
                (debit > 0 || container.next_expiry.is_some_and(|at| at <= next))
                    .then(|| (index, after(container, next, first.good, debit)))
            } else {
                container
                    .next_expiry
                    .is_some_and(|at| at <= next)
                    .then(|| (index, after(container, next, 0, 0)))
            }
        })
        .collect();
    Ok(Some(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes: changes.into(),
        outcomes: OutcomeBatch::from_chunks(outcome_chunks),
        spoiled,
        creations: Vec::new(),
        transactions: transaction_outcomes,
    }))
}
