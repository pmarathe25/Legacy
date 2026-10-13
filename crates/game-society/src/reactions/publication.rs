//! A case, witness learning, and exclusion bookkeeping publish as one guarded effect.
use super::*;
use crate::{Concept, Exclusions, Knowledge};
use game_ecs::{ReadFootprint, ReadGuard, ReadSet};
pub struct PreparedCaseReaction {
    runtime: Runtime,
    at: SimTime,
    case: CaseId,
    actor: PersonId,
    learning: PreparedCaseLearning,
    exclusions: BTreeMap<PersonId, Exclusions>,
    observers: Vec<PersonId>,
    reads: ReadSet,
}
pub struct CaseReactionPublication {
    runtime: Runtime,
    at: SimTime,
    pub case: CaseId,
    pub actor: PersonId,
    rows: ReadGuard,
}
impl Runtime {
    pub fn prepare_case_reaction(
        &self,
        store: &Store,
        mut reaction: CaseReaction,
        id: CaseId,
        observers: &BTreeMap<PersonId, u16>,
    ) -> Result<PreparedCaseReaction, Error> {
        if id.0 == 0 || observers.contains_key(&PersonId(id.0)) {
            return Err(Error::Unavailable);
        }
        let mut reads = ReadSet::new();
        reads.exclusive_entity(store, StableId(id.0))?;
        reads.read::<SocialCaseView>(store, StableId(id.0));
        reads.read::<Knowledge>(store, StableId(reaction.case.subject.0));
        if let Some(inventory) = reaction.case.inventory {
            reads.read::<game_property::InventoryCustody>(store, StableId(inventory.0));
        }
        if let Some(concept) = reaction.concept {
            reads
                .read::<Concept>(store, StableId(concept.0))
                .ok_or(Error::Unavailable)?;
            let owners = reads
                .membership::<Institution>(store, move |_, row| row.concepts.contains(&concept));
            for owner in owners {
                reads.read::<Institution>(store, owner);
            }
        }
        if let Some((actor, source, before)) = reaction.exclusion_guard
            && reads
                .read::<Exclusions>(store, StableId(actor.0))
                .and_then(|row| row.stores.get(&source))
                .copied()
                != before
        {
            return Err(Error::Unavailable);
        }
        let mut exclusions = BTreeMap::new();
        if let Some((actor, source)) = reaction.clear_exclusion {
            let mut row = reads
                .read::<Exclusions>(store, StableId(actor.0))
                .cloned()
                .ok_or(Error::Unavailable)?;
            let old = row.stores.get(&source).copied().ok_or(Error::Unavailable)?;
            let old = reads
                .read::<SocialCaseView>(store, StableId(old.0))
                .ok_or(Error::Unavailable)?;
            if old.subject != reaction.case.subject
                || old.inventory != reaction.case.inventory
                || old.area != reaction.case.area
            {
                return Err(Error::Unavailable);
            }
            row.stores.remove(&source);
            exclusions.insert(actor, row);
        }
        if let Some((actor, source)) = reaction.set_exclusion {
            if exclusions.contains_key(&actor) {
                return Err(Error::Unavailable);
            }
            let mut row = reads
                .read::<Exclusions>(store, StableId(actor.0))
                .cloned()
                .unwrap_or_default();
            row.stores.insert(source, id);
            exclusions.insert(actor, row);
        }
        for actor in observers.keys() {
            reads.read::<Knowledge>(store, StableId(actor.0));
        }
        let at = reaction.case.observed_at;
        let actor = reaction.case.subject;
        reaction.case.id = id;
        let learning =
            self.prepare_case_learning(store, reaction.case, reaction.concept, observers)?;
        Ok(PreparedCaseReaction {
            runtime: self.clone(),
            at,
            case: id,
            actor,
            learning,
            exclusions,
            observers: observers.keys().copied().collect(),
            reads,
        })
    }
}
impl PreparedCaseReaction {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        at: SimTime,
        batch: &mut Batch,
    ) -> Result<CaseReactionPublication, Error> {
        if runtime != &self.runtime || at != self.at {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let case = StableId(self.case.0);
        if batch.touches::<SocialCaseView>(case)
            || self
                .observers
                .iter()
                .any(|actor| batch.touches::<Knowledge>(StableId(actor.0)))
            || self
                .exclusions
                .keys()
                .any(|actor| batch.touches::<Exclusions>(StableId(actor.0)))
        {
            return Err(Error::Unavailable);
        }
        let mut writes = ReadFootprint::new();
        writes.protect::<SocialCaseView>(case);
        for actor in &self.observers {
            writes.protect::<Knowledge>(StableId(actor.0));
        }
        self.learning.stage_into(store, batch)?;
        for (actor, row) in self.exclusions {
            let id = StableId(actor.0);
            writes.protect::<Exclusions>(id);
            batch.insert(id, row);
        }
        Ok(CaseReactionPublication {
            runtime: self.runtime,
            at: self.at,
            case: self.case,
            actor: self.actor,
            rows: self.reads.bind(store, batch, &writes)?,
        })
    }
}
impl CaseReactionPublication {
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        at: SimTime,
        batch: &Batch,
    ) -> Result<(), Error> {
        if runtime != &self.runtime || at != self.at {
            return Err(Error::Unavailable);
        }
        self.rows.validate(store, batch)
    }
    pub fn row_guard(&self) -> ReadGuard {
        self.rows.clone()
    }
}
