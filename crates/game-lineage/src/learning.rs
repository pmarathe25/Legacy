//! Person-owned invocation learning earned through lineage lessons.
use game_core::{Error, PersonId};
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};

pub const INVOCATION_LEARNING_LIMIT: u64 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LessonJudgmentFacts {
    pub teacher_knows_topic: bool,
    pub immediate_care: bool,
}

/// Decides whether a guardian can begin a lesson from knowledge and care facts
/// assembled by the world coordinator.
pub fn judge_lesson(facts: LessonJudgmentFacts) -> Result<(), &'static str> {
    if !facts.teacher_knows_topic {
        Err("I cannot teach knowledge I do not have")
    } else if facts.immediate_care {
        Err("The pupil needs immediate physical care before a lesson")
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationLearning {
    pub learned: u64,
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    crate::canonical::register_canonical_components(schema)?;
    schema.register::<InvocationLearning>("lineage/invocation_learning", |row| {
        if row.learned <= INVOCATION_LEARNING_LIMIT {
            Ok(())
        } else {
            Err(Error::InvalidConfiguration(
                "invocation learning exceeds its limit".into(),
            ))
        }
    })
}

/// Returns lesson learning attached to a person, or zero when no lesson has
/// been recorded for them.
pub fn invocation_learning(store: &Store, person: PersonId) -> u64 {
    store
        .get::<InvocationLearning>(StableId(person.0))
        .map_or(0, |row| row.learned)
}

/// Adds lesson learning up to the owner's fixed cap and returns the amount
/// actually credited.
pub fn increase_invocation_learning(
    store: &mut Store,
    person: PersonId,
    amount: u64,
) -> Result<u64, Error> {
    if !store.contains(StableId(person.0)) {
        return Err(Error::Unavailable);
    }
    let before = invocation_learning(store, person);
    if before > INVOCATION_LEARNING_LIMIT {
        return Err(Error::InvalidSave(
            "invocation learning exceeds its limit".into(),
        ));
    }
    let credited = amount.min(INVOCATION_LEARNING_LIMIT - before);
    if credited > 0 {
        store.insert(
            StableId(person.0),
            InvocationLearning {
                learned: before + credited,
            },
        )?;
    }
    Ok(credited)
}

/// Removes lesson learning from a person. This is idempotent for an entity
/// with no learning row.
pub fn reset_invocation_learning(store: &mut Store, person: PersonId) -> Result<(), Error> {
    store.remove::<InvocationLearning>(StableId(person.0))?;
    Ok(())
}

/// Validates this owner's sparse progress rows using host-supplied person
/// membership, keeping the lineage feature independent of physiology storage.
pub fn validate_invocation_learning(
    store: &Store,
    person_exists: impl Fn(PersonId) -> bool,
) -> Result<(), Error> {
    for (id, row) in store.iter::<InvocationLearning>() {
        let person = PersonId(id.0);
        if row.learned > INVOCATION_LEARNING_LIMIT || !person_exists(person) {
            return Err(Error::InvalidSave(
                "inconsistent invocation learning".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "learning_tests.rs"]
mod tests;
