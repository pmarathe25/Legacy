//! Direct bulk admission into fused person-owned work slots.
use super::Kernel;
use game_parallel::StartWork;
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
impl Kernel {
    game_parallel_fused_admission::fused_request_methods!();
    pub fn respond_work(&mut self, id: u64, worker: u64, start: bool) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[worker])?;
        self.edit_world(|world| world.respond_work(id, worker, start))
    }
    pub fn work_without_permission(&mut self, job: StartWork) -> Result<(), String> {
        game_parallel_fused_conflict::check(&self.template, &[job.worker])?;
        self.edit_world(|world| game_parallel_block_property::work_without_permission(world, job))
    }
    fn work_actions(&mut self) -> game_parallel_fused_actions::Context<'_> {
        game_parallel_fused_actions::Context {
            world: &mut self.template,
            blocks: &mut self.blocks,
            positions: &self.positions,
            jobs: &mut self.jobs,
            waiting: &self.waiting,
            live_count: &mut self.live_count,
            revision: &mut self.revision,
        }
    }
    pub fn cancel_work(&mut self, id: u64) -> Result<(), String> {
        self.work_actions().cancel(id)
    }
    pub fn reassign_work(&mut self, id: u64, worker: u64) -> Result<(), String> {
        game_parallel_fused_admission::validate_ages(
            self.owners.family.as_ref(),
            rayon::iter::once(worker),
        )?;
        self.work_actions().reassign(id, worker)
    }
    pub(super) fn people(&self) -> Vec<PersonSnapshot> {
        game_parallel_fused_index::people(&self.blocks, &self.template.regions, &self.positions)
    }
    /// Validate the whole batch against one generation before publishing slots and stock claims.
    pub fn start_work(&mut self, requests: &[StartWork]) -> Result<(), String> {
        if requests.is_empty() {
            return Ok(());
        }
        game_parallel_fused_admission::validate_ages(
            self.owners.family.as_ref(),
            requests.par_iter().map(|request| request.worker),
        )?;
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        if self.template.production.is_none() {
            return Err("production is disabled".into());
        }
        game_parallel_fused_admission::validate_permissions(&self.template, requests)?;
        let min_id = requests.par_iter().map(|request| request.id).min().unwrap();
        let needs_repack = self.jobs.last().is_some_and(|last| min_id <= last.id)
            || (self.live_count > 0 && self.jobs.len() > self.live_count.saturating_mul(2));
        if needs_repack {
            let mut world = self.export();
            world.start_work(requests)?;
            let mut next = Self::compile(world)?;
            next.revision = revision;
            *self = next;
            return Ok(());
        }
        let mut dense = game_parallel_fused_admission::Context {
            blocks: &mut self.blocks,
            template: &mut self.template,
            jobs: &mut self.jobs,
            live_count: &mut self.live_count,
            revision: &mut self.revision,
        };
        if let Some(result) = dense.start_dense(requests, revision) {
            return result;
        }
        let people = game_parallel_fused_index::people(
            dense.blocks,
            &dense.template.regions,
            &self.positions,
        );
        dense.start_generic(requests, revision, &people, &self.positions)
    }
}
