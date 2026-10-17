//! Character-local facts and delayed reports. Queries never teach facts.
pub use game_facts::with_prefix;
use game_storage::{Records, Table};
use std::sync::Arc;
mod persistence;
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
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct Facts(
    game_facts::Facts<FactValue>,
    #[serde(skip)] Option<game_storage::Version<InventoryId, ()>>,
);
impl PartialEq for Facts {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Facts {}
impl Facts {
    pub fn insert(&mut self, key: Arc<str>, mut fact: Fact) -> Option<Fact> {
        fact.value = FactValue::promote(fact.value, &key);
        if matches!(fact.value.as_ref(), FactValue::Other(_)) {
            self.1 = None;
        }
        self.0.insert(key, fact)
    }
}
impl std::ops::Deref for Facts {
    type Target = game_facts::Facts<FactValue>;
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
pub type Fact = game_facts::Fact<FactValue>;
pub type FactRef<'a> = game_facts::FactRef<'a, FactValue>;
#[derive(Clone, Debug, Default)]
pub struct Knowledge(
    pub game_packed::Packed<PersonId, Facts>,
    Records<Subject, Cached>,
    Records<AreaId, scenes::CachedScene>,
);
impl PartialEq for Knowledge {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Knowledge {}
type KnowledgeMark = (
    game_packed::Mark,
    Records<Subject, Cached>,
    Records<AreaId, scenes::CachedScene>,
);
impl Resource for Knowledge {
    const KEY: &'static str = "knowledge.facts";
    fn validate(&self) -> Result<()> {
        require(
            self.0.values().all(|m| m.0.valid_confidence(None)),
            "invalid confidence",
        )
    }
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new((self.0.begin(), self.1.clone(), self.2.clone()))
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        let (mark, sources, scenes) = *checkpoint
            .downcast::<KnowledgeMark>()
            .expect("knowledge checkpoint");
        self.0.rollback(mark);
        self.1 = sources;
        self.2 = scenes;
    }
    fn commit(&mut self, checkpoint: Checkpoint) {
        self.0.commit(
            checkpoint
                .downcast::<KnowledgeMark>()
                .expect("knowledge checkpoint")
                .0,
        );
    }
    fn validate_checkpoint(&self, checkpoint: &dyn std::any::Any) -> Result<()> {
        let previous = checkpoint
            .downcast_ref::<KnowledgeMark>()
            .expect("knowledge checkpoint");
        for (_, facts, old) in self.0.changed_pairs_since(&previous.0) {
            require(
                facts.0.valid_confidence(old.map(|v| &v.0)),
                "invalid confidence",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Initialize {
    pub person: PersonId,
    pub facts: Facts,
}
impl Request for Initialize {
    const KEY: &'static str = "knowledge.initialize";
    type Response = ();
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
#[derive(Clone, Serialize, Deserialize)]
pub struct Tell {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub key: String,
}
impl Request for Tell {
    const KEY: &'static str = "knowledge.tell";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
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
    b.request::<Initialize>(|tx, r| {
        require(
            !tx.get::<Knowledge>()?.0.contains_key(&r.person),
            "knowledge already initialized",
        )?;
        tx.edit::<Knowledge>()?.0.insert(r.person, r.facts);
        Ok(())
    })?;
    b.validate_save(|host| {
        let people = &host.get::<Knowledge>()?.0;
        require(
            people
                .values()
                .all(|f| f.valid_at(host.now(), host.revision())),
            "knowledge from a future observation",
        )
    });
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
    b.request::<Tell>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.sender)?
                == game_transport::area(tx.get()?, r.recipient)?,
            "recipient is elsewhere",
        )?;
        if game_communication::active(tx) {
            require(
                game_communication::understood(tx, r.sender, r.recipient, &r.key, true)?,
                "report was not understood",
            )?;
        }
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
            .map(|f| f.to_owned())
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
        if facts.get(r.key.as_str()).is_none_or(|old| {
            (old.observed, old.revision, old.confidence)
                < (r.fact.observed, r.fact.revision, r.fact.confidence)
        }) {
            facts.insert(r.key.into(), r.fact);
        }
        Ok(())
    })
}
