//! Individual nutrient reserves, with no shared ownership or institutional rules.
pub use game_production_model::FoodEffect;
pub use game_production_model::NUTRIENT_NAMES as NAMES;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub type Nutrients = [u32; 6];

pub fn amounts(source: &BTreeMap<String, u32>) -> Nutrients {
    NAMES.map(|name| source.get(name).copied().unwrap_or(0))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Apply metabolism and deficiency damage on multiples of this many ticks.
    #[serde(default)]
    pub interval: u64,
    /// Apply metabolism at multiples of this duration from the simulation epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_millis: Option<u64>,
    /// Time between actual consumption and its separate health effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_effect_delay_millis: Option<u64>,
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
        let invalid_cadence = match self.rules.interval_millis {
            Some(millis) => millis == 0 || self.rules.interval != 0,
            None => self.rules.interval == 0,
        };
        if invalid_cadence
            || self
                .rules
                .health_effect_delay_millis
                .is_some_and(|delay| delay == 0 || self.rules.interval_millis.is_none())
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

    /// Time until the next metabolism event. An event at the current instant
    /// has already been applied by the progress operation that reached it.
    pub fn next_metabolism(&self, millis: u128) -> Option<u64> {
        let period = self.rules.interval_millis.filter(|&period| period != 0)?;
        Some(period - (millis % u128::from(period)) as u64)
    }

    /// Stage a physical metabolism event. The host supplies sorted identities
    /// and whether they metabolized over the preceding interval.
    pub fn metabolize(&self, people: &[(u64, bool)]) -> (Self, Vec<u64>) {
        let mut old: Vec<_> = self.people.par_iter().collect();
        old.par_sort_unstable_by_key(|row| row.person);
        let rows: Vec<_> = people
            .par_iter()
            .map(|&(person, living)| {
                let mut reserves = self.reserves_for(&old, person);
                let mut deficient = false;
                if living {
                    for (reserve, metabolism) in reserves.iter_mut().zip(self.rules.metabolism) {
                        *reserve = reserve.saturating_sub(metabolism);
                    }
                    deficient = self.rules.deficiency_damage > 0
                        && reserves
                            .iter()
                            .zip(self.rules.minimum)
                            .any(|(&reserve, min)| reserve < min);
                }
                (Person { person, reserves }, deficient)
            })
            .collect();
        let deficient = rows
            .par_iter()
            .filter_map(|(row, deficient)| deficient.then_some(row.person))
            .collect();
        let people = rows.into_par_iter().map(|(row, _)| row).collect();
        (
            Self {
                rules: self.rules.clone(),
                people,
            },
            deficient,
        )
    }

    /// Add the nutrients carried by already validated consumption receipts.
    /// Contributions from separate meals accumulate up to physical capacity.
    pub fn ingest(&self, intake: &[FoodEffect]) -> Self {
        let mut old: Vec<_> = self.people.par_iter().collect();
        old.par_sort_unstable_by_key(|row| row.person);
        let mut intake: Vec<_> = intake
            .par_iter()
            .filter(|row| row.nutrients != [0; 6])
            .collect();
        intake.par_sort_unstable_by_key(|row| row.person);
        let mut updated: Vec<_> = intake
            .par_chunk_by(|a, b| a.person == b.person)
            .map(|meals| {
                let person = meals[0].person;
                let mut reserves = self.reserves_for(&old, person);
                for meal in meals {
                    for (index, reserve) in reserves.iter_mut().enumerate() {
                        *reserve = reserve
                            .saturating_add(meal.nutrients[index])
                            .min(self.rules.capacity[index]);
                    }
                }
                Person { person, reserves }
            })
            .collect();
        let mut people: Vec<_> = old
            .into_par_iter()
            .filter(|row| {
                updated
                    .binary_search_by_key(&row.person, |updated| updated.person)
                    .is_err()
            })
            .cloned()
            .collect();
        people.append(&mut updated);
        people.par_sort_unstable_by_key(|row| row.person);
        Self {
            rules: self.rules.clone(),
            people,
        }
    }

    fn reserves_for(&self, old: &[&Person], person: u64) -> Nutrients {
        old.binary_search_by_key(&person, |row| row.person)
            .ok()
            .map_or(self.rules.initial, |index| old[index].reserves)
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
                    at_millis: None,
                    health: 0,
                    nutrients,
                });
        }
        Ok(())
    }};
}
