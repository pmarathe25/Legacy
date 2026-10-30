//! Personal attempts to obtain a meal from local physical stock.
use crate::{Person, Rules};
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_production_model::Good;
use rayon::prelude::*;
use std::collections::HashMap;

pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
}

/// People select from stock visible at the next boundary. Competing attempts
/// still require the physical store's shared-resource arbitration.
pub fn prepare(
    people: &[Person],
    rules: &Rules,
    goods: &[Good],
    view: &View,
    busy: &[u64],
    action_base: u64,
    consider: impl Fn(&Person, u64, u32) -> Option<u8> + Sync,
) -> Result<Plan, String> {
    let edible: HashMap<_, _> = goods
        .iter()
        .filter_map(|row| row.food.as_ref().map(|food| (row.id, food.portion)))
        .collect();
    let mut stock: Vec<_> = view
        .containers
        .par_iter()
        .filter_map(|row| {
            let Place::Stock { region, good } = row.place else {
                return None;
            };
            let portion = *edible.get(&good)?;
            (row.quantity(good) >= portion).then_some((region, good, portion))
        })
        .collect();
    stock.par_sort_unstable();
    let selected: Vec<_> = people
        .par_iter()
        .filter_map(|person| {
            if person.controlled
                || !person.seek_meals
                || person.health == 0
                || person.hunger.saturating_add(rules.hunger_per_tick) < rules.meal_threshold
                || busy.binary_search(&person.id).is_ok()
            {
                return None;
            }
            let region = person.region?;
            let holding = view.container(Place::Person(person.id))?;
            let first = stock.partition_point(|row| row.0 < region);
            let last = stock.partition_point(|row| row.0 <= region);
            stock[first..last]
                .iter()
                .filter(|row| row.2 <= holding.capacity - holding.used)
                .filter_map(|row| consider(person, region, row.1).map(|rank| (rank, row)))
                .min_by_key(|(rank, row)| (*rank, row.1))
                .map(|(_, row)| (person.id, region, row.1, row.2))
        })
        .collect();
    let count = u64::try_from(selected.len()).map_err(|_| "food pickup count overflow")?;
    action_base
        .checked_add(count)
        .ok_or("food pickup identity overflow")?;
    let actions = selected
        .par_iter()
        .enumerate()
        .map(|(index, &(person, region, good, quantity))| {
            let id = action_base + index as u64;
            Transaction {
                id,
                movements: vec![Movement {
                    id,
                    from: Place::Stock { region, good },
                    to: Some(Place::Person(person)),
                    good,
                    quantity,
                }],
                creations: vec![],
            }
        })
        .collect();
    Ok(Plan {
        actors: selected.par_iter().map(|row| row.0).collect(),
        actions,
    })
}
