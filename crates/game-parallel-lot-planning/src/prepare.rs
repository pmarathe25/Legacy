//! Prepare expiry, deterministic capacity claims, and exact FEFO movements.
use crate::{Batch, Draft};
use game_parallel_lot_allocation as allocation;
use game_parallel_lot_model::key;
use game_parallel_lot_model::{
    Container, Creation, Lot, Movement, Outcome, Place, Spoilage, State, TransactionOutcome,
};
use rayon::prelude::*;
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
    ids.par_sort_unstable();
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
                    && batch.move_owners[m.clone()].iter().all(|owner| owner == id)
                    && batch.creation_owners[c.clone()]
                        .iter()
                        .all(|owner| owner == id)
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
                || movement.to == Some(movement.from)
            {
                return Err("invalid lot movement".to_string());
            }
            let source = containers
                .binary_search_by_key(&movement.from, |container| container.place)
                .map_err(|_| "missing source custody".to_string())?;
            let destination = movement
                .to
                .map(|place| {
                    containers
                        .binary_search_by_key(&place, |container| container.place)
                        .map_err(|_| "missing destination custody".to_string())
                })
                .transpose()?;
            let rank = if let Some(batch) = &batch {
                game_parallel_genetics::random(seed, batch.move_owners[original], next)
            } else {
                game_parallel_genetics::random(
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
    claims
        .par_sort_unstable_by_key(|row| (row.source, row.movement.good, row.rank, row.movement.id));
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
                || creation.lot.expires_at.is_some_and(|at| at <= next)
            {
                return Err("invalid created lot".to_string());
            }
            containers
                .binary_search_by_key(&creation.to, |container| container.place)
                .map_err(|_| "missing output custody".to_string())
        })
        .collect::<Result<_, _>>()?;
    let used: Vec<_> = containers
        .par_iter()
        .map(|container| {
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
        })
        .collect();
    let mut spoiled: Vec<_> = containers
        .par_iter()
        .filter(|container| container.next_expiry.is_some_and(|at| at <= next))
        .flat_map(|container| {
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
    let source_ok: Vec<_> = (0..claims.len())
        .into_par_iter()
        .map(|_| AtomicBool::new(false))
        .collect();
    claims
        .par_chunk_by(|a, b| (a.source, a.movement.good) == (b.source, b.movement.good))
        .for_each(|group| {
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
        });
    let outgoing: Vec<_> = if let Some(batch) = &batch {
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
                || {
                    game_parallel_genetics::random(
                        seed ^ place_seed(creation.to),
                        creation.id,
                        next,
                    )
                },
                |batch| game_parallel_genetics::random(seed, batch.creation_owners[index], next),
            ),
            transaction: batch
                .as_ref()
                .map_or(creation.id, |batch| batch.creation_owners[index]),
            id: creation.id,
            quantity: creation.lot.quantity,
        }
    }));
    allocation::grant_destinations(destinations, &outgoing, &containers, &used, &accepted);
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
    let groups: Vec<_> = claims
        .par_chunk_by(|a, b| (a.source, a.movement.good) == (b.source, b.movement.good))
        .map(|group| {
            let chosen: Vec<_> = group
                .par_iter()
                .filter(|row| accepted[row.sorted].load(Ordering::Relaxed))
                .collect();
            if chosen.is_empty() {
                return (group[0].source, group[0].movement.good, 0, Vec::new());
            }
            let total: u64 = chosen.par_iter().map(|row| row.movement.quantity).sum();
            if chosen.par_iter().all(|row| row.compact) {
                let pieces = chosen
                    .par_iter()
                    .map(|row| (row.original, None, Vec::new()))
                    .collect();
                return (group[0].source, group[0].movement.good, total, pieces);
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
            (group[0].source, group[0].movement.good, total, pieces)
        })
        .collect();
    let mut pieces: Vec<_> = groups
        .par_iter()
        .flat_map(|group| group.3.par_iter().cloned())
        .collect();
    pieces.par_sort_unstable_by_key(|row| row.0);
    let piece_rows = &pieces;
    let outcomes: Vec<_> = moves
        .par_chunks(2048)
        .enumerate()
        .flat_map_iter(|(chunk, rows)| {
            let mut cursor = piece_rows.partition_point(|row| row.0 < chunk * 2048);
            rows.iter()
                .enumerate()
                .map(move |(offset, movement)| {
                    let index = chunk * 2048 + offset;
                    let matched = piece_rows.get(cursor).filter(|row| row.0 == index);
                    let lots = matched.map_or_else(Vec::new, |row| row.2.clone());
                    if matched.is_some() {
                        cursor += 1;
                    }
                    Outcome {
                        id: movement.id,
                        accepted: matched.is_some(),
                        lots,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
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
    let mut debits: Vec<_> = groups
        .par_iter()
        .filter(|row| row.2 > 0)
        .map(|row| (row.0, row.1, row.2))
        .collect();
    debits.par_sort_unstable_by_key(|row| (row.0, row.1));
    let mut changes: Vec<_> = containers
        .par_iter()
        .enumerate()
        .filter_map(|(index, container)| {
            let d0 = debits.partition_point(|row| row.0 < index);
            let d1 = debits.partition_point(|row| row.0 <= index);
            let a0 = incoming.partition_point(|row| row.0 < index);
            let a1 = incoming.partition_point(|row| row.0 <= index);
            if d0 == d1 && a0 == a1 && container.next_expiry.is_none_or(|at| at > next) {
                return None;
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
            Some((
                index,
                Container {
                    place: container.place,
                    capacity: container.capacity,
                    lots: merged,
                    used,
                    next_expiry,
                },
            ))
        })
        .collect();
    changes.par_sort_unstable_by_key(|row| row.0);
    let mut outcomes = outcomes;
    outcomes.par_sort_unstable_by_key(|row| row.id);
    Ok(Draft {
        base_instance: instance,
        base_tick: state.tick,
        base_revision: state.revision,
        next_tick: next,
        changes,
        outcomes,
        spoiled,
        creations: creation_outcomes,
        transactions: transaction_outcomes,
    })
}
