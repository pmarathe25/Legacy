//! Stage independent personal hunger, digestion and nutrient reserves.
pub mod meal;
mod model;
pub mod occupation;
pub mod provision;
pub mod travel;
use game_lot_diet::Effects;
use game_nutrition::{Nutrition, Person as NutritionPerson};
use game_production_model::FoodEffect;
pub use model::{HungerRate, Immortal, Person, RecoveryRate, Rules, StarvationRate};
use rayon::prelude::*;

/// Find the first personal hunger threshold without serially scanning people.
/// The caller decides what the threshold means and when to publish effects.
pub fn next_hunger_threshold(
    people: &[Person],
    threshold: u32,
    rate: u32,
    quantum_millis: u64,
) -> Result<Option<u128>, &'static str> {
    if quantum_millis == 0 {
        return Err("invalid fractional hunger batch");
    }
    people
        .par_iter()
        .map(|person| person.hunger_threshold_after(threshold, rate, quantum_millis))
        .try_reduce(
            || None,
            |left, right| {
                Ok(match (left, right) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) | (None, Some(a)) => Some(a),
                    (None, None) => None,
                })
            },
        )
}

/// Earliest fatal starvation event, reduced independently over people.
pub fn next_starvation_death(
    people: &[Person],
    rules: &Rules,
    boundary_millis: u64,
) -> Result<Option<u128>, &'static str> {
    let Some(damage) = rules.starvation_rate else {
        return Ok(None);
    };
    let hunger_units = rules
        .hunger_rate
        .map_or(rules.hunger_per_tick, |rate| rate.units);
    let hunger_period = rules
        .hunger_rate
        .map_or(boundary_millis, |rate| rate.per_millis);
    people
        .par_iter()
        .map(|person| {
            person.starvation_death_after(
                rules.starvation_threshold,
                hunger_units,
                hunger_period,
                damage,
            )
        })
        .try_reduce(
            || None,
            |left, right| {
                Ok(match (left, right) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) | (None, Some(a)) => Some(a),
                    (None, None) => None,
                })
            },
        )
}

/// Earliest recovery of an incapacitated immortal.
pub fn next_recovery(people: &[Person]) -> Result<Option<u128>, &'static str> {
    people.par_iter().map(Person::recovery_after).try_reduce(
        || None,
        |left, right| {
            Ok(match (left, right) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            })
        },
    )
}

#[derive(Default)]
pub struct PhysiologyDeadlines {
    pub hunger: Option<u128>,
    pub fatal_starvation: Option<u128>,
    pub recovery: Option<u128>,
}

/// Reduce the actor-owned clocks in a single pass over the same opening bodies.
pub fn next_physiology(
    people: &[Person],
    rules: &Rules,
    hunger_threshold: Option<u32>,
    boundary_millis: u64,
) -> Result<PhysiologyDeadlines, &'static str> {
    let units = rules
        .hunger_rate
        .map_or(rules.hunger_per_tick, |rate| rate.units);
    let period = rules
        .hunger_rate
        .map_or(boundary_millis, |rate| rate.per_millis);
    if hunger_threshold.is_some() && period == 0 {
        return Err("invalid fractional hunger batch");
    }
    let earliest = |a: Option<u128>, b: Option<u128>| a.into_iter().chain(b).min();
    people
        .par_iter()
        .map(|person| {
            Ok(PhysiologyDeadlines {
                hunger: hunger_threshold
                    .map(|threshold| person.hunger_threshold_after(threshold, units, period))
                    .transpose()?
                    .flatten(),
                fatal_starvation: rules
                    .starvation_rate
                    .map(|damage| {
                        person.starvation_death_after(
                            rules.starvation_threshold,
                            units,
                            period,
                            damage,
                        )
                    })
                    .transpose()?
                    .flatten(),
                recovery: person.recovery_after()?,
            })
        })
        .try_reduce(PhysiologyDeadlines::default, |a, b| {
            Ok(PhysiologyDeadlines {
                hunger: earliest(a.hunger, b.hunger),
                fatal_starvation: earliest(a.fatal_starvation, b.fatal_starvation),
                recovery: earliest(a.recovery, b.recovery),
            })
        })
}

