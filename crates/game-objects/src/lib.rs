//! Object definitions are authored data. No enum enumerates permissible objects.
use game_content::{Document, Stamp, Store};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Food {
    pub portion: u64,
    pub relief: u64,
    #[serde(default)]
    pub nutrients: BTreeMap<String, u64>,
    #[serde(default)]
    pub effects: FoodEffects,
    #[serde(default)]
    pub shelf_days: Option<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FoodEffects {
    pub health: i32,
    pub mood: i32,
    pub alertness: i32,
    pub substance: Option<String>,
    pub dependence: u32,
    pub duration: SimDuration,
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
impl Object {
    pub fn shelf_days(&self) -> Option<u64> {
        let food = self.food.as_ref()?;
        Some(
            food.shelf_days
                .map(u64::from)
                .or_else(|| {
                    self.tags
                        .iter()
                        .find_map(|t| t.strip_prefix("shelf-days:")?.parse().ok())
                })
                .unwrap_or(30)
                .max(1),
        )
    }
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
    ids: rustc_hash::FxHashMap<GoodId, usize>,
    #[serde(skip)]
    indexed: Vec<Object>,
    #[serde(skip)]
    keys: rustc_hash::FxHashMap<String, GoodId>,
    #[serde(skip)]
    foods: Arc<BTreeMap<GoodId, Food>>,
    #[serde(skip)]
    masses: BTreeMap<GoodId, u64>,
    #[serde(skip)]
    food_ranks: BTreeMap<GoodId, u64>,
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
                require(
                    food.shelf_days.is_none_or(|d| d > 0)
                        && food.nutrients.keys().all(|s| !s.is_empty())
                        && (food.effects.dependence == 0
                            || (food
                                .effects
                                .substance
                                .as_ref()
                                .is_some_and(|s| !s.is_empty())
                                && food.effects.duration.0 > 0)),
                    "invalid food properties",
                )?;
            }
        }
        require(
            self.ids.len() == self.items.len()
                && self.indexed.len() == self.items.len()
                && self.keys.len() == self.items.len()
                && self.masses.len() == self.items.len()
                && self.foods.len() == self.items.values().filter(|o| o.food.is_some()).count()
                && self
                    .food_ranks
                    .iter()
                    .enumerate()
                    .all(|(i, (g, rank))| self.foods.contains_key(g) && *rank == i as u64)
                && self.food_ranks.len() == self.foods.len()
                && self.items.iter().all(|(key, o)| {
                    self.ids
                        .get(&o.id)
                        .is_some_and(|slot| self.indexed.get(*slot) == Some(o))
                        && self.keys.get(key) == Some(&o.id)
                        && self.masses.get(&o.id) == Some(&o.mass)
                        && self.foods.get(&o.id) == o.food.as_ref()
                }),
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
        let mut objects = Self {
            items: saved.items,
            stamps: saved.stamps,
            ..Default::default()
        };
        objects.rebuild();
        objects
    }
}
impl Objects {
    fn rebuild(&mut self) {
        self.indexed = self.items.values().cloned().collect();
        self.ids = self
            .indexed
            .iter()
            .enumerate()
            .map(|(slot, o)| (o.id, slot))
            .collect();
        self.keys = self
            .items
            .iter()
            .map(|(key, o)| (key.clone(), o.id))
            .collect();
        self.foods = Arc::new(
            self.items
                .values()
                .filter_map(|o| o.food.clone().map(|f| (o.id, f)))
                .collect(),
        );
        self.masses = self.items.values().map(|o| (o.id, o.mass)).collect();
        self.food_ranks = self
            .foods
            .keys()
            .enumerate()
            .map(|(i, good)| (*good, i as u64))
            .collect();
    }
    pub fn food_ranks(&self) -> &BTreeMap<GoodId, u64> {
        &self.food_ranks
    }
    pub fn food_rules(&self) -> &Arc<BTreeMap<GoodId, Food>> {
        &self.foods
    }
    pub fn by_id(&self, id: GoodId) -> Result<&Object> {
        self.ids
            .get(&id)
            .map(|slot| &self.indexed[*slot])
            .ok_or_else(|| Error::InvalidConfiguration(format!("unknown object ID {}", id.0)))
    }
    pub fn resolve(&self, key: &str) -> Result<GoodId> {
        self.keys
            .get(key)
            .copied()
            .ok_or_else(|| Error::InvalidConfiguration(format!("unknown object {key}")))
    }
    pub fn mass(&self, id: GoodId, quantity: u64) -> Result<u64> {
        self.masses
            .get(&id)
            .ok_or_else(|| Error::InvalidConfiguration(format!("unknown object ID {}", id.0)))?
            .checked_mul(quantity)
            .ok_or(Error::Overflow)
    }
    pub fn has_meal<'a>(&self, stock: impl IntoIterator<Item = (&'a GoodId, &'a u64)>) -> bool {
        stock
            .into_iter()
            .any(|(id, q)| self.foods.get(id).is_some_and(|f| *q >= f.portion))
    }
    pub fn meal<'a>(
        &self,
        stock: impl IntoIterator<Item = (&'a GoodId, &'a u64)>,
    ) -> Option<(GoodId, Food)> {
        stock.into_iter().find_map(|(id, q)| {
            if *q == 0 {
                return None;
            }
            self.foods
                .get(id)
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
    b.request::<Install>(|tx, mut r| {
        r.0.rebuild();
        r.0.validate()?;
        require(
            tx.get::<Objects>()?.items.is_empty(),
            "object definitions are frozen for this game",
        )?;
        *tx.edit::<Objects>()? = r.0;
        Ok(())
    })
}
