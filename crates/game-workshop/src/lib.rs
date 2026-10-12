//! Workshop admission, paused work, exact progress and physical production.
//! Permissions, equipment allocation and bodily agency are supplied facts.
use game_cargo::{Inventory, InventoryLocation};
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
mod recipes;
pub use recipes::{
    Catalog, Definitions, ProductionRequirement, Recipe, RecipeId, RecipeView, SiteBindings,
};
mod components;
mod equipment;
pub use components::{
    insert_job, insert_site, job, job_mut, jobs, register_components, remove_job, remove_site,
    site, site_mut, sites,
};
pub use equipment::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub site: SiteId,
    pub remaining: u64,
    pub rate: SkillRate,
    pub body_rate: SkillRate,
    pub work_remainder: u64,
    pub tool: Option<InventoryId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub recipe: RecipeId,
    pub id: SiteId,
    pub area: AreaId,
    pub input: InventoryId,
    pub tool: InventoryId,
    pub output: InventoryId,
    pub access: BTreeSet<PersonId>,
    pub last_granted: Option<PersonId>,
}

#[derive(Clone, Copy, Debug)]
pub struct StartFacts {
    pub actor: PersonId,
    pub area: AreaId,
    pub capable: bool,
    pub adult: bool,
    pub fatigue_allows: bool,
    pub site_access: bool,
    pub occupied_by_other: bool,
    pub input_usable: bool,
    pub input_unreserved: Quantity,
    pub tool_usable: bool,
    pub rate: SkillRate,
    pub body_rate: SkillRate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Consumption {
    pub source: InventoryId,
    pub quantity: Quantity,
    expected_input: Inventory,
    admitted_quantity: Quantity,
}

impl Consumption {
    /// Apply the admitted recipe input without creating a second stock ledger.
    /// The host validates permission/finance effects before this mutation.
    pub fn commit(&self, input: &mut Inventory) -> Result<(), Error> {
        if self.source != self.expected_input.id
            || self.quantity != self.admitted_quantity
            || input != &self.expected_input
            || input.room().is_err()
        {
            return Err(Error::Unavailable);
        }
        let remaining = input
            .quantity
            .0
            .checked_sub(self.quantity.0)
            .ok_or(Error::Unavailable)?;
        input.quantity = Quantity(remaining);
        Ok(())
    }
}

/// Admission leaves stocks and the old job unchanged. The host preflights its
/// finance and event effects before applying this job and consumption together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub job: Job,
    pub consumption: Option<Consumption>,
}

