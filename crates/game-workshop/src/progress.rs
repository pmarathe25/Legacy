//! Prepared work admission and interval progress over canonical Workshop rows.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

#[derive(Clone, Copy, Debug)]
pub struct ProgressFacts {
    pub elapsed: u64,
    pub effective_rate: SkillRate,
    pub active: bool,
    pub equipped: bool,
}
#[derive(Clone, Debug)]
pub struct PreparedProgress {
    actor: PersonId,
    before: Job,
    after: Job,
    site: Site,
    recipe: Recipe,
}
#[derive(Clone, Debug)]
pub struct JobPublication {
    recipe_id: RecipeId,
    recipe: Recipe,
    guard: ReadGuard,
}
impl Definitions {
    pub fn prepare_progress(
        &self,
        store: &Store,
        actor: PersonId,
        facts: ProgressFacts,
    ) -> Result<Option<PreparedProgress>, Error> {
        let Some(before) = job(store, actor) else {
            return Ok(None);
        };
        before.validate()?;
        let site = site(store, before.site).ok_or(Error::Unavailable)?;
        let recipe = self.resolve_site(site)?.recipe;
        recipe.validate()?;
        if before.remaining > recipe.duration.0 {
            return Err(Error::Unavailable);
        }
        let progress = before.evaluate(
            facts.elapsed,
            facts.effective_rate,
            facts.active,
            facts.equipped,
        )?;
        let mut after = before.clone();
        after.apply_progress(progress);
        after.validate()?;
        Ok(Some(PreparedProgress {
            actor,
            before: before.clone(),
            after,
            site: site.clone(),
            recipe: recipe.clone(),
        }))
    }
}
impl PreparedProgress {
    pub fn actor(&self) -> PersonId {
        self.actor
    }
    pub fn after(&self) -> &Job {
        &self.after
    }
    pub fn progress(&self) -> Progress {
        Progress {
            remaining: self.after.remaining,
            remainder: self.after.work_remainder,
        }
    }
    pub fn output_forecast(&self) -> Option<(SiteId, Good, Quantity)> {
        self.after.output_forecast(
            self.progress(),
            RecipeView {
                site: &self.site,
                recipe: &self.recipe,
            },
        )
    }
    pub fn validate(&self, definitions: &Definitions, store: &Store) -> Result<(), Error> {
        if job(store, self.actor) != Some(&self.before)
            || site(store, self.site.id)
                .is_none_or(|current| !same_site_declaration(&self.site, current))
            || definitions.recipe(&self.site.recipe)? != &self.recipe
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage(
        &self,
        definitions: &Definitions,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<JobPublication, Error> {
        self.validate(definitions, store)?;
        let id = StableId(self.actor.0);
        if batch.touches::<Job>(id) {
            return Err(Error::Unavailable);
        }
        let mut own = Batch::new();
        own.insert(id, self.after.clone());
        store.validate_batch(&own)?;
        let mut reads = ReadSet::new();
        reads.expect_row(id, Some(self.before.clone()));
        reads.read::<Site>(store, StableId(self.site.id.0));
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Job>(id);
        batch.insert(id, self.after.clone());
        Ok(JobPublication {
            recipe_id: self.site.recipe.clone(),
            recipe: self.recipe.clone(),
            guard: reads.bind(store, batch, &admitted)?,
        })
    }
    pub fn commit(self, definitions: &Definitions, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage(definitions, store, &mut batch)?
            .validate(definitions, store, &batch)?;
        store.apply_batch(batch)
    }
}
impl JobPublication {
    pub fn validate(
        &self,
        definitions: &Definitions,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        if definitions.recipe(&self.recipe_id)? != &self.recipe {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StartWitness {
    pub actor: PersonId,
    pub previous: Option<Job>,
    pub site: Site,
    pub recipe: Recipe,
    pub input: Inventory,
    pub tool: Inventory,
    pub output: Inventory,
}
impl StartWitness {
    pub(crate) fn admit(self, job: Job, consumption: Option<Consumption>) -> Start {
        Start {
            admitted_job: job.clone(),
            admitted_consumption: consumption.clone(),
            job,
            consumption,
            witness: self,
        }
    }
}
impl Start {
    /// Stage the previously admitted job and its one recipe consumption together.
    /// Interruption and enterprise account effects may be appended by their owners.
    pub fn stage_into(
        &self,
        definitions: &Definitions,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<JobPublication, Error> {
        let witness = &self.witness;
        if self.job != self.admitted_job
            || self.consumption != self.admitted_consumption
            || definitions.recipe(&witness.site.recipe)? != &witness.recipe
        {
            return Err(Error::Unavailable);
        }
        let id = StableId(witness.actor.0);
        let mut reads = ReadSet::new();
        reads.expect_row(id, witness.previous.clone());
        reads.expect_row(StableId(witness.site.id.0), Some(witness.site.clone()));
        for stock in [&witness.input, &witness.tool, &witness.output] {
            reads.expect_row(StableId(stock.id.0), Some(stock.clone()));
        }
        reads.validate(store)?;
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Job>(id);
        if let Some(consumption) = &self.consumption {
            admitted.protect::<Inventory>(StableId(consumption.source.0));
        }
        if !admitted.is_untouched(batch) {
            return Err(Error::Unavailable);
        }
        let mut own = Batch::new();
        own.insert(id, self.job.clone());
        let after_input = if let Some(consumption) = &self.consumption {
            let mut input = witness.input.clone();
            consumption.commit(&mut input)?;
            own.insert(StableId(input.id.0), input.clone());
            Some(input)
        } else {
            None
        };
        store.validate_batch(&own)?;
        batch.insert(id, self.job.clone());
        if let Some(input) = after_input {
            batch.insert(StableId(input.id.0), input);
        }
        Ok(JobPublication {
            recipe_id: witness.site.recipe.clone(),
            recipe: witness.recipe.clone(),
            guard: reads.bind(store, batch, &admitted)?,
        })
    }
    pub fn commit(self, definitions: &Definitions, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(definitions, store, &mut batch)?
            .validate(definitions, store, &batch)?;
        store.apply_batch(batch)
    }
}

/// Cancellation discards admitted work without refunding spent recipe input.
#[derive(Clone, Debug)]
pub struct PreparedJobCancellation {
    actor: PersonId,
    before: Option<Job>,
}
pub fn prepare_cancel(store: &Store, actor: PersonId) -> PreparedJobCancellation {
    PreparedJobCancellation {
        actor,
        before: job(store, actor).cloned(),
    }
}
impl PreparedJobCancellation {
    pub fn stage_into(&self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        let id = StableId(self.actor.0);
        let mut reads = ReadSet::new();
        reads.expect_row(id, self.before.clone());
        reads.validate(store)?;
        let mut admitted = ReadFootprint::new();
        if self.before.is_some() {
            if batch.touches::<Job>(id) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Job>(id);
            batch.remove::<Job>(id);
        }
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?
            .validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductionReceipt {
    pub actor: PersonId,
    pub site: SiteId,
    pub output: InventoryId,
    pub quantity: Quantity,
}
#[derive(Clone, Debug)]
pub struct PreparedProgressCompletion {
    progress: PreparedProgress,
    production: Production,
    output: Inventory,
    deposit: game_cargo::PreparedDeposit,
}
/// Evidence that this batch already completes the exact opening job. Receiving
/// death cleanup may acknowledge that removal without discarding produced goods.
#[derive(Clone, Debug)]
pub struct CompletedJobReceipt {
    actor: PersonId,
    before: Job,
    batch: game_ecs::BatchKey,
}
#[derive(Clone, Debug)]
pub struct ProgressCompletionPublication {
    job: JobPublication,
    output: ReadGuard,
    completed: CompletedJobReceipt,
}
impl PreparedProgress {
    pub fn prepare_completion(
        &self,
        definitions: &Definitions,
        store: &Store,
        facts: CompletionFacts,
    ) -> Result<Option<PreparedProgressCompletion>, Error> {
        self.validate(definitions, store)?;
        if facts.actor != self.actor {
            return Err(Error::Unavailable);
        }
        let mut closing_progress = self.clone();
        closing_progress.site = site(store, self.site.id).ok_or(Error::Unavailable)?.clone();
        let output = game_cargo::inventory(store, self.site.output).ok_or(Error::Unavailable)?;
        let Some(production) = self.after.prepare_completion(
            RecipeView {
                site: &closing_progress.site,
                recipe: &self.recipe,
            },
            output,
            facts,
        )?
        else {
            return Ok(None);
        };
        let deposit = game_cargo::prepare_deposit(
            store,
            production.output,
            self.recipe.output_good,
            production.quantity,
        )?;
        Ok(Some(PreparedProgressCompletion {
            progress: closing_progress,
            production,
            output: output.clone(),
            deposit,
        }))
    }
}
impl PreparedProgressCompletion {
    pub fn receipt(&self) -> ProductionReceipt {
        ProductionReceipt {
            actor: self.production.actor,
            site: self.production.site,
            output: self.production.output,
            quantity: self.production.quantity,
        }
    }
    pub fn stage(
        &self,
        definitions: &Definitions,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ProgressCompletionPublication, Error> {
        self.progress.validate(definitions, store)?;
        self.production.validate(
            Some(&self.progress.after),
            RecipeView {
                site: &self.progress.site,
                recipe: &self.progress.recipe,
            },
            &self.output,
        )?;
        self.deposit.validate(store)?;
        let actor = StableId(self.progress.actor.0);
        let site_id = StableId(self.progress.site.id.0);
        let mut after_site = self.progress.site.clone();
        after_site.last_granted = Some(self.progress.actor);
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Job>(actor);
        admitted.protect::<Site>(site_id);
        if !admitted.is_untouched(batch) {
            return Err(Error::Unavailable);
        }
        let mut own = Batch::new();
        own.remove::<Job>(actor);
        own.insert(site_id, after_site.clone());
        self.deposit.clone().stage_into(store, &mut own)?;
        store.validate_batch(&own)?;
        let mut reads = ReadSet::new();
        reads.expect_row(actor, Some(self.progress.before.clone()));
        reads.expect_row(site_id, Some(self.progress.site.clone()));
        batch.remove::<Job>(actor);
        batch.insert(site_id, after_site);
        let output = self.deposit.clone().stage_into(store, batch)?;
        let job = JobPublication {
            recipe_id: self.progress.site.recipe.clone(),
            recipe: self.progress.recipe.clone(),
            guard: reads.bind(store, batch, &admitted)?,
        };
        Ok(ProgressCompletionPublication {
            job,
            output,
            completed: CompletedJobReceipt {
                actor: self.progress.actor,
                before: self.progress.before.clone(),
                batch: batch.key(),
            },
        })
    }
    pub fn commit(
        self,
        definitions: &Definitions,
        store: &mut Store,
    ) -> Result<ProductionReceipt, Error> {
        let receipt = self.receipt();
        let mut batch = Batch::new();
        self.stage(definitions, store, &mut batch)?
            .validate(definitions, store, &batch)?;
        store.apply_batch(batch)?;
        Ok(receipt)
    }
}
impl ProgressCompletionPublication {
    pub fn validate(
        &self,
        definitions: &Definitions,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        self.job.validate(definitions, store, batch)?;
        self.output.validate(store, batch)
    }
    pub fn completed_job(&self) -> &CompletedJobReceipt {
        &self.completed
    }
}
impl PreparedJobCancellation {
    pub fn stage_into_with_completion(
        &self,
        store: &Store,
        batch: &mut Batch,
        completed: &CompletedJobReceipt,
    ) -> Result<ReadGuard, Error> {
        let id = StableId(self.actor.0);
        if completed.actor != self.actor
            || self.before.as_ref() != Some(&completed.before)
            || completed.batch != batch.key()
            || batch.projected::<Job>(store, id)?.is_some()
        {
            return Err(Error::Unavailable);
        }
        let mut reads = ReadSet::new();
        reads.expect_row(id, self.before.clone());
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Job>(id);
        reads.bind(store, batch, &admitted)
    }
}

impl CompletedJobReceipt {
    pub fn actor(&self) -> PersonId {
        self.actor
    }
}

// Fairness is a receiving cursor, not an input to already elapsed work.
fn same_site_declaration(opening: &Site, current: &Site) -> bool {
    let mut declaration = opening.clone();
    declaration.last_granted = current.last_granted;
    declaration == *current
}
