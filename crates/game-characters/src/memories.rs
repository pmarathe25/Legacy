//! Characters interpret learned information without acquiring another person's access rights.
use crate::decisions::Mind;
use game_core::{AreaId, KnowledgeSource, SimDuration, SimTime, StockKnowledge};

/// A recipient owns the interpretation and storage of current knowledge.
pub trait MemoryLearner {
    fn learn_memory_stock(
        &mut self,
        now: SimTime,
        fact: StockKnowledge,
        source: KnowledgeSource,
    ) -> bool;
    fn learn_memory_route(&mut self, origin: AreaId, destination: AreaId, duration: SimDuration);
}

/// Borrowed receiving owner access to the person's distinct mind and stock rows.
pub struct CharacterMemory<'a, A: game_ecs::Component> {
    pub store: &'a mut game_ecs::Store,
    pub actor: game_core::PersonId,
    pub marker: std::marker::PhantomData<A>,
}
impl<A: game_ecs::Component> MemoryLearner for CharacterMemory<'_, A> {
    fn learn_memory_stock(
        &mut self,
        now: SimTime,
        mut fact: StockKnowledge,
        source: KnowledgeSource,
    ) -> bool {
        let stocks =
            crate::decisions::stocks_mut(self.store, self.actor).expect("receiving stocks exist");
        if stocks
            .known
            .get(&fact.inventory)
            .is_some_and(|old| old.observed_at > fact.observed_at)
        {
            return false;
        }
        fact.accessible = stocks
            .known
            .get(&fact.inventory)
            .is_some_and(|old| old.accessible);
        fact.learned_at = now;
        fact.source = source;
        stocks.known.insert(fact.inventory, fact);
        true
    }
    fn learn_memory_route(&mut self, origin: AreaId, destination: AreaId, duration: SimDuration) {
        let mind =
            crate::decisions::mind_mut::<A>(self.store, self.actor).expect("receiving mind exists");
        mind.routes
            .entry(origin)
            .or_default()
            .entry(destination)
            .or_insert(duration);
    }
}
impl<A> Mind<A> {
    pub fn learn_memory_route(
        &mut self,
        origin: AreaId,
        destination: AreaId,
        duration: SimDuration,
    ) {
        self.routes
            .entry(origin)
            .or_default()
            .entry(destination)
            .or_insert(duration);
    }
}
#[cfg(test)]
mod tests;

/// A recipient judges whether to attend voluntary memory instruction.
pub fn accepts_memory_teaching(
    sociability: u16,
    trust: i32,
    hunger: u64,
    meal_relief: u64,
) -> bool {
    i64::from(sociability) + i64::from(trust) >= 200 && hunger < meal_relief
}

