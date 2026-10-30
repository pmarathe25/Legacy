//! Stage and apply personal purpose effects against private physical scratch.
use game_parallel::{Goals, Purpose, PurposeEvent, PurposeOutcome};
use game_parallel_fused_data::Block;
use rayon::prelude::*;
fn locate(positions: &[(u64, usize, usize)], dense: Option<u64>, id: u64) -> Option<usize> {
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
}
fn dense_first(positions: &[(u64, usize, usize)]) -> Option<u64> {
    let first = positions.first()?.0;
    (positions.last()?.0.checked_sub(first) == Some((positions.len() - 1) as u64)).then_some(first)
}
pub fn prepare(
    purpose: &Purpose,
    goals: Option<&Goals>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    tick: u64,
) -> Purpose {
    let events: Vec<_> = goals
        .into_iter()
        .flat_map(|goals| &goals.events)
        .map(|event| PurposeEvent {
            person: event.person,
            outcome: if event.blocked {
                PurposeOutcome::Blocked
            } else {
                PurposeOutcome::Progress
            },
        })
        .collect();
    let dense = dense_first(positions);
    purpose.prepare(&events, tick, |id| {
        locate(positions, dense, id).is_some_and(|i| {
            let (_, block, offset) = positions[i];
            blocks[block].health[offset] > 0
        })
    })
}
pub fn apply(
    purpose: &Purpose,
    blocks: &mut [Block],
    positions: &[(u64, usize, usize)],
    tick: u64,
) {
    if purpose.rules.damage == 0 {
        return;
    }
    let dense = dense_first(positions);
    let mut slots: Vec<_> = purpose
        .people
        .par_iter()
        .filter(|row| tick >= row.eligible_at && row.deprivation >= purpose.rules.damage_threshold)
        .filter_map(|row| {
            locate(positions, dense, row.person).map(|i| (positions[i].1, positions[i].2))
        })
        .collect();
    if slots.par_windows(2).any(|pair| pair[0] > pair[1]) {
        slots.par_sort_unstable();
    }
    blocks
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, block)| {
            let start = slots.partition_point(|row| row.0 < index);
            let end = slots.partition_point(|row| row.0 <= index);
            for &(_, offset) in &slots[start..end] {
                if block.health[offset] > 0 {
                    block.next_health[offset] =
                        block.next_health[offset].saturating_sub(purpose.rules.damage);
                }
            }
        });
}
