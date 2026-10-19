//! Personal, experienced attachments, independent of ownership and exchange value.
use game_core::*;
mod animals;
mod choices;
pub use choices::{Gift, Locate, Recall, TellMemory};
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    Object(u64),
    Animal(u64),
    Person(PersonId),
    Plot(PlotId),
    WildPlant { patch: u64, plant: u64 },
}
impl Target {
    pub fn key(&self) -> String {
        match self {
            Self::Object(id) => format!("object:{id}"),
            Self::Animal(id) => format!("animal:{id}"),
            Self::Person(p) => format!("person:{}", p.0),
            Self::Plot(p) => format!("plot:{}", p.0),
            Self::WildPlant { patch, plant } => format!("plant:{patch}:{plant}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub target: Target,
    pub value: i32,
    pub at: SimTime,
    pub reason: String,
    pub experiences: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentiment {
    pub people: Table<PersonId, Table<String, Memory>>,
    pub witnessed_losses: u64,
    #[serde(default)]
    pub recalled: Table<PersonId, SimTime>,
    #[serde(default)]
    pub locations: Table<PersonId, Table<u64, InventoryId>>,
}
impl Resource for Sentiment {
    const KEY: &'static str = "sentiment.attachments";
    fn validate(&self) -> Result<()> {
        require(
            self.people.values().all(|r| {
                r.iter().all(|(k, m)| {
                    k == &m.target.key()
                        && (-1000..=1000).contains(&m.value)
                        && !m.reason.trim().is_empty()
                })
            }),
            "invalid attachment",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        let empty = Table::new();
        require(
            self.people
                .changed_pairs(&old.people)
                .all(|(_, rows, previous)| {
                    rows.changed(previous.unwrap_or(&empty)).all(|(k, m)| {
                        k == &m.target.key()
                            && (-1000..=1000).contains(&m.value)
                            && !m.reason.trim().is_empty()
                    })
                }),
            "invalid attachment",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Experience {
    pub person: PersonId,
    pub target: Target,
    pub intensity: i32,
    pub reason: String,
}
impl Request for Experience {
    const KEY: &'static str = "sentiment.experience";
    type Response = i32;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Changed {
    pub person: PersonId,
    pub target: Target,
    pub value: i32,
    pub reason: String,
}
impl Event for Changed {
    const KEY: &'static str = "sentiment.changed";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Lost {
    pub person: PersonId,
    pub item: u64,
    pub distress: u64,
}
impl Event for Lost {
    const KEY: &'static str = "sentiment.lost";
}
fn local(tx: &Transaction<'_>, person: PersonId, target: &Target) -> Result<()> {
    let area = game_transport::area(tx.get()?, person)?;
    let at = match target {
        Target::Animal(id) => tx
            .get::<game_fauna::Fauna>()?
            .animals
            .get(id)
            .map(|a| a.area),
        Target::Object(id) => tx
            .get::<game_items::Items>()?
            .objects
            .get(id)
            .and_then(|i| i.inventory)
            .map(|i| game_cargo::locality(tx, i))
            .transpose()?,
        Target::Person(p) => game_transport::area(tx.get()?, *p).ok(),
        Target::Plot(p) => tx
            .get::<game_agriculture::Farms>()?
            .plots
            .get(p)
            .map(|p| p.area),
        Target::WildPlant { patch, plant } => tx
            .get::<game_botany::Botany>()?
            .stands
            .get(patch)
            .filter(|s| s.plants.iter().any(|p| p.id == *plant))
            .and_then(|_| {
                tx.get::<game_ecology::Ecology>()
                    .ok()?
                    .patches
                    .get(patch)
                    .map(|p| p.area)
            }),
    };
    require(
        at == Some(area),
        "attachment needs a personally present subject",
    )
}
pub fn value(tx: &Transaction<'_>, person: PersonId, target: &Target) -> i32 {
    tx.get::<Sentiment>()
        .ok()
        .and_then(|s| s.people.get(&person)?.get(&target.key()))
        .map_or(0, |m| m.value)
}
/// Only the particular objects that this physical quantity would remove contribute.
pub fn relinquishing(
    tx: &Transaction<'_>,
    person: PersonId,
    inv: InventoryId,
    good: GoodId,
    q: u64,
) -> i64 {
    let Ok(items) = tx.get::<game_items::Items>() else {
        return 0;
    };
    let held = tx
        .get::<game_cargo::Cargo>()
        .ok()
        .and_then(|c| c.0.get(&inv))
        .map_or(0, |i| i.amount(good));
    game_items::departing(items, inv, good, held, q)
        .into_iter()
        .map(|id| i64::from(value(tx, person, &Target::Object(id))))
        .sum()
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_fauna::plugin(b)?;
    b.plugin("sentiment", 1)?;
    b.resource::<Sentiment>()?;
    choices::register(b)?;
    animals::register(b)?;
    b.request::<Experience>(|tx, r| {
        require(
            !r.reason.trim().is_empty()
                && r.reason.len() <= 300
                && (-1000..=1000).contains(&r.intensity),
            "invalid sentimental experience",
        )?;
        local(tx, r.person, &r.target)?;
        if let Target::Object(item) = r.target
            && let Some(inventory) = tx.get::<game_items::Items>()?.objects[&item].inventory
        {
            tx.edit::<Sentiment>()?
                .locations
                .entry(r.person)
                .or_default()
                .insert(item, inventory);
        }
        let at = tx.now();
        let key = r.target.key();
        let m = tx
            .edit::<Sentiment>()?
            .people
            .entry(r.person)
            .or_default()
            .entry(key)
            .or_insert_with(|| Memory {
                target: r.target,
                value: 0,
                at,
                reason: r.reason.clone(),
                experiences: 0,
            });
        let previous = m.value;
        m.value = m.value.saturating_add(r.intensity).clamp(-1000, 1000);
        m.at = at;
        m.reason = r.reason;
        m.experiences = m.experiences.saturating_add(1);
        let result = m.value;
        let event = Changed {
            person: r.person,
            target: m.target.clone(),
            value: result,
            reason: m.reason.clone(),
        };
        let significant = m.experiences == 1 || previous / 100 != result / 100;
        if significant {
            tx.emit(event, vec![r.person])?;
        }
        Ok(result)
    })?;
    b.subscribe::<game_workshop::Produced>("sentiment.making", |tx, e| {
        let o = tx.get::<game_objects::Objects>()?.by_id(e.good)?;
        if o.food.is_some() || !o.tags.contains("crafted") {
            return Ok(());
        }
        let label = o.name.clone();
        let item = tx.call(game_items::Mark {
            person: e.person,
            inventory: e.inventory,
            good: e.good,
            label,
        })?;
        let investment = game_vitality::disposition(tx, e.person, "creativity");
        tx.call(Experience {
            person: e.person,
            target: Target::Object(item),
            intensity: (investment / 30).clamp(1, 40) as i32,
            reason: format!("made it by {}", e.skill),
        })?;
        Ok(())
    })?;
    b.subscribe::<game_agriculture::Harvested>("sentiment.tending", |tx, e| {
        if e.quantity > 0 {
            tx.call(Experience {
                person: e.person,
                target: Target::Plot(e.plot),
                intensity: 3,
                reason: "tended and harvested plants".into(),
            })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_items::Moved>("sentiment.receiving-and-loss", |tx, e| {
        if let Some(to) = e.to {
            let observers = if let Ok(area) = game_cargo::locality(tx, e.from)
                && tx.registered::<game_property::Observers>()
            {
                tx.call(game_property::Observers { area })?
            } else {
                vec![]
            };
            let key = Target::Object(e.item).key();
            for person in observers {
                if tx
                    .get::<Sentiment>()?
                    .people
                    .get(&person)
                    .is_some_and(|r| r.contains_key(&key))
                {
                    tx.edit::<Sentiment>()?
                        .locations
                        .entry(person)
                        .or_default()
                        .insert(e.item, to);
                }
            }
            if let game_cargo::Custody::Carried(receiver) =
                tx.get::<game_cargo::Cargo>()?.0[&to].custody
                && let game_cargo::Custody::Carried(sender) =
                    tx.get::<game_cargo::Cargo>()?.0[&e.from].custody
                && sender != receiver
            {
                let feeling = tx
                    .get::<game_relationships::Social>()
                    .ok()
                    .and_then(|s| s.relationships.get(&receiver)?.get(&sender))
                    .map_or(3, |r| (r.affection + r.trust) / 4);
                tx.call(Experience {
                    person: receiver,
                    target: Target::Object(e.item),
                    intensity: feeling,
                    reason: format!("received from {}", sender.0),
                })?;
            }
            return Ok(());
        }
        let key = Target::Object(e.item).key();
        let memories: Vec<_> = tx
            .get::<Sentiment>()?
            .people
            .iter()
            .filter_map(|(p, r)| r.get(&key).filter(|m| m.value > 0).map(|m| (*p, m.value)))
            .collect();
        let custody = tx.get::<game_cargo::Cargo>()?.0[&e.from].custody.clone();
        let observers = if let Ok(area) = game_cargo::locality(tx, e.from)
            && tx.registered::<game_property::Observers>()
        {
            tx.call(game_property::Observers { area })?
        } else {
            vec![]
        };
        for (person, attachment) in memories {
            if custody != game_cargo::Custody::Carried(person) && !observers.contains(&person) {
                continue;
            }
            let distress = (attachment / 10).max(1) as u64;
            if tx.registered::<game_vitality::Experience>() {
                tx.call(game_vitality::Experience {
                    person,
                    stress: distress as i32,
                    satisfaction: -(distress as i32),
                })?;
            }
            tx.edit::<Sentiment>()?.witnessed_losses =
                tx.get::<Sentiment>()?.witnessed_losses.saturating_add(1);
            tx.emit(
                Lost {
                    person,
                    item: e.item,
                    distress,
                },
                vec![person],
            )?;
        }
        Ok(())
    })?;
    Ok(())
}
