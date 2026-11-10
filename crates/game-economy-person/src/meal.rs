//! Personal meal choices based on actual, unexpired carried goods.
use crate::{Person, Rules};
use game_lot_diet::{Meal, Plan};
use game_lots::{Place, View};
use game_production_model::Good;
use rayon::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct Choice {
    pub person: u64,
    pub good: u32,
}

/// Select one carried meal per hungry, autonomous person at the current
/// instant. The host supplies people unavailable for another activity.
pub fn choose_now(
    people: &[Person],
    rules: &Rules,
    goods: &[Good],
    physical: &game_lot_runtime::Runtime,
    blocked: &[u64],
) -> Result<Vec<Meal>, String> {
    if people.par_windows(2).any(|rows| rows[0].id >= rows[1].id)
        || blocked.par_windows(2).any(|rows| rows[0] >= rows[1])
    {
        return Err("meal people and exclusions must be ordered and unique".into());
    }
    let edible: HashMap<_, _> = goods
        .iter()
        .filter_map(|good| good.food.as_ref().map(|food| (good.id, food.portion)))
        .collect();
    Ok(people
        .par_iter()
        .filter_map(|person| {
            if person.controlled
                || !person.seek_meals
                || person.health == 0
                || person.hunger < rules.meal_threshold
                || blocked.binary_search(&person.id).is_ok()
            {
                return None;
            }
            let holder = physical.custody(Place::Person(person.id))?;
            let good = holder
                .lots
                .chunk_by(|a, b| a.good == b.good)
                .find_map(|lots| {
                    let portion = edible.get(&lots[0].good)?;
                    let available: u128 = lots
                        .iter()
                        .filter(|lot| lot.expires_at.is_none_or(|at| at > physical.tick()))
                        .map(|lot| u128::from(lot.quantity))
                        .sum();
                    (*portion > 0 && available >= u128::from(*portion)).then_some(lots[0].good)
                })?;
            Some(Meal {
                action: person.id,
                person: person.id,
                good,
            })
        })
        .collect())
}

pub fn prepare(
    people: &[Person],
    rules: &Rules,
    goods: &[Good],
    view: &View,
    choice: Option<Choice>,
    blocked: &[u64],
) -> Result<(Vec<(u64, u32)>, Plan), String> {
    let edible: HashMap<_, _> = goods
        .iter()
        .filter_map(|good| good.food.as_ref().map(|food| (good.id, food)))
        .collect();
    let mut selected: Vec<_> = people
        .par_iter()
        .filter_map(|person| {
            if person.controlled
                || blocked.binary_search(&person.id).is_ok()
                || !person.seek_meals
                || person.health == 0
                || person.hunger.saturating_add(rules.hunger_per_tick) < rules.meal_threshold
            {
                return None;
            }
            let holding = view.container(Place::Person(person.id))?;
            let good = holding.goods.iter().find(|item| {
                edible
                    .get(&item.good)
                    .is_some_and(|food| item.quantity >= food.portion)
            })?;
            Some((person.id, good.good))
        })
        .collect();
    if let Some(choice) = choice {
        if people
            .binary_search_by_key(&choice.person, |p| p.id)
            .ok()
            .is_none_or(|i| !people[i].controlled || people[i].health == 0)
            || !edible.contains_key(&choice.good)
        {
            return Err("meal requires a living controlled person and edible good".into());
        }
        selected.push((choice.person, choice.good));
        selected.par_sort_unstable();
    }
    let meals: Vec<_> = selected
        .par_iter()
        .enumerate()
        .map(|(index, &(person, good))| Meal {
            action: index as u64 + 1,
            person,
            good,
        })
        .collect();
    let plan = game_lot_diet::plan(&meals, goods)?;
    Ok((selected, plan))
}
