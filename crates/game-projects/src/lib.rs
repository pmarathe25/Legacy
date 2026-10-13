//! Concrete plans and enterprise accounts, held as shared components.
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod output_reads;
pub use output_reads::{SiteOutputGuard, SiteOutputRead, prepare_site_output_read};
mod types;
pub use types::*;
mod storage;
pub use storage::*;
mod transitions;
pub use transitions::*;
mod perception;
#[cfg(test)]
mod perception_tests;
pub use perception::*;
mod opportunities;
#[cfg(test)]
mod opportunities_tests;
pub use opportunities::*;
mod choices;
pub use choices::*;
mod prospects;
pub use prospects::*;
mod financing;
pub use financing::*;
mod judgment;
pub use judgment::*;
mod boundary_validation;
pub use boundary_validation::*;
mod validation;
pub use validation::{validate_absent, validate_runtime};
#[cfg(test)]
mod transition_tests;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: Rules,
    pub last_event: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnterpriseWork {
    pub actor: PersonId,
    pub site: SiteId,
}
/// Opening construction membership and touched rows guard labor exclusivity.
pub struct PreparedAdvance {
    change: PreparedChange,
    jobs: Vec<(PersonId, Construction)>,
}
impl PreparedAdvance {
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if constructions(store)
            .map(|(actor, row)| (actor, row.clone()))
            .collect::<Vec<_>>()
            != self.jobs
        {
            return Err(Error::Unavailable);
        }
        self.change.validate_rows(runtime, store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ProjectFinalization, Error> {
        self.validate(runtime, store)?;
        self.change.stage_rows(runtime, store, batch)
    }
    pub fn commit(self, runtime: &mut Runtime, store: &mut Store) -> Result<Vec<Effect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, &mut batch)?;
        finish.validate_advance_batch(runtime, store, &batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
    }
}
impl Runtime {
    pub fn prepare_advance(
        &self,
        store: &Store,
        from: SimTime,
        next: SimTime,
        work: &[EnterpriseWork],
    ) -> Result<PreparedAdvance, Error> {
        let elapsed = next.0.checked_sub(from.0).ok_or(Error::Overflow)?;
        let jobs = constructions(store)
            .map(|(actor, row)| (actor, row.clone()))
            .collect::<Vec<_>>();
        let mut actors = BTreeSet::new();
        let mut building = BTreeSet::new();
        if work
            .iter()
            .any(|row| !actors.insert(row.actor) || construction(store, row.actor).is_some())
            || jobs.iter().any(|(_, row)| !building.insert(row.project))
        {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedChange::new(self);
        change.guard_jobs(store);
        for (actor, job) in &jobs {
            let before = project(store, job.project).ok_or(Error::Unavailable)?;
            let mut after = before.clone();
            after.progress_ms = after
                .progress_ms
                .checked_add(elapsed)
                .ok_or(Error::Overflow)?;
            let credit = after.contributions.entry(*actor).or_default();
            *credit = credit.checked_add(elapsed).ok_or(Error::Overflow)?;
            change.project(store, job.project, after);
        }
        for input in work {
            change.guard_enterprise_sites(store, input.site);
            if let Some((id, before)) = enterprises(store).find(|(_, row)| row.site == input.site) {
                let mut after = change
                    .enterprises
                    .get(&id)
                    .and_then(|(_, after)| after.clone())
                    .unwrap_or_else(|| before.clone());
                let credit = after.labor.entry(input.actor).or_default();
                *credit = credit.checked_add(elapsed).ok_or(Error::Overflow)?;
                change.enterprise(store, id, after);
            }
        }
        Ok(PreparedAdvance { change, jobs })
    }
    pub fn funding_proposal_bound(&self, store: &Store, next: SimTime) -> Result<u64, Error> {
        projects(store)
            .filter(|(_, plan)| {
                !matches!(
                    plan.status,
                    ProjectStatus::Failed | ProjectStatus::Cancelled
                ) && match &plan.kind {
                    ProjectKind::Workshop { .. } => true,
                    ProjectKind::Farm { .. } => !plan.materials_consumed && plan.finish_by > next,
                }
            })
            .try_fold(0_u64, |total, _| {
                total.checked_add(1).ok_or(Error::Overflow)
            })
    }
}

#[cfg(test)]
mod component_tests;

/// The first stable farm record supplies the authored seed and food endpoints.
pub fn site_output(
    store: &game_ecs::Store,
    site: game_core::SiteId,
    good: game_core::Good,
) -> Option<game_core::InventoryId> {
    output_reads::site_output(store, site, good)
}

#[derive(Clone, Copy)]
pub enum LaborActivity {
    Other,
    Workshop(SiteId),
    Farm(SiteId),
}
#[derive(Clone, Copy)]
pub struct LaborFacts {
    pub actor: PersonId,
    pub alive: bool,
    pub health: u64,
    pub activity: LaborActivity,
}
impl Runtime {
    pub fn prepare_labor_advance(
        &self,
        store: &Store,
        from: SimTime,
        next: SimTime,
        actors: &[LaborFacts],
    ) -> Result<PreparedAdvance, Error> {
        let work = actors
            .iter()
            .filter(|actor| actor.alive && actor.health > 0)
            .filter_map(|actor| match actor.activity {
                LaborActivity::Other => None,
                LaborActivity::Workshop(site) | LaborActivity::Farm(site) => Some(EnterpriseWork {
                    actor: actor.actor,
                    site,
                }),
            })
            .collect::<Vec<_>>();
        self.prepare_advance(store, from, next, &work)
    }
}

pub mod receipt_validation;
