//! Atomic, bounded request admission; the addressed person remains free to ignore it.
use game_parallel::{FoodRequest, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;
pub fn admit(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    requests: &[FoodRequest],
) -> Result<(), String> {
    let cargo = world
        .cargo
        .as_ref()
        .filter(|cargo| cargo.personal_food_access)
        .ok_or("personal food access is disabled")?;
    let mut ids: Vec<_> = cargo
        .food_requests
        .par_iter()
        .chain(requests.par_iter())
        .map(|r| r.id)
        .collect();
    let mut askers: Vec<_> = cargo
        .food_requests
        .par_iter()
        .chain(requests.par_iter())
        .map(|r| r.requester)
        .collect();
    if ids.par_windows(2).any(|p| p[0] > p[1]) {
        ids.par_sort_unstable();
    }
    if askers.par_windows(2).any(|p| p[0] > p[1]) {
        askers.par_sort_unstable();
    }
    if ids.par_windows(2).any(|p| p[0] == p[1]) || askers.par_windows(2).any(|p| p[0] == p[1]) {
        return Err("duplicate food request or requester".into());
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
    let dense = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let locate = |id: u64| {
        dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < positions.len())
            .or_else(|| {
                if dense.is_none() {
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
    if requests.par_iter().any(|r| {
        if r.amount == 0
            || r.requester == r.giver
            || r.expires_at <= world.tick
            || suspended == Some(r.requester)
            || busy.binary_search(&r.requester).is_ok()
        {
            return true;
        }
        let (Some((a, ai)), Some((b, bi))) = (locate(r.requester), locate(r.giver)) else {
            return true;
        };
        a.journey.is_some()
            || b.journey.is_some()
            || a.region != b.region
            || a.health[ai] == 0
            || b.health[bi] == 0
            || a.work[ai] != NONE
    }) {
        return Err(
            "requester and addressed person must be living and local; requester must be available"
                .into(),
        );
    }
    Ok(())
}
