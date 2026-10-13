//! Cancellation stages only this feature's existing work rows.
use crate::*;
use game_core::{Error, PersonId};
use game_ecs::{Batch, StableId, Store};
use std::collections::BTreeSet;
pub struct PreparedGatheringCancellation {
    before: Vec<(PersonId, Option<Gathering>)>,
}
impl PreparedGatheringCancellation {
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(actor, before)| gathering(store, *actor) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_guarded_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.validate(store)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        for (actor, before) in self.before {
            let id = StableId(actor.0);
            if batch.touches::<Gathering>(id) {
                return Err(Error::Unavailable);
            }
            reads.expect_row(id, before.clone());
            if before.is_some() {
                admitted.protect::<Gathering>(id);
                batch.remove::<Gathering>(id);
            }
        }
        reads.bind(store, batch, &admitted)
    }
}
impl Runtime {
    pub fn prepare_cancel_gathering(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> PreparedGatheringCancellation {
        let mut seen = BTreeSet::new();
        PreparedGatheringCancellation {
            before: actors
                .iter()
                .copied()
                .filter(|actor| seen.insert(*actor))
                .map(|actor| (actor, gathering(store, actor).cloned()))
                .collect(),
        }
    }
}
