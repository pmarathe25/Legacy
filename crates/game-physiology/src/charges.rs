//! Instant physical costs are guarded and staged by the body owner.
use crate::{Body, body};
use game_core::{Error, PersonId};
use game_ecs::{Batch, StableId, Store};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFatigueCharge {
    actor: PersonId,
    before: Body,
    after: Body,
}
pub fn prepare_fatigue_charge(
    store: &Store,
    actor: PersonId,
    quantity: u64,
    limit: u64,
) -> Result<PreparedFatigueCharge, Error> {
    let before = body(store, actor).ok_or(Error::Unavailable)?;
    let fatigue = before
        .vitals
        .fatigue
        .checked_add(quantity)
        .filter(|fatigue| *fatigue <= limit)
        .ok_or(Error::Unavailable)?;
    let mut after = before.clone();
    after.vitals.fatigue = fatigue;
    after.validate()?;
    Ok(PreparedFatigueCharge {
        actor,
        before: before.clone(),
        after,
    })
}
impl PreparedFatigueCharge {
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        if body(store, self.actor) != Some(&self.before) {
            return Err(Error::Unavailable);
        }
        batch.insert(StableId(self.actor.0), self.after);
        Ok(())
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?;
        store.apply_batch(batch)
    }
}
