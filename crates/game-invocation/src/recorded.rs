//! Historical content belongs to invocation; current learning belongs to characters.
use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordedMemory {
    Stock {
        life: PersonId,
        fact: StockKnowledge,
    },
    Route {
        life: PersonId,
        origin: AreaId,
        destination: AreaId,
        duration: SimDuration,
    },
}
impl RecordedMemory {
    pub fn life(&self) -> PersonId {
        match self {
            Self::Stock { life, .. } | Self::Route { life, .. } => *life,
        }
    }
    /// The receiving owner retains its current rights and newer knowledge. The
    /// historical source cannot grant those rights or overwrite current routes.
    pub fn teach_into(
        &self,
        recipient: &mut impl game_characters::memories::MemoryLearner,
        now: SimTime,
        source: KnowledgeSource,
    ) {
        match self {
            Self::Stock { fact, .. } => {
                recipient.learn_memory_stock(now, fact.clone(), source);
            }
            Self::Route {
                origin,
                destination,
                duration,
                ..
            } => {
                recipient.learn_memory_route(*origin, *destination, *duration);
            }
        }
    }
}

#[cfg(test)]
mod tests;
