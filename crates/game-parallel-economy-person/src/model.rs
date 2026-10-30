use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: u64,
    pub region: Option<u64>,
    pub health: u32,
    pub hunger: u32,
    /// A personal preference, not an obligation.
    pub willing_to_work: bool,
    pub seek_meals: bool,
    #[serde(default)]
    pub controlled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub hunger_per_tick: u32,
    pub meal_threshold: u32,
    pub starvation_threshold: u32,
    pub starvation_damage: u32,
    pub max_health: u32,
    pub carrying_capacity: u64,
    pub max_work_hunger: u32,
}
