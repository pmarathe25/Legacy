//! Prepared receiving effects for instantaneous injuries.
use crate::Body;
use game_core::{Error, PersonId};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};

#[derive(Clone, Debug)]
pub struct PreparedInjury {
    actor: PersonId,
    damage: u64,
    after: Body,
    lethal: bool,
    reads: ReadSet,
}

pub fn prepare_injury(
    store: &Store,
    actor: PersonId,
    damage: u64,
    immortal: bool,
) -> Result<PreparedInjury, Error> {
    let mut reads = ReadSet::new();
    let mut after = reads
        .read::<Body>(store, StableId(actor.0))
        .ok_or(Error::Unavailable)?
        .clone();
    after.validate()?;
    after.vitals.health = after.vitals.health.saturating_sub(damage);
    let lethal = after.vitals.alive && after.vitals.health == 0 && !immortal;
    if lethal {
        after.vitals.alive = false;
    }
    after.validate()?;
    Ok(PreparedInjury {
        actor,
        damage,
        after,
        lethal,
        reads,
    })
}

/// Prepare an owner-directed death while retaining the opening Body for a
/// composed mortality transaction. An already dead actor has no new transition.
pub fn prepare_death(store: &Store, actor: PersonId) -> Result<PreparedInjury, Error> {
    let health = crate::body(store, actor)
        .ok_or(Error::Unavailable)?
        .vitals
        .health;
    prepare_injury(store, actor, health, false)
}

pub struct InjuryReceipt {
    actor: PersonId,
    damage: u64,
    lethal: bool,
    guard: ReadGuard,
}
impl PreparedInjury {
    pub fn lethal(&self) -> bool {
        self.lethal
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<InjuryReceipt, Error> {
        self.reads.validate(store)?;
        let id = StableId(self.actor.0);
        if batch.touches::<Body>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Body>(id);
        batch.insert(id, self.after);
        Ok(InjuryReceipt {
            actor: self.actor,
            damage: self.damage,
            lethal: self.lethal,
            guard: self.reads.bind(store, batch, &admitted)?,
        })
    }
    pub fn commit(self, store: &mut Store) -> Result<bool, Error> {
        let mut batch = Batch::new();
        let receipt = self.stage_into(store, &mut batch)?;
        receipt.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(receipt.lethal())
    }
}
impl InjuryReceipt {
    pub fn matches(&self, actor: PersonId, damage: u64) -> bool {
        self.actor == actor && self.damage == damage
    }
    pub fn lethal(&self) -> bool {
        self.lethal
    }
    pub fn validate(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.guard.validate(store, batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::Vitals;
    fn body_row() -> Body {
        Body {
            vitals: Vitals {
                hunger: 37,
                fatigue: 71,
                health: 100,
                alive: true,
            },
            fatigue_remainder: 123,
            health_remainder: 456,
        }
    }
    fn fixture_store(reject_injured: bool) -> Store {
        let mut schema = game_ecs::Schema::new();
        if reject_injured {
            schema
                .register::<Body>("physiology/body", |body| {
                    body.validate()?;
                    if body.vitals.health < 100 {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
        } else {
            crate::register_components(&mut schema).unwrap();
        }
        let mut store = Store::with_schema(schema);
        crate::insert_body(&mut store, PersonId(1), body_row()).unwrap();
        store
    }
    #[test]
    fn standalone_injury_preserves_body_fields_and_immortality_and_rejected_schema_is_inert() {
        let mut store = fixture_store(false);
        let before = store.snapshot().unwrap();
        let injury = prepare_injury(&store, PersonId(1), 40, false).unwrap();
        assert!(!injury.lethal());
        assert_eq!(store.snapshot().unwrap(), before);
        assert!(!injury.commit(&mut store).unwrap());
        let after = crate::body(&store, PersonId(1)).unwrap();
        assert_eq!(
            (
                after.vitals.health,
                after.vitals.hunger,
                after.vitals.fatigue,
                after.fatigue_remainder,
                after.health_remainder
            ),
            (60, 37, 71, 123, 456)
        );
        assert!(!crate::receive_injury(&mut store, PersonId(1), u64::MAX, true).unwrap());
        assert!(crate::body(&store, PersonId(1)).unwrap().vitals.alive);
        assert!(crate::receive_injury(&mut store, PersonId(1), 0, false).unwrap());
        assert!(!crate::body(&store, PersonId(1)).unwrap().vitals.alive);
        assert!(!crate::receive_injury(&mut store, PersonId(1), 10, false).unwrap());
        let mut rejected = fixture_store(true);
        let before = rejected.snapshot().unwrap();
        assert_eq!(
            crate::receive_injury(&mut rejected, PersonId(1), 1, false),
            Err(Error::Unavailable)
        );
        assert_eq!(rejected.snapshot().unwrap(), before);
    }
    #[test]
    fn stale_body_and_foreign_body_writes_reject_without_damage() {
        let mut store = fixture_store(false);
        let prepared = prepare_injury(&store, PersonId(1), 10, false).unwrap();
        crate::body_mut(&mut store, PersonId(1))
            .unwrap()
            .vitals
            .fatigue += 1;
        let before = store.snapshot().unwrap();
        assert_eq!(prepared.commit(&mut store), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
        let prepared = prepare_injury(&store, PersonId(1), 100, false).unwrap();
        let mut batch = Batch::new();
        let receipt = prepared.stage_into(&store, &mut batch).unwrap();
        assert!(receipt.lethal());
        batch.remove::<Body>(StableId(1));
        assert_eq!(receipt.validate(&store, &batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(
            receipt.validate(&store, &Batch::new()),
            Err(Error::Unavailable)
        );
    }
}
