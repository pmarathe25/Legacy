//! Property activity policy expressed through the receiver's representation.
use crate::Effect;
use game_core::{Error, PersonId, SimTime};
use game_ecs::{Batch, Component, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeMap;

/// Factual receiving activity stamps for cancellation projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancellationActivity {
    Other,
    Taking,
    Report { completes: SimTime },
}
impl Effect {
    pub fn activity_reset(&self, activity: CancellationActivity) -> Option<PersonId> {
        match (self, activity) {
            (Self::ClearTaking { actor }, CancellationActivity::Taking) => Some(*actor),
            (
                Self::ClearReport {
                    sender, completes, ..
                },
                CancellationActivity::Report { completes: at },
            ) if *completes == at => Some(*sender),
            _ => None,
        }
    }
}
/// Property owns the reporting work's default duration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    pub report_duration: game_core::SimDuration,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            report_duration: game_core::MINUTE,
        }
    }
}

pub struct PreparedActivityEffects<A> {
    reads: ReadSet,
    replacements: BTreeMap<PersonId, A>,
}
impl<A: Component> PreparedActivityEffects<A> {
    pub fn replacements(&self) -> &BTreeMap<PersonId, A> {
        &self.replacements
    }
    /// Bind the receiver's one admitted activity edit per affected actor.
    pub fn bind(self, store: &Store, batch: &Batch) -> Result<ReadGuard, Error> {
        let mut admitted = ReadFootprint::new();
        for actor in self.replacements.keys() {
            admitted.protect::<A>(StableId(actor.0));
        }
        self.reads.bind(store, batch, &admitted)
    }
}
/// The receiver translates only its activity representation; Property chooses
/// conditional clearing and ordered replacement from its own effects.
pub fn prepare_activity_effects<A: Component>(
    store: &Store,
    effects: &[Effect],
    projected: impl IntoIterator<Item = (PersonId, A)>,
    is_taking: impl Fn(&A) -> bool,
    report_deadline: impl Fn(&A) -> Option<SimTime>,
    idle: A,
    start: impl Fn(&Effect) -> Option<A>,
) -> Result<PreparedActivityEffects<A>, Error> {
    let mut reads = ReadSet::new();
    let mut replacements: BTreeMap<_, _> = projected.into_iter().collect();
    for effect in effects {
        let actor = match effect {
            Effect::ClearTaking { actor } | Effect::StartTake { actor, .. } => *actor,
            Effect::ClearReport { sender, .. } | Effect::StartReport { sender, .. } => *sender,
            _ => continue,
        };
        let opening = reads
            .read::<A>(store, StableId(actor.0))
            .ok_or(Error::Unavailable)?;
        let current = replacements.get(&actor).unwrap_or(opening);
        let replacement = match effect {
            Effect::ClearTaking { .. } if is_taking(current) => Some(idle.clone()),
            Effect::ClearReport { completes, .. }
                if report_deadline(current) == Some(*completes) =>
            {
                Some(idle.clone())
            }
            Effect::StartTake { .. } | Effect::StartReport { .. } => start(effect),
            _ => None,
        };
        if let Some(replacement) = replacement {
            replacements.insert(actor, replacement);
        }
    }
    for actor in replacements.keys() {
        reads
            .read::<A>(store, StableId(actor.0))
            .ok_or(Error::Unavailable)?;
    }
    Ok(PreparedActivityEffects {
        reads,
        replacements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    #[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Physical(u64);
    #[test]
    fn cancellation_projection_distinguishes_taking_from_report_deadline() {
        let take = Effect::ClearTaking { actor: PersonId(1) };
        let report = Effect::ClearReport {
            sender: PersonId(2),
            completes: SimTime(9),
            interrupted: true,
        };
        assert_eq!(
            take.activity_reset(CancellationActivity::Taking),
            Some(PersonId(1))
        );
        assert_eq!(
            take.activity_reset(CancellationActivity::Report {
                completes: SimTime(9)
            }),
            None
        );
        assert_eq!(
            report.activity_reset(CancellationActivity::Report {
                completes: SimTime(9)
            }),
            Some(PersonId(2))
        );
        assert_eq!(
            report.activity_reset(CancellationActivity::Report {
                completes: SimTime(10)
            }),
            None
        );
    }
    fn fixture() -> Store {
        let mut schema = game_ecs::Schema::new();
        schema
            .register::<Physical>("foreign/physical", |row| {
                if row.0 == 99 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        let mut batch = Batch::new();
        batch.insert(StableId(1), Physical(3));
        store.apply_batch(batch).unwrap();
        store
    }
    fn prepare(store: &Store) -> PreparedActivityEffects<Physical> {
        prepare_activity_effects(
            store,
            &[Effect::ClearTaking { actor: PersonId(1) }],
            [],
            |row| row.0 == 3,
            |_| None,
            Physical(0),
            |_| None,
        )
        .unwrap()
    }
    #[test]
    fn owner_guard_rejects_stale_opening_and_foreign_replacement() {
        let mut store = fixture();
        let prepared = prepare(&store);
        let mut batch = Batch::new();
        batch.insert(StableId(1), Physical(0));
        let guard = prepared.bind(&store, &batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        batch.insert(StableId(1), Physical(2));
        assert!(guard.validate(&store, &batch).is_err());
        let prepared = prepare(&store);
        let mut edit = Batch::new();
        edit.insert(StableId(1), Physical(4));
        store.apply_batch(edit).unwrap();
        assert!(prepared.bind(&store, &batch).is_err());
    }
    #[test]
    fn strict_receiver_rejection_preserves_all_composed_rows() {
        let mut store = fixture();
        let before = store.snapshot().unwrap();
        let prepared = prepare_activity_effects(
            &store,
            &[],
            [(PersonId(1), Physical(99))],
            |_| false,
            |_| None,
            Physical(0),
            |_| None,
        )
        .unwrap();
        let mut batch = Batch::new();
        batch.insert(StableId(1), Physical(99));
        let guard = prepared.bind(&store, &batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
