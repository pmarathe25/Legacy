//! Invocation commands and borrowed, source-tagged knowledge views.
use crate::Context;
use game_parallel::KnowledgeRecord;
impl Context<'_> {
    pub fn invoke(&mut self, lives: &[u64]) -> Result<(), String> {
        let lineage = self.world.lineage.as_mut().ok_or("lineage is disabled")?;
        let history: Vec<_> = lineage.history.iter().map(|life| life.person).collect();
        lineage
            .invocation
            .as_mut()
            .ok_or("invocation is disabled")?
            .activate(lives, &history, self.world.tick)
    }
    pub fn dismiss_invocation(&mut self) -> Result<(), String> {
        self.world
            .lineage
            .as_mut()
            .and_then(|lineage| lineage.invocation.as_mut())
            .ok_or("invocation is disabled")?
            .dismiss(self.world.tick)
    }
    pub fn accept_invocation_warning(&mut self, revision: u64) -> Result<(), String> {
        self.world
            .lineage
            .as_mut()
            .and_then(|lineage| lineage.invocation.as_mut())
            .ok_or("invocation is disabled")?
            .accept(revision, self.world.tick)
    }
    pub fn borrowed_memories(&self) -> Result<Vec<(u64, KnowledgeRecord)>, String> {
        let invocation = self
            .world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.invocation.as_ref())
            .ok_or("invocation is disabled")?;
        let mut records = Vec::new();
        if let Some(knowledge) = &self.world.knowledge {
            for &life in &invocation.active {
                if let Some(mind) = knowledge.minds.iter().find(|mind| mind.person == life) {
                    records.extend(mind.records.iter().cloned().map(|record| (life, record)));
                }
            }
        }
        Ok(records)
    }
}
