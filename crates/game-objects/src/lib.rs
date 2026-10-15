//! Object definitions are authored data. No enum enumerates permissible objects.
use game_content::{Document, Stamp, Store};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Food {
    pub portion: u64,
    pub relief: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub id: GoodId,
    pub name: String,
    pub mass: u64,
    #[serde(default)]
    pub tags: BTreeSet<String>,
    #[serde(default)]
    pub food: Option<Food>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    pub objects: BTreeMap<String, Object>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "SavedObjects")]
pub struct Objects {
    pub items: BTreeMap<String, Object>,
    pub stamps: Vec<Stamp>,
    #[serde(skip)]
    ids: BTreeMap<GoodId, String>,
}
impl Resource for Objects {
    const KEY: &'static str = "objects.catalog";
    fn validate(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        for (key, o) in &self.items {
            require(
                !key.is_empty()
                    && !o.name.is_empty()
                    && o.id.0 > 0
                    && ids.insert(o.id)
                    && o.mass > 0,
                "invalid or duplicate object",
            )?;
            if let Some(food) = &o.food {
                require(food.portion > 0 && food.relief > 0, "invalid edible object")?;
            }
        }
        require(
            self.ids.len() == self.items.len()
                && self
                    .items
                    .iter()
                    .all(|(key, o)| self.ids.get(&o.id) == Some(key)),
            "invalid object index",
        )
    }
}
#[derive(Deserialize)]
struct SavedObjects {
    items: BTreeMap<String, Object>,
    stamps: Vec<Stamp>,
}
impl From<SavedObjects> for Objects {
    fn from(saved: SavedObjects) -> Self {
        let ids = saved
            .items
            .iter()
            .map(|(key, object)| (object.id, key.clone()))
            .collect();
        Self {
            items: saved.items,
            stamps: saved.stamps,
            ids,
        }
    }
}
impl Objects {
    pub fn by_id(&self, id: GoodId) -> Result<&Object> {
        self.ids
            .get(&id)
            .and_then(|key| self.items.get(key))
            .ok_or_else(|| Error::InvalidConfiguration(format!("unknown object ID {}", id.0)))
    }
    pub fn resolve(&self, key: &str) -> Result<GoodId> {
        self.items
            .get(key)
            .map(|o| o.id)
            .ok_or_else(|| Error::InvalidConfiguration(format!("unknown object {key}")))
    }
    pub fn mass(&self, id: GoodId, quantity: u64) -> Result<u64> {
        self.by_id(id)?
            .mass
            .checked_mul(quantity)
            .ok_or(Error::Overflow)
    }
    pub fn meal<'a>(
        &self,
        stock: impl IntoIterator<Item = (&'a GoodId, &'a u64)>,
    ) -> Option<(GoodId, Food)> {
        stock.into_iter().find_map(|(id, q)| {
            self.by_id(*id)
                .ok()?
                .food
                .as_ref()
                .filter(|food| *q >= food.portion)
                .map(|food| (*id, food.clone()))
        })
    }
}
pub fn load(store: &mut Store) -> Result<Objects> {
    let mut items = BTreeMap::new();
    let mut stamps = vec![];
    for path in store.scan("objects")? {
        let doc: Document<Definitions> = store.load(&path)?;
        stamps.push(doc.stamp()?);
        for (key, object) in doc.data.objects {
            require(
                items.insert(key, object).is_none(),
                "duplicate object key across documents",
            )?;
        }
    }
    let catalog = Objects::from(SavedObjects { items, stamps });
    catalog.validate()?;
    require(!catalog.items.is_empty(), "no object definitions")?;
    Ok(catalog)
}
pub fn standard() -> Result<Objects> {
    load(&mut Store::standard())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Install(pub Objects);
impl Request for Install {
    const KEY: &'static str = "objects.install";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("objects", 1)?;
    b.resource::<Objects>()?;
    b.request::<Install>(|tx, r| {
        r.0.validate()?;
        require(
            tx.get::<Objects>()?.items.is_empty(),
            "object definitions are frozen for this game",
        )?;
        *tx.edit::<Objects>()? = r.0;
        Ok(())
    })
}
