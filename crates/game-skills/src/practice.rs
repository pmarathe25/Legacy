//! Sparse learning transactions over shared personal skill components.
use crate::*;
use game_ecs::{Batch, StableId};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    actor: PersonId,
    before: Option<Skills>,
    after: Skills,
}

/// Frozen learning effects; unrelated people's practice remains independent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedPractice {
    rules: Rules,
    changes: Vec<Change>,
}
impl Runtime {
    pub fn prepare_practice(
        &self,
        store: &Store,
        deltas: impl IntoIterator<Item = PracticeDelta>,
    ) -> Result<PreparedPractice, Error> {
        self.rules.validate()?;
        let mut changes = BTreeMap::<PersonId, Change>::new();
        for delta in deltas {
            if delta.milliseconds == 0 {
                continue;
            }
            if !store.contains(StableId(delta.actor.0)) {
                return Err(Error::Unavailable);
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = changes.entry(delta.actor) {
                let before = skills(store, delta.actor).cloned();
                let after = before.clone().unwrap_or_default();
                after.validate_progress(&self.rules)?;
                entry.insert(Change {
                    actor: delta.actor,
                    before,
                    after,
                });
            }
            changes
                .get_mut(&delta.actor)
                .expect("admitted practice actor")
                .after
                .credit(
                    delta.skill,
                    delta.milliseconds,
                    delta.mastery_cap,
                    &self.rules,
                );
        }
        Ok(PreparedPractice {
            rules: self.rules.clone(),
            changes: changes
                .into_values()
                .filter(|change| match &change.before {
                    Some(before) => before != &change.after,
                    None => change.after != Skills::default(),
                })
                .collect(),
        })
    }

    /// Earliest level transition among actual activity-owner practice effects.
    pub fn practice_boundary(
        &self,
        store: &Store,
        from: SimTime,
        limit: SimTime,
        deltas: &[PracticeDelta],
    ) -> Result<SimTime, Error> {
        if limit < from {
            return Err(Error::Unavailable);
        }
        let mut next = limit;
        for delta in deltas {
            if let Some(remaining) =
                self.until_next_level(store, delta.actor, delta.skill, delta.mastery_cap)?
                && remaining <= delta.milliseconds
            {
                next = next.min(SimTime(
                    from.0.checked_add(remaining).ok_or(Error::Overflow)?,
                ));
            }
        }
        Ok(next)
    }
}
impl PreparedPractice {
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if self.rules != runtime.rules {
            return Err(Error::Unavailable);
        }
        for change in &self.changes {
            if !store.contains(StableId(change.actor.0))
                || skills(store, change.actor) != change.before.as_ref()
            {
                return Err(Error::Unavailable);
            }
            change.after.validate_progress(&self.rules)?;
        }
        Ok(())
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<(), Error> {
        self.validate(runtime, store)?;
        let mut batch = Batch::new();
        for change in self.changes {
            batch.insert(StableId(change.actor.0), change.after);
        }
        store.apply_batch(batch)
    }
}
