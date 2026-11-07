//! Individuals seek observed local supplies for their own current work goals.
use super::{Choice, LocalTransport, Plan, finish};
use game_economy_person::Person;
use game_lots::{ContainerView, Place, View};
use game_production_model::Recipe;
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
    pub plan_inputs: bool,
    pub review: u64,
}

const MAX_DEPTH: usize = 16;
const MAX_VISITS: usize = 64;

struct Search<'a> {
    person: u64,
    region: u64,
    max_quantity: u64,
    tick: u64,
    review: u64,
    holder: &'a ContainerView,
    view: &'a View,
    stock: &'a [(u64, u32, u64, u64)],
    producers: &'a [(u32, &'a Recipe)],
    plan_inputs: bool,
}
impl Search<'_> {
    fn choose(
        &self,
        recipe: &Recipe,
        visited: &mut [u32; MAX_DEPTH],
        depth: usize,
        budget: &mut usize,
    ) -> Option<Choice> {
        if *budget == 0 || depth == MAX_DEPTH || visited[..depth].contains(&recipe.id) {
            return None;
        }
        *budget -= 1;
        visited[depth] = recipe.id;
        let missing = |input: &game_production_model::Amount| {
            let destination = Place::Stock {
                region: self.region,
                good: input.good,
            };
            let regional = self.view.container(destination)?;
            let amount = input.quantity.saturating_sub(regional.quantity(input.good));
            (amount > 0 && regional.used < regional.capacity).then_some((destination, amount))
        };
        if let Some(choice) = recipe.inputs.iter().find_map(|input| {
            let (destination, amount) = missing(input)?;
            let regional = self.view.container(destination)?;
            let quantity = self
                .holder
                .quantity(input.good)
                .min(amount)
                .min(self.max_quantity)
                .min(regional.capacity.saturating_sub(regional.used));
            (quantity > 0).then_some(Choice {
                person: self.person,
                from: Place::Person(self.person),
                to: destination,
                good: input.good,
                quantity,
            })
        }) {
            return Some(choice);
        }
        if let Some(choice) = recipe.inputs.iter().find_map(|input| {
            let (_, amount) = missing(input)?;
            let start = self
                .stock
                .partition_point(|row| (row.0, row.1) < (self.region, input.good));
            let end = self
                .stock
                .partition_point(|row| (row.0, row.1) <= (self.region, input.good));
            if start == end {
                return None;
            }
            let chosen =
                start + ((self.person.wrapping_add(self.tick) % (end - start) as u64) as usize);
            let (_, _, store, available) = self.stock[chosen];
            let quantity = available
                .min(amount)
                .min(self.max_quantity)
                .min(self.holder.capacity.saturating_sub(self.holder.used));
            (quantity > 0).then_some(Choice {
                person: self.person,
                from: Place::Inventory(store),
                to: Place::Person(self.person),
                good: input.good,
                quantity,
            })
        }) {
            return Some(choice);
        }
        if !self.plan_inputs {
            return None;
        }
        for input in &recipe.inputs {
            if missing(input).is_none() {
                continue;
            }
            let start = self.producers.partition_point(|row| row.0 < input.good);
            let end = self.producers.partition_point(|row| row.0 <= input.good);
            let producers = &self.producers[start..end];
            if producers.is_empty() {
                continue;
            }
            let offset = self.review as usize % producers.len();
            for (_, producer) in producers[offset..].iter().chain(&producers[..offset]) {
                if *budget == 0 {
                    return None;
                }
                *budget -= 1;
                if producer.outputs.iter().any(|output| {
                    self.view
                        .container(Place::Stock {
                            region: self.region,
                            good: output.good,
                        })
                        .is_none_or(|stock| output.quantity > stock.capacity)
                }) {
                    continue;
                }
                if let Some(choice) = self.choose(producer, visited, depth + 1, budget) {
                    return Some(choice);
                }
            }
        }
        None
    }
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
        let mut producers: Vec<_> = if self.plan_inputs {
            self.recipes
                .par_iter()
                .flat_map_iter(|recipe| {
                    recipe
                        .outputs
                        .iter()
                        .map(move |output| (output.good, recipe))
                })
                .collect()
        } else {
            Vec::new()
        };
        producers.par_sort_unstable_by_key(|row| (row.0, row.1.id));
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
                let mut budget = MAX_VISITS;
                Search {
                    person: person.id,
                    region,
                    max_quantity: preference.max_quantity,
                    tick: self.tick,
                    review: self.review,
                    holder,
                    view: self.view,
                    stock: &stock,
                    producers: &producers,
                    plan_inputs: self.plan_inputs,
                }
                .choose(recipe, &mut [0; MAX_DEPTH], 0, &mut budget)
            })
            .collect();
        finish(rows, base, None)
    }
}
