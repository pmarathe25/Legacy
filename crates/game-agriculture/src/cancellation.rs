//! Cancellation stages only this feature's existing work rows.
use crate::*;
use game_core::{Error, PersonId};
use game_ecs::{Batch, StableId, Store};
use std::collections::BTreeSet;
pub struct PreparedCultivationCancellation {
    before: Vec<(PersonId, Option<Cultivation>)>,
}
impl PreparedCultivationCancellation {
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(actor, before)| cultivation(store, *actor) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        self.validate(store)?;
        for (actor, before) in self.before {
            if before.is_some() {
                batch.remove::<Cultivation>(StableId(actor.0));
            }
        }
        Ok(())
    }
}
impl Runtime {
    pub fn prepare_cancel_cultivation(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> PreparedCultivationCancellation {
        let mut seen = BTreeSet::new();
        PreparedCultivationCancellation {
            before: actors
                .iter()
                .copied()
                .filter(|actor| seen.insert(*actor))
                .map(|actor| (actor, cultivation(store, actor).cloned()))
                .collect(),
        }
    }
}
