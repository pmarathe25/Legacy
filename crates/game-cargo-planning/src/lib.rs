//! Provisional physical stock arbitration. This neither consumes nor reserves stock.
use game_cargo::{Cargo, Inventory};
use game_core::{GoodId, InventoryId, PersonId};
use game_drafting::arbitration::{self, Demand, Intent, Resolution, ResourceKey};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockDemand {
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockIntent {
    pub id: u64,
    pub actor: PersonId,
    /// Other people required by the action; the actor is included automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<PersonId>,
    pub priority: u8,
    pub demands: Vec<StockDemand>,
}

pub use game_drafting::arbitration::Error as StockPlanError;

/// A process-local proposal against a particular physical-stock revision.
/// Owners must revalidate permissions, location and all effects before committing.
pub struct StockPlan {
    pub resolution: Resolution,
    version: game_storage::Version<InventoryId, Arc<Inventory>>,
}
impl StockPlan {
    /// Only tests whether the physical cargo table is unchanged; other owner
    /// state, including access permissions, is deliberately outside this check.
    pub fn stock_unchanged(&self, cargo: &Cargo) -> bool {
        cargo.0.matches(&self.version)
    }
}

fn resource(demand: &StockDemand) -> ResourceKey {
    ResourceKey {
        domain: 0,
        owner: demand.inventory.0,
        item: demand.good.0,
    }
}

/// Resolve all-or-nothing claims without changing state or exposing them as
/// character knowledge. Decisions are provisional, not reservations.
pub fn plan_stock(
    cargo: &Cargo,
    pool: &rayon::ThreadPool,
    round: u64,
    intents: &[StockIntent],
) -> std::result::Result<StockPlan, StockPlanError> {
    plan_stock_inner(cargo, pool, round, intents, None)
}

/// Plan claims using an explicit order of real actor IDs, within each priority.
pub fn plan_stock_ordered(
    cargo: &Cargo,
    pool: &rayon::ThreadPool,
    intents: &[StockIntent],
    precedence: &[PersonId],
) -> std::result::Result<StockPlan, StockPlanError> {
    let order: Vec<_> = precedence.iter().map(|person| person.0).collect();
    plan_stock_inner(cargo, pool, 0, intents, Some(&order))
}

fn plan_stock_inner(
    cargo: &Cargo,
    pool: &rayon::ThreadPool,
    round: u64,
    intents: &[StockIntent],
    precedence: Option<&[u64]>,
) -> std::result::Result<StockPlan, StockPlanError> {
    let mut available = BTreeMap::new();
    let inputs: Vec<_> = intents
        .iter()
        .map(|intent| {
            let demands = intent
                .demands
                .iter()
                .map(|demand| {
                    let key = resource(demand);
                    available.entry(key).or_insert_with(|| {
                        cargo
                            .0
                            .get(&demand.inventory)
                            .map_or(0, |inventory| inventory.amount(demand.good))
                    });
                    Demand {
                        resource: key,
                        quantity: demand.quantity,
                    }
                })
                .collect();
            Intent {
                id: intent.id,
                actor: intent.actor.0,
                participants: intent.participants.iter().map(|p| p.0).collect(),
                priority: intent.priority,
                demands,
            }
        })
        .collect();
    let resolution = match precedence {
        Some(order) => arbitration::arbitrate_ordered(pool, &available, &inputs, order)?,
        None => arbitration::arbitrate(pool, &available, &inputs, round)?,
    };
    Ok(StockPlan {
        resolution,
        version: cargo.0.version(),
    })
}
