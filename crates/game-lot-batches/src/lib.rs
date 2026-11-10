//! Flatten transactions without changing their ownership or physical semantics.
use game_lot_model::{Container, SimpleTransfer, State, Transaction};
use game_lot_planning::{Batch, Draft, prefixes};
use rayon::prelude::*;
use std::borrow::Borrow;
mod compact_shared;
mod disjoint_transform;
mod fan_in;
mod fan_out;
mod grouped_sources;
mod independent;
mod independent_transfer;
mod local_creation;
mod partitioned;

/// Try no-time, disjoint fungible transformations after item validation.
pub fn prepare_disjoint_transformations(
    state: &State,
    instance: u64,
    transactions: &[Transaction],
) -> Result<Option<Draft>, String> {
    disjoint_transform::prepare(state, instance, transactions)
}

/// Try independent local two-input, identified-output transactions after the
/// custody owner has validated identities. `None` selects the general arbiter.
pub fn prepare_local_creations(
    state: &State,
    instance: u64,
    transactions: &[Transaction],
    advance: bool,
) -> Result<Option<Draft>, String> {
    local_creation::prepare(state, instance, transactions, advance)
}

/// The custody owner validated item identities and will track changes. Generic
/// disjoint transfers can retain identified cohorts; exact selection falls back.
pub fn prepare_independent_items(
    state: &State,
    instance: u64,
    transactions: &[Transaction],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transactions.len() < 128
        || transactions
            .par_iter()
            .any(|row| row.movements.iter().any(|movement| movement.item.is_some()))
    {
        return Ok(None);
    }
    let mut ids: Vec<_> = transactions.par_iter().map(|row| row.id).collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.first() == Some(&0) || ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("invalid or duplicate transaction id".into());
    }
    independent_transfer::prepare(state, instance, transactions, advance)
}

/// Attempt a disjoint flat transfer batch; `None` requests the general arbiter.
pub fn prepare_flat_transfers(
    state: &State,
    instance: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transfers.par_iter().any(|row| row.movement.item.is_some())
        || state
            .containers
            .par_iter()
            .any(|c| c.lots.iter().any(|lot| lot.item.is_some()))
    {
        return Ok(None);
    }
    prepare_flat_transfers_fungible(state, instance, transfers, advance)
}

/// Caller has validated that all identified items are nonexpiring and neither
/// endpoint of any transfer contains one.
pub fn prepare_flat_transfers_fungible(
    state: &State,
    instance: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    independent_transfer::prepare_flat(state, instance, transfers, advance)
}

/// Resolve exclusive transfers separately from genuinely contested endpoints.
pub fn prepare_partitioned_flat(
    state: &State,
    instance: u64,
    seed: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    if transfers.par_iter().any(|row| row.movement.item.is_some())
        || state
            .containers
            .par_iter()
            .any(|c| c.lots.iter().any(|lot| lot.item.is_some()))
    {
        return Ok(None);
    }
    prepare_partitioned_flat_fungible(state, instance, seed, transfers, advance)
}

/// Caller has validated that all identified items are nonexpiring and neither
/// endpoint of any transfer contains one.
pub fn prepare_partitioned_flat_fungible(
    state: &State,
    instance: u64,
    seed: u64,
    transfers: &[SimpleTransfer],
    advance: bool,
) -> Result<Option<Draft>, String> {
    partitioned::prepare(state, instance, seed, transfers, advance)
}

/// Stage homogeneous fungible transfers into new empty custody. Unsupported
/// shapes return `None` so the caller can use ordinary transactions.
/// The caller must exclude precise-item requests and any transfer touching
/// identified custody; identified items must be nonexpiring.
pub fn prepare_fan_out_flat_with_empty_fungible(
    state: &State,
    instance: u64,
    seed: u64,
    transfers: &[SimpleTransfer],
    mut additions: Vec<Container>,
    advance: bool,
) -> Result<Option<(Draft, Vec<Container>)>, String> {
    additions.par_sort_unstable_by_key(|row| row.place);
    if additions
        .par_windows(2)
        .any(|pair| pair[0].place == pair[1].place)
        || additions.par_iter().any(|row| {
            row.validate().is_err()
                || row.used != 0
                || state
                    .containers
                    .binary_search_by_key(&row.place, |old| old.place)
                    .is_ok()
        })
    {
        return Err("invalid or existing new custody".into());
    }
    let mut ids: Vec<_> = transfers.par_iter().map(|row| row.action).collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.first() == Some(&0) || ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("invalid or duplicate transaction id".into());
    }
    Ok(
        fan_out::prepare_flat_with_additions(
            state, instance, seed, transfers, &additions, advance,
        )?
        .map(|draft| (draft, additions)),
    )
}

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
    additions: Vec<Container>,
    compact: &[u64],
) -> Result<(Draft, Vec<Container>), String> {
    let identified = state
        .containers
        .par_iter()
        .any(|c| c.lots.iter().any(|lot| lot.item.is_some()))
        || transactions.par_iter().any(|t| {
            t.creations.iter().any(|c| c.lot.item.is_some())
                || t.movements.iter().any(|m| m.item.is_some())
        });
    if identified {
        return Err("identified items require the custody store".into());
    }
    prepare_fungible_validated(
        state,
        instance,
        seed,
        transactions,
        advance,
        additions,
        compact,
    )
}

