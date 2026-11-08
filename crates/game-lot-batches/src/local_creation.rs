//! Isolated local transformations retain exact receipts without global arbitration.
use game_lot_model::{
    Container, Lot, Outcome, OutcomeBatch, State, Transaction, TransactionOutcome,
};
use game_lot_planning::Draft;
use rayon::prelude::*;

pub fn prepare(
    state: &State,
    instance: u64,
    transactions: &[Transaction],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128 {
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
    // Expiration and unusual operations use the general implementation.
    if state
        .containers
        .par_iter()
        .any(|row| row.next_expiry.is_some_and(|at| at <= next))
    {
        return Ok(None);
    }
    let rows: Option<Vec<_>> = transactions
        .par_iter()
        .map(|transaction| {
            if transaction.id == 0
                || transaction.movements.len() != 2
                || transaction.creations.len() != 1
            {
                return None;
            }
            let [first, second] = transaction.movements.as_slice() else {
                return None;
            };
            let creation = &transaction.creations[0];
            if first.id == 0
                || second.id == 0
                || creation.id == 0
                || first.from != second.from
                || first.from != creation.to
                || first.to.is_some()
                || second.to.is_some()
                || first.item.is_some()
                || second.item.is_some()
                || first.good == second.good
                || first.quantity == 0
                || second.quantity == 0
                || !first.from.accepts(first.good)
                || !second.from.accepts(second.good)
                || !creation.to.accepts(creation.lot.good)
                || creation.lot.quantity != 1
                || creation.lot.item.is_none_or(|id| id == 0)
                || creation.lot.expires_at.is_some()
            {
                return None;
            }
            let source = state
                .containers
                .binary_search_by_key(&first.from, |row| row.place)
                .ok()?;
            let holder = &state.containers[source];
            if holder.lots.iter().any(|lot| {
                lot.item.is_some() && (lot.good == first.good || lot.good == second.good)
            }) {
                return None;
            }
            Some((source, transaction))
        })
        .collect();
    let Some(mut rows) = rows else {
        return Ok(None);
    };
    if rows.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        rows.par_sort_unstable_by_key(|row| row.0);
    }
    if rows.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Ok(None);
    }
    let mut transaction_ids: Vec<_> = rows.par_iter().map(|row| row.1.id).collect();
    if transaction_ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        transaction_ids.par_sort_unstable();
    }
    if transaction_ids
        .par_windows(2)
        .any(|pair| pair[0] == pair[1])
    {
        return Ok(None);
    }
    let mut operation_ids: Vec<_> = rows
        .par_iter()
        .flat_map_iter(|row| {
            [
                row.1.movements[0].id,
                row.1.movements[1].id,
                row.1.creations[0].id,
            ]
        })
        .collect();
    if operation_ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        operation_ids.par_sort_unstable();
    }
    if operation_ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }

    let resolved: Result<Vec<_>, String> = rows
        .par_iter()
        .map(|(index, transaction)| {
            let old = &state.containers[*index];
            let [first, second] = transaction.movements.as_slice() else {
                unreachable!()
            };
            let available = |good| {
                old.lots
                    .iter()
                    .filter(|lot| lot.good == good)
                    .map(|lot| u128::from(lot.quantity))
                    .sum::<u128>()
            };
            let accepted = available(first.good) >= u128::from(first.quantity)
                && available(second.good) >= u128::from(second.quantity)
                && u128::from(old.used) - u128::from(first.quantity) - u128::from(second.quantity)
                    < u128::from(old.capacity);
            let mut remaining = [first.quantity, second.quantity];
            let mut receipts = [Vec::new(), Vec::new()];
            let mut lots = Vec::new();
            if accepted {
                for lot in &old.lots {
                    let leg = if lot.good == first.good {
                        Some(0)
                    } else if lot.good == second.good {
                        Some(1)
                    } else {
                        None
                    };
                    let consumed = leg.map_or(0, |i| remaining[i].min(lot.quantity));
                    if let Some(i) = leg
                        && consumed != 0
                    {
                        remaining[i] -= consumed;
                        receipts[i].push(Lot {
                            quantity: consumed,
                            ..lot.clone()
                        });
                    }
                    if consumed < lot.quantity {
                        lots.push(Lot {
                            quantity: lot.quantity - consumed,
                            ..lot.clone()
                        });
                    }
                }
                lots.push(transaction.creations[0].lot.clone());
            }
            let change = if accepted {
                Some((*index, Container::new(old.place, old.capacity, lots)?))
            } else {
                None
            };
            let [a, b] = receipts;
            Ok((
                change,
                [
                    Outcome {
                        id: first.id,
                        accepted,
                        lots: a.into(),
                    },
                    Outcome {
                        id: second.id,
                        accepted,
                        lots: b.into(),
                    },
                ],
                (transaction.creations[0].id, accepted),
                TransactionOutcome {
                    id: transaction.id,
                    accepted,
                },
            ))
        })
        .collect();
    let resolved = resolved?;
    let changes = resolved
        .par_iter()
        .filter_map(|row| row.0.clone())
        .collect::<Vec<_>>();
    let mut outcomes: Vec<_> = resolved
        .par_iter()
        .flat_map_iter(|row| row.1.iter().cloned())
        .collect();
    if outcomes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        outcomes.par_sort_unstable_by_key(|row| row.id);
    }
    let mut creations: Vec<_> = resolved.par_iter().map(|row| row.2).collect();
    if creations.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        creations.par_sort_unstable_by_key(|row| row.0);
    }
    let mut transactions: Vec<_> = resolved.par_iter().map(|row| row.3).collect();
    if transactions
        .par_windows(2)
        .any(|pair| pair[0].id > pair[1].id)
    {
        transactions.par_sort_unstable_by_key(|row| row.id);
    }
    Ok(Some(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes: changes.into(),
        outcomes: OutcomeBatch::from_outcomes(outcomes),
        spoiled: Vec::new(),
        creations,
        transactions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_lot_model::{Creation, Movement, Place};

    #[test]
    fn isolated_creation_matches_general_arbiter_for_mixed_success_and_cohorts() {
        let mut containers = Vec::new();
        let mut actions = Vec::new();
        for id in 1..=256 {
            let place = Place::Person(id);
            let lots = vec![
                Lot {
                    good: 1,
                    quantity: 1,
                    item: None,
                    expires_at: Some(10),
                },
                Lot {
                    good: 1,
                    quantity: 2,
                    item: None,
                    expires_at: Some(20),
                },
                Lot {
                    good: 2,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                },
                Lot {
                    good: 3,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                },
            ];
            containers.push(Container::new(place, 10, lots).unwrap());
            actions.push(Transaction {
                id,
                movements: vec![
                    Movement {
                        id: id * 3 - 2,
                        from: place,
                        to: None,
                        good: 1,
                        quantity: 2,
                        item: None,
                    },
                    Movement {
                        id: id * 3 - 1,
                        from: place,
                        to: None,
                        good: 2,
                        quantity: if id % 3 == 0 { 2 } else { 1 },
                        item: None,
                    },
                ],
                creations: vec![Creation {
                    id: id * 3,
                    to: place,
                    lot: Lot {
                        good: 3,
                        quantity: 1,
                        item: Some(id + 10_000),
                        expires_at: None,
                    },
                }],
            });
        }
        let state = State {
            tick: 0,
            revision: 0,
            containers,
        };
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let fast = prepare(&state, 1, &actions, true).unwrap().unwrap();
                    let reference = crate::prepare_unpartitioned_validated(
                        &state,
                        1,
                        7,
                        &actions,
                        true,
                        vec![],
                        &[],
                    )
                    .unwrap()
                    .0;
                    assert_eq!(
                        fast.outcomes
                            .iter()
                            .map(|row| row.to_owned())
                            .collect::<Vec<_>>(),
                        reference
                            .outcomes
                            .iter()
                            .map(|row| row.to_owned())
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(fast.transactions, reference.transactions);
                    assert_eq!(fast.creations, reference.creations);
                    assert_eq!(fast.changes.into_flat(), reference.changes.into_flat());
                })
        };
        run(1);
        run(4);
        let mut expiring = state.clone();
        expiring.tick = 9;
        assert!(prepare(&expiring, 1, &actions, true).unwrap().is_none());
        let mut overlapping = actions.clone();
        let shared = overlapping[0].movements[0].from;
        overlapping[1].movements[0].from = shared;
        overlapping[1].movements[1].from = shared;
        overlapping[1].creations[0].to = shared;
        assert!(prepare(&state, 1, &overlapping, true).unwrap().is_none());
        let mut selected = actions.clone();
        selected[0].movements[0].item = Some(50);
        assert!(prepare(&state, 1, &selected, true).unwrap().is_none());
    }

    #[test]
    fn independent_identified_transfers_match_general_arbiter() {
        let count = 256u64;
        let mut containers = Vec::new();
        let mut actions = Vec::new();
        for id in 1..=count {
            let source = Place::Person(id);
            let destination = Place::Person(id + count);
            containers.push(
                Container::new(
                    source,
                    3,
                    vec![
                        Lot {
                            good: 1,
                            quantity: 1,
                            item: Some(id),
                            expires_at: None,
                        },
                        Lot {
                            good: 1,
                            quantity: 1,
                            item: None,
                            expires_at: Some(if id % 7 == 0 { 1 } else { 10 }),
                        },
                    ],
                )
                .unwrap(),
            );
            containers.push(
                Container::new(
                    destination,
                    2,
                    if id % 5 == 0 {
                        vec![Lot {
                            good: 2,
                            quantity: 2,
                            item: None,
                            expires_at: None,
                        }]
                    } else {
                        vec![]
                    },
                )
                .unwrap(),
            );
            actions.push(Transaction {
                id,
                movements: vec![Movement {
                    id,
                    from: source,
                    to: Some(destination),
                    good: 1,
                    quantity: if id % 3 == 0 { 2 } else { 1 },
                    item: None,
                }],
                creations: vec![],
            });
        }
        containers.sort_unstable_by_key(|row| row.place);
        let state = State {
            tick: 0,
            revision: 0,
            containers,
        };
        let fast = crate::prepare_independent_items(&state, 1, &actions, true)
            .unwrap()
            .unwrap();
        let reference =
            crate::prepare_unpartitioned_validated(&state, 1, 7, &actions, true, vec![], &[])
                .unwrap()
                .0;
        assert_eq!(
            fast.outcomes
                .iter()
                .map(|row| row.to_owned())
                .collect::<Vec<_>>(),
            reference
                .outcomes
                .iter()
                .map(|row| row.to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(fast.transactions, reference.transactions);
        assert_eq!(fast.spoiled, reference.spoiled);
        assert_eq!(fast.changes.into_flat(), reference.changes.into_flat());
        let mut exact = actions.clone();
        exact[0].movements[0].item = Some(1);
        assert!(
            crate::prepare_independent_items(&state, 1, &exact, true)
                .unwrap()
                .is_none()
        );
        let mut overlapping = actions;
        overlapping[1].movements[0].to = overlapping[0].movements[0].to;
        assert!(
            crate::prepare_independent_items(&state, 1, &overlapping, true)
                .unwrap()
                .is_none()
        );
    }
}
