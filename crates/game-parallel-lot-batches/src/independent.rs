//! Fast path for receipt-only consumptions with one independent source each.
use game_parallel_lot_model::{
    Container, Lot, Movement, Outcome, Spoilage, State, Transaction, TransactionOutcome,
};
use game_parallel_lot_planning::Draft;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Row {
    source: usize,
    transaction: u64,
    movement: Movement,
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
                good: lot.good,
                quantity: lot.quantity - consumed,
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
    transactions: &[Transaction],
    compact: &[u64],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128
        || compact.len() != transactions.len()
        || transactions.par_iter().any(|t| {
            t.movements.len() != 1
                || !t.creations.is_empty()
                || t.movements[0].to.is_some()
                || t.movements[0].id == 0
                || t.movements[0].quantity == 0
                || !t.movements[0].from.accepts(t.movements[0].good)
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
    let rows: Option<Vec<_>> = transactions
        .par_iter()
        .map(|t| {
            let movement = t.movements[0];
            state
                .containers
                .binary_search_by_key(&movement.from, |c| c.place)
                .ok()
                .map(|source| Row {
                    source,
                    transaction: t.id,
                    movement,
                })
        })
        .collect();
    let Some(mut rows) = rows else {
        return Ok(None);
    };
    if rows
        .par_windows(2)
        .any(|pair| pair[0].source > pair[1].source)
    {
        rows.par_sort_unstable_by_key(|row| row.source);
    }
    if rows
        .par_windows(2)
        .any(|pair| pair[0].source == pair[1].source)
    {
        return Ok(None);
    }
    let mut movement_ids: Vec<_> = rows.par_iter().map(|r| r.movement.id).collect();
    movement_ids.par_sort_unstable();
    if movement_ids.par_windows(2).any(|p| p[0] == p[1]) {
        return Ok(None);
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
    let resolved: Vec<_> = rows
        .par_iter()
        .map(|row| {
            let container = &state.containers[row.source];
            let available: u64 = container
                .lots
                .iter()
                .filter(|lot| {
                    lot.good == row.movement.good && lot.expires_at.is_none_or(|at| at > next)
                })
                .map(|lot| lot.quantity)
                .sum();
            let accepted = row.movement.quantity <= available;
            let changed = accepted || container.next_expiry.is_some_and(|at| at <= next);
            let change = changed.then(|| {
                (
                    row.source,
                    after(
                        container,
                        next,
                        row.movement.good,
                        if accepted { row.movement.quantity } else { 0 },
                    ),
                )
            });
            (
                Outcome {
                    id: row.movement.id,
                    accepted,
                    lots: Default::default(),
                },
                TransactionOutcome {
                    id: row.transaction,
                    accepted,
                },
                change,
            )
        })
        .collect();
    let mut changes: Vec<_> = resolved
        .par_iter()
        .filter_map(|row| row.2.clone())
        .collect();
    changes.par_extend(
        state
            .containers
            .par_iter()
            .enumerate()
            .filter_map(|(index, container)| {
                (container.next_expiry.is_some_and(|at| at <= next)
                    && rows.binary_search_by_key(&index, |row| row.source).is_err())
                .then(|| (index, after(container, next, 0, 0)))
            }),
    );
    changes.par_sort_unstable_by_key(|row| row.0);
    let mut outcomes: Vec<_> = resolved.par_iter().map(|row| row.0.clone()).collect();
    outcomes.par_sort_unstable_by_key(|row| row.id);
    let mut transaction_outcomes: Vec<_> = resolved.par_iter().map(|row| row.1).collect();
    transaction_outcomes.par_sort_unstable_by_key(|row| row.id);
    Ok(Some(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes,
        outcomes,
        spoiled,
        creations: Vec::new(),
        transactions: transaction_outcomes,
    }))
}
