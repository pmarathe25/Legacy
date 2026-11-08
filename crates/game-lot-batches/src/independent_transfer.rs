//! Disjoint single-leg transactions can resolve without shared arbitration.
use game_lot_model::{
    Container, Lot, Movement, OutcomeBatch, OutcomeChunk, Place, SimpleTransfer, Spoilage, State,
    Transaction, TransactionOutcome, key,
};
use game_lot_planning::{Changes, Draft};
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Row {
    source: usize,
    destination: usize,
    transaction: u64,
    movement: Movement,
}

pub(crate) fn after(
    container: &Container,
    tick: u64,
    good: u32,
    mut debit: u64,
    incoming: &[Lot],
) -> Container {
    let surviving = container.lots.iter().filter_map(|lot| {
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
            item: lot.item,
            good: lot.good,
            quantity: lot.quantity - consumed,
            expires_at: lot.expires_at,
        })
    });
    let lots: Vec<_> = if incoming.is_empty() {
        surviving.collect()
    } else {
        // Both inputs are already sorted and have unique cohort keys. Merge
        // once so many dated lots do not require repeated vector insertions.
        let mut existing = surviving.peekable();
        let mut additions = incoming.iter().peekable();
        let mut output = Vec::with_capacity(container.lots.len().saturating_add(incoming.len()));
        loop {
            match (existing.peek(), additions.peek()) {
                (Some(old), Some(new)) => match key(old).cmp(&key(new)) {
                    std::cmp::Ordering::Less => output.push(existing.next().unwrap()),
                    std::cmp::Ordering::Greater => {
                        output.push((*additions.next().unwrap()).clone())
                    }
                    std::cmp::Ordering::Equal => {
                        let mut lot = existing.next().unwrap();
                        lot.quantity += additions.next().unwrap().quantity;
                        output.push(lot);
                    }
                },
                (Some(_), None) => {
                    output.extend(existing);
                    break;
                }
                (None, Some(_)) => {
                    output.extend(additions.cloned());
                    break;
                }
                (None, None) => break,
            }
        }
        output
    };
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
    // A batch with one shared endpoint cannot use the disjoint path. Detect it
    // before locating endpoints; another planner validates and resolves it.
    let first = action(0).1;
    let second = action(1).1;
    if (first.to == second.to
        && (2..length)
            .into_par_iter()
            .all(|index| action(index).1.to == first.to))
        || (first.from == second.from
            && (2..length)
                .into_par_iter()
                .all(|index| action(index).1.from == first.from))
    {
        return Ok(None);
    }
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
    // When both ordered endpoint ranges are disjoint, their concatenation is
    // already canonical and proves exclusivity without a global sort.
    let separated = rows.first().zip(rows.last()).and_then(|(first, last)| {
        let source_first = if last.source < first.destination {
            Some(true)
        } else if last.destination < first.source {
            Some(false)
        } else {
            None
        }?;
        rows.par_windows(2)
            .all(|pair| {
                pair[0].source < pair[1].source && pair[0].destination < pair[1].destination
            })
            .then_some(source_first)
    });
    let endpoints = if separated.is_some() {
        // Both columns are sorted and disjoint already. Keep them in place
        // instead of allocating a second copy of every endpoint.
        None
    } else {
        let mut endpoints: Vec<_> = rows
            .par_iter()
            .flat_map_iter(|r| [r.source, r.destination])
            .collect();
        endpoints.par_sort_unstable();
        if endpoints.par_windows(2).any(|p| p[0] == p[1]) {
            return Ok(None);
        }
        Some(endpoints)
    };
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
    // Each logical chunk owns one packed arena. Receipts retain immutable
    // ranges into it instead of allocating a lot vector for every movement.
    // Keep a fixed logical partition independent of worker count while
    // exposing parallelism for smaller batches with expensive lot cohorts.
    let chunk_size = rows.len().div_ceil(64).clamp(16, 2048);
    let parts: Vec<_> = rows
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut lots = Vec::with_capacity(chunk.len());
            let mut headers = Vec::with_capacity(chunk.len());
            let mut transactions = Vec::with_capacity(chunk.len());
            let mut sources = Vec::with_capacity(chunk.len());
            let mut destinations = Vec::with_capacity(chunk.len());
            for row in chunk {
                let source = &state.containers[row.source];
                let destination = &state.containers[row.destination];
                let live_used: u64 = if destination.next_expiry.is_none_or(|at| at > next) {
                    destination.used
                } else {
                    destination
                        .lots
                        .iter()
                        .filter(|lot| lot.expires_at.is_none_or(|at| at > next))
                        .map(|lot| lot.quantity)
                        .sum()
                };
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
                let begin = lots.len();
                for lot in &source.lots {
                    if remaining == 0 {
                        break;
                    }
                    if lot.good == row.movement.good && lot.expires_at.is_none_or(|at| at > next) {
                        let quantity = remaining.min(lot.quantity);
                        lots.push(Lot {
                            item: lot.item,
                            good: lot.good,
                            quantity,
                            expires_at: lot.expires_at,
                        });
                        remaining -= quantity;
                    }
                }
                debug_assert_eq!(remaining, 0);
                let end = lots.len();
                if accepted || source.next_expiry.is_some_and(|at| at <= next) {
                    sources.push((
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
                if accepted || destination.next_expiry.is_some_and(|at| at <= next) {
                    destinations.push((
                        row.destination,
                        after(destination, next, 0, 0, &lots[begin..end]),
                    ));
                }
                headers.push((row.movement.id, accepted, begin..end));
                transactions.push(TransactionOutcome {
                    id: row.transaction,
                    accepted,
                });
            }
            (
                OutcomeChunk { headers, lots },
                transactions,
                sources,
                destinations,
            )
        })
        .collect();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.extract");
    }
    let (outcome_parts, rest): (Vec<_>, Vec<_>) = parts
        .into_par_iter()
        .map(|(outcomes, transactions, sources, destinations)| {
            (outcomes, (transactions, sources, destinations))
        })
        .unzip();
    let (transaction_parts, rest): (Vec<_>, Vec<_>) = rest
        .into_par_iter()
        .map(|(transactions, sources, destinations)| (transactions, (sources, destinations)))
        .unzip();
    let (mut source_parts, mut destination_parts): (Vec<_>, Vec<_>) = rest.into_par_iter().unzip();
    let (mut outcomes, mut transaction_outcomes): (OutcomeBatch, Vec<_>) = rayon::join(
        || OutcomeBatch::from_chunks(outcome_parts),
        || transaction_parts.into_par_iter().flatten().collect(),
    );
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.untouched_expiry");
    }
    let extra: Vec<_> = if spoiled.is_empty() {
        Vec::new()
    } else {
        state
            .containers
            .par_iter()
            .enumerate()
            .filter_map(|(index, container)| {
                (container.next_expiry.is_some_and(|at| at <= next)
                    && endpoints.as_ref().map_or_else(
                        || {
                            rows.binary_search_by_key(&index, |row| row.source).is_err()
                                && rows
                                    .binary_search_by_key(&index, |row| row.destination)
                                    .is_err()
                        },
                        |endpoints| endpoints.binary_search(&index).is_err(),
                    ))
                .then(|| (index, after(container, next, 0, 0, &[])))
            })
            .collect()
    };
    let already_ordered = separated.is_some() && extra.is_empty();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.independent.sort");
    }
    let changes = if already_ordered {
        if separated == Some(false) {
            destination_parts.append(&mut source_parts);
            Changes::ordered_runs(destination_parts)
        } else {
            source_parts.append(&mut destination_parts);
            Changes::ordered_runs(source_parts)
        }
    } else {
        let (mut sources, destinations): (Vec<_>, Vec<_>) = rayon::join(
            || source_parts.into_par_iter().flatten().collect(),
            || destination_parts.into_par_iter().flatten().collect(),
        );
        sources.par_extend(destinations);
        sources.par_extend(extra);
        sources.par_sort_unstable_by_key(|row| row.0);
        sources.into()
    };
    outcomes.sort_by_id();
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
