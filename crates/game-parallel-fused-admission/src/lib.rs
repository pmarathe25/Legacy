//! Direct admission for dense, ID-ordered worker cohorts.
use game_parallel::StartWork;
use game_parallel::World;
use game_parallel_fused_data::{Block, NONE};
use game_parallel_fused_metadata::Store;
use game_parallel_production_model::{PersonSnapshot, person_index};
use game_parallel_production_packed::PackedJobs;
use rayon::prelude::*;
#[macro_export]
macro_rules! fused_request_methods {
    () => {
        pub fn request_work(
            &mut self,
            proposal: game_parallel::work_requests::Proposal,
        ) -> Result<u64, String> {
            Ok(self.request_work_batch(&[proposal])?[0])
        }
        pub fn request_work_batch(
            &mut self,
            proposals: &[game_parallel::work_requests::Proposal],
        ) -> Result<Vec<u64>, String> {
            $crate::request_work_batch(
                &mut self.template,
                &self.blocks,
                &self.positions,
                &mut self.revision,
                proposals,
            )
        }
    };
}
/// Admit addressed work requests directly into the current generation.
pub fn request_work_batch(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    revision: &mut u64,
    proposals: &[game_parallel::work_requests::Proposal],
) -> Result<Vec<u64>, String> {
    if proposals.is_empty() {
        return Ok(Vec::new());
    }
    let actors: Vec<_> = proposals.par_iter().map(|p| p.requester).collect();
    game_parallel_fused_conflict::check(world, &actors)?;
    let suspended = world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick));
    if actors.par_iter().any(|&actor| Some(actor) == suspended) {
        return Err("player control is suspended".into());
    }
    let dense = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    if actors.par_iter().any(|actor| {
        let index = dense
            .and_then(|first| actor.checked_sub(first))
            .and_then(|index| usize::try_from(index).ok())
            .filter(|&index| index < positions.len())
            .or_else(|| positions.binary_search_by_key(actor, |row| row.0).ok());
        index.is_some_and(|i| {
            let (_, block, offset) = positions[i];
            blocks[block].work[offset] != NONE
        })
    }) {
        return Err("requester is busy".into());
    }
    let next_revision = revision.checked_add(1).ok_or("generation overflow")?;
    let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
    let production = world.production.as_ref().ok_or("production is disabled")?;
    let requests = world
        .autowork
        .as_mut()
        .ok_or("autonomous work is disabled")?
        .requests
        .as_mut()
        .ok_or("work requests are disabled")?;
    let ids = requests.open_batch(proposals, &people, production, world.tick)?;
    *revision = next_revision;
    Ok(ids)
}
pub fn validate_permissions(world: &World, requests: &[StartWork]) -> Result<(), String> {
    if world.conflict.is_some() || world.care.is_some() {
        game_parallel_fused_conflict::check(
            world,
            &requests
                .par_iter()
                .map(|request| request.worker)
                .collect::<Vec<_>>(),
        )?;
    }
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        .is_some_and(|person| requests.par_iter().any(|request| request.worker == person))
    {
        return Err("player control is suspended".into());
    }
    if let Some(property) = &world.property {
        property.check_jobs(&world.production.as_ref().unwrap().recipes, requests)?;
    }
    Ok(())
}
/// Validate ages without materializing editable family records.
pub fn validate_ages(
    family: Option<&game_parallel_family_lifecycle::Runtime>,
    workers: impl rayon::iter::ParallelIterator<Item = u64>,
) -> Result<(), String> {
    let Some(family) = family.filter(|family| family.rules.min_work_age > 0) else {
        return Ok(());
    };
    if workers.any(|person| {
        family
            .store
            .locate(person)
            .is_some_and(|slot| family.store.state(slot, false).age < family.rules.min_work_age)
    }) {
        return Err("person is too young for work".into());
    }
    Ok(())
}
pub struct Context<'a> {
    pub blocks: &'a mut [Block],
    pub template: &'a mut World,
    pub jobs: &'a mut Store,
    pub live_count: &'a mut usize,
    pub revision: &'a mut u64,
}
impl Context<'_> {
    pub fn start_dense(
        &mut self,
        requests: &[StartWork],
        revision: u64,
    ) -> Option<Result<(), String>> {
        if requests.len() < 4096
            || requests.par_windows(2).any(|pair| {
                pair[0].id >= pair[1].id || pair[0].worker.checked_add(1) != Some(pair[1].worker)
            })
        {
            return None;
        }
        let first = requests[0].worker;
        let locate = |id: u64| {
            id.checked_sub(first)
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < requests.len())
        };
        let checks: Vec<(usize, bool, bool)> = self
            .blocks
            .par_iter()
            .map(|block| {
                let mut matched = 0usize;
                let mut invalid = false;
                let mut busy = false;
                for (offset, &id) in block.ids.iter().enumerate() {
                    if let Some(index) = locate(id) {
                        let request = &requests[index];
                        matched += 1;
                        invalid |= block.journey.is_some()
                            || request.worker != id
                            || request.region != self.template.regions[block.region].id
                            || block.health[offset] == 0;
                        busy |= block.work[offset] != NONE;
                    }
                }
                (matched, invalid, busy)
            })
            .collect();
        if checks.iter().any(|row| row.1)
            || checks.iter().map(|row| row.0).sum::<usize>() != requests.len()
        {
            return Some(Err("worker must be living and local".into()));
        }
        if checks.iter().any(|row| row.2) {
            return Some(Err("duplicate job or busy worker".into()));
        }
        let state = self
            .template
            .production
            .as_ref()
            .expect("compiled production");
        let draft = match PackedJobs::from_jobs(&[]).prepare_planned(
            &state.recipes,
            &state.inventories,
            &[],
            requests,
        ) {
            Ok(draft) => draft,
            Err(error) => return Some(Err(error)),
        };
        let (blocks, needs) = draft.into_parts();
        let base = self.jobs.len();
        let Some(live) = (*self.live_count).checked_add(requests.len()) else {
            return Some(Err("job count overflow".into()));
        };
        if base.checked_add(requests.len()).is_none() {
            return Some(Err("job count overflow".into()));
        }
        if let Err(error) = self.jobs.append_blocks(blocks) {
            return Some(Err(error));
        }
        let jobs = &self.jobs;
        self.blocks.par_iter_mut().for_each(|block| {
            let mut changed = false;
            for (offset, &id) in block.ids.iter().enumerate() {
                if let Some(index) = locate(id) {
                    let handle = base + index;
                    block.work[offset] = handle;
                    block.remaining[offset] = jobs.row(handle).remaining;
                    block.work_local[offset] = true;
                    changed = true;
                }
            }
            if changed {
                block.uniform_output = game_parallel_fused_data::uniform_output(&block.work, jobs);
            }
        });
        self.template
            .production
            .as_mut()
            .unwrap()
            .inventories
            .par_iter_mut()
            .for_each(|inventory| {
                for stock in &mut inventory.stocks {
                    if let Some(amount) = needs.get(&(inventory.region, stock.good)) {
                        stock.quantity -= *amount as u64;
                    }
                }
            });
        *self.live_count = live;
        *self.revision = revision;
        Some(Ok(()))
    }
    pub fn start_generic(
        &mut self,
        requests: &[StartWork],
        revision: u64,
        people: &[PersonSnapshot],
        positions: &[(u64, usize, usize)],
    ) -> Result<(), String> {
        let production = self
            .template
            .production
            .as_ref()
            .expect("compiled production");
        let empty = PackedJobs::from_jobs(&[]);
        let draft = empty.prepare_start(
            &production.recipes,
            &production.inventories,
            people,
            requests,
        )?;
        if requests.par_iter().any(|request| {
            let index = person_index(people, request.worker).expect("validated worker");
            let (_, block, offset) = positions[index];
            self.blocks[block].work[offset] != NONE
        }) {
            return Err("duplicate job or busy worker".into());
        }
        let (blocks, needs) = draft.into_parts();
        let base = self.jobs.len();
        let count: usize = blocks.iter().map(|block| block.rows.len()).sum();
        base.checked_add(count).ok_or("job count overflow")?;
        let live = self
            .live_count
            .checked_add(count)
            .ok_or("job count overflow")?;
        let assignments = game_parallel_fused_index::assignments(&blocks, positions, people, base)?;
        self.jobs.append_blocks(blocks)?;
        let jobs = &self.jobs;
        let regions = &self.template.regions;
        self.blocks
            .par_iter_mut()
            .enumerate()
            .for_each(|(block_index, block)| {
                let start = assignments.partition_point(|row| row.block < block_index);
                let end = assignments.partition_point(|row| row.block <= block_index);
                if start == end {
                    return;
                }
                for row in &assignments[start..end] {
                    let job = jobs.row(row.handle);
                    block.work[row.offset] = row.handle;
                    block.remaining[row.offset] = job.remaining;
                    block.work_local[row.offset] = job.region == regions[block.region].id;
                }
                block.uniform_output = game_parallel_fused_data::uniform_output(&block.work, jobs);
            });
        self.template
            .production
            .as_mut()
            .unwrap()
            .inventories
            .par_iter_mut()
            .for_each(|inventory| {
                for stock in &mut inventory.stocks {
                    if let Some(amount) = needs.get(&(inventory.region, stock.good)) {
                        stock.quantity -= *amount as u64;
                    }
                }
            });
        *self.live_count = live;
        *self.revision = revision;
        Ok(())
    }
}
