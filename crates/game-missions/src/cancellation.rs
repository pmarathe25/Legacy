//! Linked pending speech cancels once, in the initiating actors' order.
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClearTalk {
    pub speaker: PersonId,
    pub completes: SimTime,
}
pub struct PreparedTalkCancellations {
    before: BTreeMap<PersonId, PendingTalk>,
    membership: Vec<(PersonId, Vec<PersonId>)>,
    groups: Vec<(PersonId, Vec<ClearTalk>)>,
}
fn related(store: &Store, actor: PersonId) -> Vec<PersonId> {
    talks(store)
        .filter(|(_, talk)| talk.speaker == actor || talk.listener == actor)
        .map(|(id, _)| id)
        .collect()
}
impl Runtime {
    pub fn prepare_cancel_talks(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> PreparedTalkCancellations {
        let mut seen = BTreeSet::new();
        let mut before = BTreeMap::new();
        let mut membership = Vec::new();
        let mut groups = Vec::new();
        for actor in actors.iter().copied().filter(|actor| seen.insert(*actor)) {
            let ids = related(store, actor);
            let mut effects = Vec::new();
            for id in &ids {
                if !before.contains_key(id) {
                    let row = talk(store, *id).unwrap();
                    effects.push(ClearTalk {
                        speaker: row.speaker,
                        completes: row.completes,
                    });
                    before.insert(*id, row.clone());
                }
            }
            membership.push((actor, ids));
            groups.push((actor, effects));
        }
        PreparedTalkCancellations {
            before,
            membership,
            groups,
        }
    }
}
impl PreparedTalkCancellations {
    pub fn effects(&self) -> impl Iterator<Item = (PersonId, &[ClearTalk])> {
        self.groups
            .iter()
            .map(|(actor, effects)| (*actor, effects.as_slice()))
    }
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(id, row)| talk(store, *id) != Some(row))
            || self
                .membership
                .iter()
                .any(|(actor, ids)| related(store, *actor) != *ids)
        {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<TalkCancellationFinalization, Error> {
        self.stage_into_composed(store, batch, None)
    }
    pub fn stage_into_composed(
        self,
        store: &Store,
        batch: &mut Batch,
        replacement: Option<&TalkReplacement>,
    ) -> Result<TalkCancellationFinalization, Error> {
        self.validate(store)?;
        let replaced = replacement.map(TalkReplacement::speaker);
        if replaced.is_some_and(|actor| !batch.touches::<PendingTalk>(StableId(actor.0)))
            || self
                .before
                .keys()
                .filter(|id| Some(**id) != replaced)
                .any(|id| batch.touches::<PendingTalk>(StableId(id.0)))
        {
            return Err(Error::Unavailable);
        }
        for id in self.before.keys().filter(|id| Some(**id) != replaced) {
            batch.remove::<PendingTalk>(StableId(id.0));
        }
        Ok(TalkCancellationFinalization {
            groups: self.groups,
        })
    }
}
pub struct TalkCancellationFinalization {
    groups: Vec<(PersonId, Vec<ClearTalk>)>,
}
impl TalkCancellationFinalization {
    pub fn finish(self) -> Vec<(PersonId, Vec<ClearTalk>)> {
        self.groups
    }
}
#[cfg(test)]
mod tests;