/// Earliest change to a stored whole hunger or health value. Strictly before
/// this time, a caller that only observes those values can keep fractional
/// elapsed credit in one place until it needs an exact body.
pub fn next_whole_vital_change(
    people: &[Person],
    rules: &Rules,
    boundary_millis: u64,
) -> Result<Option<u128>, &'static str> {
    let units = rules
        .hunger_rate
        .map_or(rules.hunger_per_tick, |rate| rate.units);
    let period = rules
        .hunger_rate
        .map_or(boundary_millis, |rate| rate.per_millis);
    people
        .par_iter()
        .map(|person| {
            let first_hunger = match person.hunger.checked_add(1) {
                Some(threshold) => person.hunger_threshold_after(threshold, units, period)?,
                None => None,
            };
            let first_damage = match rules.starvation_rate {
                Some(rate) if rate.units > 0 => {
                    let credit = rate
                        .per_millis
                        .checked_sub(person.starvation_remainder)
                        .filter(|&credit| credit > 0)
                        .ok_or("invalid fractional starvation damage")?;
                    person
                        .hunger_threshold_after(rules.starvation_threshold, units, period)?
                        .map(|crossing| {
                            crossing + u128::from(credit).div_ceil(u128::from(rate.units))
                        })
                }
                _ => None,
            };
            let first_healing = if let Some(immortal) = &person.immortal
                && let Some(rate) = immortal.recovery_rate
                && rate.units > 0
                && person.health < rules.max_health
            {
                let credit = rate
                    .per_millis
                    .checked_sub(immortal.recovery_remainder)
                    .filter(|&credit| credit > 0)
                    .ok_or("invalid fractional recovery")?;
                Some(u128::from(credit).div_ceil(u128::from(rate.units)))
            } else {
                None
            };
            Ok([first_hunger, first_damage, first_healing]
                .into_iter()
                .flatten()
                .min())
        })
        .try_reduce(
            || None,
            |left, right| Ok(left.into_iter().chain(right).min()),
        )
}

/// Stage hunger and starvation from the same opening state.
pub fn progress_vitals(
    people: &mut [Person],
    rules: &Rules,
    elapsed_millis: u64,
    boundary_millis: u64,
) -> Result<(), &'static str> {
    let next = stage_vitals(people, rules, elapsed_millis, boundary_millis)?;
    people
        .par_iter_mut()
        .zip(next)
        .for_each(|(person, next)| *person = next);
    Ok(())
}

/// Prepare an exact elapsed physiology batch without mutating the opening bodies.
pub fn stage_vitals(
    people: &[Person],
    rules: &Rules,
    elapsed_millis: u64,
    boundary_millis: u64,
) -> Result<Vec<Person>, &'static str> {
    people
        .par_iter()
        .map(|person| project_vitals(person, rules, elapsed_millis, boundary_millis))
        .collect()
}

/// Project one body through a continuous span with no intervening actions.
pub fn project_vitals(
    person: &Person,
    rules: &Rules,
    elapsed_millis: u64,
    boundary_millis: u64,
) -> Result<Person, &'static str> {
    let hunger_units = rules
        .hunger_rate
        .map_or(rules.hunger_per_tick, |rate| rate.units);
    let hunger_period = rules
        .hunger_rate
        .map_or(boundary_millis, |rate| rate.per_millis);
    let hunger = person.hunger_at(hunger_units, elapsed_millis, hunger_period)?;
    let mut result = person.clone();
    if let Some(damage) = rules.starvation_rate {
        result.advance_starvation(
            rules.starvation_threshold,
            hunger_units,
            hunger_period,
            damage,
            elapsed_millis,
        )?;
    } else if result.starvation_remainder != 0 {
        return Err("starvation remainder has no rate");
    }
    result.advance_recovery(elapsed_millis, rules.max_health)?;
    (result.hunger, result.hunger_remainder) = hunger;
    Ok(result)
}

/// People whose fatal starvation deadline falls within the given duration.
pub fn starvation_deaths_within(
    people: &[Person],
    rules: &Rules,
    boundary_millis: u64,
    elapsed_millis: u64,
) -> Result<Vec<u64>, &'static str> {
    let Some(damage) = rules.starvation_rate else {
        return Ok(Vec::new());
    };
    let hunger_units = rules
        .hunger_rate
        .map_or(rules.hunger_per_tick, |rate| rate.units);
    let hunger_period = rules
        .hunger_rate
        .map_or(boundary_millis, |rate| rate.per_millis);
    let rows: Vec<_> = people
        .par_iter()
        .map(|person| {
            person
                .starvation_death_after(
                    rules.starvation_threshold,
                    hunger_units,
                    hunger_period,
                    damage,
                )
                .map(|after| {
                    after
                        .filter(|time| *time <= u128::from(elapsed_millis))
                        .map(|_| person.id)
                })
        })
        .collect::<Result<_, _>>()?;
    Ok(rows.into_par_iter().flatten().collect())
}

/// Progress individual hunger without applying meals, damage or decisions.
/// The enclosing coordinator owns world time and draft invalidation.
pub fn progress_hunger(
    people: &mut [Person],
    rate: u32,
    elapsed_millis: u64,
    quantum_millis: u64,
) -> Result<(), &'static str> {
    if quantum_millis == 0
        || people
            .par_iter()
            .any(|p| !p.valid_hunger_progress(quantum_millis))
    {
        return Err("invalid fractional hunger batch");
    }
    people.par_iter_mut().for_each(|person| {
        person
            .advance_hunger(rate, elapsed_millis, quantum_millis)
            .expect("validated fractional hunger");
    });
    Ok(())
}

