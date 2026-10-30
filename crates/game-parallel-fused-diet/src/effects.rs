//! Bounded, parallel digestion of effects from physically consumed food.
use game_parallel::World;
use game_parallel_fused_data::Block;
use rayon::prelude::*;

pub fn apply_due(
    world: &World,
    blocks: &mut [Block],
    positions: &[(u64, usize, usize)],
    tick: u64,
    max_health: u32,
) {
    let Some(owner) = &world.food_effects else {
        return;
    };
    if owner
        .pending
        .par_windows(2)
        .all(|pair| pair[0].person < pair[1].person)
        && blocks.par_iter().all(|block| {
            block
                .ids
                .first()
                .zip(block.ids.last())
                .is_some_and(|(&first, &last)| {
                    last.checked_sub(first) == Some((block.ids.len() - 1) as u64)
                })
        })
    {
        blocks.par_iter_mut().for_each(|block| {
            let first = block.ids[0];
            let last = *block.ids.last().expect("nonempty block");
            let start = owner.pending.partition_point(|row| row.person < first);
            let end = owner.pending.partition_point(|row| row.person <= last);
            for effect in &owner.pending[start..end] {
                let slot = (effect.person - first) as usize;
                if effect.at <= tick && block.next_health[slot] > 0 {
                    block.next_health[slot] =
                        (i128::from(block.next_health[slot]) + i128::from(effect.health))
                            .clamp(0, i128::from(max_health)) as u32;
                }
            }
        });
        return;
    }
    let mut due: Vec<_> = owner
        .pending
        .par_iter()
        .filter(|row| row.at <= tick)
        .map(|row| (row.person, row.health))
        .collect();
    if due.is_empty() {
        return;
    }
    if due.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        due.par_sort_unstable_by_key(|row| row.0);
    }
    let dense = super::dense_positions(positions);
    let mut slots: Vec<_> = due
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| {
            let (block, slot) = super::person_position(positions, dense, group[0].0)
                .expect("validated food effect person");
            let change = group.par_iter().map(|row| i128::from(row.1)).sum::<i128>();
            (block, slot, change)
        })
        .collect();
    if slots
        .par_windows(2)
        .any(|pair| (pair[0].0, pair[0].1) > (pair[1].0, pair[1].1))
    {
        slots.par_sort_unstable_by_key(|row| (row.0, row.1));
    }
    blocks
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, block)| {
            let first = slots.partition_point(|row| row.0 < index);
            let end = slots.partition_point(|row| row.0 <= index);
            for &(_, slot, change) in &slots[first..end] {
                if block.next_health[slot] > 0 {
                    block.next_health[slot] = (i128::from(block.next_health[slot]) + change)
                        .clamp(0, i128::from(max_health))
                        as u32;
                }
            }
        });
}

pub fn clear_due(world: &mut World, tick: u64) {
    if let Some(owner) = &mut world.food_effects {
        if owner.pending.par_iter().all(|row| row.at <= tick) {
            world.food_effects = None;
            return;
        }
        owner.pending = std::mem::take(&mut owner.pending)
            .into_par_iter()
            .filter(|row| row.at > tick)
            .collect();
        if owner.pending.is_empty() {
            world.food_effects = None;
        }
    }
}