impl RecipeView<'_> {
    pub fn prepare_start(
        &self,
        previous: Option<&Job>,
        input: &Inventory,
        tool: &Inventory,
        output: &Inventory,
        facts: StartFacts,
    ) -> Result<Start, Error> {
        self.recipe.validate()?;
        if input.good != self.recipe.input_good
            || output.good != self.recipe.output_good
            || !facts.capable
            || !facts.adult
            || !facts.fatigue_allows
            || !facts.site_access
            || facts.occupied_by_other
            || facts.actor.0 == 0
            || facts.area != self.site.area
            || input.id != self.site.input
            || output.id != self.site.output
            || input.location != InventoryLocation::Area(self.site.area)
            || output.location != InventoryLocation::Area(self.site.area)
            || !facts.tool_usable
            || tool.good != Good::Tool
            || tool.quantity.0 < 1_000
            || tool.room().is_err()
            || input.room().is_err()
            || output.room()? < self.recipe.output_quantity
            || self.recipe.duration.0 == 0
            || self.recipe.input_quantity.0 == 0
            || self.recipe.output_quantity.0 == 0
        {
            return Err(Error::Unavailable);
        }
        if let Some(previous) = previous.filter(|job| job.site == self.site.id) {
            game_skills::advance_work(0, previous.rate, previous.work_remainder)?;
            game_skills::advance_work(0, previous.body_rate, 0)?;
            if previous.remaining > self.recipe.duration.0 {
                return Err(Error::Unavailable);
            }
            let mut job = previous.clone();
            job.tool = Some(tool.id);
            return Ok(Start {
                job,
                consumption: None,
            });
        }
        if !facts.input_usable
            || facts.input_unreserved < self.recipe.input_quantity
            || input.quantity < self.recipe.input_quantity
        {
            return Err(Error::Unavailable);
        }
        // Validate frozen rates before any stock can be consumed.
        game_skills::advance_work(0, facts.rate, 0)?;
        game_skills::advance_work(0, facts.body_rate, 0)?;
        Ok(Start {
            job: Job {
                site: self.site.id,
                remaining: self.recipe.duration.0,
                rate: facts.rate,
                body_rate: facts.body_rate,
                work_remainder: 0,
                tool: Some(tool.id),
            },
            consumption: Some(Consumption {
                source: self.site.input,
                quantity: self.recipe.input_quantity,
                expected_input: input.clone(),
                admitted_quantity: self.recipe.input_quantity,
            }),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub remaining: u64,
    pub remainder: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct CompletionFacts {
    pub actor: PersonId,
    pub active: bool,
    /// A worker alive at the opening boundary may finish in the interval that
    /// kills them. Death and output are then both committed by the host.
    pub alive_at_start: bool,
    pub equipped: bool,
}

impl Job {
    pub fn practice(
        &self,
        actor: PersonId,
        elapsed: u64,
        active: bool,
        equipped: bool,
    ) -> Option<game_skills::PracticeDelta> {
        (elapsed > 0 && active && equipped).then_some(game_skills::PracticeDelta {
            actor,
            skill: Skill::Craft,
            milliseconds: elapsed,
            mastery_cap: None,
        })
    }

    /// Effective rate includes live skill/body/invocation facts from their
    /// owners. Interruption preserves the frozen job and fractional work.
    pub fn evaluate(
        &self,
        elapsed: u64,
        effective_rate: SkillRate,
        active: bool,
        equipped: bool,
    ) -> Result<Progress, Error> {
        if !active || !equipped {
            return Ok(Progress {
                remaining: self.remaining,
                remainder: self.work_remainder,
            });
        }
        let (work, remainder) =
            game_skills::advance_work(elapsed, effective_rate, self.work_remainder)?;
        Ok(Progress {
            remaining: self.remaining.saturating_sub(work),
            remainder,
        })
    }

    pub fn completion_after(&self, effective_rate: SkillRate) -> Result<u64, Error> {
        game_skills::work_duration(self.remaining, effective_rate, self.work_remainder)
    }

    pub fn apply_progress(&mut self, progress: Progress) {
        self.remaining = progress.remaining;
        self.work_remainder = progress.remainder;
    }

    pub fn output_forecast(
        &self,
        progress: Progress,
        site: RecipeView<'_>,
    ) -> Option<(SiteId, Good, Quantity)> {
        (self.site == site.site.id && progress.remaining == 0).then_some((
            site.site.id,
            site.recipe.output_good,
            site.recipe.output_quantity,
        ))
    }

    pub fn prepare_completion(
        &self,
        site: RecipeView<'_>,
        output: &Inventory,
        facts: CompletionFacts,
    ) -> Result<Option<Production>, Error> {
        if self.site != site.site.id
            || output.id != site.site.output
            || output.good != site.recipe.output_good
            || facts.actor.0 == 0
        {
            return Err(Error::Unavailable);
        }
        if self.remaining != 0
            || !facts.active
            || !facts.alive_at_start
            || !facts.equipped
            || output.room()? < site.recipe.output_quantity
        {
            return Ok(None);
        }
        Ok(Some(Production {
            actor: facts.actor,
            site: site.site.id,
            output: output.id,
            quantity: site.recipe.output_quantity,
            expected_job: self.clone(),
            expected_output: output.clone(),
            expected_site: site.site.clone(),
            expected_recipe: site.recipe.clone(),
            admitted_actor: facts.actor,
        }))
    }
}

/// Cancellation discards already-spent input; ordinary pauses retain the job.
pub fn cancel(job: &mut Option<Job>) {
    *job = None;
}

pub fn interrupted(fatigue_allows: bool, equipped: bool) -> bool {
    !fatigue_allows || !equipped
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Production {
    pub actor: PersonId,
    pub site: SiteId,
    pub output: InventoryId,
    pub quantity: Quantity,
    expected_job: Job,
    expected_output: Inventory,
    expected_site: Site,
    expected_recipe: Recipe,
    admitted_actor: PersonId,
}

impl Production {
    /// Read-only admission of the exact job, site and stock rows captured at
    /// completion. Host finance and event guards run before publication.
    pub fn validate(
        &self,
        job: Option<&Job>,
        site: RecipeView<'_>,
        output: &Inventory,
    ) -> Result<(), Error> {
        if self.actor != self.admitted_actor
            || self.site != self.expected_site.id
            || self.output != self.expected_output.id
            || self.quantity != self.expected_recipe.output_quantity
            || job != Some(&self.expected_job)
            || site.site != &self.expected_site
            || site.recipe != &self.expected_recipe
            || output != &self.expected_output
            || output.room()? < self.quantity
        {
            return Err(Error::Unavailable);
        }
        output
            .quantity
            .0
            .checked_add(self.quantity.0)
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    /// All guards run before the output, job or fairness cursor is changed.
    /// The host publishes the corresponding Produced receipt after success.
    pub fn commit(
        self,
        definitions: &Definitions,
        job: &mut Option<Job>,
        site: &mut Site,
        output: &mut Inventory,
    ) -> Result<(), Error> {
        self.validate(job.as_ref(), definitions.resolve_site(site)?, output)?;
        output.quantity.0 += self.quantity.0;
        *job = None;
        site.last_granted = Some(self.actor);
        Ok(())
    }

    /// Completes the authoritative job component without an aggregate actor or
    /// a duplicate job slot. Removing the component is the only structural edit;
    /// the host refreshes its query bindings after the commit barrier.
    pub fn commit_to_store(
        self,
        definitions: &Definitions,
        store: &mut game_ecs::Store,
        site: &mut Site,
        output: &mut Inventory,
    ) -> Result<(), Error> {
        self.validate(
            job(store, self.actor),
            definitions.resolve_site(site)?,
            output,
        )?;
        remove_job(store, self.actor)?;
        output.quantity.0 += self.quantity.0;
        site.last_granted = Some(self.actor);
        Ok(())
    }

    /// Publishes the authoritative job, site and output stock together. All
    /// opening guards and registered component validators run before any write.
    pub fn commit_stocks(
        self,
        definitions: &Definitions,
        store: &mut game_ecs::Store,
    ) -> Result<(), Error> {
        let declaration = site(store, self.site).ok_or(Error::Unavailable)?;
        let output = game_cargo::inventory(store, self.output).ok_or(Error::Unavailable)?;
        self.validate(
            job(store, self.actor),
            definitions.resolve_site(declaration)?,
            output,
        )?;
        let mut after_site = declaration.clone();
        let mut after_output = output.clone();
        after_site.last_granted = Some(self.actor);
        after_output.quantity.0 = after_output
            .quantity
            .0
            .checked_add(self.quantity.0)
            .ok_or(Error::Overflow)?;
        let mut batch = game_ecs::Batch::new();
        batch.remove::<Job>(game_ecs::StableId(self.actor.0));
        batch.insert(game_ecs::StableId(self.site.0), after_site);
        batch.insert(game_ecs::StableId(self.output.0), after_output);
        store.apply_batch(batch)
    }

    /// Completes a job and updates its site in the host's shared store. The
    /// output stock remains a cargo-owned row supplied by the host.
    pub fn commit_components(
        self,
        definitions: &Definitions,
        store: &mut game_ecs::Store,
        output: &mut Inventory,
    ) -> Result<(), Error> {
        let declaration = site(store, self.site).ok_or(Error::Unavailable)?;
        self.validate(
            job(store, self.actor),
            definitions.resolve_site(declaration)?,
            output,
        )?;
        remove_job(store, self.actor)?;
        // Removing a Job preserves its entity and every other component,
        // including a Site when actor and site happen to share a stable ID.
        site_mut(store, self.site)
            .expect("validated site survives job removal")
            .last_granted = Some(self.actor);
        output.quantity.0 += self.quantity.0;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
