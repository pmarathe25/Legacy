//! Disjoint single-leg transactions can resolve without shared arbitration.
use game_parallel_lot_model::{
    Container, Lot, Movement, Outcome, Place, SimpleTransfer, Spoilage, State, Transaction,
    TransactionOutcome, key,
};
use game_parallel_lot_planning::Draft;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Row {
    source: usize,
    destination: usize,
    transaction: u64,
    movement: Movement,
}

fn after(
    container: &Container,
    tick: u64,
    good: u32,
    mut debit: u64,
    incoming: &[Lot],
) -> Container {
    let mut lots: Vec<_> = container
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
    for lot in incoming {
        match lots.binary_search_by_key(&key(lot), key) {
            Ok(index) => lots[index].quantity += lot.quantity,
            Err(index) => lots.insert(index, lot.clone()),
        }
    }
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
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128
        || transactions.par_iter().any(|t| {
            t.movements.len() != 1 || !t.creations.is_empty() || {
                let m = t.movements[0];
                m.id == 0
                    || m.quantity == 0
                    || !m.from.accepts(m.good)
                    || m.to.is_none_or(|to| to == m.from || !to.accepts(m.good))
            }
        })
    {
        return Ok(None);
    }
    prepare_rows(
        state,
        instance,
        transactions.len(),
        |index| {
            let transaction = &transactions[index];
            (transaction.id, transaction.movements[0])
        },
        advance,
    )
}

/// Validate action identities before attempting the general flat transfer path.
pub fn prepare_flat(
    state: &State,
    instance: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    let mut ids: Vec<_> = transfers.par_iter().map(|row| row.action).collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.first() == Some(&0) || ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("invalid or duplicate transaction id".into());
    }
    if transfers.len() < 128
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
    prepare_rows(
        state,
        instance,
        transfers.len(),
        |index| (transfers[index].action, transfers[index].movement),
        advance,
    )
}

fn prepare_rows(
    state: &State,
    instance: u64,
    length: usize,
    action: impl Fn(usize) -> (u64, Movement) + Sync,
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
    let mut phase = game_diagnostics::span("physical.independent.locate");
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
    let rows: Option<Vec<_>> = (0..length)
        .into_par_iter()
        .map(|index| {
            let (transaction, movement) = action(index);
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
        phase = game_diagnostics::span("physical.independent.validate");
    }
    let mut endpoints: Vec<_> = rows
        .par_iter()
        .flat_map_iter(|r| [r.source, r.destination])
        .collect();
    endpoints.par_sort_unstable();
    if endpoints.par_windows(2).any(|p| p[0] == p[1]) {
        return Ok(None);
    }
    let mut movement_ids: Vec<_> = rows.par_iter().map(|r| r.movement.id).collect();
    if movement_ids.par_windows(2).any(|p| p[0] > p[1]) {
        movement_ids.par_sort_unstable();
    }
    if movement_ids.par_windows(2).any(|p| p[0] == p[1]) {
        return Ok(None);
    }

    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.spoilage");
    }
    let mut spoiled: Vec<_> = state
        .containers
        .par_iter()
        .filter(|c| c.next_expiry.is_some_and(|at| at <= next))
        .flat_map_iter(|c| {
            c.lots.iter().filter_map(move |lot| {
                lot.expires_at.filter(|&at| at <= next).map(|at| Spoilage {
                    place: c.place,
                    good: lot.good,
                    quantity: lot.quantity,
                    expires_at: at,
                })
            })
        })
        .collect();
    spoiled.par_sort_unstable_by_key(|row| (row.place, row.good, row.expires_at));

    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.resolve_and_group");
    }
    let parts: Vec<_> = rows
        .into_par_iter()
        .with_min_len(2048)
        .map(|row| {
            let source = &state.containers[row.source];
            let destination = &state.containers[row.destination];
            let live_used: u64 = destination
                .lots
                .iter()
                .filter(|lot| lot.expires_at.is_none_or(|at| at > next))
                .map(|lot| lot.quantity)
                .sum();
            let available: u64 = source
                .lots
                .iter()
                .filter(|lot| {
                    lot.good == row.movement.good && lot.expires_at.is_none_or(|at| at > next)
                })
                .map(|lot| lot.quantity)
                .sum();
            let accepted = row.movement.quantity <= available
                && row.movement.quantity <= destination.capacity - live_used;
            let mut remaining = if accepted { row.movement.quantity } else { 0 };
            let mut moved = Vec::new();
            for lot in &source.lots {
                if remaining == 0 {
                    break;
                }
                if lot.good == row.movement.good && lot.expires_at.is_none_or(|at| at > next) {
                    let quantity = remaining.min(lot.quantity);
                    moved.push(Lot {
                        good: lot.good,
                        quantity,
                        expires_at: lot.expires_at,
                    });
                    remaining -= quantity;
                }
            }
            debug_assert_eq!(remaining, 0);
            let source_change =
                (accepted || source.next_expiry.is_some_and(|at| at <= next)).then(|| {
                    (
                        row.source,
                        after(
                            source,
                            next,
                            row.movement.good,
                            if accepted { row.movement.quantity } else { 0 },
                            &[],
                        ),
                    )
                });
            let destination_change = (accepted
                || destination.next_expiry.is_some_and(|at| at <= next))
            .then(|| (row.destination, after(destination, next, 0, 0, &moved)));
            (
                Outcome {
                    id: row.movement.id,
                    accepted,
                    lots: moved,
                },
                TransactionOutcome {
                    id: row.transaction,
                    accepted,
                },
                source_change,
                destination_change,
            )
        })
        .fold(
            || (Vec::new(), Vec::new(), Vec::new()),
            |(mut outcomes, mut transactions, mut changes),
             (outcome, transaction, source, destination)| {
                outcomes.push(outcome);
                transactions.push(transaction);
                changes.extend(source);
                changes.extend(destination);
                (outcomes, transactions, changes)
            },
        )
        .collect();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.extract");
    }
    let (outcome_parts, rest): (Vec<_>, Vec<_>) = parts
        .into_par_iter()
        .map(|(outcomes, transactions, changes)| (outcomes, (transactions, changes)))
        .unzip();
    let (transaction_parts, change_parts): (Vec<_>, Vec<_>) = rest.into_par_iter().unzip();
    let (mut outcomes, (mut transaction_outcomes, mut changes)): (Vec<_>, (Vec<_>, Vec<_>)) =
        rayon::join(
            || outcome_parts.into_par_iter().flatten().collect(),
            || {
                rayon::join(
                    || transaction_parts.into_par_iter().flatten().collect(),
                    || change_parts.into_par_iter().flatten().collect(),
                )
            },
        );
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.untouched_expiry");
    }
    changes.par_extend(
        state
            .containers
            .par_iter()
            .enumerate()
            .filter_map(|(index, container)| {
                (container.next_expiry.is_some_and(|at| at <= next)
                    && endpoints.binary_search(&index).is_err())
                .then(|| (index, after(container, next, 0, 0, &[])))
            }),
    );
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.sort");
    }
    changes.par_sort_unstable_by_key(|row| row.0);
    if outcomes.par_windows(2).any(|p| p[0].id > p[1].id) {
        outcomes.par_sort_unstable_by_key(|row| row.id);
    }
    if transaction_outcomes
        .par_windows(2)
        .any(|p| p[0].id > p[1].id)
    {
        transaction_outcomes.par_sort_unstable_by_key(|row| row.id);
    }
    #[cfg(feature = "profile")]
    drop(phase);
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
