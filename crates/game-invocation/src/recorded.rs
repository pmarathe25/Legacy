//! Historical content belongs to invocation; current learning belongs to characters.
use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    pub fn validate(&self) -> Result<(), Error> {
        let valid = match self {
            Self::Stock { life, fact } => {
                life.0 != 0
                    && fact.inventory.0 != 0
                    && fact.area.0 != 0
                    && fact.observed_at <= fact.learned_at
            }
            Self::Route {
                life,
                origin,
                destination,
                duration,
            } => {
                life.0 != 0
                    && origin.0 != 0
                    && destination.0 != 0
                    && origin != destination
                    && duration.0 > 0
            }
        };
        if !valid {
            return Err(Error::InvalidSave("invalid recorded memory".into()));
        }
        Ok(())
    }
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

use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InvocationAction {
    Activate {
        lives: BTreeSet<PersonId>,
    },
    Dismiss,
    AcceptWarning {
        warning: WarningId,
    },
    RecordMemory {
        life: PersonId,
        inventory: InventoryId,
        material: InventoryId,
        tool: InventoryId,
    },
    RecordRoute {
        life: PersonId,
        origin: AreaId,
        destination: AreaId,
        material: InventoryId,
        tool: InventoryId,
    },
    ReadRecord {
        record: RecordId,
    },
    MoveRecord {
        record: RecordId,
        destination: StockPosition,
    },
    TeachMemory {
        life: PersonId,
        inventory: InventoryId,
        recipient: PersonId,
    },
}

impl RecordedMemory {
    pub fn prepare_learning<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        now: SimTime,
        source: KnowledgeSource,
    ) -> Result<game_characters::memories::PreparedMemoryLearning, Error> {
        match self {
            Self::Stock { fact, .. } => game_characters::memories::prepare_memory_stock::<A>(
                store,
                actor,
                now,
                fact.clone(),
                source,
            ),
            Self::Route {
                origin,
                destination,
                duration,
                ..
            } => game_characters::memories::prepare_memory_route::<A>(
                store,
                actor,
                *origin,
                *destination,
                *duration,
            ),
        }
    }
}
