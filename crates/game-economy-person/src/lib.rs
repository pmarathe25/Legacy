//! Stage independent personal hunger, digestion and nutrient reserves.
pub mod meal;
mod model;
pub mod occupation;
pub mod provision;
pub mod travel;
use game_lot_diet::Effects;
use game_nutrition::{Nutrition, Person as NutritionPerson};
use game_production_model::FoodEffect;
pub use model::{Immortal, Person, Rules};
use rayon::prelude::*;

pub struct Draft {
    pub people: Vec<Person>,
    pub nutrition: Option<Nutrition>,
    pub pending: Vec<FoodEffect>,
}
pub fn prepare(
    people: &[Person],
    rules: &Rules,
    nutrition: Option<&Nutrition>,
    pending: &[FoodEffect],
    effects: Effects,
    tick: u64,
) -> Draft {
    let mut due: Vec<_> = pending
        .par_iter()
        .filter(|row| row.at <= tick)
        .copied()
        .collect();
    due.par_sort_unstable_by_key(|row| (row.person, row.at, row.health, row.nutrients));
    let reserves = nutrition.map(|owner| {
        let mut rows: Vec<_> = owner.people.par_iter().collect();
        rows.par_sort_unstable_by_key(|row| row.person);
        rows
    });
    let food = &effects.hunger_relief;
    let rows: Vec<_> = people
        .par_iter()
        .map(|person| {
            let mut person = person.clone();
            if let Some(immortal) = &person.immortal {
                person.health = person
                    .health
                    .saturating_add(immortal.recovery_per_tick)
                    .min(rules.max_health);
            }
            let nutrition_rules = nutrition.map(|owner| &owner.rules);
            let mut values = nutrition_rules.map(|rules| {
                reserves
                    .as_ref()
                    .and_then(|rows| {
                        rows.binary_search_by_key(&person.id, |row| row.person)
                            .ok()
                            .map(|index| rows[index].reserves)
                    })
                    .unwrap_or(rules.initial)
            });
            if person.health > 0 {
                if person.immortal.is_none() {
                    person.hunger = person.hunger.saturating_add(rules.hunger_per_tick);
                }
                if let Ok(index) = food.binary_search_by_key(&person.id, |row| row.0) {
                    person.hunger = person.hunger.saturating_sub(food[index].1);
                }
                let first = due.partition_point(|row| row.person < person.id);
                let last = due.partition_point(|row| row.person <= person.id);
                for effect in &due[first..last] {
                    if person.health == 0 {
                        break;
                    }
                    if effect.health < 0 {
                        person.health = person.health.saturating_sub(effect.health.unsigned_abs());
                    } else {
                        person.health = person
                            .health
                            .saturating_add(effect.health as u32)
                            .min(rules.max_health);
                    }
                    if let (Some(values), Some(nutrition_rules)) = (&mut values, nutrition_rules) {
                        for (index, value) in values.iter_mut().enumerate() {
                            *value = value
                                .saturating_add(effect.nutrients[index])
                                .min(nutrition_rules.capacity[index]);
                        }
                    }
                }
                if person.immortal.is_none() && person.hunger >= rules.starvation_threshold {
                    person.health = person.health.saturating_sub(rules.starvation_damage);
                }
                if let (Some(values), Some(nutrition_rules)) = (&mut values, nutrition_rules)
                    && person.immortal.is_none()
                    && tick.is_multiple_of(nutrition_rules.interval)
                {
                    for (index, value) in values.iter_mut().enumerate() {
                        *value = value.saturating_sub(nutrition_rules.metabolism[index]);
                    }
                    if values
                        .iter()
                        .zip(nutrition_rules.minimum)
                        .any(|(&value, minimum)| value < minimum)
                    {
                        person.health = person
                            .health
                            .saturating_sub(nutrition_rules.deficiency_damage);
                    }
                }
            }
            (person, values)
        })
        .collect();
    let people = rows.par_iter().map(|row| row.0.clone()).collect();
    let nutrition = nutrition.map(|owner| Nutrition {
        rules: owner.rules.clone(),
        people: rows
            .into_par_iter()
            .map(|(person, values)| NutritionPerson {
                person: person.id,
                reserves: values.expect("enabled nutrition"),
            })
            .collect(),
    });
    let mut pending: Vec<_> = pending
        .par_iter()
        .filter(|row| row.at > tick)
        .copied()
        .chain(effects.pending.into_par_iter())
        .collect();
    pending.par_sort_unstable_by_key(|row| (row.person, row.at));
    Draft {
        people,
        nutrition,
        pending,
    }
}

#[macro_export]
macro_rules! work_choice_method {
    () => {
        pub fn set_work_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
            let index = self
                .people
                .binary_search_by_key(&person, |row| row.id)
                .map_err(|_| "unknown person")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.people[index].willing_to_work = willing;
            self.epoch = epoch;
            Ok(())
        }
    };
}
#[macro_export]
macro_rules! departure_method {
    () => {
        pub fn depart(&mut self, departures: &[Departure]) -> Result<(), String> {
            self.attempt_departures(departures).map(|_| ())
        }
        /// Attempt local departures and report physical route closures per person.
        pub fn attempt_departures(
            &mut self,
            departures: &[Departure],
        ) -> Result<Vec<$crate::travel::DepartureOutcome>, String> {
            if departures.iter().any(|request| {
                self.confinement
                    .as_ref()
                    .is_some_and(|owner| owner.held(request.person))
            }) {
                return Err("person is held behind a physical barrier".into());
            }
            if self.combat.as_ref().is_some_and(|combat| {
                departures
                    .par_iter()
                    .any(|request| combat.actors().binary_search(&request.person).is_ok())
            }) {
                return Err("a combatant cannot depart during a fight".into());
            }
            if self
                .combat
                .as_ref()
                .and_then(|combat| combat.state.injuries.as_ref())
                .is_some_and(|injuries| {
                    departures
                        .par_iter()
                        .any(|request| injuries.immobilized(request.person))
                })
            {
                return Err("a wound prevents departure".into());
            }
            let working = $crate::travel::working(self.physical.work());
            let (travel, people, outcomes) = self
                .travel
                .as_ref()
                .ok_or("travel is disabled")?
                .attempt(&self.people, departures, &working, self.tick())?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            let mut restraints = self.animal_restraints.clone();
            if let (Some(owner), Some(fauna)) = (&mut restraints, &self.fauna) {
                let available = |person| {
                    self.confinement
                        .as_ref()
                        .is_none_or(|owner| !owner.held(person))
                };
                owner.reconcile(&people, fauna, &available, self.tick());
            }
            self.travel = Some(travel);
            self.people = people;
            self.animal_restraints = restraints;
            self.epoch = epoch;
            Ok(outcomes)
        }
    };
}
