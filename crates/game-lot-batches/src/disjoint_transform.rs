//! No-time fungible transformations over disjoint source and destination custody.
use crate::independent_transfer;
use game_lot_model::{Lot, OutcomeBatch, OutcomeChunk, State, Transaction, TransactionOutcome};
use game_lot_planning::{Changes, Draft};
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Row {
    source: usize,
    destination: usize,
    transaction: usize,
}

/// The caller must have excluded identified items in touched custody.
pub fn prepare(
    state: &State,
    instance: u64,
    transactions: &[Transaction],
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128
        || transactions.par_iter().any(|transaction| {
            if transaction.id == 0
                || transaction.movements.len() != 1
                || transaction.creations.len() != 1
            {
                return true;
            }
            let movement = transaction.movements[0];
            let creation = &transaction.creations[0];
            movement.id == 0
                || movement.item.is_some()
                || movement.to.is_some()
                || movement.quantity == 0
                || !movement.from.accepts(movement.good)
                || creation.id == 0
                || creation.lot.item.is_some()
                || creation.lot.batch == Some(0)
                || creation.lot.quantity == 0
                || creation.lot.expires_at.is_some()
                || !creation.to.accepts(creation.lot.good)
                || movement.from == creation.to
        })
    {
        return Ok(None);
    }
    if state
        .containers
        .par_iter()
        .any(|container| container.next_expiry.is_some_and(|at| at <= state.tick))
    {
        return Ok(None);
    }
    let mut ids: Vec<_> = transactions.par_iter().map(|row| row.id).collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    state
        .revision
        .checked_add(1)
        .ok_or("lot revision overflow")?;
    let mut operations: Vec<_> = transactions
        .par_iter()
        .flat_map_iter(|row| [row.movements[0].id, row.creations[0].id])
        .collect();
    if operations.par_windows(2).any(|pair| pair[0] > pair[1]) {
        operations.par_sort_unstable();
    }
    if operations.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(None);
    }
    let rows: Option<Vec<_>> = transactions
        .par_iter()
        .enumerate()
        .map(|(transaction, row)| {
            let source = state
                .containers
                .binary_search_by_key(&row.movements[0].from, |container| container.place)
                .ok()?;
            let destination = state
                .containers
                .binary_search_by_key(&row.creations[0].to, |container| container.place)
                .ok()?;
            if state.containers[source]
                .lots
                .iter()
                .any(|lot| lot.item.is_some())
                || state.containers[destination]
                    .lots
                    .iter()
                    .any(|lot| lot.item.is_some())
            {
                return None;
            }
            Some(Row {
                source,
                destination,
                transaction,
            })
        })
        .collect();
    let Some(mut rows) = rows else {
        return Ok(None);
    };
    if rows
        .par_windows(2)
        .any(|pair| pair[0].source >= pair[1].source)
    {
        rows.par_sort_unstable_by_key(|row| row.source);
    }
    if rows
        .par_windows(2)
        .any(|pair| pair[0].source >= pair[1].source || pair[0].destination >= pair[1].destination)
    {
        return Ok(None);
    }
    let Some((first, last)) = rows.first().zip(rows.last()) else {
        return Ok(None);
    };
    let source_first = if last.source < first.destination {
        true
    } else if last.destination < first.source {
        false
    } else {
        return Ok(None);
    };
    let chunk_size = rows.len().div_ceil(64).clamp(16, 2048);
    let parts: Vec<_> = rows
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut lots = Vec::with_capacity(chunk.len());
            let mut headers = Vec::with_capacity(chunk.len());
            let mut txs = Vec::with_capacity(chunk.len());
            let mut creations = Vec::with_capacity(chunk.len());
            let mut sources = Vec::with_capacity(chunk.len());
            let mut destinations = Vec::with_capacity(chunk.len());
            for row in chunk {
                let transaction = &transactions[row.transaction];
                let movement = transaction.movements[0];
                let creation = &transaction.creations[0];
                let source = &state.containers[row.source];
                let destination = &state.containers[row.destination];
                let available: u128 = source
                    .lots
                    .iter()
                    .filter(|lot| lot.good == movement.good)
                    .map(|lot| u128::from(lot.quantity))
                    .sum();
                let accepted = available >= u128::from(movement.quantity)
                    && creation.lot.quantity <= destination.capacity - destination.used;
                let start = lots.len();
                let mut remaining = if accepted { movement.quantity } else { 0 };
                for lot in &source.lots {
                    if remaining == 0 {
                        break;
                    }
                    if lot.good == movement.good {
                        let quantity = remaining.min(lot.quantity);
                        lots.push(Lot {
                            quantity,
                            ..lot.clone()
                        });
                        remaining -= quantity;
                    }
                }
                debug_assert_eq!(remaining, 0);
                if accepted {
                    sources.push((
                        row.source,
                        independent_transfer::after(
                            source,
                            state.tick,
                            movement.good,
                            movement.quantity,
                            &[],
                        ),
                    ));
                    destinations.push((
                        row.destination,
                        independent_transfer::after(
                            destination,
                            state.tick,
                            0,
                            0,
                            std::slice::from_ref(&creation.lot),
                        ),
                    ));
                }
                headers.push((movement.id, accepted, start..lots.len()));
                txs.push(TransactionOutcome {
                    id: transaction.id,
                    accepted,
                });
                creations.push((creation.id, accepted));
            }
            (
                OutcomeChunk { headers, lots },
                txs,
                creations,
                sources,
                destinations,
            )
        })
        .collect();
    let (outcome_parts, rest): (Vec<_>, Vec<_>) = parts
        .into_par_iter()
        .map(|(outcomes, txs, creations, sources, destinations)| {
            (outcomes, (txs, creations, sources, destinations))
        })
        .unzip();
    let (tx_parts, rest): (Vec<_>, Vec<_>) = rest
        .into_par_iter()
        .map(|(txs, creations, sources, destinations)| (txs, (creations, sources, destinations)))
        .unzip();
    let (creation_parts, rest): (Vec<_>, Vec<_>) = rest
        .into_par_iter()
        .map(|(creations, sources, destinations)| (creations, (sources, destinations)))
        .unzip();
    let (mut source_parts, mut destination_parts): (Vec<_>, Vec<_>) = rest.into_par_iter().unzip();
    let mut outcomes = OutcomeBatch::from_chunks(outcome_parts);
    outcomes.sort_by_id();
    let mut txs: Vec<_> = tx_parts.into_par_iter().flatten().collect();
    if txs.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        txs.par_sort_unstable_by_key(|row| row.id);
    }
    let mut creations: Vec<_> = creation_parts.into_par_iter().flatten().collect();
    if creations.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        creations.par_sort_unstable_by_key(|row| row.0);
    }
    let changes = if source_first {
        source_parts.append(&mut destination_parts);
        Changes::ordered_runs(source_parts)
    } else {
        destination_parts.append(&mut source_parts);
        Changes::ordered_runs(destination_parts)
    };
    Ok(Some(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: state.tick,
        changes,
        outcomes,
        spoiled: vec![],
        creations,
        transactions: txs,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_lot_model::{Container, Creation, Movement, Place};

    fn fixture() -> (State, Vec<Transaction>) {
        let mut containers = Vec::new();
        let mut transactions = Vec::new();
        for id in 1..=256u64 {
            let source = Place::JobInput(id);
            containers.push(
                Container::new(
                    source,
                    10,
                    vec![
                        Lot {
                            batch: None,
                            good: 1,
                            quantity: 1,
                            item: None,
                            expires_at: Some(10),
                        },
                        Lot {
                            batch: None,
                            good: 1,
                            quantity: 2,
                            item: None,
                            expires_at: None,
                        },
                    ],
                )
                .unwrap(),
            );
            transactions.push(Transaction {
                id,
                movements: vec![Movement {
                    id: id * 2 - 1,
                    from: source,
                    to: None,
                    good: 1,
                    quantity: if id % 4 == 0 { 4 } else { 2 },
                    item: None,
                }],
                creations: vec![Creation {
                    id: id * 2,
                    to: Place::JobOutput(id),
                    lot: Lot {
                        batch: None,
                        good: 2,
                        quantity: if id % 4 == 1 { 5 } else { 1 },
                        item: None,
                        expires_at: None,
                    },
                }],
            });
        }
        for id in 1..=256u64 {
            containers.push(
                Container::new(
                    Place::JobOutput(id),
                    5,
                    vec![Lot {
                        batch: None,
                        good: 2,
                        quantity: 1,
                        item: None,
                        expires_at: None,
                    }],
                )
                .unwrap(),
            );
        }
        (
            State {
                tick: 0,
                revision: 0,
                containers,
            },
            transactions,
        )
    }
    type Signature = (
        Vec<(usize, Container)>,
        Vec<game_lot_model::Outcome>,
        Vec<(u64, bool)>,
        Vec<TransactionOutcome>,
    );
    fn signature(draft: Draft) -> Signature {
        (
            draft.changes.into_flat(),
            draft.outcomes.iter().map(|row| row.to_owned()).collect(),
            draft.creations,
            draft.transactions,
        )
    }
    #[test]
    fn disjoint_transformations_match_general_arbiter_across_workers() {
        let (state, actions) = fixture();
        for reversed in [false, true] {
            let mut actions = actions.clone();
            if reversed {
                actions.reverse();
            }
            for workers in [1, 4, 6] {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap()
                    .install(|| {
                        let fast = prepare(&state, 1, &actions).unwrap().unwrap();
                        let reference = crate::prepare_unpartitioned_validated(
                            &state,
                            1,
                            7,
                            &actions,
                            false,
                            vec![],
                            &[],
                        )
                        .unwrap()
                        .0;
                        assert_eq!(signature(fast), signature(reference));
                    });
            }
        }
    }
    #[test]
    fn destination_range_before_source_matches_general_arbiter() {
        let (mut state, mut actions) = fixture();
        let swap = |place| match place {
            Place::JobInput(id) => Place::JobOutput(id),
            Place::JobOutput(id) => Place::JobInput(id),
            other => other,
        };
        for container in &mut state.containers {
            container.place = swap(container.place);
        }
        state.containers.sort_unstable_by_key(|row| row.place);
        for action in &mut actions {
            action.movements[0].from = swap(action.movements[0].from);
            action.creations[0].to = swap(action.creations[0].to);
        }
        let fast = prepare(&state, 1, &actions).unwrap().unwrap();
        let reference =
            crate::prepare_unpartitioned_validated(&state, 1, 7, &actions, false, vec![], &[])
                .unwrap()
                .0;
        assert_eq!(signature(fast), signature(reference));
    }
    #[test]
    fn disjoint_transformations_fall_back_for_unsafe_inputs() {
        let (state, actions) = fixture();
        assert!(prepare(&state, 1, &actions[..20]).unwrap().is_none());
        let mut overlap = actions.clone();
        overlap[0].creations[0].to = Place::JobInput(2);
        assert!(prepare(&state, 1, &overlap).unwrap().is_none());
        let mut duplicate = actions.clone();
        duplicate[0].creations[0].id = duplicate[1].movements[0].id;
        assert!(prepare(&state, 1, &duplicate).unwrap().is_none());
        let mut duplicate_transaction = actions.clone();
        duplicate_transaction[0].id = duplicate_transaction[1].id;
        let mut full_revision = state.clone();
        full_revision.revision = u64::MAX;
        assert!(
            prepare(&full_revision, 1, &duplicate_transaction)
                .unwrap()
                .is_none()
        );
        let mut perishable = actions.clone();
        perishable[0].creations[0].lot.expires_at = Some(10);
        assert!(prepare(&state, 1, &perishable).unwrap().is_none());
        let mut expired = state.clone();
        expired.tick = 10;
        assert!(prepare(&expired, 1, &actions).unwrap().is_none());
        let mut identified = state;
        identified.containers[0] = Container::new(
            Place::JobInput(1),
            10,
            vec![Lot {
                batch: None,
                good: 1,
                quantity: 1,
                item: Some(5),
                expires_at: None,
            }],
        )
        .unwrap();
        assert!(prepare(&identified, 1, &actions).unwrap().is_none());
    }
}
