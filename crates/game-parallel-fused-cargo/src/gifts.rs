//! Prepare disjoint, physically local person-to-person food transfers.
use game_parallel::{Cargo, FoodGift, Holding, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;

pub fn prepare_gifts(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    gifts: &[FoodGift],
) -> Result<Cargo, String> {
    let cargo = world.cargo.as_ref().ok_or("personal cargo is disabled")?;
    let mut participants: Vec<_> = gifts
        .par_iter()
        .flat_map_iter(|g| [g.giver, g.recipient])
        .collect();
    participants.par_sort_unstable();
    if participants.par_windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("each participant may handle food once per batch".into());
    }
    let mut busy: Vec<_> = world
        .conflict
        .as_ref()
        .into_iter()
        .flat_map(|c| c.actors())
        .chain(world.care.as_ref().into_iter().flat_map(|c| c.actors()))
        .collect();
    busy.par_sort_unstable();
    let suspended = world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick));
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .map(|h| (h.person, h.food))
        .collect();
    if holdings.par_windows(2).any(|p| p[0].0 > p[1].0) {
        holdings.par_sort_unstable_by_key(|h| h.0);
    }
    let current = |person: u64| {
        holdings
            .binary_search_by_key(&person, |row| row.0)
            .ok()
            .map_or(0, |i| holdings[i].1)
    };
    let locate = |person: u64| {
        positions
            .binary_search_by_key(&person, |row| row.0)
            .ok()
            .map(|i| {
                let (_, block, slot) = positions[i];
                (&blocks[block], slot)
            })
    };
    let changes: Vec<_> = gifts
        .par_iter()
        .map(|gift| -> Result<_, String> {
            if gift.amount == 0 || suspended == Some(gift.giver) {
                return Err("invalid or suspended giver".into());
            }
            let (a, ai) = locate(gift.giver).ok_or("unknown giver")?;
            let (b, bi) = locate(gift.recipient).ok_or("unknown recipient")?;
            if a.journey.is_some()
                || b.journey.is_some()
                || a.region != b.region
                || a.health[ai] == 0
                || b.health[bi] == 0
            {
                return Err("giver and recipient must be living and local".into());
            }
            if a.work[ai] != NONE
                || b.work[bi] != NONE
                || busy.binary_search(&gift.giver).is_ok()
                || busy.binary_search(&gift.recipient).is_ok()
            {
                return Err("participant is busy".into());
            }
            let given = current(gift.giver)
                .checked_sub(gift.amount)
                .ok_or("insufficient carried food")?;
            let received = current(gift.recipient)
                .checked_add(gift.amount)
                .filter(|&n| n <= cargo.capacity)
                .ok_or("carrying capacity exceeded")?;
            Ok([(gift.giver, given), (gift.recipient, received)])
        })
        .collect::<Result<_, _>>()?;
    let mut changes: Vec<_> = changes.into_par_iter().flat_map_iter(|pair| pair).collect();
    changes.par_sort_unstable_by_key(|row| row.0);
    let mut next_holdings: Vec<_> = holdings
        .par_iter()
        .filter(|h| changes.binary_search_by_key(&h.0, |row| row.0).is_err())
        .map(|&(person, food)| Holding { person, food })
        .collect();
    next_holdings.par_extend(
        changes
            .into_par_iter()
            .filter_map(|(person, food)| (food > 0).then_some(Holding { person, food })),
    );
    if next_holdings
        .par_windows(2)
        .any(|p| p[0].person > p[1].person)
    {
        next_holdings.par_sort_unstable_by_key(|h| h.person);
    }
    Ok(Cargo {
        capacity: cargo.capacity,
        auto_eat_travel: cargo.auto_eat_travel,
        auto_eat_local: cargo.auto_eat_local,
        personal_food_access: cargo.personal_food_access,
        food_stores: cargo.food_stores.par_iter().cloned().collect(),
        food_store_claims: cargo.food_store_claims.par_iter().cloned().collect(),
        food_store_choices: cargo.food_store_choices.par_iter().cloned().collect(),
        food_depositors: cargo.food_depositors.par_iter().cloned().collect(),
        food_store_blueprints: cargo.food_store_blueprints.par_iter().cloned().collect(),
        food_store_builders: cargo.food_store_builders.par_iter().cloned().collect(),
        inherit_store_building: cargo.inherit_store_building,
        acquire_when_hungry: cargo.acquire_when_hungry.par_iter().copied().collect(),
        food_aid: cargo.food_aid.par_iter().cloned().collect(),
        food_sharing: cargo.food_sharing.par_iter().cloned().collect(),
        inherit_food_sharing: cargo.inherit_food_sharing,
        food_requests: cargo.food_requests.par_iter().copied().collect(),
        next_food_request: cargo.next_food_request,
        food_askers: cargo.food_askers.par_iter().cloned().collect(),
        auto_seek_age: cargo.auto_seek_age,
        food_enrollments: cargo.food_enrollments.par_iter().copied().collect(),
        auto_pack_travel: cargo.auto_pack_travel,
        holdings: next_holdings,
    })
}

/// Apply already validated disjoint transfers after unrelated custody changes.
pub fn apply(cargo: &mut Cargo, gifts: &[FoodGift]) {
    if gifts.is_empty() {
        return;
    }
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .map(|h| (h.person, h.food))
        .collect();
    if holdings.par_windows(2).any(|p| p[0].0 > p[1].0) {
        holdings.par_sort_unstable_by_key(|h| h.0);
    }
    let dense = holdings.first().and_then(|first| {
        holdings.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((holdings.len() - 1) as u64)).then_some(first.0)
        })
    });
    let current = |id: u64| {
        dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < holdings.len())
            .or_else(|| {
                if dense.is_none() {
                    holdings.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
            .map_or(0, |i| holdings[i].1)
    };
    let (mut changes, recipients): (Vec<_>, Vec<_>) = rayon::join(
        || {
            gifts
                .par_iter()
                .map(|gift| (gift.giver, current(gift.giver) - gift.amount))
                .collect()
        },
        || {
            gifts
                .par_iter()
                .map(|gift| (gift.recipient, current(gift.recipient) + gift.amount))
                .collect()
        },
    );
    changes.par_extend(recipients);
    if changes.par_windows(2).any(|p| p[0].0 > p[1].0) {
        changes.par_sort_unstable_by_key(|row| row.0);
    }
    let dense_changes = changes.first().and_then(|first| {
        changes.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((changes.len() - 1) as u64)).then_some(first.0)
        })
    });
    let mut next: Vec<_> = holdings
        .par_iter()
        .filter(|h| {
            if let Some(first) = dense_changes {
                h.0 < first || h.0 - first >= changes.len() as u64
            } else {
                changes.binary_search_by_key(&h.0, |row| row.0).is_err()
            }
        })
        .map(|&(person, food)| Holding { person, food })
        .collect();
    let additions: Vec<_> = changes
        .into_par_iter()
        .filter_map(|(person, food)| (food > 0).then_some(Holding { person, food }))
        .collect();
    if next.is_empty() {
        next = additions;
    } else {
        next.par_extend(additions);
    }
    if next.par_windows(2).any(|p| p[0].person > p[1].person) {
        next.par_sort_unstable_by_key(|h| h.person);
    }
    cargo.holdings = next;
}
