//! Fused physical and active-job prototype over person-owned blocks.
mod admission;
mod care;
mod cargo;
mod civic;
mod combat;
pub use civic::PropertyCommand;
mod departure;
mod lineage;
mod trade;
use game_parallel::World;
use game_parallel_fused_data::Block;
use game_parallel_fused_metadata::Store;
pub struct Kernel {
    template: World,
    blocks: Vec<Block>,
    food: Vec<u32>,
    jobs: Store,
    owners: game_parallel_fused_import::Owners,
    waiting: Vec<usize>,
    receipts: game_parallel_production_receipts::Receipts,
    positions: Vec<(u64, usize, usize)>,
    live_count: usize,
    revision: u64,
    instance: u64,
    prepared: u64,
    fatigue_dirty: bool,
    disease_dirty: bool,
}
/// A private, unpublished generation prepared from one committed tick.
pub struct Draft {
    tick: u64,
    state: game_parallel_fused_tick::Draft,
    decisions: game_parallel_fused_decisions::Draft,
    live_count: usize,
    revision: u64,
    instance: u64,
    prepared: u64,
}
impl Kernel {
    game_parallel_agreement_host::fused_agreement_methods!();
    game_parallel_goal_host::fused_goal_methods!();
    game_parallel_skill_host::teaching_methods!();
    game_parallel_fused_missions::mission_methods!();
    game_parallel_fused_dependent_care::caregiver_policy_methods!();
    game_parallel_fused_agriculture::volunteer_methods!();
    game_parallel_fused_cargo::food_choice_methods!();
    game_parallel_fused_store_build::builder_methods!();
    game_parallel_fused_diet::diet_methods!();
    game_parallel_fused_decisions::pressure_methods!();
    /// Compile the physical+production subset; unsupported owners are rejected.
    pub fn compile(world: World) -> Result<Self, String> {
        let imported = game_parallel_fused_import::compile(world)?;
        let instance = game_parallel_fused_import::instance()?;
        let live_count = imported.jobs.len();
        let mut template = imported.world;
        let mut blocks = imported.blocks;
        let owners =
            game_parallel_fused_import::owners(&mut template, &mut blocks, &imported.positions)?;
        Ok(Self {
            template,
            owners,
            blocks,
            food: imported.food,
            jobs: imported.jobs,
            waiting: imported.waiting,
            receipts: imported.receipts,
            positions: imported.positions,
            live_count,
            revision: 0,
            instance,
            prepared: 0,
            fatigue_dirty: false,
            disease_dirty: false,
        })
    }
    game_parallel_fused_tick::prepare_methods!();
    pub fn commit(&mut self, mut draft: Draft) -> Result<(), &'static str> {
        if self.instance != draft.instance
            || self.prepared != draft.prepared
            || self.template.tick.checked_add(1) != Some(draft.tick)
            || self.revision.checked_add(1) != Some(draft.revision)
        {
            return Err("stale fused draft");
        }
        game_parallel_fused_departure::Publication {
            blocks: &mut self.blocks,
            world: &mut self.template,
            jobs: &self.jobs,
            positions: &mut self.positions,
            owners: &mut self.owners,
        }
        .commit(draft.state.family);
        game_parallel_fused_cargo::commit(&mut self.template, &draft.state.carried_meals);
        game_parallel_fused_food::commit_stores!(self, draft);
        game_parallel_fused_cargo::commit_acquisitions(
            &mut self.template,
            &draft.state.acquired_food,
        );
        game_parallel_fused_cargo::commit_social_food(&mut self.template, draft.state.social_food);
        if let Some(agriculture) = draft.state.agriculture {
            self.template.agriculture = Some(agriculture);
        }
        if let Some(shipments) = draft.state.shipments {
            self.template.shipments = shipments;
        }
        self.food = draft.state.food;
        self.waiting = draft.state.waiting;
        game_parallel_fused_output::commit(self.template.production.as_mut(), &draft.state.updates);
        self.receipts = draft.state.receipts;
        game_parallel_fused_import::commit_health(
            &mut self.template,
            draft.state.skills,
            (draft.state.goals, draft.state.dreams),
            draft.state.purpose,
            draft.state.disease,
            &mut self.disease_dirty,
            &mut self.fatigue_dirty,
        );
        self.template.tick = draft.tick;
        self.live_count = draft.live_count;
        game_parallel_fused_diet::clear_due(&mut self.template, draft.tick);
        draft.decisions.commit_exchanges(&mut self.template);
        if let Some(diet) = draft.state.diet {
            diet.commit(&mut self.template);
        }
        if let Some(builds) = draft.state.store_builds {
            builds.commit(&mut self.template);
        }
        game_parallel_fused_work_experience::commit_work_memory!(self, draft);
        if let Some(work) = draft.decisions.autowork {
            work.commit(
                &mut self.template,
                &mut self.blocks,
                &self.positions,
                &mut self.jobs,
            );
        }
        if let Some(care) = draft.decisions.care {
            self.template.care = Some(care.state);
        }
        if let Some(conflict) = draft.decisions.conflict {
            if let Some(opinions) = self
                .template
                .opinions
                .as_mut()
                .filter(|opinions| opinions.tracks_harm())
            {
                opinions.learn_harm(&conflict.experienced_harm());
            }
            self.owners
                .conflict
                .as_mut()
                .expect("prepared conflict runtime")
                .commit(
                    self.template.conflict.as_mut().expect("prepared conflict"),
                    self.template.tactics.as_mut(),
                    conflict,
                );
        }
        game_parallel_fused_confrontation::publish!(self, draft);
        draft
            .decisions
            .lineage
            .commit(&mut self.template, draft.tick, self.owners.family.as_ref());
        self.revision = draft.revision;
        if let Some(social) = draft.decisions.society.social {
            self.owners.commit_social(social, &mut self.template);
        }
        if let Some(knowledge) = draft.decisions.society.knowledge {
            self.owners.commit_knowledge(knowledge, &mut self.template);
        }
        game_parallel_fused_departure::clear_empty(&mut self.jobs, self.live_count);
        game_parallel_fused_missions::publish!(self, draft.decisions.missions);
        game_parallel_fused_autonomy::publish!(self, draft.decisions.society.autonomy);
        self.owners.enroll_births(&mut self.template);
        game_parallel_block_property::inheritance::publish(
            &mut self.template.property,
            self.owners.family.as_ref(),
            self.template.tick,
        );
        Ok(())
    }
    pub fn job_storage_len(&self) -> usize {
        self.jobs.len()
    }
    pub fn tick(&self) -> u64 {
        self.template.tick
    }
    pub fn export(&self) -> World {
        game_parallel_fused_import::export::Context {
            template: &self.template,
            blocks: &self.blocks,
            food: &self.food,
            jobs: &self.jobs,
            waiting: &self.waiting,
            receipts: &self.receipts,
            positions: &self.positions,
            owners: &self.owners,
            fatigue_dirty: self.fatigue_dirty,
            disease_dirty: self.disease_dirty,
        }
        .export()
    }
}

#[cfg(test)]
extern crate self as game_parallel_fused_physical;
#[cfg(test)]
#[path = "../tests/support/equivalence.rs"]
mod equivalence;
