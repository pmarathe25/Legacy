//! Import/export of sparse fatigue records around person-owned hot columns.
use game_parallel_fatigue::{Fatigue, Person};
use game_parallel_fused_data::{Block, FatigueColumns};
use rayon::prelude::*;
use std::collections::HashMap;

/// Called only after world validation. Preserve even explicitly stored zero rows.
pub fn import(state: &Fatigue, blocks: &mut [Block]) {
    let records: HashMap<_, _> = state
        .people
        .par_iter()
        .map(|person| (person.id, person))
        .collect();
    blocks.par_iter_mut().for_each(|block| {
        let mut columns = FatigueColumns::empty(block.ids.len());
        for (i, id) in block.ids.iter().enumerate() {
            if let Some(person) = records.get(id) {
                columns.values[i] = person.fatigue;
                columns.flags[i] = 1 | (u8::from(person.resting) << 1);
            }
        }
        block.fatigue = Some(columns);
    });
}
/// Births are appended after publication and begin with no accumulated fatigue.
pub fn ensure(blocks: &mut [Block]) {
    blocks.par_iter_mut().for_each(|block| {
        if block.fatigue.is_none() {
            block.fatigue = Some(FatigueColumns::empty(block.ids.len()));
        }
    });
}
/// Materialize sparse rows only at an external observation or persistence boundary.
pub fn export(state: &mut Fatigue, blocks: &[Block], positions: &[(u64, usize, usize)]) {
    state.people = positions
        .par_iter()
        .filter_map(|&(id, block, offset)| {
            let columns = blocks[block].fatigue.as_ref().expect("fatigue columns");
            (columns.flags[offset] & 1 != 0).then_some(Person {
                id,
                fatigue: columns.values[offset],
                resting: columns.flags[offset] & 2 != 0,
            })
        })
        .collect();
}
