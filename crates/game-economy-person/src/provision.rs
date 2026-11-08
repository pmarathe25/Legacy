//! Personal attempts to obtain a meal from local physical custody.
use crate::{Person, Rules};
use game_lots::{Movement, Place, Transaction, View};
use game_production_model::Good;
use rayon::prelude::*;
use std::collections::HashMap;

pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
}

struct Sources {
    region: u64,
    good: u32,
    portion: u64,
    regional: bool,
    stores: std::ops::Range<usize>,
}

pub type LocalAccess<'a> = dyn Fn(u64, u64) -> Option<Place> + Sync + 'a;
pub type AccessView<'a> = (&'a View, &'a [(u64, u64)], Option<&'a LocalAccess<'a>>);

/// People select from stock visible at the next boundary. Competing attempts
/// still require the physical store's shared-resource arbitration.
pub fn prepare(
    people: &[Person],
    rules: &Rules,
    goods: &[Good],
    (view, stores, local_access): AccessView<'_>,
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
            (row.quantity(good) >= portion).then_some((region, good, 0u8, 0u64, portion))
        })
        .collect();
    let edible_index = &edible;
    stock.par_extend(stores.par_iter().flat_map_iter(|&(region, id)| {
        view.container(Place::Inventory(id))
            .into_iter()
            .flat_map(move |container| {
                container.goods.iter().filter_map(move |item| {
                    let portion = *edible_index.get(&item.good)?;
                    (item.quantity >= portion).then_some((region, item.good, 1, id, portion))
                })
            })
    }));
    stock.par_sort_unstable();
    let starts: Vec<_> = stock
        .par_iter()
        .enumerate()
        .filter_map(|(index, row)| {
            (index == 0 || (stock[index - 1].0, stock[index - 1].1) != (row.0, row.1))
                .then_some(index)
        })
        .collect();
    let sources: Vec<_> = starts
        .par_iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = starts.get(index + 1).copied().unwrap_or(stock.len());
            let regional = stock[start].2 == 0;
            Sources {
                region: stock[start].0,
                good: stock[start].1,
                portion: stock[start].4,
                regional,
                stores: start + usize::from(regional)..end,
            }
        })
        .collect();
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
            let location = local_access.map(|access| access(person.id, region));
            let first = sources.partition_point(|row| row.region < region);
            let last = sources.partition_point(|row| row.region <= region);
            sources[first..last]
                .iter()
                .filter(|row| row.portion <= holding.capacity - holding.used)
                .filter_map(|row| {
                    let regional = row
                        .regional
                        .then_some(())
                        .filter(|_| {
                            location.is_none_or(|place| matches!(place, Some(Place::Stock { .. })))
                        })
                        .and_then(|_| consider(person, region, row.good))
                        .map(|rank| {
                            (
                                rank,
                                0u8,
                                Place::Stock {
                                    region,
                                    good: row.good,
                                },
                            )
                        });
                    let store = match location {
                        Some(Some(Place::Inventory(id))) => stock[row.stores.clone()]
                            .binary_search_by_key(&id, |source| source.3)
                            .ok()
                            .map(|_| (0, 1, Place::Inventory(id))),
                        Some(_) => None,
                        None if !row.stores.is_empty() => {
                            let offset = (person.id % row.stores.len() as u64) as usize;
                            Some((0, 1, Place::Inventory(stock[row.stores.start + offset].3)))
                        }
                        None => None,
                    };
                    regional
                        .into_iter()
                        .chain(store)
                        .min_by_key(|choice| (choice.0, choice.1))
                        .map(|choice| (choice.0, row.good, choice.1, choice.2, row.portion))
                })
                .min_by_key(|row| (row.0, row.1, row.2))
                .map(|(_, good, _, from, portion)| (person.id, good, portion, from))
        })
        .collect();
    let count = u64::try_from(selected.len()).map_err(|_| "food pickup count overflow")?;
    action_base
        .checked_add(count)
        .ok_or("food pickup identity overflow")?;
    let actions = selected
        .par_iter()
        .enumerate()
        .map(|(index, &(person, good, quantity, from))| {
            let id = action_base + index as u64;
            Transaction {
                id,
                movements: vec![Movement {
                    item: None,
                    id,
                    from,
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
