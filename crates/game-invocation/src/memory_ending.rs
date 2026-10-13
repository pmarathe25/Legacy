//! Memory work owns its collateral participants and cancellation membership.
use crate::*;
use game_core::{Error, InventoryId, PersonId, Quantity};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedMemoryEnding {
    requested: Vec<PersonId>,
    rows: BTreeMap<PersonId, MemoryAction>,
    participants: Vec<PersonId>,
}
impl Runtime {
    pub fn prepare_memory_ending(
        &self,
        store: &Store,
        requested: &[PersonId],
    ) -> PreparedMemoryEnding {
        let mut seen = BTreeSet::new();
        let requested = requested
            .iter()
            .copied()
            .filter(|actor| seen.insert(*actor))
            .collect::<Vec<_>>();
        let mut rows = BTreeMap::new();
        let mut participants = Vec::new();
        for actor in &requested {
            for (owner, action) in memory_actions(store).filter(|(_, action)| {
                action.actor == *actor
                    || matches!(action.work,MemoryWork::Teach {recipient,..} if recipient==*actor)
            }) {
                if rows.contains_key(&owner) {
                    continue;
                }
                participants.push(action.actor);
                if let MemoryWork::Teach { recipient, .. } = action.work {
                    participants.push(recipient);
                }
                rows.insert(owner, action.clone());
            }
        }
        PreparedMemoryEnding {
            requested,
            rows,
            participants,
        }
    }
    pub fn recollection_is_active(&self, store: &Store, actor: PersonId) -> bool {
        memory_action(store, actor).is_some_and(|action| {
            matches!(
                action.work,
                MemoryWork::Write { .. } | MemoryWork::Teach { .. }
            )
        })
    }
    pub fn recollection_is_obsolete(
        &self,
        store: &Store,
        actor: PersonId,
        lives: &BTreeSet<PersonId>,
    ) -> bool {
        memory_action(store, actor).is_some_and(|action| match &action.work {
            MemoryWork::Write { content, .. } | MemoryWork::Teach { content, .. } => {
                !lives.contains(&content.life())
            }
            _ => false,
        })
    }
}
/// Cancellation keeps its scoped participant membership and removed work rows
/// protected until every receiving owner has staged the final transaction.
#[derive(Debug)]
pub struct MemoryEndingReceipt {
    pub participants: Vec<PersonId>,
    reads: ReadGuard,
    replacement: Option<MemoryReplacement>,
}
impl MemoryEndingReceipt {
    pub fn validate_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        if let Some(replacement) = &self.replacement {
            replacement.validate_batch(store, batch)?;
        }
        self.reads.validate(store, batch)
    }
}
impl PreparedMemoryEnding {
    pub fn stage_into_with_guard(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<MemoryEndingReceipt, Error> {
        self.stage_into_composed(runtime, store, None, batch)
    }
    pub fn participants(&self) -> &[PersonId] {
        &self.participants
    }
    /// Preserve only the Invocation replacement admitted by its start plan.
    /// Conflicting foreign edits still fail the final combined batch validator.
    pub fn stage_into_composed(
        self,
        runtime: &Runtime,
        store: &Store,
        replacement: Option<&MemoryReplacement>,
        batch: &mut Batch,
    ) -> Result<MemoryEndingReceipt, Error> {
        // Only the opaque start receipt can authorize the existing replacement
        // edit. Arbitrary writes to any other scoped action remain forbidden.
        if let Some(replacement) = replacement {
            replacement.validate_batch(store, batch)?;
        }
        let mut reads = ReadSet::new();
        let requested = self.requested.clone();
        reads.membership::<MemoryAction>(store, move |_, action| {
            requested.contains(&action.actor)
                || matches!(action.work, MemoryWork::Teach { recipient, .. }
                    if requested.contains(&recipient))
        });
        let mut admitted = ReadFootprint::new();
        for (actor, action) in &self.rows {
            reads.expect_row(StableId(actor.0), Some(action.clone()));
            admitted.protect::<MemoryAction>(StableId(actor.0));
        }
        if let Some(replacement) = replacement {
            let id = StableId(replacement.actor().0);
            reads.read::<MemoryAction>(store, id);
            admitted.protect::<MemoryAction>(id);
        }
        let participants = self.stage_into(
            runtime,
            store,
            replacement.map(MemoryReplacement::actor),
            batch,
        )?;
        Ok(MemoryEndingReceipt {
            participants,
            reads: reads.bind(store, batch, &admitted)?,
            replacement: replacement.cloned(),
        })
    }
    pub(crate) fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        replacement: Option<PersonId>,
        batch: &mut Batch,
    ) -> Result<Vec<PersonId>, Error> {
        let current = runtime.prepare_memory_ending(store, &self.requested);
        if current != self {
            return Err(Error::Unavailable);
        }
        for actor in self
            .rows
            .keys()
            .copied()
            .filter(|actor| Some(*actor) != replacement)
        {
            batch.remove::<MemoryAction>(StableId(actor.0));
        }
        Ok(self.participants)
    }
}

impl MemoryAction {
    pub fn activity(&self) -> MemoryActivity {
        match self.work {
            MemoryWork::Write { .. } => MemoryActivity::Write(self.completes),
            MemoryWork::Read { .. } => MemoryActivity::Read(self.completes),
            MemoryWork::Move { .. } => MemoryActivity::Move(self.completes),
            MemoryWork::Teach { .. } => MemoryActivity::Teach(self.completes),
        }
    }
    pub fn starting_participants(&self) -> Vec<(PersonId, MemoryActivity)> {
        let mut participants = vec![(self.actor, self.activity())];
        if let MemoryWork::Teach { recipient, .. } = self.work {
            participants.push((recipient, MemoryActivity::Teach(self.completes)));
        }
        participants
    }
    pub fn material_consumption(&self) -> Option<(InventoryId, Quantity)> {
        match self.work {
            MemoryWork::Write { material, .. } => Some((material, Quantity(1000))),
            _ => None,
        }
    }
}
impl PreparedMemoryEnding {
    pub fn owners(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.rows.keys().copied()
    }
    pub fn activity_resets(&self, references: &impl MemoryReferences) -> Vec<PersonId> {
        self.participants
            .iter()
            .copied()
            .filter(|actor| {
                references
                    .person(*actor)
                    .is_some_and(|person| person.activity != MemoryActivity::Other)
            })
            .collect()
    }
}

impl Runtime {
    /// The memory owner receives control effects and selects which work ends.
    /// Reading and physical record movement survive ordinary dismissal.
    pub fn prepare_memory_control_reactions(
        &self,
        store: &Store,
        effects: &[ControlEffect],
    ) -> PreparedMemoryEnding {
        let requested = effects
            .iter()
            .filter_map(|effect| match effect {
                ControlEffect::Started { actor, lives }
                    if self.recollection_is_obsolete(store, *actor, lives) =>
                {
                    Some(*actor)
                }
                ControlEffect::Dismissed { actor }
                    if self.recollection_is_active(store, *actor) =>
                {
                    Some(*actor)
                }
                ControlEffect::DeathCleanup { actor } => Some(*actor),
                _ => None,
            })
            .collect::<Vec<_>>();
        self.prepare_memory_ending(store, &requested)
    }
}
