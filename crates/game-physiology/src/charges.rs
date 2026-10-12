//! Instant physical costs are guarded and staged by the body owner.
use crate::{Body, body};
use game_core::{Error, PersonId};
use game_ecs::{Batch, BatchKey, StableId, Store};

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
        self.stage_into_with_receipt(store, batch).map(|_| ())
    }
    pub fn stage_into_with_receipt(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<FatigueReceipt, Error> {
        if body(store, self.actor) != Some(&self.before) {
            return Err(Error::Unavailable);
        }
        batch.insert(StableId(self.actor.0), self.after);
        Ok(FatigueReceipt {
            actor: self.actor,
            batch: batch.key(),
        })
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?;
        store.apply_batch(batch)
    }
}

/// Proof of the one validated fatigue edit admitted to this exact batch.
#[derive(Clone, Debug)]
pub struct FatigueReceipt {
    actor: PersonId,
    batch: BatchKey,
}
#[derive(Clone, Debug)]
pub struct PreparedSurvival {
    actor: PersonId,
    before: Body,
}
pub fn prepare_survival(store: &Store, actor: PersonId) -> Result<PreparedSurvival, Error> {
    Ok(PreparedSurvival {
        actor,
        before: body(store, actor).ok_or(Error::Unavailable)?.clone(),
    })
}
impl PreparedSurvival {
    pub fn alive(&self) -> bool {
        self.before.vitals.alive
    }
    /// A survival decision tolerates only its explicitly staged fatigue receipt.
    pub fn check(
        &self,
        store: &Store,
        batch: &Batch,
        fatigue: Option<&FatigueReceipt>,
    ) -> Result<bool, Error> {
        let edits = batch
            .edited_ids::<Body>()
            .filter(|id| *id == StableId(self.actor.0))
            .count();
        let allowed = fatigue
            .is_some_and(|receipt| receipt.actor == self.actor && receipt.batch == batch.key());
        if body(store, self.actor) != Some(&self.before)
            || edits != usize::from(allowed)
            || fatigue.is_some() && !allowed
        {
            return Err(Error::Unavailable);
        }
        Ok(self.alive())
    }
}
