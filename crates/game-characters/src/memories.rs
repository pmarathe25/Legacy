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

impl<A> MemoryLearner for Mind<A> {
    /// Historical information may replace equally old observations, but never a
    /// newer one. Reading or teaching does not grant current physical access.
    fn learn_memory_stock(
        &mut self,
        now: SimTime,
        mut fact: StockKnowledge,
        source: KnowledgeSource,
    ) -> bool {
        if self
            .knowledge
            .get(&fact.inventory)
            .is_some_and(|old| old.observed_at > fact.observed_at)
        {
            return false;
        }
        fact.accessible = self
            .knowledge
            .get(&fact.inventory)
            .is_some_and(|old| old.accessible);
        fact.learned_at = now;
        fact.source = source;
        self.knowledge.insert(fact.inventory, fact);
        true
    }
    /// A recalled route supplies a missing edge. It does not rewrite a route
    /// already learned by the current person.
    fn learn_memory_route(&mut self, origin: AreaId, destination: AreaId, duration: SimDuration) {
        self.routes
            .entry(origin)
            .or_default()
            .entry(destination)
            .or_insert(duration);
    }
}

#[cfg(test)]
mod tests;