/// Caller has validated that existing identified items are nonexpiring, no
/// transaction touches their containers, and no creation has an identity.
pub fn prepare_fungible_validated(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
    additions: Vec<Container>,
    compact: &[u64],
) -> Result<(Draft, Vec<Container>), String> {
    prepare_impl(
        state,
        instance,
        seed,
        transactions,
        advance,
        additions,
        compact,
        true,
    )
}

/// Run the general arbiter without partitioning, for differential checks and
/// workloads that need a reference implementation.
pub fn prepare_unpartitioned(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
    additions: Vec<Container>,
    compact: &[u64],
) -> Result<(Draft, Vec<Container>), String> {
    if state
        .containers
        .par_iter()
        .any(|c| c.lots.iter().any(|lot| lot.item.is_some()))
        || transactions.par_iter().any(|t| {
            t.creations.iter().any(|c| c.lot.item.is_some())
                || t.movements.iter().any(|m| m.item.is_some())
        })
    {
        return Err("identified items require the custody store".into());
    }
    prepare_unpartitioned_validated(
        state,
        instance,
        seed,
        transactions,
        advance,
        additions,
        compact,
    )
}

/// The custody owner has validated and will track all item identities.
pub fn prepare_unpartitioned_validated(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
    additions: Vec<Container>,
    compact: &[u64],
) -> Result<(Draft, Vec<Container>), String> {
    prepare_impl(
        state,
        instance,
        seed,
        transactions,
        advance,
        additions,
        compact,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_impl(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[Transaction],
    advance: bool,
    mut additions: Vec<Container>,
    compact: &[u64],
    partition: bool,
) -> Result<(Draft, Vec<Container>), String> {
    #[cfg(feature = "profile")]
    let mut phase = game_diagnostics::span("physical.batches.validate");
    additions.par_sort_unstable_by_key(|row| row.place);
    if additions
        .par_windows(2)
        .any(|pair| pair[0].place == pair[1].place)
        || additions.par_iter().any(|row| {
            row.validate().is_err()
                || row.used != 0
                || state
                    .containers
                    .binary_search_by_key(&row.place, |old| old.place)
                    .is_ok()
        })
    {
        return Err("invalid or existing new custody".into());
    }
    let mut ids: Vec<_> = transactions.par_iter().map(|row| row.id).collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.first() == Some(&0) || ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("invalid or duplicate transaction id".into());
    }
    let mut compact = compact.to_vec();
    if compact.par_windows(2).any(|pair| pair[0] > pair[1]) {
        compact.par_sort_unstable();
    }
    if compact.par_windows(2).any(|pair| pair[0] == pair[1])
        || compact.par_iter().any(|id| ids.binary_search(id).is_err())
        || transactions.par_iter().any(|row| {
            compact.binary_search(&row.id).is_ok()
                && (row.movements.len() != 1
                    || row.movements[0].to.is_some()
                    || !row.creations.is_empty())
        })
    {
        return Err("invalid compact consumption".into());
    }
    if transactions
        .par_iter()
        .any(|row| row.movements.is_empty() && row.creations.is_empty())
    {
        return Err("empty physical transaction".into());
    }
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.batches.partition");
    }
    if partition
        && additions.is_empty()
        && let Some(draft) =
            compact_shared::prepare(state, instance, seed, transactions, &compact, advance)?
    {
        return Ok((draft, additions));
    }
    if partition
        && additions.is_empty()
        && let Some(draft) = independent::prepare(state, instance, transactions, &compact, advance)?
    {
        return Ok((draft, additions));
    }
    if partition
        && additions.is_empty()
        && compact.is_empty()
        && let Some(draft) = independent_transfer::prepare(state, instance, transactions, advance)?
    {
        return Ok((draft, additions));
    }
    if partition
        && compact.is_empty()
        && let Some(draft) = fan_out::prepare_with_additions(
            state,
            instance,
            seed,
            transactions,
            &additions,
            advance,
        )?
    {
        return Ok((draft, additions));
    }
    if partition
        && additions.is_empty()
        && compact.is_empty()
        && let Some(draft) = fan_in::prepare(state, instance, seed, transactions, advance)?
    {
        return Ok((draft, additions));
    }
    if partition
        && let Some(draft) = partitioned::prepare_transactions(
            state,
            instance,
            seed,
            transactions,
            &compact,
            &additions,
            advance,
        )?
    {
        return Ok((draft, additions));
    }
    #[cfg(feature = "profile")]
    drop(phase);
    let draft = prepare_general(
        state,
        instance,
        seed,
        transactions,
        advance,
        &additions,
        &compact,
    )?;
    Ok((draft, additions))
}

