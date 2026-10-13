//! Sparse cost progression over person-bound components and supplied survival facts.
use crate::*;
use game_core::{Error, PersonId, SimTime};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedAdvance {
    rules: Rules,
    active: Option<Active>,
    updates: Vec<(PersonId, Burden, Burden)>,
}
impl ControlState {
    pub fn prepare_advance(
        &self,
        store: &Store,
        now: SimTime,
        next: SimTime,
        mut alive: impl FnMut(PersonId) -> bool,
    ) -> Result<PreparedAdvance, Error> {
        next.0.checked_sub(now.0).ok_or(Error::Overflow)?;
        one_session(store)?;
        let mut updates = Vec::new();
        for (actor, before) in burdens(store) {
            if !alive(actor) {
                continue;
            }
            let active = self
                .active(store)
                .filter(|active| active.actor == actor)
                .map(|active| ActiveInput {
                    life_count: active.lives.len(),
                    accepted: &active.accepted,
                    episode: active.episode.as_ref(),
                    next_loss_burden: active.next_loss_burden,
                });
            let after = advance_burden(AdvanceInput {
                rules: &self.rules,
                before,
                actor,
                now,
                next,
                alive: true,
                active,
            })?
            .burden;
            updates.push((actor, before.clone(), after));
        }
        Ok(PreparedAdvance {
            rules: self.rules.clone(),
            active: self.active(store).cloned(),
            updates,
        })
    }
}
impl PreparedAdvance {
    pub fn stage_into(
        self,
        state: &ControlState,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ReadGuard, Error> {
        if state.rules != self.rules
            || state.active(store) != self.active.as_ref()
            || self
                .updates
                .iter()
                .any(|(actor, before, _)| burden(store, *actor) != Some(before))
        {
            return Err(Error::Unavailable);
        }
        let mut reads = ReadSet::new();
        reads.membership::<ControlSession>(store, |_, _| true);
        for (actor, _) in sessions(store) {
            reads.read::<ControlSession>(store, StableId(actor.0));
        }
        let mut admitted = ReadFootprint::new();
        for (actor, before, after) in self.updates {
            reads.expect_row(StableId(actor.0), Some(before));
            admitted.protect::<Burden>(StableId(actor.0));
            batch.insert(StableId(actor.0), after);
        }
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, state: &ControlState, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let receipt = self.stage_into(state, store, &mut batch)?;
        receipt.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
