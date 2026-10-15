//! Character-local facts and delayed reports. Queries never teach facts.
mod geography;
use game_core::*;
use game_sim::*;
pub use geography::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub value: serde_json::Value,
    pub source: PersonId,
    pub observed: SimTime,
    pub revision: u64,
    pub confidence: u8,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knowledge(pub BTreeMap<PersonId, BTreeMap<String, Fact>>);
impl Resource for Knowledge {
    const KEY: &'static str = "knowledge.facts";
    fn validate(&self) -> Result<()> {
        require(
            self.0
                .values()
                .all(|m| m.values().all(|f| f.confidence <= 100)),
            "invalid confidence",
        )
    }
}
#[derive(Serialize, Deserialize)]
pub struct ObserveStore {
    pub person: PersonId,
    pub inventory: InventoryId,
}
impl Request for ObserveStore {
    const KEY: &'static str = "knowledge.observe_store";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Report {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub key: String,
    pub delay: SimDuration,
}
impl Request for Report {
    const KEY: &'static str = "knowledge.report";
    type Response = SimTime;
}
/// Spoken information requires both people to be physically together.
#[derive(Serialize, Deserialize)]
pub struct Tell {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub key: String,
}
impl Request for Tell {
    const KEY: &'static str = "knowledge.tell";
    type Response = SimTime;
}
#[derive(Serialize, Deserialize)]
struct Deliver {
    recipient: PersonId,
    key: String,
    fact: Fact,
}
impl Request for Deliver {
    const KEY: &'static str = "knowledge.deliver";
    type Response = ();
}
pub fn store_key(id: InventoryId) -> String {
    format!("store:{}", id.0)
}
pub fn view(host: &Host, person: PersonId) -> Result<BTreeMap<String, Fact>> {
    Ok(host
        .get::<Knowledge>()?
        .0
        .get(&person)
        .cloned()
        .unwrap_or_default())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("knowledge", 1)?;
    b.resource::<Knowledge>()?;
    b.validate_save(|host| {
        require(
            host.get::<Knowledge>()?.0.values().all(|facts| {
                facts
                    .values()
                    .all(|f| f.observed <= host.now() && f.revision <= host.revision())
            }),
            "knowledge from a future observation",
        )
    });
    geography::register(b)?;
    b.request::<ObserveStore>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.person)? == game_cargo::locality(tx, r.inventory)?,
            "cannot observe distant inventory",
        )?;
        let area = game_cargo::locality(tx, r.inventory)?;
        let carrier = match tx.get::<game_cargo::Cargo>()?.0[&r.inventory].custody {
            game_cargo::Custody::Carried(p) => Some(p),
            _ => None,
        };
        let container = &tx.get::<game_cargo::Cargo>()?.0[&r.inventory];
        let capacity = container.capacity;
        let mass = container.mass;
        geography::remember(
            tx,
            r.person,
            location_key(r.inventory),
            &StoreLocation {
                area,
                carrier,
                capacity,
                mass,
            },
        )?;
        let value = serde_json::to_value(&tx.get::<game_cargo::Cargo>()?.0[&r.inventory].goods)
            .map_err(|e| Error::Rejected(e.to_string()))?;
        let fact = Fact {
            value,
            source: r.person,
            observed: tx.now(),
            revision: tx.revision_after(),
            confidence: 100,
        };
        tx.edit::<Knowledge>()?
            .0
            .entry(r.person)
            .or_default()
            .insert(store_key(r.inventory), fact);
        Ok(())
    })?;
    b.request::<Tell>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.sender)?
                == game_transport::area(tx.get()?, r.recipient)?,
            "recipient is elsewhere",
        )?;
        tx.call(Report {
            sender: r.sender,
            recipient: r.recipient,
            key: r.key,
            delay: SimDuration(0),
        })
    })?;
    b.request::<Report>(|tx, r| {
        let mut fact = tx
            .get::<Knowledge>()?
            .0
            .get(&r.sender)
            .and_then(|m| m.get(&r.key))
            .cloned()
            .ok_or_else(|| Error::Rejected("sender does not know this fact".into()))?;
        fact.source = r.sender;
        fact.confidence = fact.confidence.min(90);
        let at = tx.now().after(r.delay)?;
        tx.schedule(
            at,
            30,
            Deliver {
                recipient: r.recipient,
                key: r.key,
                fact,
            },
        )?;
        Ok(at)
    })?;
    b.request::<Deliver>(|tx, r| {
        let facts = tx.edit::<Knowledge>()?.0.entry(r.recipient).or_default();
        if facts.get(&r.key).is_none_or(|old| {
            (old.observed, old.revision, old.confidence)
                < (r.fact.observed, r.fact.revision, r.fact.confidence)
        }) {
            facts.insert(r.key, r.fact);
        }
        Ok(())
    })
}