/// The caller has validated the complete batch and any selected partition.
#[allow(clippy::too_many_arguments)]
fn prepare_general<T: Borrow<Transaction> + Sync>(
    state: &State,
    instance: u64,
    seed: u64,
    transactions: &[T],
    advance: bool,
    additions: &[Container],
    compact: &[u64],
) -> Result<Draft, String> {
    #[cfg(feature = "profile")]
    let mut phase = game_diagnostics::span("physical.batches.flatten");
    let large = transactions.len() < 64
        && transactions.par_iter().any(|row| {
            let row = row.borrow();
            row.movements.len() >= 2048 || row.creations.len() >= 2048
        });
    let moves: Vec<_> = if large {
        transactions
            .par_iter()
            .flat_map(|row| row.borrow().movements.par_iter().copied())
            .collect()
    } else {
        transactions
            .par_iter()
            .flat_map_iter(|row| row.borrow().movements.iter().copied())
            .collect()
    };
    let creations: Vec<_> = if large {
        transactions
            .par_iter()
            .flat_map(|row| row.borrow().creations.par_iter().cloned())
            .collect()
    } else {
        transactions
            .par_iter()
            .flat_map_iter(|row| row.borrow().creations.iter().cloned())
            .collect()
    };
    let move_counts: Vec<_> = transactions
        .par_iter()
        .map(|row| row.borrow().movements.len() as u64)
        .collect();
    let creation_counts: Vec<_> = transactions
        .par_iter()
        .map(|row| row.borrow().creations.len() as u64)
        .collect();
    let move_offsets = prefixes(&move_counts);
    let creation_offsets = prefixes(&creation_counts);
    let groups: Vec<_> = transactions
        .par_iter()
        .enumerate()
        .map(|(index, row)| {
            let row = row.borrow();
            let a = move_offsets[index] as usize;
            let b = creation_offsets[index] as usize;
            (
                row.id,
                a..a + row.movements.len(),
                b..b + row.creations.len(),
            )
        })
        .collect();
    let move_owners: Vec<_> = if large {
        transactions
            .par_iter()
            .flat_map(|row| {
                let row = row.borrow();
                (0..row.movements.len())
                    .into_par_iter()
                    .map(move |_| row.id)
            })
            .collect()
    } else {
        transactions
            .par_iter()
            .flat_map_iter(|row| {
                let row = row.borrow();
                std::iter::repeat_n(row.id, row.movements.len())
            })
            .collect()
    };
    let creation_owners: Vec<_> = if large {
        transactions
            .par_iter()
            .flat_map(|row| {
                let row = row.borrow();
                (0..row.creations.len())
                    .into_par_iter()
                    .map(move |_| row.id)
            })
            .collect()
    } else {
        transactions
            .par_iter()
            .flat_map_iter(|row| {
                let row = row.borrow();
                std::iter::repeat_n(row.id, row.creations.len())
            })
            .collect()
    };
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.batches.general");
    }
    let draft = game_lot_planning::prepare(
        state,
        instance,
        seed,
        &moves,
        &creations,
        Some(Batch {
            groups: &groups,
            move_owners: &move_owners,
            creation_owners: &creation_owners,
            additions,
            compact,
        }),
        advance,
    )?;
    #[cfg(feature = "profile")]
    drop(phase);
    Ok(draft)
}
