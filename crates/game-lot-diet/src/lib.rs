//! Convert actual dated consumption into personal hunger and nutrition effects.
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction};
use game_nutrition::amounts;
use game_production_model::{FoodEffect, Good};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn validate_pending(
    pending: &[FoodEffect],
    people: &HashSet<u64>,
    tick: u64,
) -> Result<(), String> {
    if pending.par_iter().any(|row| {
        row.at <= tick
            || (row.health == 0 && row.nutrients == [0; 6])
            || !people.contains(&row.person)
    }) {
        return Err("invalid pending food effect".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub struct Meal {
    pub action: u64,
    pub person: u64,
    pub good: u32,
}
struct Record {
    meal: Meal,
    portion: u64,
    relief: u32,
    health: i32,
    nutrients: [u32; 6],
}
pub struct Plan {
    actions: Vec<Transaction>,
    records: Vec<Record>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Effects {
    pub hunger_relief: Vec<(u64, u32)>,
    pub pending: Vec<FoodEffect>,
}
impl Plan {
    pub fn actions(&self) -> &[Transaction] {
        &self.actions
    }
    /// Derive partial-interval effects from staged consumption, before custody
    /// commits. The caller supplies the boundary for delayed effects.
    pub fn interval_effects(
        &self,
        decisions: &[game_lots::TransactionOutcome],
        receipts: &[game_lots::Outcome],
        effect_tick: u64,
    ) -> Result<Effects, String> {
        if decisions.len() != self.records.len() || receipts.len() != self.records.len() {
            return Err("invalid interval meal receipts".into());
        }
        let evaluate = |record: &Record,
                        decision: &game_lots::TransactionOutcome,
                        receipt: &game_lots::Outcome| {
            if decision.accepted != receipt.accepted {
                return Err("inconsistent meal receipt".to_string());
            }
            if !decision.accepted {
                if !receipt.lots.is_empty() {
                    return Err("rejected meal has physical goods".into());
                }
                return Ok(None);
            }
            if receipt.lots.iter().any(|lot| lot.good != record.meal.good)
                || receipt
                    .lots
                    .iter()
                    .map(|lot| u128::from(lot.quantity))
                    .sum::<u128>()
                    != u128::from(record.portion)
            {
                return Err("invalid consumed food receipt".into());
            }
            let pending =
                (record.health != 0 || record.nutrients != [0; 6]).then_some(FoodEffect {
                    person: record.meal.person,
                    at: effect_tick,
                    health: record.health,
                    nutrients: record.nutrients,
                });
            Ok(Some(((record.meal.person, record.relief), pending)))
        };
        // Meal planning validates unique action IDs, so aligned receipts are
        // also unique and need no separate sort or lookup.
        let aligned = self
            .records
            .par_iter()
            .zip(decisions.par_iter())
            .zip(receipts.par_iter())
            .all(|((record, decision), receipt)| {
                record.meal.action == decision.id && decision.id == receipt.id
            });
        let rows: Vec<_> = if aligned {
            self.records
                .par_iter()
                .zip(decisions.par_iter())
                .zip(receipts.par_iter())
                .map(|((record, decision), receipt)| evaluate(record, decision, receipt))
                .collect::<Result<_, String>>()?
        } else {
            let mut decisions: Vec<_> = decisions.iter().collect();
            let mut receipts: Vec<_> = receipts.iter().collect();
            decisions.par_sort_unstable_by_key(|row| row.id);
            receipts.par_sort_unstable_by_key(|row| row.id);
            if decisions
                .par_windows(2)
                .any(|rows| rows[0].id == rows[1].id)
                || receipts.par_windows(2).any(|rows| rows[0].id == rows[1].id)
            {
                return Err("invalid interval meal receipts".into());
            }
            self.records
                .par_iter()
                .map(|record| {
                    let id = record.meal.action;
                    let decision = decisions
                        .binary_search_by_key(&id, |row| row.id)
                        .ok()
                        .map(|index| decisions[index])
                        .ok_or("missing meal decision")?;
                    let receipt = receipts
                        .binary_search_by_key(&id, |row| row.id)
                        .ok()
                        .map(|index| receipts[index])
                        .ok_or("missing meal receipt")?;
                    evaluate(record, decision, receipt)
                })
                .collect::<Result<_, String>>()?
        };
        let mut hunger_relief: Vec<_> = rows
            .par_iter()
            .filter_map(|row| row.as_ref().map(|row| row.0))
            .collect();
        hunger_relief.par_sort_unstable_by_key(|row| row.0);
        let mut pending: Vec<_> = rows
            .into_par_iter()
            .filter_map(|row| row.and_then(|row| row.1))
            .collect();
        pending.par_sort_unstable_by_key(|row| row.person);
        Ok(Effects {
            hunger_relief,
            pending,
        })
    }
    /// Derive effects only from exact accepted physical receipts, before commit.
    pub fn effects(&self, events: &Events) -> Result<Effects, String> {
        let mut decisions: Vec<_> = events.physical_actions.par_iter().collect();
        decisions.par_sort_unstable_by_key(|row| row.id);
        let mut receipts: Vec<_> = events.physical_movements.par_iter().collect();
        receipts.par_sort_unstable_by_key(|row| (row.action, row.movement.id));
        if decisions
            .par_windows(2)
            .any(|pair| pair[0].id == pair[1].id)
            || receipts.par_windows(2).any(|pair| {
                (pair[0].action, pair[0].movement.id) == (pair[1].action, pair[1].movement.id)
            })
        {
            return Err("duplicate physical meal receipt".into());
        }
        let results: Vec<_> = self
            .records
            .par_iter()
            .map(|record| {
                let id = record.meal.action;
                let decision = decisions
                    .binary_search_by_key(&id, |row| row.id)
                    .ok()
                    .map(|index| decisions[index])
                    .ok_or("missing meal decision")?;
                let receipt = receipts
                    .binary_search_by_key(&(id, id), |row| (row.action, row.movement.id))
                    .ok()
                    .map(|index| receipts[index])
                    .ok_or("missing meal receipt")?;
                if receipt.request
                    != (Movement {
                        item: None,
                        id,
                        from: Place::Person(record.meal.person),
                        to: None,
                        good: record.meal.good,
                        quantity: record.portion,
                    })
                {
                    return Err("meal action differs from receipt".into());
                }
                if decision.accepted != receipt.movement.accepted {
                    return Err("inconsistent meal receipt".to_string());
                }
                if !decision.accepted {
                    if !receipt.movement.lots.is_empty() {
                        return Err("rejected meal has physical goods".into());
                    }
                    return Ok(None);
                }
                if receipt
                    .movement
                    .lots
                    .iter()
                    .any(|lot| lot.good != record.meal.good)
                    || receipt
                        .movement
                        .lots
                        .iter()
                        .map(|lot| u128::from(lot.quantity))
                        .sum::<u128>()
                        != u128::from(record.portion)
                {
                    return Err("invalid consumed food receipt".to_string());
                }
                let pending = if record.health != 0 || record.nutrients != [0; 6] {
                    Some(FoodEffect {
                        person: record.meal.person,
                        at: events
                            .tick
                            .checked_add(1)
                            .ok_or("food effect tick overflow")?,
                        health: record.health,
                        nutrients: record.nutrients,
                    })
                } else {
                    None
                };
                Ok(Some(((record.meal.person, record.relief), pending)))
            })
            .collect::<Result<_, String>>()?;
        let mut hunger_relief: Vec<_> = results
            .par_iter()
            .filter_map(|row| row.as_ref().map(|row| row.0))
            .collect();
        hunger_relief.par_sort_unstable_by_key(|row| row.0);
        let mut pending: Vec<_> = results
            .into_par_iter()
            .filter_map(|row| row.and_then(|row| row.1))
            .collect();
        pending.par_sort_unstable_by_key(|row| row.person);
        Ok(Effects {
            hunger_relief,
            pending,
        })
    }
}
/// A person can choose at most one meal in this batch.
pub fn plan(meals: &[Meal], goods: &[Good]) -> Result<Plan, String> {
    let mut index = HashMap::new();
    for good in goods {
        if good.id == 0 || index.insert(good.id, good).is_some() {
            return Err("invalid food catalog".into());
        }
    }
    let mut people: Vec<_> = meals.par_iter().map(|row| row.person).collect();
    let mut actions: Vec<_> = meals.par_iter().map(|row| row.action).collect();
    people.par_sort_unstable();
    actions.par_sort_unstable();
    if people.first() == Some(&0)
        || actions.first() == Some(&0)
        || people.par_windows(2).any(|rows| rows[0] == rows[1])
        || actions.par_windows(2).any(|rows| rows[0] == rows[1])
    {
        return Err("duplicate or invalid meal actor".into());
    }
    let records: Vec<_> = meals
        .par_iter()
        .map(|meal| {
            let food = index
                .get(&meal.good)
                .and_then(|good| good.food.as_ref())
                .ok_or("unknown edible good")?;
            if food.portion == 0 {
                return Err("invalid food portion".to_string());
            }
            Ok(Record {
                meal: *meal,
                portion: food.portion,
                relief: food.hunger_relief,
                health: food.health_effect,
                nutrients: amounts(&food.nutrients),
            })
        })
        .collect::<Result<_, String>>()?;
    let actions = records
        .par_iter()
        .map(|record| Transaction {
            id: record.meal.action,
            movements: vec![Movement {
                item: None,
                id: record.meal.action,
                from: Place::Person(record.meal.person),
                to: None,
                good: record.meal.good,
                quantity: record.portion,
            }],
            creations: vec![],
        })
        .collect();
    Ok(Plan { actions, records })
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
