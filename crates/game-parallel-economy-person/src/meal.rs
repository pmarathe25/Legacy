//! Personal meal choices based on actual, unexpired carried goods.
use crate::{Person, Rules};
use game_parallel_lot_diet::{Meal, Plan};
use game_parallel_lots::{Place, View};
use game_parallel_production_model::Good;
use rayon::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct Choice {
    pub person: u64,
    pub good: u32,
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
    let plan = game_parallel_lot_diet::plan(&meals, goods)?;
    Ok((selected, plan))
}
