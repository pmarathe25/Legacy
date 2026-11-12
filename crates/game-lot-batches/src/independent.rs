//! Fast path for consumptions with one independent source each.
use game_lot_model::{
    Container, Lot, Movement, Outcome, Spoilage, State, Transaction, TransactionOutcome,
};
use game_lot_planning::Draft;
use rayon::prelude::*;
use std::borrow::Borrow;

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
                batch: lot.batch,
                item: lot.item,
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

pub fn prepare<T: Borrow<Transaction> + Sync>(
    state: &State,
    instance: u64,
    transactions: &[T],
    compact: &[u64],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128
        || (!compact.is_empty() && compact.len() != transactions.len())
        || transactions.par_iter().any(|t| {
            let t = t.borrow();
            t.movements.len() != 1
                || !t.creations.is_empty()
                || t.movements[0].to.is_some()
                || t.movements[0].id == 0
                || t.movements[0].quantity == 0
                || t.movements[0].item.is_some()
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
            let t = t.borrow();
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
    if movement_ids.par_windows(2).any(|p| p[0] > p[1]) {
        movement_ids.par_sort_unstable();
    }
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
            let lots = if accepted && compact.is_empty() {
                let mut remaining = row.movement.quantity;
                container
                    .lots
                    .iter()
                    .filter_map(|lot| {
                        if lot.good != row.movement.good
                            || lot.expires_at.is_some_and(|at| at <= next)
                        {
                            return None;
                        }
                        let quantity = remaining.min(lot.quantity);
                        remaining -= quantity;
                        (quantity > 0).then(|| Lot {
                            quantity,
                            ..lot.clone()
                        })
                    })
                    .collect::<Vec<_>>()
                    .into()
            } else {
                Default::default()
            };
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
                    lots,
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
    if changes.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        changes.par_sort_unstable_by_key(|row| row.0);
    }
    let mut outcomes: Vec<_> = resolved.par_iter().map(|row| row.0.clone()).collect();
    if outcomes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        outcomes.par_sort_unstable_by_key(|row| row.id);
    }
    let mut transaction_outcomes: Vec<_> = resolved.par_iter().map(|row| row.1).collect();
    if transaction_outcomes
        .par_windows(2)
        .any(|pair| pair[0].id > pair[1].id)
    {
        transaction_outcomes.par_sort_unstable_by_key(|row| row.id);
    }
    Ok(Some(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes: changes.into(),
        outcomes: game_lot_model::OutcomeBatch::from_outcomes(outcomes),
        spoiled,
        creations: Vec::new(),
        transactions: transaction_outcomes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_lot_model::Place;

    #[test]
    fn full_independent_receipts_match_general_arbitration() {
        let mut containers: Vec<_> = (1..=256)
            .map(|id| {
                Container::new(
                    Place::Person(id),
                    10,
                    vec![
                        Lot {
                            batch: None,
                            item: None,
                            good: 1,
                            quantity: 1,
                            expires_at: Some(1),
                        },
                        Lot {
                            batch: None,
                            item: None,
                            good: 1,
                            quantity: 2,
                            expires_at: Some(3),
                        },
                        Lot {
                            batch: None,
                            item: None,
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        },
                    ],
                )
                .unwrap()
            })
            .collect();
        containers.push(
            Container::new(
                Place::Person(999),
                2,
                vec![Lot {
                    batch: None,
                    item: None,
                    good: 1,
                    quantity: 1,
                    expires_at: Some(1),
                }],
            )
            .unwrap(),
        );
        let state = State {
            next_batch: None,
            tick: 0,
            revision: 0,
            containers,
        };
        let actions: Vec<_> = (1..=256)
            .rev()
            .map(|id| Transaction {
                id,
                movements: vec![Movement {
                    batch: None,
                    id,
                    from: Place::Person(id),
                    to: None,
                    good: 1,
                    quantity: id % 4 + 1,
                    item: None,
                }],
                creations: vec![],
            })
            .collect();
        for workers in [1, 2, 4, 6] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    for advance in [false, true] {
                        let (fast, _) = crate::prepare_fungible_validated(
                            &state,
                            1,
                            7,
                            &actions,
                            advance,
                            vec![],
                            &[],
                        )
                        .unwrap();
                        let (general, _) = crate::prepare_unpartitioned_validated(
                            &state,
                            1,
                            7,
                            &actions,
                            advance,
                            vec![],
                            &[],
                        )
                        .unwrap();
                        assert_eq!(fast.transactions, general.transactions);
                        assert_eq!(fast.spoiled, general.spoiled);
                        assert_eq!(
                            fast.outcomes.iter().collect::<Vec<_>>(),
                            general.outcomes.iter().collect::<Vec<_>>()
                        );
                        assert_eq!(fast.changes.into_flat(), general.changes.into_flat());
                    }
                });
        }
        let mut shared = actions.clone();
        shared[0].movements[0].from = shared[1].movements[0].from;
        assert!(prepare(&state, 1, &shared, &[], false).unwrap().is_none());
        let mut missing = actions.clone();
        missing[0].movements[0].from = Place::Person(1000);
        assert!(prepare(&state, 1, &missing, &[], false).unwrap().is_none());
    }
}
