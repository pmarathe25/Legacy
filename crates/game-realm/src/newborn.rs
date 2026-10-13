//! Regional personal state receives a reserved newborn in the birth transaction.
use crate::*;
use game_core::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

pub struct PreparedNewborn {
    child: PersonId,
    reads: ReadSet,
}
impl Runtime {
    pub fn prepare_newborn(
        &self,
        store: &Store,
        child: PersonId,
    ) -> Result<PreparedNewborn, Error> {
        let mut reads = ReadSet::new();
        let id = StableId(child.0);
        if child.0 == 0
            || reads.read::<PersonalKnowledge>(store, id).is_some()
            || reads.read::<Requests>(store, id).is_some()
        {
            return Err(Error::Unavailable);
        }
        Ok(PreparedNewborn { child, reads })
    }
}
impl PreparedNewborn {
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        let id = StableId(self.child.0);
        if batch.touches::<PersonalKnowledge>(id) || batch.touches::<Requests>(id) {
            return Err(Error::Unavailable);
        }
        batch.insert(id, PersonalKnowledge::default());
        batch.insert(id, Requests::default());
        let mut admitted = ReadFootprint::new();
        admitted.protect::<PersonalKnowledge>(id);
        admitted.protect::<Requests>(id);
        self.reads.bind(store, batch, &admitted)
    }
}
