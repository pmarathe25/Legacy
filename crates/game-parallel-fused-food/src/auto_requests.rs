//! Generate bounded, individually addressed food requests.
use game_parallel::{FoodRequest, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;
use std::cmp::Reverse;
pub struct AutomaticRequests {
    pub requests: Vec<FoodRequest>,
    pub actors: Vec<u64>,
    pub next_id: u64,
}
pub fn prepare_requests(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
    tick: u64,
) -> Result<Option<AutomaticRequests>, &'static str> {
    let Some(cargo) = world
        .cargo
        .as_ref()
        .filter(|cargo| !cargo.food_askers.is_empty())
    else {
        return Ok(None);
    };
    let mut busy = occupied.to_vec();
    busy.extend(world.conflict.as_ref().into_iter().flat_map(|c| c.actors()));
    busy.extend(world.care.as_ref().into_iter().flat_map(|c| c.actors()));
    if busy.par_windows(2).any(|p| p[0] > p[1]) {
        busy.par_sort_unstable();
    }
    let mut pending: Vec<_> = cargo
        .food_requests
        .par_iter()
        .map(|request| request.requester)
        .collect();
    if pending.par_windows(2).any(|p| p[0] > p[1]) {
        pending.par_sort_unstable();
    }
    if !cargo.food_askers.par_iter().any(|policy| {
        policy.enabled
            && tick.is_multiple_of(policy.interval)
            && busy.binary_search(&policy.person).is_err()
            && pending.binary_search(&policy.person).is_err()
    }) {
        return Ok(None);
    }
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .map(|h| (h.person, h.food))
        .collect();
    if holdings.par_windows(2).any(|p| p[0].0 > p[1].0) {
        holdings.par_sort_unstable_by_key(|h| h.0);
    }
    let dense_positions = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let dense_holdings = holdings.first().and_then(|first| {
        holdings.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((holdings.len() - 1) as u64)).then_some(first.0)
        })
    });
    let index = |id: u64, first: Option<u64>, len: usize| {
        first
            .and_then(|base| id.checked_sub(base))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < len)
    };
    let locate = |id| {
        index(id, dense_positions, positions.len())
            .or_else(|| {
                if dense_positions.is_none() {
                    positions.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
            .map(|i| {
                let (_, block, slot) = positions[i];
                (&blocks[block], slot)
            })
    };
    let held_by = |id| {
        index(id, dense_holdings, holdings.len())
            .or_else(|| {
                if dense_holdings.is_none() {
                    holdings.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
            .map_or(0, |i| holdings[i].1)
    };
    let mut known: Vec<_> = if cargo
        .food_askers
        .par_iter()
        .any(|policy| policy.choose_known)
    {
        world.opinions.as_ref().map_or_else(Vec::new, |opinions| {
            opinions
                .records
                .par_iter()
                .filter(|opinion| opinion.obligation > 0)
                .collect()
        })
    } else {
        Vec::new()
    };
    known.par_sort_unstable_by_key(|opinion| {
        (
            opinion.observer,
            Reverse(opinion.obligation),
            opinion.target,
        )
    });
    let mut candidates: Vec<_> = cargo
        .food_askers
        .par_iter()
        .filter_map(|policy| {
            if !policy.enabled
                || !tick.is_multiple_of(policy.interval)
                || busy.binary_search(&policy.person).is_ok()
                || pending.binary_search(&policy.person).is_ok()
            {
                return None;
            }
            let (a, ai) = locate(policy.person)?;
            let giver = if policy.choose_known {
                let first = known.partition_point(|row| row.observer < policy.person);
                let last = known.partition_point(|row| row.observer <= policy.person);
                known[first..last]
                    .iter()
                    .take(32)
                    .find_map(|row| {
                        let (candidate, slot) = locate(row.target)?;
                        (candidate.journey.is_none()
                            && candidate.region == a.region
                            && candidate.health[slot] > 0)
                            .then_some(row.target)
                    })
                    .unwrap_or(policy.giver)
            } else {
                policy.giver
            };
            let (b, bi) = locate(giver)?;
            let held = held_by(policy.person);
            let actively_working = a.work[ai] != NONE
                && a.hunger[ai] <= world.production.as_ref().map_or(0, |p| p.max_work_hunger);
            if a.journey.is_some()
                || b.journey.is_some()
                || a.region != b.region
                || a.health[ai] == 0
                || b.health[bi] == 0
                || actively_working
                || a.fatigue
                    .as_ref()
                    .is_some_and(|state| state.flags[ai] & 2 != 0)
                || held >= world.rules.food_per_meal
                || a.hunger[ai].saturating_add(world.rules.hunger_per_tick)
                    < world.rules.hunger_satisfied
            {
                return None;
            }
            Some((policy, giver))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }
    if candidates
        .par_windows(2)
        .any(|p| p[0].0.person > p[1].0.person)
    {
        candidates.par_sort_unstable_by_key(|a| a.0.person);
    }
    let next_id = cargo
        .next_food_request
        .checked_add(candidates.len() as u64)
        .ok_or("food request id overflow")?;
    let requests: Vec<_> = candidates
        .par_iter()
        .enumerate()
        .map(|(i, &(policy, giver))| {
            Ok(FoodRequest {
                id: cargo.next_food_request + i as u64,
                requester: policy.person,
                giver,
                amount: policy.amount,
                expires_at: tick
                    .checked_add(policy.lifetime)
                    .ok_or("food request expiry overflow")?,
            })
        })
        .collect::<Result<_, &'static str>>()?;
    let actors = candidates
        .par_iter()
        .map(|(policy, _)| policy.person)
        .collect();
    Ok(Some(AutomaticRequests {
        requests,
        actors,
        next_id,
    }))
}