/// Sparse, guarded learning over the receiving character's current knowledge.
/// Preparing this receipt never copies a whole mind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedMemoryLearning {
    actor: game_core::PersonId,
    patch: MemoryLearningPatch,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum MemoryLearningPatch {
    Stock {
        before: Option<StockKnowledge>,
        after: StockKnowledge,
    },
    Route {
        origin: AreaId,
        destination: AreaId,
        before: Option<SimDuration>,
        duration: SimDuration,
    },
}
pub fn prepare_memory_stock<A: game_ecs::Component>(
    store: &game_ecs::Store,
    actor: game_core::PersonId,
    now: SimTime,
    mut fact: StockKnowledge,
    source: KnowledgeSource,
) -> Result<PreparedMemoryLearning, game_core::Error> {
    let source_id = match &source {
        KnowledgeSource::Observed(id) | KnowledgeSource::Reported(id) => id.0,
        KnowledgeSource::Recorded(id) => id.0,
    };
    if fact.inventory.0 == 0 || fact.area.0 == 0 || fact.observed_at > now || source_id == 0 {
        return Err(game_core::Error::InvalidConfiguration(
            "invalid learned memory stock".into(),
        ));
    }
    crate::decisions::mind::<A>(store, actor).ok_or(game_core::Error::Unavailable)?;
    let before = crate::decisions::stocks(store, actor)
        .ok_or(game_core::Error::Unavailable)?
        .known
        .get(&fact.inventory)
        .cloned();
    if let Some(old) = before
        .as_ref()
        .filter(|old| old.observed_at > fact.observed_at)
    {
        fact = old.clone();
    } else {
        fact.accessible = before.as_ref().is_some_and(|old| old.accessible);
        fact.learned_at = now;
        fact.source = source;
    }
    Ok(PreparedMemoryLearning {
        actor,
        patch: MemoryLearningPatch::Stock {
            before,
            after: fact,
        },
    })
}
pub fn prepare_memory_route<A: game_ecs::Component>(
    store: &game_ecs::Store,
    actor: game_core::PersonId,
    origin: AreaId,
    destination: AreaId,
    duration: SimDuration,
) -> Result<PreparedMemoryLearning, game_core::Error> {
    if origin.0 == 0 || destination.0 == 0 || origin == destination || duration.0 == 0 {
        return Err(game_core::Error::InvalidConfiguration(
            "invalid learned memory route".into(),
        ));
    }
    let mind = crate::decisions::mind::<A>(store, actor).ok_or(game_core::Error::Unavailable)?;
    let before = mind
        .routes
        .get(&origin)
        .and_then(|links| links.get(&destination))
        .copied();
    Ok(PreparedMemoryLearning {
        actor,
        patch: MemoryLearningPatch::Route {
            origin,
            destination,
            before,
            duration,
        },
    })
}
impl PreparedMemoryLearning {
    pub fn actor(&self) -> game_core::PersonId {
        self.actor
    }
    pub fn validate<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
    ) -> Result<(), game_core::Error> {
        let mind =
            crate::decisions::mind::<A>(store, self.actor).ok_or(game_core::Error::Unavailable)?;
        mind.validate()?;
        let unchanged = match &self.patch {
            MemoryLearningPatch::Stock { before, after } => {
                crate::decisions::stocks(store, self.actor)
                    .and_then(|s| s.known.get(&after.inventory))
                    == before.as_ref()
            }
            MemoryLearningPatch::Route {
                origin,
                destination,
                before,
                ..
            } => {
                mind.routes
                    .get(origin)
                    .and_then(|links| links.get(destination))
                    .copied()
                    == *before
            }
        };
        if !unchanged {
            return Err(game_core::Error::Unavailable);
        }
        Ok(())
    }
    /// Stock learning publishes a native row; route learning remains a guarded
    /// field effect owned by the receiving character.
    pub fn stage_into<A: game_ecs::Component>(
        self,
        store: &game_ecs::Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<MemoryLearningFinalization, game_core::Error> {
        self.validate::<A>(store)?;
        let stock = matches!(self.patch, MemoryLearningPatch::Stock { .. });
        if batch.touches::<Mind<A>>(game_ecs::StableId(self.actor.0))
            || stock && batch.touches::<crate::decisions::Stocks>(game_ecs::StableId(self.actor.0))
        {
            return Err(game_core::Error::Unavailable);
        }
        if let MemoryLearningPatch::Stock { after, .. } = &self.patch {
            let mut row = crate::decisions::stocks(store, self.actor)
                .cloned()
                .ok_or(game_core::Error::Unavailable)?;
            row.known.insert(after.inventory, after.clone());
            batch.insert(game_ecs::StableId(self.actor.0), row);
        }
        let finalization = MemoryLearningFinalization {
            key: batch.key(),
            learning: self,
            stock,
        };
        finalization.validate_batch::<A>(store, batch)?;
        Ok(finalization)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryLearningFinalization {
    key: game_ecs::BatchKey,
    learning: PreparedMemoryLearning,
    stock: bool,
}
impl MemoryLearningFinalization {
    pub fn validate_batch<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
        batch: &game_ecs::Batch,
    ) -> Result<(), game_core::Error> {
        self.learning.validate::<A>(store)?;
        let ids = batch
            .edited_ids::<crate::decisions::Stocks>()
            .filter(|id| id.0 == self.learning.actor.0)
            .collect::<Vec<_>>();
        if self.key != batch.key()
            || batch.touches::<Mind<A>>(game_ecs::StableId(self.learning.actor.0))
            || self.stock && ids != vec![game_ecs::StableId(self.learning.actor.0)]
        {
            return Err(game_core::Error::Unavailable);
        }
        Ok(())
    }
    pub fn finish<A: game_ecs::Component>(self, store: &mut game_ecs::Store) {
        if let MemoryLearningPatch::Route {
            origin,
            destination,
            duration,
            ..
        } = self.learning.patch
        {
            crate::decisions::mind_mut::<A>(store, self.learning.actor)
                .expect("validated learning recipient")
                .learn_memory_route(origin, destination, duration);
        }
    }
}
