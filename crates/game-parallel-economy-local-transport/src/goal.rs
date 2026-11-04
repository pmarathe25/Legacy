//! Individuals seek observed local supplies for their own current work goals.
use super::{Choice, LocalTransport, Plan, finish};
use game_parallel_economy_person::Person;
use game_parallel_lots::{Place, View};
use game_parallel_production_model::Recipe;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    #[serde(default)]
    pub enabled: bool,
    pub max_quantity: u64,
}
#[derive(Clone, Copy)]
pub struct Need {
    pub person: u64,
    pub recipe: u32,
}
pub struct Context<'a> {
    pub owner: &'a LocalTransport,
    /// One selected personal goal per person, sorted by person.
    pub needs: &'a [Need],
    pub recipes: &'a [Recipe],
    pub people: &'a [Person],
    pub view: &'a View,
    pub busy: &'a [u64],
    pub unwilling: &'a [u64],
    pub max_hunger: u32,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if self.needs.is_empty() || !self.owner.goal_fetchers.par_iter().any(|p| p.enabled) {
            return finish(vec![], base, None);
        }
        let mut recipes: Vec<_> = self.recipes.par_iter().collect();
        if recipes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            recipes.par_sort_unstable_by_key(|recipe| recipe.id);
        }
        let mut stock: Vec<_> = self
            .owner
            .stores
            .par_iter()
            .flat_map_iter(|store| {
                self.view
                    .container(Place::Inventory(store.id))
                    .into_iter()
                    .flat_map(move |holder| {
                        holder
                            .goods
                            .iter()
                            .map(move |good| (store.region, good.good, store.id, good.quantity))
                    })
            })
            .collect();
        stock.par_sort_unstable_by_key(|row| (row.0, row.1, row.2));
        let rows: Vec<_> = self
            .owner
            .goal_fetchers
            .par_iter()
            .filter_map(|preference| {
                if !preference.enabled
                    || self.busy.binary_search(&preference.person).is_ok()
                    || self.unwilling.binary_search(&preference.person).is_ok()
                {
                    return None;
                }
                let person = self
                    .people
                    .binary_search_by_key(&preference.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                if person.controlled
                    || person.health == 0
                    || !person.willing_to_work
                    || person.hunger > self.max_hunger
                {
                    return None;
                }
                let region = person.region?;
                let need = self
                    .needs
                    .binary_search_by_key(&person.id, |row| row.person)
                    .ok()
                    .map(|i| self.needs[i])?;
                let recipe = recipes
                    .binary_search_by_key(&need.recipe, |row| row.id)
                    .ok()
                    .map(|i| recipes[i])?;
                let holder = self.view.container(Place::Person(person.id))?;
                let deposit = recipe.inputs.iter().find_map(|input| {
                    let destination = Place::Stock {
                        region,
                        good: input.good,
                    };
                    let regional = self.view.container(destination)?;
                    let missing = input.quantity.saturating_sub(regional.quantity(input.good));
                    if missing == 0 {
                        return None;
                    }
                    let carrying = holder.quantity(input.good);
                    let quantity = carrying
                        .min(missing)
                        .min(preference.max_quantity)
                        .min(regional.capacity.saturating_sub(regional.used));
                    (quantity > 0).then_some(Choice {
                        person: person.id,
                        from: Place::Person(person.id),
                        to: destination,
                        good: input.good,
                        quantity,
                    })
                });
                deposit.or_else(|| {
                    recipe.inputs.iter().find_map(|input| {
                        let regional = self.view.container(Place::Stock {
                            region,
                            good: input.good,
                        })?;
                        let missing = input.quantity.saturating_sub(regional.quantity(input.good));
                        if missing == 0 || regional.used >= regional.capacity {
                            return None;
                        }
                        let start =
                            stock.partition_point(|row| (row.0, row.1) < (region, input.good));
                        let end =
                            stock.partition_point(|row| (row.0, row.1) <= (region, input.good));
                        if start == end {
                            return None;
                        }
                        let chosen = start
                            + ((person.id.wrapping_add(self.tick) % (end - start) as u64) as usize);
                        let (_, _, store, available) = stock[chosen];
                        let quantity = available
                            .min(missing)
                            .min(preference.max_quantity)
                            .min(holder.capacity.saturating_sub(holder.used));
                        (quantity > 0).then_some(Choice {
                            person: person.id,
                            from: Place::Inventory(store),
                            to: Place::Person(person.id),
                            good: input.good,
                            quantity,
                        })
                    })
                })
            })
            .collect();
        finish(rows, base, None)
    }
}
