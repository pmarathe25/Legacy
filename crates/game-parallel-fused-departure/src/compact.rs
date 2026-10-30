//! Rebuild the metadata arena from live jobs before staging another generation.
use game_parallel_fused_data::{Block, NONE, uniform_output};
use game_parallel_fused_metadata::Store;
use rayon::prelude::*;
/// Reclaim historical rows when they substantially exceed live work.
pub fn compact_if_needed(
    blocks: &mut [Block],
    jobs: &mut Store,
    waiting: &mut [usize],
    live_count: usize,
) -> Result<(), &'static str> {
    if jobs.len() > live_count.saturating_mul(2).max(256) {
        compact(blocks, jobs, waiting).map_err(|_| "job metadata compaction failed")?;
    }
    Ok(())
}
pub fn compact(
    blocks: &mut [Block],
    jobs: &mut Store,
    waiting: &mut [usize],
) -> Result<(), String> {
    let mut live: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .work
                .iter()
                .zip(&block.remaining)
                .filter_map(|(&handle, &remaining)| {
                    (handle != NONE).then_some((handle, remaining, false))
                })
        })
        .chain(waiting.par_iter().map(|&handle| (handle, 0, true)))
        .collect();
    live.par_sort_unstable_by_key(|row| row.0);
    let rows = live
        .par_iter()
        .map(|&(handle, remaining, waiting)| jobs.job(handle, remaining, waiting))
        .collect();
    let next = Store::new(rows)?;
    let remap = |handle: usize| {
        live.binary_search_by_key(&handle, |row| row.0)
            .expect("live handle")
    };
    blocks.par_iter_mut().for_each(|block| {
        for handle in &mut block.work {
            if *handle != NONE {
                *handle = remap(*handle);
            }
        }
        block.uniform_output = uniform_output(&block.work, &next);
    });
    waiting
        .par_iter_mut()
        .for_each(|handle| *handle = remap(*handle));
    *jobs = next;
    Ok(())
}
