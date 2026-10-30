//! Serializable contracts for production state.
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Good {
    pub id: u32,
    pub name: String,
    /// Authored food properties, when this physical good can be eaten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub food: Option<FoodProfile>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodProfile {
    pub portion: u64,
    pub hunger_relief: u32,
    #[serde(default)]
    pub nutrients: BTreeMap<String, u32>,
    #[serde(default)]
    pub health_effect: i32,
    #[serde(default)]
    pub shelf_days: Option<u32>,
}
/// Delayed physiological effects of food already consumed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodEffects {
    #[serde(default)]
    pub pending: Vec<FoodEffect>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodEffect {
    pub person: u64,
    pub at: u64,
    pub health: i32,
    #[serde(default)]
    pub nutrients: [u32; 6],
}
impl FoodEffects {
    pub fn validate(
        &self,
        people: &std::collections::HashSet<u64>,
        now: u64,
    ) -> Result<(), &'static str> {
        if self.pending.iter().any(|row| {
            row.at <= now
                || (row.health == 0 && row.nutrients == [0; 6])
                || !people.contains(&row.person)
        }) {
            return Err("invalid pending food effect");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Amount {
    pub good: u32,
    pub quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: u32,
    pub name: String,
    pub ticks: u32,
    pub inputs: Vec<Amount>,
    pub outputs: Vec<Amount>,
}
/// Authored skill category associated with a physical recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeCategory {
    pub recipe: u32,
    pub category: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stock {
    pub good: u32,
    pub quantity: u64,
    pub capacity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub region: u64,
    pub stocks: Vec<Stock>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobPhase {
    Working,
    Waiting,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub remaining: u64,
    pub phase: JobPhase,
    pub reserved: SmallVec<[Amount; 1]>,
    pub outputs: SmallVec<[Amount; 1]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub job: u64,
    pub kind: EventKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Finished,
    Delivered,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    pub id: u64,
    pub worker: u64,
    pub region: u64,
    pub recipe: u32,
    pub units: u32,
}

pub fn valid_items(items: &[Amount], goods: &std::collections::HashSet<u32>) -> bool {
    !items.is_empty()
        && items.iter().enumerate().all(|(index, item)| {
            item.quantity > 0
                && goods.contains(&item.good)
                && !items[..index].iter().any(|other| other.good == item.good)
        })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodStoreBlueprint {
    pub id: u32,
    pub good: u32,
    pub quantity: u64,
    pub capacity: u32,
}
/// A person's revocable intention to construct stores from their own material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodStoreBuilder {
    pub person: u64,
    pub blueprint: u32,
    pub remaining: u32,
    pub interval: u64,
    #[serde(default)]
    pub object_to_taking: bool,
    #[serde(default)]
    pub use_after_build: bool,
    #[serde(default)]
    pub deposit_after_build: bool,
    #[serde(default = "enabled_builder")]
    pub enabled: bool,
}
fn enabled_builder() -> bool {
    true
}

/// A physical person row: ID, local region, health and hunger.
pub type PersonSnapshot = (u64, Option<u64>, u32, u32);

/// Locate an ID in a validated, sorted physical-person index.
/// Dense ID ranges use direct addressing; sparse IDs retain binary search.
pub fn person_index(people: &[PersonSnapshot], id: u64) -> Option<usize> {
    let (first, last) = (people.first()?.0, people.last()?.0);
    if last.checked_sub(first) == Some((people.len() - 1) as u64) {
        let index = usize::try_from(id.checked_sub(first)?).ok()?;
        (index < people.len()).then_some(index)
    } else {
        people.binary_search_by_key(&id, |person| person.0).ok()
    }
}
