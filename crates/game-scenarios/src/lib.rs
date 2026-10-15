//! Declarative starting scenarios. Definitions contain references, not executable scripts.
use game_content::{Document, Store};
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub key: String,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub duration: SimDuration,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct People {
    pub key: String,
    pub count: usize,
    pub kind: game_physiology::Kind,
    pub area: String,
    #[serde(default)]
    pub traits: BTreeMap<String, i32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub key: String,
    #[serde(default)]
    pub area: Option<String>,
    #[serde(default)]
    pub carried_by: Option<String>,
    pub capacity: u64,
    pub owner: String,
    #[serde(default)]
    pub access: Vec<String>,
    #[serde(default)]
    pub known_to: Vec<String>,
    #[serde(default)]
    pub goods: BTreeMap<String, u64>,
    #[serde(default)]
    pub per_person: BTreeMap<String, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plot {
    pub key: String,
    pub area: String,
    pub steward: String,
    pub crop: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub key: String,
    pub area: String,
    pub object: String,
    pub stock: u64,
    pub capacity: u64,
    pub renew: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Institution {
    pub key: String,
    pub name: String,
    pub area: String,
    pub members: Vec<String>,
    pub leader: Option<String>,
    pub offices: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fighter {
    pub person: String,
    pub team: u32,
    pub x: i64,
    pub y: i64,
    pub controlled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Combat {
    pub key: String,
    pub area: String,
    pub width: i64,
    pub obstacles: usize,
    pub interval: SimDuration,
    pub movement: i64,
    pub reach: i64,
    pub damage: u64,
    pub fighters: Vec<Fighter>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reincarnation {
    pub mother: String,
    pub father: String,
    pub caregiver: String,
    pub area: String,
    pub delay: SimDuration,
    pub access: Vec<String>,
    #[serde(default)]
    pub traits: BTreeMap<String, i32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub label: String,
    pub fixture: bool,
    pub ruler: String,
    pub physiology: String,
    pub characters: String,
    pub environment: String,
    pub health: String,
    #[serde(default)]
    pub invocation: Option<String>,
    pub autonomy: bool,
    pub renewal: bool,
    pub areas: Vec<Area>,
    pub links: Vec<Link>,
    pub people: Vec<People>,
    pub inventories: Vec<Inventory>,
    #[serde(default)]
    pub plots: Vec<Plot>,
    #[serde(default)]
    pub patches: Vec<Patch>,
    #[serde(default)]
    pub institutions: Vec<Institution>,
    #[serde(default)]
    pub combat: Option<Combat>,
    #[serde(default)]
    pub reincarnation: Option<Reincarnation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    pub id: String,
    pub label: String,
    pub version: u32,
    pub fixture: bool,
}
impl Definition {
    pub fn actors(&self) -> Vec<(String, &People)> {
        self.people
            .iter()
            .flat_map(|p| {
                (0..p.count).map(move |i| {
                    (
                        if p.count == 1 {
                            p.key.clone()
                        } else {
                            format!("{}-{}", p.key, i + 1)
                        },
                        p,
                    )
                })
            })
            .collect()
    }
    pub fn select(&self, refs: &[String]) -> Result<Vec<String>> {
        let actors = self.actors();
        let mut selected = BTreeSet::new();
        for key in refs {
            if key == "*" {
                selected.extend(actors.iter().map(|(key, _)| key.clone()));
            } else if actors.iter().any(|(name, _)| name == key) {
                selected.insert(key.clone());
            } else if self.people.iter().any(|p| p.key == *key) {
                selected.extend(
                    actors
                        .iter()
                        .filter(|(_, p)| p.key == *key)
                        .map(|(key, _)| key.clone()),
                );
            } else {
                return Err(Error::InvalidConfiguration(format!(
                    "unknown person or group {key}"
                )));
            }
        }
        Ok(selected.into_iter().collect())
    }
    pub fn validate(&self) -> Result<()> {
        require(!self.label.is_empty(), "scenario needs a label")?;
        let keys = |keys: Vec<&str>| -> Result<BTreeSet<String>> {
            let mut result = BTreeSet::new();
            for key in keys {
                require(
                    !key.is_empty() && result.insert(key.into()),
                    "duplicate or empty scenario key",
                )?;
            }
            Ok(result)
        };
        let areas = keys(self.areas.iter().map(|a| a.key.as_str()).collect())?;
        require(!areas.is_empty(), "no areas")?;
        let count = self.people.iter().try_fold(0usize, |count, p| {
            count.checked_add(p.count).ok_or(Error::Overflow)
        })?;
        require(
            (1..=10_000).contains(&count) && self.people.iter().all(|p| p.count > 0),
            "invalid population",
        )?;
        let actors = self.actors();
        let people = keys(actors.iter().map(|(key, _)| key.as_str()).collect())?;
        let inventories = keys(self.inventories.iter().map(|i| i.key.as_str()).collect())?;
        keys(self.plots.iter().map(|p| p.key.as_str()).collect())?;
        keys(self.patches.iter().map(|p| p.key.as_str()).collect())?;
        keys(self.institutions.iter().map(|p| p.key.as_str()).collect())?;
        require(
            actors
                .iter()
                .any(|(key, p)| key == &self.ruler && p.kind == game_physiology::Kind::Mortal),
            "ruler must reference a mortal person",
        )?;
        for p in &self.people {
            require(areas.contains(&p.area), "unknown person's area")?;
        }
        for link in &self.links {
            require(
                link.from != link.to
                    && link.duration.0 > 0
                    && areas.contains(&link.from)
                    && areas.contains(&link.to),
                "invalid link",
            )?;
        }
        for i in &self.inventories {
            require(
                people.contains(&i.owner) && i.area.is_some() != i.carried_by.is_some(),
                "invalid inventory placement or owner",
            )?;
            if let Some(a) = &i.area {
                require(areas.contains(a), "unknown inventory area")?;
            }
            if let Some(p) = &i.carried_by {
                require(people.contains(p), "unknown inventory carrier")?;
            }
            self.select(&i.access)?;
            self.select(&i.known_to)?;
        }
        for p in &self.plots {
            require(
                areas.contains(&p.area) && people.contains(&p.steward),
                "unknown plot area or steward",
            )?;
        }
        for p in &self.patches {
            require(
                areas.contains(&p.area) && p.stock <= p.capacity,
                "invalid resource patch",
            )?;
        }
        for i in &self.institutions {
            let members: BTreeSet<_> = self.select(&i.members)?.into_iter().collect();
            require(
                areas.contains(&i.area)
                    && !members.is_empty()
                    && i.leader.as_ref().is_none_or(|p| members.contains(p))
                    && i.offices.values().all(|p| members.contains(p)),
                "invalid institution references",
            )?;
        }
        if let Some(c) = &self.combat {
            require(
                areas.contains(&c.area)
                    && c.fighters.len() >= 2
                    && (100..=1_000_000).contains(&c.width)
                    && c.obstacles <= 256
                    && c.interval.0 > 0
                    && c.movement > 0
                    && c.reach > 0,
                "invalid encounter",
            )?;
            keys(c.fighters.iter().map(|f| f.person.as_str()).collect())?;
            for f in &c.fighters {
                require(
                    people.contains(&f.person)
                        && f.x >= 0
                        && f.y >= 0
                        && f.x <= c.width
                        && f.y <= c.width,
                    "invalid fighter",
                )?;
            }
        }
        if let Some(r) = &self.reincarnation {
            require(
                [&r.mother, &r.father, &r.caregiver]
                    .iter()
                    .all(|p| people.contains(*p))
                    && areas.contains(&r.area)
                    && r.delay.0 > 0,
                "invalid reincarnation fixture",
            )?;
            for i in &r.access {
                require(inventories.contains(i), "unknown successor inventory")?;
            }
        }
        Ok(())
    }
}
pub fn load(store: &mut Store) -> Result<BTreeMap<String, Document<Definition>>> {
    let mut scenarios = BTreeMap::new();
    for path in store.scan("scenarios")? {
        let doc: Document<Definition> = store.load(&path)?;
        doc.data.validate()?;
        require(
            scenarios.insert(doc.id.clone(), doc).is_none(),
            "duplicate scenario ID",
        )?;
    }
    Ok(scenarios)
}
pub fn list(store: &mut Store) -> Result<Vec<Info>> {
    Ok(load(store)?
        .into_values()
        .map(|doc| Info {
            id: doc.id,
            label: doc.data.label,
            version: doc.version,
            fixture: doc.data.fixture,
        })
        .collect())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    default_scenario: String,
}
pub fn default_scenario(store: &mut Store) -> Result<String> {
    let doc: Document<Manifest> = store.load("manifest.toml")?;
    Ok(doc.data.default_scenario)
}
