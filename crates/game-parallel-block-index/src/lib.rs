//! Persistent physical-person projections into owner-visible slots.
use game_parallel_block_data::Block;
use game_parallel_people::Region;
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
pub type Positions = Vec<(usize, usize)>;
pub fn rebuild(blocks: &[Block], regions: &[Region]) -> (Vec<PersonSnapshot>, Positions) {
    let mut rows: Vec<_> = blocks
        .par_iter()
        .enumerate()
        .flat_map_iter(|(block_index, block)| {
            (0..block.ids.len()).map(move |offset| (block.ids[offset], block_index, offset))
        })
        .collect();
    if rows.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        rows.par_sort_unstable_by_key(|row| row.0);
    }
    let positions = rows
        .par_iter()
        .map(|&(_, block, offset)| (block, offset))
        .collect();
    let people = rows
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
        .collect();
    (people, positions)
}
pub fn refresh(
    blocks: &[Block],
    regions: &[Region],
    people: &mut [PersonSnapshot],
    positions: &[(usize, usize)],
) {
    people
        .par_iter_mut()
        .zip(positions.par_iter())
        .for_each(|(person, &(index, offset))| {
            let block = &blocks[index];
            person.1 = block.journey.is_none().then_some(regions[block.region].id);
            person.2 = block.health[offset];
            person.3 = block.hunger[offset];
        });
}
