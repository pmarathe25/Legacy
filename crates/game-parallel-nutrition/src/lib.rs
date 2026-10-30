//! Individual nutrient reserves, with no shared ownership or institutional rules.
pub use game_parallel_production_model::FoodEffect;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const NAMES: [&str; 6] = ["energy", "protein", "fat", "fibre", "vitamins", "minerals"];
pub type Nutrients = [u32; 6];

pub fn amounts(source: &BTreeMap<String, u32>) -> Nutrients {
    NAMES.map(|name| source.get(name).copied().unwrap_or(0))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Apply metabolism and deficiency damage on multiples of this many ticks.
    pub interval: u64,
    pub capacity: Nutrients,
    pub initial: Nutrients,
    pub metabolism: Nutrients,
    pub minimum: Nutrients,
    pub deficiency_damage: u32,
    /// Nutrients in one meal from the abstract regional or carried food supply.
    pub scalar_meal: Nutrients,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub person: u64,
    pub reserves: Nutrients,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nutrition {
    pub rules: Rules,
    #[serde(default)]
    pub people: Vec<Person>,
}

impl Nutrition {
    pub fn validate(&self, people: &HashSet<u64>) -> Result<(), &'static str> {
        if self.rules.interval == 0
            || (0..6).any(|i| {
                self.rules.initial[i] > self.rules.capacity[i]
                    || self.rules.minimum[i] > self.rules.capacity[i]
            })
            || self.people.par_iter().any(|row| {
                !people.contains(&row.person)
                    || (0..6).any(|i| row.reserves[i] > self.rules.capacity[i])
            })
        {
            return Err("invalid nutrition state");
        }
        let mut ids: Vec<_> = self.people.par_iter().map(|row| row.person).collect();
        ids.par_sort_unstable();
        if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate nutrition person");
        }
        Ok(())
    }
}

/// Wrap a checked carried-food action with its delayed nutrient intake.
#[macro_export]
macro_rules! eat_scalar {
    ($world:expr, $person:expr) => {{
        let nutrients = $world
            .nutrition
            .as_ref()
            .map(|owner| owner.rules.scalar_meal)
            .unwrap_or([0; 6]);
        let at = if nutrients != [0; 6] {
            Some(
                $world
                    .tick
                    .checked_add(1)
                    .ok_or("food effect tick overflow")?,
            )
        } else {
            None
        };
        $world.cargo_context().eat_carried_food($person)?;
        if let Some(at) = at {
            $world
                .food_effects
                .get_or_insert_with(Default::default)
                .pending
                .push($crate::FoodEffect {
                    person: $person,
                    at,
                    health: 0,
                    nutrients,
                });
        }
        Ok(())
    }};
}
