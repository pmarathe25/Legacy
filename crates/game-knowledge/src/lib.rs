//! Character-local facts and delayed reports. Queries never teach facts.
pub use game_facts::{same_value, with_prefix};
use game_storage::{Records, Table};
use std::sync::Arc;
mod scenes;
mod snapshots;
pub use snapshots::FactValue;
use snapshots::{Cached, Subject};
pub type Stock = Table<GoodId, u64>;
mod geography;
use game_core::*;
use game_sim::*;
pub use geography::*;
use serde::{Deserialize, Serialize};
pub type Facts = game_memory::Facts<FactValue>;
pub type Fact = game_memory::Fact<FactValue>;
pub type FactRef<'a> = game_facts::FactRef<'a, FactValue>;
pub type Knowledge = game_memory::Memories<FactValue>;
pub type Initialize = game_memory::Initialize<FactValue>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Perception {
    #[serde(skip)]
    subjects: Records<Subject, Cached>,
    #[serde(skip)]
    scenes: Records<AreaId, scenes::CachedScene>,
}
impl Resource for Perception {
    const KEY: &'static str = "knowledge.perception";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ObserveStore {
    pub person: PersonId,
    pub inventory: InventoryId,
}
impl Request for ObserveStore {
    const KEY: &'static str = "knowledge.observe_store";
    type Response = ();
}
pub fn store_key(id: InventoryId) -> String {
    format!("store:{}", id.0)
}
pub fn view(host: &Host, person: PersonId) -> Result<Facts> {
    Ok(host
        .get::<Knowledge>()?
        .0
        .get(&person)
        .cloned()
        .unwrap_or_default())
}
/// A receiver-owned entry point for interpreted observations, inference and readable records.
pub type Receive = game_memory::Receive<FactValue>;
#[derive(Clone, Serialize, Deserialize)]
pub struct Conclude {
    pub person: PersonId,
    pub key: String,
    pub value: serde_json::Value,
    pub confidence: u8,
}
impl Request for Conclude {
    const KEY: &'static str = "knowledge.conclude";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_memory::plugin::<FactValue>(b)?;
    b.plugin("knowledge", 1)?;
    b.resource::<Perception>()?;
    b.request::<Conclude>(|tx, r| {
        tx.call(Receive {
            person: r.person,
            key: r.key,
            fact: Fact {
                value: Arc::new(FactValue::Other(r.value)),
                source: r.person,
                observed: tx.now(),
                revision: tx.revision_after(),
                confidence: r.confidence,
            },
        })
    })?;
    geography::register(b)?;
    b.request::<ObserveStore>(|tx, r| {
        let area = game_transport::area(tx.get()?, r.person)?;
        if game_cargo::locality(tx, r.inventory).ok() != Some(area) {
            let memory = tx
                .get::<Knowledge>()?
                .0
                .get(&r.person)
                .and_then(|f| known::<StoreLocation>(f, &location_key(r.inventory)));
            require(
                memory.is_some_and(|m| m.area == area),
                "cannot observe distant inventory",
            )?;
            return geography::remember(
                tx,
                r.person,
                Subject::Stock(r.inventory),
                FactValue::Other(serde_json::json!({"absent_from":area})),
            );
        }
        let container = &tx.get::<game_cargo::Cargo>()?.0[&r.inventory];
        let carrier = match container.custody {
            game_cargo::Custody::Carried(p) => Some(p),
            _ => None,
        };
        let data = StoreLocation {
            area,
            carrier,
            capacity: container.capacity,
            mass: container.mass,
        };
        let goods = container.goods.clone();
        geography::remember(
            tx,
            r.person,
            Subject::Location(r.inventory),
            FactValue::Location(data),
        )?;
        geography::remember(
            tx,
            r.person,
            Subject::Stock(r.inventory),
            FactValue::Stock(goods),
        )?;
        Ok(())
    })?;
    Ok(())
}

/// Forecast from personally remembered measurements and theories; no world-state query.
pub fn forecast(
    facts: &Facts,
    subject: &str,
    outcome: &str,
    changes: &std::collections::BTreeMap<String, i64>,
) -> Option<game_reasoning::Prediction> {
    let fact = facts.get(&format!("measured:{subject}:{outcome}"))?;
    let baseline: game_reasoning::Observation = fact.value.decode()?;
    let claims = with_prefix(facts, &format!("theory:{subject}:"))
        .take(128)
        .filter_map(|(_, f)| {
            let mut c: game_reasoning::Claim = f.value.decode()?;
            c.confidence = c.confidence.min(f.confidence);
            Some(c)
        })
        .collect::<Vec<_>>();
    let mut prediction = game_reasoning::predict(&claims, &baseline, changes)?;
    prediction.confidence = prediction.confidence.min(fact.confidence);
    Some(prediction)
}
