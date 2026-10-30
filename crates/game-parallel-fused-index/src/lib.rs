//! Route prevalidated jobs into disjoint fused person blocks.
use game_parallel_packed_jobs_model::Block;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
pub struct Assignment {
    pub block: usize,
    pub offset: usize,
    pub handle: usize,
}
pub fn assignments(
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    people: &[PersonSnapshot],
    base: usize,
) -> Result<Vec<Assignment>, String> {
    let mut next_handle = base;
    let mut offsets = Vec::with_capacity(blocks.len());
    for block in blocks {
        offsets.push(next_handle);
        next_handle = next_handle
            .checked_add(block.rows.len())
            .ok_or("job count overflow")?;
    }
    let mut assignments: Vec<_> = blocks
        .par_iter()
        .zip(offsets.par_iter())
        .flat_map_iter(|(block, &start)| {
            block.rows.iter().enumerate().map(move |(offset, row)| {
                let index = person_index(people, row.worker).expect("validated worker");
                let (_, block, slot) = positions[index];
                Assignment {
                    block,
                    offset: slot,
                    handle: start + offset,
                }
            })
        })
        .collect();
    if assignments
        .par_windows(2)
        .any(|pair| (pair[0].block, pair[0].offset) > (pair[1].block, pair[1].offset))
    {
        assignments.par_sort_unstable_by_key(|row| (row.block, row.offset));
    }
    Ok(assignments)
}

/// Materialize a compatibility view for owners that have not yet adopted block views.
pub fn people(
    blocks: &[game_parallel_fused_data::Block],
    regions: &[game_parallel_people::Region],
    positions: &[(u64, usize, usize)],
) -> Vec<PersonSnapshot> {
    positions
        .par_iter()
        .map(|&(id, block, offset)| {
            let block = &blocks[block];
            (
                id,
                block.journey.is_none().then_some(regions[block.region].id),
                block.health[offset],
                block.hunger[offset],
            )
        })
        .collect()
}
