//! Trait names and their numeric effects are data, not Rust variants.
use game_content::{Document, Stamp, Store};
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trait {
    pub name: String,
    pub minimum: i32,
    pub maximum: i32,
    pub default: i32,
    #[serde(default)]
    pub weights: BTreeMap<String, i32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    pub traits: BTreeMap<String, Trait>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Traits {
    pub definitions: Records<String, Trait>,
    pub people: Records<PersonId, BTreeMap<String, i32>>,
    pub stamps: Vec<Stamp>,
}
impl Resource for Traits {
    const KEY: &'static str = "traits.dispositions";
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        if self
            .definitions
            .changed(&previous.definitions)
            .next()
            .is_some()
        {
            return self.validate();
        }
        for (_, profile) in self.people.changed(&previous.people) {
            self.validate_profile(profile)?;
        }
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        for (key, t) in &self.definitions {
            require(
                !key.is_empty()
                    && !t.name.is_empty()
                    && t.minimum < t.maximum
                    && (t.minimum..=t.maximum).contains(&t.default),
                "invalid trait range",
            )?;
            for term in t.weights.keys() {
                require(
                    ["need", "goal", "value", "effort", "risk"].contains(&term.as_str()),
                    "unknown trait utility term",
                )?;
            }
        }
        for profile in self.people.values() {
            self.validate_profile(profile)?;
        }
        Ok(())
    }
}
impl Traits {
    pub fn validate_profile(&self, profile: &BTreeMap<String, i32>) -> Result<()> {
        for (key, value) in profile {
            let t = self
                .definitions
                .get(key)
                .ok_or_else(|| Error::InvalidConfiguration(format!("unknown trait {key}")))?;
            require(
                (t.minimum..=t.maximum).contains(value),
                "trait outside authored range",
            )?;
        }
        Ok(())
    }
    pub fn weights(&self, person: PersonId) -> BTreeMap<String, i64> {
        let mut weights = BTreeMap::new();
        for (key, t) in &self.definitions {
            let value = self
                .people
                .get(&person)
                .and_then(|p| p.get(key))
                .copied()
                .unwrap_or(t.default);
            for (term, coefficient) in &t.weights {
                let delta =
                    (i64::from(value) - i64::from(t.default)) * i64::from(*coefficient) / 100;
                let w = weights.entry(term.clone()).or_insert(100i64);
                *w = w.saturating_add(delta).clamp(0, 1000);
            }
        }
        weights
    }
}
pub fn load(store: &mut Store) -> Result<Traits> {
    let mut definitions = BTreeMap::new();
    let mut stamps = vec![];
    for path in store.scan("traits")? {
        let doc: Document<Definitions> = store.load(&path)?;
        stamps.push(doc.stamp()?);
        for (key, t) in doc.data.traits {
            require(definitions.insert(key, t).is_none(), "duplicate trait key")?;
        }
    }
    let traits = Traits {
        definitions: definitions.into(),
        people: Records::new(),
        stamps,
    };
    traits.validate()?;
    Ok(traits)
}
#[derive(Serialize, Deserialize)]
pub struct Install(pub Traits);
impl Request for Install {
    const KEY: &'static str = "traits.install";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Set {
    pub person: PersonId,
    pub profile: BTreeMap<String, i32>,
}
impl Request for Set {
    const KEY: &'static str = "traits.set";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("traits", 1)?;
    b.resource::<Traits>()?;
    b.request::<Install>(|tx, r| {
        r.0.validate()?;
        require(
            tx.get::<Traits>()?.definitions.is_empty(),
            "trait definitions are frozen for this game",
        )?;
        *tx.edit::<Traits>()? = r.0;
        Ok(())
    })?;
    b.request::<Set>(|tx, r| {
        tx.get::<Traits>()?.validate_profile(&r.profile)?;
        tx.edit::<Traits>()?.people.insert(r.person, r.profile);
        Ok(())
    })
}
