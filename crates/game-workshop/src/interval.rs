//! Receive numerical work progress and choose completion, pause, or continuation.
use crate::*;
use game_ecs::{Batch, Component, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

pub struct PreparedWorkInterval {
    actor: PersonId,
    progress: Option<PreparedProgress>,
    completion: Option<PreparedProgressCompletion>,
    failed: bool,
    died: bool,
}
pub struct WorkIntervalPublication {
    pub completion: Option<ProgressCompletionPublication>,
    pub progress: Option<JobPublication>,
    pub activity: Option<ReadGuard>,
}
pub fn prepare_work_interval(
    progress: Option<&PreparedProgress>,
    definitions: &Definitions,
    store: &Store,
    facts: CompletionFacts,
    died: bool,
) -> Result<PreparedWorkInterval, Error> {
    let ready = progress.is_some_and(|progress| progress.progress().remaining == 0)
        && facts.active
        && facts.equipped;
    let completion = if ready {
        progress
            .unwrap()
            .prepare_completion(definitions, store, facts)?
    } else {
        None
    };
    let failed = ready && completion.is_none();
    Ok(PreparedWorkInterval {
        actor: facts.actor,
        progress: progress.cloned(),
        completion,
        failed,
        died,
    })
}
impl PreparedWorkInterval {
    pub fn failed(&self) -> bool {
        self.failed
    }
    pub fn production(&self) -> Option<ProductionReceipt> {
        self.completion
            .as_ref()
            .map(PreparedProgressCompletion::receipt)
    }
    /// Workshop publishes its successful completion's receiving activity reset
    /// under the concrete component's opening and projected-write guard.
    pub fn stage_into<C: Component>(
        self,
        definitions: &Definitions,
        store: &Store,
        batch: &mut Batch,
        idle: C,
    ) -> Result<WorkIntervalPublication, Error> {
        let completion = self
            .completion
            .as_ref()
            .map(|plan| plan.stage(definitions, store, batch))
            .transpose()?;
        let progress = if completion.is_none() && !self.died && job(store, self.actor).is_some() {
            self.progress
                .as_ref()
                .map(|progress| progress.stage(definitions, store, batch))
                .transpose()?
        } else {
            None
        };
        let activity = if completion.is_some() && !self.died {
            let id = StableId(self.actor.0);
            let mut reads = ReadSet::new();
            reads.read::<C>(store, id);
            if batch.touches::<C>(id) {
                return Err(Error::Unavailable);
            }
            batch.insert(id, idle);
            let mut writes = ReadFootprint::new();
            writes.protect::<C>(id);
            Some(reads.bind(store, batch, &writes)?)
        } else {
            None
        };
        Ok(WorkIntervalPublication {
            completion,
            progress,
            activity,
        })
    }
}