pub struct Draft {
    pub people: Vec<Person>,
    pub nutrition: Option<Nutrition>,
    pub pending: Vec<FoodEffect>,
}

/// Apply authored food physiology to a living person.
pub fn apply_food_health(person: &mut Person, health_effect: i32, max_health: u32) {
    if person.health == 0 {
        return;
    }
    if health_effect < 0 {
        person.health = person.health.saturating_sub(health_effect.unsigned_abs());
    } else {
        person.health = person
            .health
            .saturating_add(health_effect as u32)
            .min(max_health);
    }
}
pub fn prepare(
    people: &[Person],
    rules: &Rules,
    nutrition: Option<&Nutrition>,
    pending: &[FoodEffect],
    effects: Effects,
    tick: u64,
) -> Draft {
    prepare_inner(people, rules, nutrition, pending, effects, tick, true)
}

/// Apply boundary food, recovery and damage after hunger has already been
/// progressed by the coordinator. This does not select meals or decisions.
pub fn settle_elapsed(
    people: &[Person],
    rules: &Rules,
    nutrition: Option<&Nutrition>,
    pending: &[FoodEffect],
    effects: Effects,
    tick: u64,
) -> Draft {
    prepare_inner(people, rules, nutrition, pending, effects, tick, false)
}

fn prepare_inner(
    people: &[Person],
    rules: &Rules,
    nutrition: Option<&Nutrition>,
    pending: &[FoodEffect],
    effects: Effects,
    tick: u64,
    add_boundary_hunger: bool,
) -> Draft {
    let mut due: Vec<_> = pending
        .par_iter()
        .filter(|row| row.at_millis.is_none() && row.at <= tick)
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
            if let Some(immortal) = &person.immortal
                && (add_boundary_hunger || immortal.recovery_rate.is_none())
            {
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
                if add_boundary_hunger && person.immortal.is_none() {
                    person.hunger = person.hunger.saturating_add(rules.hunger_per_tick);
                    if person.hunger == u32::MAX {
                        person.hunger_remainder = 0;
                    }
                }
                if let Ok(index) = food.binary_search_by_key(&person.id, |row| row.0) {
                    if food[index].1 > person.hunger {
                        person.hunger_remainder = 0;
                    }
                    person.hunger = person.hunger.saturating_sub(food[index].1);
                }
                let first = due.partition_point(|row| row.person < person.id);
                let last = due.partition_point(|row| row.person <= person.id);
                for effect in &due[first..last] {
                    if person.health == 0 {
                        break;
                    }
                    apply_food_health(&mut person, effect.health, rules.max_health);
                    if let (Some(values), Some(nutrition_rules)) = (&mut values, nutrition_rules) {
                        for (index, value) in values.iter_mut().enumerate() {
                            *value = value
                                .saturating_add(effect.nutrients[index])
                                .min(nutrition_rules.capacity[index]);
                        }
                    }
                }
                if person.immortal.is_none()
                    && person.hunger >= rules.starvation_threshold
                    && (add_boundary_hunger || rules.starvation_rate.is_none())
                {
                    person.health = person.health.saturating_sub(rules.starvation_damage);
                }
                if let (Some(values), Some(nutrition_rules)) = (&mut values, nutrition_rules)
                    && person.immortal.is_none()
                    && nutrition_rules.interval_millis.is_none()
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
        .filter(|row| row.at_millis.is_some() || row.at > tick)
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
            if let Some(walking) = self
                .local_transport
                .as_ref()
                .and_then(|owner| owner.walking.as_ref())
                && departures.par_iter().any(|request| {
                    self.people
                        .binary_search_by_key(&request.person, |p| p.id)
                        .ok()
                        .and_then(|i| self.people[i].region)
                        .is_some_and(|region| {
                            !walking.present(
                                request.person,
                                region,
                                game_economy_local_transport::Endpoint::Stock,
                            )
                        })
                })
            {
                return Err("regional departure requires the local stock location".into());
            }
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
            if let Some(walking) = self
                .local_transport
                .as_mut()
                .and_then(|owner| owner.walking.as_mut())
            {
                let mut departed: Vec<_> = outcomes
                    .par_iter()
                    .filter_map(|outcome| outcome.departed.then_some(outcome.person))
                    .collect();
                departed.par_sort_unstable();
                if !departed.is_empty() {
                    walking.positions = std::mem::take(&mut walking.positions)
                        .into_par_iter()
                        .filter(|position| departed.binary_search(&position.person).is_err())
                        .collect();
                }
            }
            self.epoch = epoch;
            Ok(outcomes)
        }
    };
}
