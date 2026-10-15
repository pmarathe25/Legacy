//! Character-local facts and delayed reports. Queries never teach facts.
use game_storage::{Records, Table};
use std::sync::Arc;
mod persistence;
mod snapshots;
pub use snapshots::FactValue;
use snapshots::{Cached, Subject};
pub type Stock = Table<GoodId, u64>;
mod geography;
use game_core::*;
use game_sim::*;
pub use geography::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Facts(Table<Arc<str>, Fact>);
impl Facts {
    pub fn insert(&mut self, key: Arc<str>, mut fact: Fact) -> Option<Fact> {
        fact.value = FactValue::promote(fact.value, &key);
        self.0.insert(key, fact)
    }
}
impl std::ops::Deref for Facts {
    type Target = Table<Arc<str>, Fact>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl FromIterator<(Arc<str>, Fact)> for Facts {
    fn from_iter<I: IntoIterator<Item = (Arc<str>, Fact)>>(rows: I) -> Self {
        let mut facts = Self::default();
        for (key, fact) in rows {
            facts.insert(key, fact);
        }
        facts
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub value: Arc<FactValue>,
    pub source: PersonId,
    pub observed: SimTime,
    pub revision: u64,
    pub confidence: u8,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct Knowledge(
    pub Table<PersonId, Facts>,
    #[serde(skip)] Records<Subject, Cached>,
);
impl PartialEq for Knowledge {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Knowledge {}
impl Resource for Knowledge {
    const KEY: &'static str = "knowledge.facts";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.0.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        for (person, facts) in self.0.changed(&previous.0) {
            if let Some(old) = previous.0.get(person) {
                require(
                    facts.changed(old).all(|(_, f)| f.confidence <= 100),
                    "invalid confidence",
                )?;
            } else {
                self.validate_rows(std::iter::once(facts))?;
            }
        }
        Ok(())
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
pub fn view(host: &Host, person: PersonId) -> Result<Facts> {
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
        let data = StoreLocation {
            area,
            carrier,
            capacity,
            mass,
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
            .and_then(|m| m.get(r.key.as_str()))
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
    b.request::<Deliver>(|tx, mut r| {
        r.fact.value = FactValue::promote(r.fact.value, &r.key);
        let facts = tx.edit::<Knowledge>()?.0.entry(r.recipient).or_default();
        if facts.get(r.key.as_str()).is_none_or(|old| {
            (old.observed, old.revision, old.confidence)
                < (r.fact.observed, r.fact.revision, r.fact.confidence)
        }) {
            facts.insert(r.key.into(), r.fact);
        }
        Ok(())
    })
}

impl Knowledge {
    fn validate_rows<'a>(&'a self, mut rows: impl Iterator<Item = &'a Facts>) -> Result<()> {
        require(
            rows.all(|m| m.values().all(|f| f.confidence <= 100)),
            "invalid confidence",
        )
    }
}
