//! Typed receiving effects for an activity that ends without another completion.
use game_core::{Error, PersonId};
use game_ecs::{Batch, Component, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityEndingEffect {
    Interrupted { actor: PersonId },
}
pub struct PreparedActivityEnding<C: Component> {
    actor: PersonId,
    reads: ReadSet,
    marker: std::marker::PhantomData<C>,
}
pub fn prepare_activity_ending<C: Component>(
    store: &Store,
    actor: PersonId,
    mut factual_reads: ReadSet,
) -> PreparedActivityEnding<C> {
    factual_reads.read::<C>(store, StableId(actor.0));
    PreparedActivityEnding {
        actor,
        reads: factual_reads,
        marker: std::marker::PhantomData,
    }
}
impl<C: Component> PreparedActivityEnding<C> {
    pub fn effect(&self) -> ActivityEndingEffect {
        ActivityEndingEffect::Interrupted { actor: self.actor }
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch, idle: C) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        let id = StableId(self.actor.0);
        if batch.touches::<C>(id) {
            return Err(Error::Unavailable);
        }
        batch.insert(id, idle);
        let mut writes = ReadFootprint::new();
        writes.protect::<C>(id);
        self.reads.bind(store, batch, &writes)
    }
}
/// Canonical terminal bodies are dead with no remaining physical health.
pub fn require_terminal_body(body: &crate::Body) -> Result<(), Error> {
    if body.vitals.alive || body.vitals.health != 0 {
        Err(Error::Unavailable)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::Vitals;
    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Activity(u8);
    fn store() -> Store {
        let mut schema = game_ecs::Schema::new();
        schema
            .register::<Activity>("test/ending", |activity| {
                if activity.0 == 9 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        store.insert_or_spawn(StableId(1), Activity(1)).unwrap();
        store
    }
    #[test]
    fn ending_guards_opening_and_receiving_reset_without_publishing_foreign_failure() {
        let mut store = store();
        let opening = store.snapshot().unwrap();
        let ending = prepare_activity_ending::<Activity>(&store, PersonId(1), ReadSet::new());
        assert_eq!(
            ending.effect(),
            ActivityEndingEffect::Interrupted { actor: PersonId(1) }
        );
        let mut batch = Batch::new();
        let guard = ending.stage_into(&store, &mut batch, Activity(0)).unwrap();
        guard.validate(&store, &batch).unwrap();
        batch.insert(StableId(1), Activity(2));
        assert!(guard.validate(&store, &batch).is_err());
        assert_eq!(store.snapshot().unwrap(), opening);
        let ending = prepare_activity_ending::<Activity>(&store, PersonId(1), ReadSet::new());
        let mut batch = Batch::new();
        ending.stage_into(&store, &mut batch, Activity(9)).unwrap();
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(store.snapshot().unwrap(), opening);
        let ending = prepare_activity_ending::<Activity>(&store, PersonId(1), ReadSet::new());
        store.insert_or_spawn(StableId(1), Activity(3)).unwrap();
        assert!(
            ending
                .stage_into(&store, &mut Batch::new(), Activity(0))
                .is_err()
        );
    }
    #[test]
    fn terminal_body_requires_both_death_and_zero_remaining_health() {
        for (alive, health, terminal) in [
            (true, 0, false),
            (true, 1, false),
            (false, 1, false),
            (false, 0, true),
        ] {
            let body = crate::Body {
                vitals: Vitals {
                    alive,
                    health,
                    hunger: 0,
                    fatigue: 0,
                },
                fatigue_remainder: 0,
                health_remainder: 0,
            };
            assert_eq!(require_terminal_body(&body).is_ok(), terminal);
        }
    }
}
