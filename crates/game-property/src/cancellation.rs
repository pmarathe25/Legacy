//! Coalesced cancellations retain the initiating actor's publication order.
use crate::*;
use game_core::{Error, PersonId, SimTime};
use std::collections::BTreeSet;

pub struct PreparedCancellations {
    change: PreparedProperty,
    steps: Vec<(PersonId, usize)>,
}
impl Runtime {
    pub fn prepare_cancel_many(
        &self,
        store: &Store,
        actors: &[PersonId],
        now: SimTime,
    ) -> PreparedCancellations {
        let mut change = PreparedProperty::new();
        let mut seen = BTreeSet::new();
        let mut steps = Vec::new();
        for actor in actors {
            if !seen.insert(*actor) {
                continue;
            }
            let before = change.effects().len();
            change.cancel(store, *actor, now);
            steps.push((*actor, change.effects().len() - before));
        }
        PreparedCancellations { change, steps }
    }
}
impl PreparedCancellations {
    pub fn effects(&self) -> impl Iterator<Item = (PersonId, &[Effect])> {
        let mut start = 0;
        self.steps.iter().map(move |(actor, count)| {
            let effects = &self.change.effects()[start..start + count];
            start += count;
            (*actor, effects)
        })
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        self.change.validate(runtime, store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<CancellationFinalization, Error> {
        let change = self.change.stage_into(runtime, store, batch)?;
        Ok(CancellationFinalization {
            change,
            steps: self.steps,
        })
    }
    pub fn commit(
        self,
        runtime: &mut Runtime,
        store: &mut Store,
    ) -> Result<Vec<(PersonId, Vec<Effect>)>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
    }
}
pub struct CancellationFinalization {
    change: PropertyFinalization,
    steps: Vec<(PersonId, usize)>,
}
impl CancellationFinalization {
    pub fn finish(self, runtime: &mut Runtime) -> Vec<(PersonId, Vec<Effect>)> {
        let mut effects = self.change.finish(runtime).into_iter();
        self.steps
            .into_iter()
            .map(|(actor, count)| (actor, effects.by_ref().take(count).collect()))
            .collect()
    }
}

#[cfg(test)]
mod tests;
