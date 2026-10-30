//! Stage actual completions and the requester's local perception before publication.
use game_parallel::World;
use game_parallel_fused_data::Block;
use game_parallel_fused_metadata::Store;
use rayon::prelude::*;

pub struct Draft {
    completed: Vec<(u64, bool)>,
    witnessed: Vec<u64>,
    help: Vec<(u64, u64)>,
}
pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    jobs: &Store,
) -> Option<Draft> {
    let requests = world.autowork.as_ref()?.requests.as_ref()?;
    if requests.work_links.is_empty() {
        return None;
    }
    let mut finished: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| block.finished.iter().map(|&handle| jobs.row(handle).id))
        .collect();
    finished.par_sort_unstable();
    let resolved: Vec<_> = requests
        .work_links
        .par_iter()
        .filter_map(|link| {
            if finished.binary_search(&link.job).is_ok() {
                return Some((link, true));
            }
            let dead = positions
                .binary_search_by_key(&link.worker, |row| row.0)
                .map_or(true, |index| {
                    let (_, block, offset) = positions[index];
                    blocks[block].health[offset] == 0
                });
            dead.then_some((link, false))
        })
        .collect();
    if resolved.is_empty() {
        return None;
    }
    let mut witnessed: Vec<u64> = resolved
        .par_iter()
        .filter_map(|(link, _)| {
            let index = positions
                .binary_search_by_key(&link.requester, |row| row.0)
                .ok()?;
            let (_, block, offset) = positions[index];
            let person = &blocks[block];
            let worker_index = positions
                .binary_search_by_key(&link.worker, |row| row.0)
                .ok()?;
            let (_, worker_block, _) = positions[worker_index];
            let worker = &blocks[worker_block];
            (person.journey.is_none()
                && person.health[offset] > 0
                && world.regions[person.region].id == link.region
                && worker.journey.is_none()
                && world.regions[worker.region].id == link.region)
                .then_some(link.job)
        })
        .collect();
    witnessed.par_sort_unstable();
    let mut observations: Vec<_> = resolved
        .par_iter()
        .filter_map(|(link, success)| {
            witnessed.binary_search(&link.job).is_ok().then_some((
                link.requester,
                link.worker,
                link.recipe,
                link.request,
                *success,
            ))
        })
        .collect();
    observations.par_sort_unstable();
    let help = observations
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter_map(|group| {
            let first = group[0];
            first.4.then_some((first.0, first.1))
        })
        .collect();
    Some(Draft {
        help,
        completed: resolved
            .par_iter()
            .map(|(link, success)| (link.job, *success))
            .collect(),
        witnessed,
    })
}
impl Draft {
    pub fn commit(mut self, world: &mut World, tick: u64) {
        if let Some(opinions) = world.opinions.as_mut().filter(|owner| owner.tracks_help()) {
            opinions.learn_help(&self.help);
        }
        self.witnessed.par_sort_unstable();
        world
            .autowork
            .as_mut()
            .unwrap()
            .requests
            .as_mut()
            .unwrap()
            .resolve(&self.completed, tick, |link| {
                self.witnessed.binary_search(&link.job).is_ok()
            });
    }
}
