//! Prepare expiry, deterministic capacity claims, and exact FEFO movements.
use crate::{Batch, Draft};
use game_lot_allocation as allocation;
use game_lot_model::key;
use game_lot_model::{
    Container, Creation, Lot, Movement, OutcomeBatch, OutcomeChunk, Place, PlaceIndex, Spoilage,
    State, TransactionOutcome,
};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Clone, Copy)]
struct Claim {
    original: usize,
    movement: Movement,
    source: usize,
    destination: Option<usize>,
    rank: u64,
    sorted: usize,
    compact: bool,
}
fn place_seed(place: Place) -> u64 {
    match place {
        Place::Inventory(id) => id ^ 0x11,
        Place::Stock { region, good } => region.rotate_left(17) ^ u64::from(good) ^ 0x55,
        Place::Person(id) => id ^ 0x22,
        Place::Site(id) => id ^ 0x66,
        Place::JobInput(id) => id ^ 0x33,
        Place::JobOutput(id) => id ^ 0x44,
    }
}
fn live(lot: &Lot, tick: u64) -> bool {
    lot.expires_at.is_none_or(|at| at > tick)
}

pub fn prepare(
    state: &State,
    instance: u64,
    seed: u64,
    moves: &[Movement],
    creations: &[Creation],
    batch: Option<Batch<'_>>,
    advance: bool,
) -> Result<Draft, String> {
    #[cfg(feature = "profile")]
    let mut phase = game_diagnostics::span("physical.general.setup");
    let next = if advance {
        state.tick.checked_add(1).ok_or("lot tick overflow")?
    } else {
        state.tick
    };
    state
        .revision
        .checked_add(1)
        .ok_or("lot revision overflow")?;
    let mut ids: Vec<_> = moves
        .par_iter()
        .map(|row| row.id)
        .chain(creations.par_iter().map(|row| row.id))
        .collect();
    if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
        ids.par_sort_unstable();
    }
    if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("duplicate lot movement id".into());
    }
    if let Some(batch) = &batch {
        let groups = batch.groups;
        let valid = batch.move_owners.len() == moves.len()
            && batch.creation_owners.len() == creations.len()
            && (!groups.is_empty() || (moves.is_empty() && creations.is_empty()))
            && groups
                .first()
                .is_none_or(|row| row.1.start == 0 && row.2.start == 0)
            && groups
                .last()
                .is_none_or(|row| row.1.end == moves.len() && row.2.end == creations.len())
            && groups
                .par_windows(2)
                .all(|rows| rows[0].1.end == rows[1].1.start && rows[0].2.end == rows[1].2.start)
            && groups.par_iter().all(|(id, m, c)| {
                *id != 0
                    && m.start <= m.end
                    && c.start <= c.end
                    && m.end <= moves.len()
                    && c.end <= creations.len()
                    && !(m.is_empty() && c.is_empty())
                    && if m.len() >= 2048 {
                        batch.move_owners[m.clone()]
                            .par_iter()
                            .all(|owner| owner == id)
                    } else {
                        batch.move_owners[m.clone()].iter().all(|owner| owner == id)
                    }
                    && if c.len() >= 2048 {
                        batch.creation_owners[c.clone()]
                            .par_iter()
                            .all(|owner| owner == id)
                    } else {
                        batch.creation_owners[c.clone()]
                            .iter()
                            .all(|owner| owner == id)
                    }
            })
            && batch.compact.par_windows(2).all(|pair| pair[0] < pair[1])
            && groups
                .par_iter()
                .filter(|(id, _, _)| batch.compact.binary_search(id).is_ok())
                .count()
                == batch.compact.len()
            && groups.par_iter().all(|(id, m, c)| {
                batch.compact.binary_search(id).is_err()
                    || (m.len() == 1 && c.is_empty() && moves[m.start].to.is_none())
            });
        if !valid {
            return Err("invalid transaction layout".into());
        }
    }
    // Borrow existing custody; additions are only published if this draft commits.
    let additions = batch.as_ref().map_or(&[][..], |batch| batch.additions);
    let mut containers: Vec<_> = state
        .containers
        .par_iter()
        .chain(additions.par_iter())
        .collect();
    if !additions.is_empty() {
        containers.par_sort_unstable_by_key(|row| row.place);
    }
    let places = PlaceIndex::new(&containers, |row: &&Container| row.place);
    let mut claims: Vec<_> = moves
        .par_iter()
        .enumerate()
        .map(|(original, &movement)| {
            if movement.id == 0
                || !movement.from.accepts(movement.good)
                || movement
                    .to
                    .is_some_and(|place| !place.accepts(movement.good))
                || movement.quantity == 0
                || movement
                    .item
                    .is_some_and(|id| id == 0 || movement.quantity != 1)
                || movement.to == Some(movement.from)
            {
                return Err("invalid lot movement".to_string());
            }
            let source = places
                .locate(movement.from)
                .ok_or_else(|| "missing source custody".to_string())?;
            let destination = movement
                .to
                .map(|place| {
                    places
                        .locate(place)
                        .ok_or_else(|| "missing destination custody".to_string())
                })
                .transpose()?;
            let rank = if let Some(batch) = &batch {
                game_genetics::random(seed, batch.move_owners[original], next)
            } else {
                game_genetics::random(
                    seed ^ place_seed(movement.from) ^ u64::from(movement.good),
                    movement.id,
                    next,
                )
            };
            Ok(Claim {
                original,
                movement,
                source,
                destination,
                rank,
                sorted: 0,
                compact: batch.as_ref().is_some_and(|batch| {
                    batch
                        .compact
                        .binary_search(&batch.move_owners[original])
                        .is_ok()
                }),
            })
        })
        .collect::<Result<_, String>>()?;
    let claim_key = |row: &Claim| (row.source, row.movement.good, row.rank, row.movement.id);
    if claims
        .par_windows(2)
        .any(|pair| claim_key(&pair[0]) > claim_key(&pair[1]))
    {
        claims.par_sort_unstable_by_key(claim_key);
    }
    claims
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, row)| row.sorted = index);
    let creation_destinations: Vec<_> = creations
        .par_iter()
        .map(|creation| {
            if creation.id == 0
                || !creation.to.accepts(creation.lot.good)
                || creation.lot.quantity == 0
                || creation.lot.batch == Some(0)
                || (creation.lot.batch.is_some() && creation.lot.item.is_some())
                || creation.lot.item.is_some_and(|id| {
                    id == 0 || creation.lot.quantity != 1 || creation.lot.expires_at.is_some()
                })
                || creation.lot.expires_at.is_some_and(|at| at <= next)
            {
                return Err("invalid created lot".to_string());
            }
            places
                .locate(creation.to)
                .ok_or_else(|| "missing output custody".to_string())
        })
        .collect::<Result<_, _>>()?;
    let expiring: Vec<_> = containers
        .par_iter()
        .enumerate()
        .filter_map(|(index, container)| {
            container
                .next_expiry
                .is_some_and(|at| at <= next)
                .then_some(index)
        })
        .collect();
    let mut spoiled: Vec<_> = expiring
        .par_iter()
        .flat_map(|&index| {
            let container = containers[index];
            container.lots.par_iter().filter_map(move |lot| {
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
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.general.source_arbitration");
    }
    let source_ok: Vec<_> = (0..claims.len())
        .into_par_iter()
        .map(|_| AtomicBool::new(false))
        .collect();
    let admit_generic = |group: &[Claim]| {
        let lots = &containers[group[0].source].lots;
        let first = lots.partition_point(|lot| lot.good < group[0].movement.good);
        let last = lots.partition_point(|lot| lot.good <= group[0].movement.good);
        let available: u64 = lots[first..last]
            .par_iter()
            .filter(|lot| live(lot, next))
            .map(|lot| lot.quantity)
            .sum();
        if group.len() == 1 {
            if group[0].movement.quantity <= available {
                source_ok[group[0].sorted].store(true, Ordering::Relaxed);
            }
            return;
        }
        let amounts: Vec<_> = group
            .par_iter()
            .map(|claim| claim.movement.quantity)
            .collect();
        group
            .par_iter()
            .zip(allocation::choose(&amounts, available).into_par_iter())
            .for_each(|(claim, granted)| {
                if granted {
                    source_ok[claim.sorted].store(true, Ordering::Relaxed);
                }
            });
    };
    let mut frozen: Vec<_> = if moves.par_iter().any(|movement| movement.item.is_some()) {
        claims
            .par_chunk_by(|a, b| (a.source, a.movement.good) == (b.source, b.movement.good))
            .flat_map_iter(|group| {
                let lots = &containers[group[0].source].lots;
                let first = lots.partition_point(|lot| lot.good < group[0].movement.good);
                let last = lots.partition_point(|lot| lot.good <= group[0].movement.good);
                if group.iter().any(|claim| claim.movement.item.is_some()) {
                    if let [claim] = group {
                        let item = claim.movement.item.expect("exact item group");
                        if let Some(i) = lots[first..last]
                            .iter()
                            .position(|lot| lot.item == Some(item) && live(lot, next))
                        {
                            source_ok[claim.sorted].store(true, Ordering::Relaxed);
                            return vec![(claim.sorted, vec![(first + i, 1u64)])];
                        }
                        return Vec::new();
                    }
                    let mut remaining: Vec<_> = lots[first..last]
                        .iter()
                        .map(|lot| if live(lot, next) { lot.quantity } else { 0 })
                        .collect();
                    let index: HashMap<_, _> = lots[first..last]
                        .iter()
                        .enumerate()
                        .filter_map(|(i, lot)| lot.item.map(|id| (id, i)))
                        .collect();
                    let mut available: u64 = remaining.iter().sum();
                    let mut cursor = 0;
                    let mut allocations = Vec::new();
                    for claim in group {
                        if let Some(id) = claim.movement.item {
                            if let Some(&i) = index.get(&id)
                                && remaining[i] == 1
                            {
                                remaining[i] = 0;
                                available -= 1;
                                source_ok[claim.sorted].store(true, Ordering::Relaxed);
                                allocations.push((claim.sorted, vec![(first + i, 1u64)]));
                            }
                            continue;
                        }
                        if claim.movement.quantity > available {
                            remaining.fill(0);
                            available = 0;
                            continue;
                        }
                        let mut needed = claim.movement.quantity;
                        let mut pieces = Vec::new();
                        while needed > 0 {
                            while remaining[cursor] == 0 {
                                cursor += 1;
                            }
                            let take = needed.min(remaining[cursor]);
                            pieces.push((first + cursor, take));
                            remaining[cursor] -= take;
                            needed -= take;
                        }
                        available -= claim.movement.quantity;
                        source_ok[claim.sorted].store(true, Ordering::Relaxed);
                        allocations.push((claim.sorted, pieces));
                    }
                    return allocations;
                }
                admit_generic(group);
                Vec::new()
            })
            .collect()
    } else {
        claims
            .par_chunk_by(|a, b| (a.source, a.movement.good) == (b.source, b.movement.good))
            .for_each(admit_generic);
        Vec::new()
    };
    frozen.par_sort_unstable_by_key(|row| row.0);
    let outgoing: Vec<_> = if let Some(batch) = &batch
        && (!creations.is_empty() || claims.par_iter().any(|claim| claim.destination.is_some()))
    {
        let mut rows: Vec<_> = claims
            .par_iter()
            .filter(|row| source_ok[row.sorted].load(Ordering::Relaxed))
            .map(|row| {
                (
                    batch.move_owners[row.original],
                    row.source,
                    u128::from(row.movement.quantity),
                )
            })
            .collect();
        rows.par_sort_unstable_by_key(|row| (row.0, row.1));
        rows.par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|group| (group[0].0, group[0].1, group.iter().map(|row| row.2).sum()))
            .collect()
    } else {
        Vec::new()
    };
    let accepted: Vec<_> = (0..claims.len() + creations.len())
        .into_par_iter()
        .map(|_| AtomicBool::new(false))
        .collect();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.general.destination_arbitration");
    }
    let mut destinations: Vec<_> = claims
        .par_iter()
        .enumerate()
        .filter_map(|(index, claim)| {
            if !source_ok[index].load(Ordering::Relaxed) {
                return None;
            }
            if claim.destination.is_none() {
                accepted[index].store(true, Ordering::Relaxed);
                return None;
            }
            Some(allocation::Destination {
                index,
                place: claim.destination.unwrap(),
                rank: claim.rank,
                transaction: batch
                    .as_ref()
                    .map_or(claim.movement.id, |batch| batch.move_owners[claim.original]),
                id: claim.movement.id,
                quantity: claim.movement.quantity,
            })
        })
        .collect();
    destinations.par_extend(creations.par_iter().enumerate().map(|(index, creation)| {
        allocation::Destination {
            index: claims.len() + index,
            place: creation_destinations[index],
            rank: batch.as_ref().map_or_else(
                || game_genetics::random(seed ^ place_seed(creation.to), creation.id, next),
                |batch| game_genetics::random(seed, batch.creation_owners[index], next),
            ),
            transaction: batch
                .as_ref()
                .map_or(creation.id, |batch| batch.creation_owners[index]),
            id: creation.id,
            quantity: creation.lot.quantity,
        }
    }));
    allocation::grant_destinations(
        destinations,
        &outgoing,
        &containers,
        |index| {
            let container = containers[index];
            if container.next_expiry.is_some_and(|at| at <= next) {
                container
                    .lots
                    .par_iter()
                    .filter(|lot| live(lot, next))
                    .map(|lot| lot.quantity)
                    .sum()
            } else {
                container.used
            }
        },
        &accepted,
    );
    let mut transaction_outcomes = Vec::new();
    if let Some(batch) = &batch {
        let original_positions: Vec<_> = (0..claims.len())
            .into_par_iter()
            .map(|_| AtomicUsize::new(0))
            .collect();
        claims
            .par_iter()
            .for_each(|row| original_positions[row.original].store(row.sorted, Ordering::Relaxed));
        transaction_outcomes = batch
            .groups
            .par_iter()
            .map(|(id, movement_range, creation_range)| {
                if movement_range.len().saturating_add(creation_range.len()) >= 2048 {
                    let granted = movement_range.clone().into_par_iter().all(|i| {
                        accepted[original_positions[i].load(Ordering::Relaxed)]
                            .load(Ordering::Relaxed)
                    }) && creation_range
                        .clone()
                        .into_par_iter()
                        .all(|i| accepted[claims.len() + i].load(Ordering::Relaxed));
                    if !granted {
                        movement_range.clone().into_par_iter().for_each(|i| {
                            accepted[original_positions[i].load(Ordering::Relaxed)]
                                .store(false, Ordering::Relaxed);
                        });
                        creation_range.clone().into_par_iter().for_each(|i| {
                            accepted[claims.len() + i].store(false, Ordering::Relaxed);
                        });
                    }
                    return TransactionOutcome {
                        id: *id,
                        accepted: granted,
                    };
                }
                let indices = || {
                    movement_range
                        .clone()
                        .map(|i| original_positions[i].load(Ordering::Relaxed))
                        .chain(creation_range.clone().map(|i| claims.len() + i))
                };
                let granted = indices().all(|index| accepted[index].load(Ordering::Relaxed));
                if !granted {
                    for index in indices() {
                        accepted[index].store(false, Ordering::Relaxed);
                    }
                }
                TransactionOutcome {
                    id: *id,
                    accepted: granted,
                }
            })
            .collect();
        transaction_outcomes.par_sort_unstable_by_key(|row| row.id);
    }
    let mut creation_outcomes: Vec<_> = creations
        .par_iter()
        .enumerate()
        .map(|(index, row)| {
            (
                row.id,
                accepted[claims.len() + index].load(Ordering::Relaxed),
            )
        })
        .collect();
    creation_outcomes.par_sort_unstable_by_key(|row| row.0);
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.general.cohorts");
    }
    let groups: Vec<_> = claims
        .par_chunk_by(|a, b| (a.source, a.movement.good) == (b.source, b.movement.good))
        .map(|group| {
            if group.iter().any(|claim| claim.movement.item.is_some()) {
                let mut pieces = Vec::new();
                let mut precise = Vec::new();
                for claim in group
                    .iter()
                    .filter(|claim| accepted[claim.sorted].load(Ordering::Relaxed))
                {
                    let allocations = &frozen[frozen
                        .binary_search_by_key(&claim.sorted, |row| row.0)
                        .expect("admitted item allocation")]
                    .1;
                    let mut lots = Vec::new();
                    for &(index, quantity) in allocations {
                        precise.push((claim.source, index, quantity));
                        if !claim.compact {
                            let original = &containers[claim.source].lots[index];
                            lots.push(Lot {
                                batch: original.batch,
                                item: original.item,
                                good: original.good,
                                quantity,
                                expires_at: original.expires_at,
                            });
                        }
                    }
                    pieces.push((claim.original, claim.destination, lots));
                }
                return (group[0].source, group[0].movement.good, 0, pieces, precise);
            }
            // Most independent transactions consume from a source once. Resolve
            // that case directly, without allocating prefix arrays per source.
            if let [claim] = group {
                if !accepted[claim.sorted].load(Ordering::Relaxed) {
                    return (claim.source, claim.movement.good, 0, Vec::new(), Vec::new());
                }
                if claim.compact {
                    return (
                        claim.source,
                        claim.movement.good,
                        claim.movement.quantity,
                        vec![(claim.original, None, Vec::new())],
                        Vec::new(),
                    );
                }
                let stock = &containers[claim.source].lots;
                let first = stock.partition_point(|lot| lot.good < claim.movement.good);
                let last = stock.partition_point(|lot| lot.good <= claim.movement.good);
                if last - first >= 2048 {
                    let lots: Vec<_> = stock[first..last]
                        .par_iter()
                        .filter(|lot| live(lot, next))
                        .collect();
                    let amounts: Vec<_> = lots.par_iter().map(|lot| lot.quantity).collect();
                    let prefixes = allocation::prefixes(&amounts);
                    let parts: Vec<_> = lots
                        .par_iter()
                        .zip(prefixes.par_iter())
                        .filter_map(|(lot, &before)| {
                            let quantity = u128::from(claim.movement.quantity)
                                .saturating_sub(before)
                                .min(u128::from(lot.quantity))
                                as u64;
                            (quantity > 0).then_some(Lot {
                                batch: lot.batch,
                                item: lot.item,
                                good: lot.good,
                                quantity,
                                expires_at: lot.expires_at,
                            })
                        })
                        .collect();
                    return (
                        claim.source,
                        claim.movement.good,
                        claim.movement.quantity,
                        vec![(claim.original, claim.destination, parts)],
                        Vec::new(),
                    );
                }
                let mut remaining = claim.movement.quantity;
                let mut parts = Vec::new();
                for lot in &stock[first..last] {
                    if !live(lot, next) {
                        continue;
                    }
                    let quantity = remaining.min(lot.quantity);
                    if quantity != 0 {
                        parts.push(Lot {
                            batch: lot.batch,
                            item: lot.item,
                            good: lot.good,
                            quantity,
                            expires_at: lot.expires_at,
                        });
                        remaining -= quantity;
                    }
                    if remaining == 0 {
                        break;
                    }
                }
                debug_assert_eq!(remaining, 0);
                return (
                    claim.source,
                    claim.movement.good,
                    claim.movement.quantity,
                    vec![(claim.original, claim.destination, parts)],
                    Vec::new(),
                );
            }
            let chosen: Vec<_> = group
                .par_iter()
                .filter(|row| accepted[row.sorted].load(Ordering::Relaxed))
                .collect();
            if chosen.is_empty() {
                return (
                    group[0].source,
                    group[0].movement.good,
                    0,
                    Vec::new(),
                    Vec::new(),
                );
            }
            let total: u64 = chosen.par_iter().map(|row| row.movement.quantity).sum();
            if chosen.par_iter().all(|row| row.compact) {
                let pieces = chosen
                    .par_iter()
                    .map(|row| (row.original, None, Vec::new()))
                    .collect();
                return (
                    group[0].source,
                    group[0].movement.good,
                    total,
                    pieces,
                    Vec::new(),
                );
            }
            let stock = &containers[group[0].source].lots;
            let first = stock.partition_point(|lot| lot.good < group[0].movement.good);
            let last = stock.partition_point(|lot| lot.good <= group[0].movement.good);
            let lots: Vec<_> = stock[first..last]
                .par_iter()
                .filter(|lot| live(lot, next))
                .collect();
            let lot_amounts: Vec<_> = lots.par_iter().map(|lot| lot.quantity).collect();
            let lot_prefix = allocation::prefixes(&lot_amounts);
            let amounts: Vec<_> = chosen.par_iter().map(|row| row.movement.quantity).collect();
            let prefixes = allocation::prefixes(&amounts);
            let pieces: Vec<_> = chosen
                .par_iter()
                .zip(prefixes.par_iter())
                .map(|(claim, &begin)| {
                    if claim.compact {
                        return (claim.original, None, Vec::new());
                    }
                    let end = begin + u128::from(claim.movement.quantity);
                    let mut index = lot_prefix
                        .partition_point(|&offset| offset <= begin)
                        .saturating_sub(1);
                    let mut parts = Vec::new();
                    while index < lots.len() && lot_prefix[index] < end {
                        let lot = lots[index];
                        let left = begin.max(lot_prefix[index]);
                        let right = end.min(lot_prefix[index] + u128::from(lot.quantity));
                        if right > left {
                            parts.push(Lot {
                                batch: lot.batch,
                                item: lot.item,
                                good: lot.good,
                                quantity: (right - left) as u64,
                                expires_at: lot.expires_at,
                            });
                        }
                        index += 1;
                    }
                    (claim.original, claim.destination, parts)
                })
                .collect();
            (
                group[0].source,
                group[0].movement.good,
                total,
                pieces,
                Vec::new(),
            )
        })
        .collect();
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.general.receipts_and_incoming");
    }
    let mut debits: Vec<_> = groups
        .par_iter()
        .filter(|row| row.2 > 0)
        .map(|row| (row.0, row.1, row.2))
        .collect();
    debits.par_sort_unstable_by_key(|row| (row.0, row.1));
    let mut precise: Vec<_> = groups
        .par_iter()
        .flat_map_iter(|group| group.4.iter().copied())
        .collect();
    precise.par_sort_unstable_by_key(|row| (row.0, row.1));
    let precise: Vec<_> = precise
        .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
        .map(|group| {
            (
                group[0].0,
                group[0].1,
                group.iter().map(|row| row.2).sum::<u64>(),
            )
        })
        .collect();
    let mut pieces: Vec<_> = groups
        .into_par_iter()
        .flat_map_iter(|group| group.3)
        .collect();
    pieces.par_sort_unstable_by_key(|row| row.0);
    let piece_rows = &pieces;
    // Bound shared arenas by both receipt count and lot count. A single very
    // large receipt remains intact, but cannot retain unrelated receipts.
    const MAX_RECEIPT_LOTS: usize = 4096;
    let chunks: Vec<_> = moves
        .par_chunks(2048)
        .enumerate()
        .flat_map_iter(|(chunk, rows)| {
            let mut cursor = piece_rows.partition_point(|row| row.0 < chunk * 2048);
            let mut headers = Vec::with_capacity(rows.len());
            let mut lots = Vec::new();
            let mut chunks = Vec::new();
            for (offset, movement) in rows.iter().enumerate() {
                let index = chunk * 2048 + offset;
                let matched = piece_rows.get(cursor).filter(|row| row.0 == index);
                let count = matched.map_or(0, |row| row.2.len());
                if !headers.is_empty() && lots.len().saturating_add(count) > MAX_RECEIPT_LOTS {
                    chunks.push(OutcomeChunk {
                        headers: std::mem::take(&mut headers),
                        lots: std::mem::take(&mut lots),
                    });
                }
                let begin = lots.len();
                if let Some(row) = matched {
                    lots.extend_from_slice(&row.2);
                    cursor += 1;
                }
                headers.push((movement.id, matched.is_some(), begin..lots.len()));
            }
            if !headers.is_empty() {
                chunks.push(OutcomeChunk { headers, lots });
            }
            chunks
        })
        .collect();
    let mut outcomes = OutcomeBatch::from_chunks(chunks);
    let mut incoming: Vec<_> = pieces
        .par_iter()
        .flat_map_iter(|(_, destination, lots)| {
            destination
                .iter()
                .flat_map(|&destination| lots.iter().cloned().map(move |lot| (destination, lot)))
        })
        .collect();
    incoming.par_extend(
        creations
            .par_iter()
            .enumerate()
            .filter_map(|(index, creation)| {
                accepted[claims.len() + index]
                    .load(Ordering::Relaxed)
                    .then(|| (creation_destinations[index], creation.lot.clone()))
            }),
    );
    incoming.par_sort_unstable_by_key(|row| (row.0, key(&row.1)));
    // Only touched or expiring custody needs materializing. Searching every
    // unchanged container for incoming and outgoing lots scales with the
    // entire population even when activity is sparse.
    #[cfg(feature = "profile")]
    {
        drop(phase);
        phase = game_diagnostics::span("physical.general.materialize");
    }
    let mut touched: Vec<_> = debits
        .par_iter()
        .map(|row| row.0)
        .chain(precise.par_iter().map(|row| row.0))
        .chain(incoming.par_iter().map(|row| row.0))
        .chain(expiring.par_iter().copied())
        .collect();
    touched.par_sort_unstable();
    let touched: Vec<_> = touched
        .par_chunk_by(|a, b| a == b)
        .map(|group| group[0])
        .collect();
    let mut changes: Vec<_> = touched
        .par_iter()
        .map(|&index| {
            let container = containers[index];
            let d0 = debits.partition_point(|row| row.0 < index);
            let d1 = debits.partition_point(|row| row.0 <= index);
            let a0 = incoming.partition_point(|row| row.0 < index);
            let a1 = incoming.partition_point(|row| row.0 <= index);
            let p0 = precise.partition_point(|row| row.0 < index);
            let p1 = precise.partition_point(|row| row.0 <= index);
            if p0 < p1 {
                let mut lots = Vec::with_capacity(container.lots.len() + a1 - a0);
                let mut good = None;
                let mut remaining = 0;
                for (lot_index, lot) in container
                    .lots
                    .iter()
                    .enumerate()
                    .filter(|(_, lot)| live(lot, next))
                {
                    if good != Some(lot.good) {
                        good = Some(lot.good);
                        remaining = debits[d0..d1]
                            .binary_search_by_key(&lot.good, |row| row.1)
                            .ok()
                            .map_or(0, |i| debits[d0 + i].2);
                    }
                    let generic = remaining.min(lot.quantity);
                    remaining -= generic;
                    let exact = precise[p0..p1]
                        .binary_search_by_key(&lot_index, |row| row.1)
                        .ok()
                        .map_or(0, |i| precise[p0 + i].2);
                    let quantity = lot
                        .quantity
                        .checked_sub(generic + exact)
                        .expect("validated precise debit");
                    if quantity > 0 {
                        lots.push(Lot {
                            batch: lot.batch,
                            item: lot.item,
                            good: lot.good,
                            quantity,
                            expires_at: lot.expires_at,
                        });
                    }
                }
                lots.extend(incoming[a0..a1].iter().map(|row| row.1.clone()));
                lots.sort_unstable_by_key(key);
                let mut merged: Vec<Lot> = Vec::with_capacity(lots.len());
                for lot in lots {
                    if let Some(last) = merged.last_mut().filter(|last| key(last) == key(&lot)) {
                        last.quantity += lot.quantity;
                    } else {
                        merged.push(lot);
                    }
                }
                let used = merged.iter().map(|lot| lot.quantity).sum();
                let next_expiry = merged.iter().filter_map(|lot| lot.expires_at).min();
                return (
                    index,
                    Container {
                        place: container.place,
                        capacity: container.capacity,
                        lots: merged,
                        used,
                        next_expiry,
                    },
                );
            }
            // A single carried lot with no incoming goods needs no per-good
            // grouping, prefix scan, or merge when it is consumed or expires.
            if a0 == a1
                && let [lot] = container.lots.as_slice()
            {
                let debit = debits[d0..d1]
                    .binary_search_by_key(&lot.good, |row| row.1)
                    .ok()
                    .map_or(0, |i| debits[d0 + i].2);
                let quantity = if live(lot, next) {
                    lot.quantity
                        .checked_sub(debit)
                        .expect("validated lot debit")
                } else {
                    debug_assert_eq!(debit, 0);
                    0
                };
                let lots = if quantity == 0 {
                    Vec::new()
                } else {
                    vec![Lot {
                        batch: lot.batch,
                        item: lot.item,
                        good: lot.good,
                        quantity,
                        expires_at: lot.expires_at,
                    }]
                };
                return (
                    index,
                    Container {
                        place: container.place,
                        capacity: container.capacity,
                        lots,
                        used: quantity,
                        next_expiry: (quantity > 0).then_some(lot.expires_at).flatten(),
                    },
                );
            }
            // Parallelize across small containers instead of starting several
            // nested parallel scans for each handful of lots. Large custody
            // retains the inner-parallel path below.
            if container.lots.len().saturating_add(a1 - a0) <= 64 {
                let mut lots = Vec::with_capacity(container.lots.len() + a1 - a0);
                let mut good = None;
                let mut remaining = 0;
                for lot in container.lots.iter().filter(|lot| live(lot, next)) {
                    if good != Some(lot.good) {
                        good = Some(lot.good);
                        remaining = debits[d0..d1]
                            .binary_search_by_key(&lot.good, |row| row.1)
                            .ok()
                            .map_or(0, |i| debits[d0 + i].2);
                    }
                    let consumed = remaining.min(lot.quantity);
                    remaining -= consumed;
                    if consumed < lot.quantity {
                        lots.push(Lot {
                            batch: lot.batch,
                            item: lot.item,
                            good: lot.good,
                            quantity: lot.quantity - consumed,
                            expires_at: lot.expires_at,
                        });
                    }
                }
                lots.extend(incoming[a0..a1].iter().map(|row| row.1.clone()));
                lots.sort_unstable_by_key(key);
                let mut merged: Vec<Lot> = Vec::with_capacity(lots.len());
                for lot in lots {
                    if let Some(last) = merged.last_mut().filter(|last| key(last) == key(&lot)) {
                        last.quantity =
                            (u128::from(last.quantity) + u128::from(lot.quantity)) as u64;
                    } else {
                        merged.push(lot);
                    }
                }
                let used = merged
                    .iter()
                    .map(|lot| u128::from(lot.quantity))
                    .sum::<u128>() as u64;
                let next_expiry = merged.iter().filter_map(|lot| lot.expires_at).min();
                return (
                    index,
                    Container {
                        place: container.place,
                        capacity: container.capacity,
                        lots: merged,
                        used,
                        next_expiry,
                    },
                );
            }
            let live_lots: Vec<_> = container
                .lots
                .par_iter()
                .filter(|lot| live(lot, next))
                .collect();
            let mut lots: Vec<Lot> = live_lots
                .par_chunk_by(|a, b| a.good == b.good)
                .flat_map_iter(|group| {
                    let debit = debits[d0..d1]
                        .binary_search_by_key(&group[0].good, |row| row.1)
                        .ok()
                        .map_or(0, |i| debits[d0 + i].2);
                    let amounts: Vec<_> = group.par_iter().map(|lot| lot.quantity).collect();
                    let prefixes = allocation::prefixes(&amounts);
                    group
                        .par_iter()
                        .zip(prefixes.par_iter())
                        .filter_map(|(lot, &before)| {
                            let consumed = u128::from(debit)
                                .saturating_sub(before)
                                .min(u128::from(lot.quantity))
                                as u64;
                            (consumed < lot.quantity).then(|| Lot {
                                batch: lot.batch,
                                item: lot.item,
                                good: lot.good,
                                quantity: lot.quantity - consumed,
                                expires_at: lot.expires_at,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            lots.par_extend(incoming[a0..a1].par_iter().map(|row| row.1.clone()));
            lots.par_sort_unstable_by_key(key);
            let merged: Vec<_> = lots
                .par_chunk_by(|a, b| key(a) == key(b))
                .map(|group| Lot {
                    batch: group[0].batch,
                    item: group[0].item,
                    good: group[0].good,
                    expires_at: group[0].expires_at,
                    quantity: group
                        .par_iter()
                        .map(|lot| u128::from(lot.quantity))
                        .sum::<u128>() as u64,
                })
                .collect();
            let used = merged
                .par_iter()
                .map(|lot| u128::from(lot.quantity))
                .sum::<u128>() as u64;
            let next_expiry = merged.par_iter().filter_map(|lot| lot.expires_at).min();
            (
                index,
                Container {
                    place: container.place,
                    capacity: container.capacity,
                    lots: merged,
                    used,
                    next_expiry,
                },
            )
        })
        .collect();
    changes.par_sort_unstable_by_key(|row| row.0);
    outcomes.sort_by_id();
    #[cfg(feature = "profile")]
    drop(phase);
    Ok(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes: changes.into(),
        outcomes,
        spoiled,
        creations: creation_outcomes,
        transactions: transaction_outcomes,
    })
}
