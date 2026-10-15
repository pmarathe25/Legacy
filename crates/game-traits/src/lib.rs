//! Trait names and their numeric effects are data, not Rust variants.
use game_content::{Document, Stamp, Store};
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(from = "SavedTraits")]
pub struct Traits {
    pub definitions: Records<String, Trait>,
    pub people: Records<PersonId, BTreeMap<String, i32>>,
    pub stamps: Vec<Stamp>,
    #[serde(skip)]
    compiled: Table<PersonId, Weights>,
}
#[derive(Deserialize)]
struct SavedTraits {
    definitions: Records<String, Trait>,
    people: Records<PersonId, BTreeMap<String, i32>>,
    stamps: Vec<Stamp>,
}
impl From<SavedTraits> for Traits {
    fn from(saved: SavedTraits) -> Self {
        let mut traits = Self {
            definitions: saved.definitions,
            people: saved.people,
            stamps: saved.stamps,
            compiled: Table::default(),
        };
        traits.rebuild();
        traits
    }
}
impl PartialEq for Traits {
    fn eq(&self, other: &Self) -> bool {
        self.definitions == other.definitions
            && self.people == other.people
            && self.stamps == other.stamps
    }
}
impl Eq for Traits {}
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
        for (person, profile) in self.people.changed(&previous.people) {
            self.validate_profile(profile)?;
            require(
                self.compiled
                    .get(person)
                    .is_none_or(|weights| *weights == self.compile(profile)),
                "stale compiled trait weights",
            )?;
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
        require(
            self.compiled.iter().all(|(person, weights)| {
                self.people
                    .get(person)
                    .is_some_and(|profile| *weights == self.compile(profile))
            }),
            "invalid compiled trait weights",
        )
    }
}
const TERMS: [&str; 5] = ["need", "goal", "value", "effort", "risk"];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weights([i64; 5]);
impl Weights {
    pub fn get(&self, term: &str) -> Option<&i64> {
        TERMS.iter().position(|t| *t == term).map(|i| &self.0[i])
    }
}
impl std::ops::Index<&str> for Weights {
    type Output = i64;
    fn index(&self, term: &str) -> &i64 {
        self.get(term).expect("known utility term")
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
    pub fn weights(&self, person: PersonId) -> Weights {
        self.compiled.get(&person).copied().unwrap_or_else(|| {
            self.people
                .get(&person)
                .map_or(Weights([100; 5]), |profile| self.compile(profile))
        })
    }
    fn rebuild(&mut self) {
        self.compiled = self
            .people
            .iter()
            .map(|(person, profile)| (*person, self.compile(profile)))
            .collect();
    }
    fn compile(&self, profile: &BTreeMap<String, i32>) -> Weights {
        let mut weights = Weights([100; 5]);
        for (key, value) in profile {
            let Some(t) = self.definitions.get(key) else {
                continue;
            };
            for (term, coefficient) in &t.weights {
                let delta =
                    (i64::from(*value) - i64::from(t.default)) * i64::from(*coefficient) / 100;
                let Some(i) = TERMS.iter().position(|t| *t == term) else {
                    continue;
                };
                let w = &mut weights.0[i];
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
        compiled: Table::default(),
    };
    traits.validate()?;
    Ok(traits)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Install(pub Traits);
impl Request for Install {
    const KEY: &'static str = "traits.install";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
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
    b.request::<Install>(|tx, mut r| {
        r.0.rebuild();
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
        let weights = tx.get::<Traits>()?.compile(&r.profile);
        let state = tx.edit::<Traits>()?;
        state.people.insert(r.person, r.profile);
        state.compiled.insert(r.person, weights);
        Ok(())
    })
}
