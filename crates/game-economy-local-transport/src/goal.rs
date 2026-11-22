//! Individuals seek observed local supplies for their own current work goals.
use super::{Choice, LocalTransport, Plan, finish};
use game_economy_person::Person;
use game_lots::{ContainerView, Place, View};
use game_production_model::Recipe;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    #[serde(default)]
    pub enabled: bool,
    pub max_quantity: u64,
    #[serde(default)]
    pub search: Option<SearchPreference>,
}

/// A person's choice to search one known store for their current goal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchPreference {
    pub store: u64,
    #[serde(default)]
    pub alternatives: Arc<Vec<u64>>,
    #[serde(default = "one_visit")]
    pub max_visits: u32,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub retry_ticks: u64,
    #[serde(default)]
    pub max_ticks: u64,
    #[serde(default)]
    pub next_review: u64,
    #[serde(default)]
    pub active_recipe: Option<u32>,
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default)]
    pub examined: u32,
    #[serde(default)]
    pub target: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<ElapsedSearch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElapsedSearch {
    pub retry_millis: NonZeroU64,
    pub max_millis: NonZeroU64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_review: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u128>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub owns_walk: bool,
}

impl SearchPreference {
    pub fn valid_clock(&self) -> bool {
        self.elapsed
            .map_or(self.retry_ticks > 0 && self.max_ticks > 0, |clock| {
                self.retry_ticks == 0
                    && self.max_ticks == 0
                    && self.expires_at == 0
                    && self.next_review == 0
                    && clock.expires_at.is_some() == self.active_recipe.is_some()
                    && (!clock.owns_walk || self.active_recipe.is_some())
            })
    }

    pub fn can_start(&self, tick: u64, now: u128) -> bool {
        self.elapsed.map_or(tick >= self.next_review, |clock| {
            clock.next_review.is_none_or(|review| now >= review)
        })
    }

    pub fn expired(&self, tick: u64, now: u128) -> bool {
        self.elapsed.map_or(tick >= self.expires_at, |clock| {
            clock.expires_at.is_some_and(|expiry| now >= expiry)
        })
    }

    pub fn begin(&mut self, recipe: u32, tick: u64, now: u128) -> Result<(), &'static str> {
        if let Some(clock) = &mut self.elapsed {
            let next_review = now
                .checked_add(u128::from(clock.retry_millis.get()))
                .ok_or("local goal search retry overflow")?;
            let expires_at = now
                .checked_add(u128::from(clock.max_millis.get()))
                .ok_or("local goal search expiry overflow")?;
            clock.next_review = Some(next_review);
            clock.expires_at = Some(expires_at);
        } else {
            let next_review = tick
                .checked_add(self.retry_ticks)
                .ok_or("local goal search retry overflow")?;
            let expires_at = tick
                .checked_add(self.max_ticks)
                .ok_or("local goal search expiry overflow")?;
            self.next_review = next_review;
            self.expires_at = expires_at;
        }
        self.active_recipe = Some(recipe);
        self.examined = 0;
        self.target = Some(self.store);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.active_recipe = None;
        self.target = None;
        if let Some(clock) = &mut self.elapsed {
            clock.expires_at = None;
            clock.owns_walk = false;
        }
    }
}

fn one_visit() -> u32 {
    1
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
    current_store: Option<u64>,
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
        if let Some(store) = self.current_store {
            let source = self.view.container(Place::Inventory(store))?;
            return recipe.inputs.iter().find_map(|input| {
                let needed = input
                    .quantity
                    .saturating_sub(self.holder.quantity(input.good));
                let quantity = source
                    .quantity(input.good)
                    .min(needed)
                    .min(self.max_quantity)
                    .min(self.holder.capacity.saturating_sub(self.holder.used));
                (quantity > 0).then_some(Choice {
                    person: self.person,
                    from: Place::Inventory(store),
                    to: Place::Person(self.person),
                    good: input.good,
                    quantity,
                })
            });
        }
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
        finish(self.choices(false), base, None)
    }

    /// Interval actors use only their current endpoint and their own cargo.
    pub(super) fn directed_choices(&self) -> Vec<Choice> {
        self.choices(true)
    }

    fn choices(&self, local_only: bool) -> Vec<Choice> {
        if self.needs.is_empty() || !self.owner.goal_fetchers.par_iter().any(|p| p.enabled) {
            return Vec::new();
        }
        let mut recipes: Vec<_> = self.recipes.par_iter().collect();
        if recipes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            recipes.par_sort_unstable_by_key(|recipe| recipe.id);
        }
        let visible_stores = if local_only || self.owner.walking.is_some() {
            &[][..]
        } else {
            self.owner.stores.as_slice()
        };
        let mut stock: Vec<_> = visible_stores
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
        self.owner
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
                    || (!local_only && !person.willing_to_work)
                    || person.hunger > self.max_hunger
                {
                    return None;
                }
                let region = person.region?;
                let current_store = match self
                    .owner
                    .walking
                    .as_ref()
                    .map(|walking| walking.location(person.id, region))
                {
                    Some(super::walking::Location::At {
                        endpoint: super::Endpoint::Store { id },
                        ..
                    }) => Some(id),
                    Some(super::walking::Location::Walking { .. }) => return None,
                    _ => None,
                };
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
                    current_store,
                }
                .choose(recipe, &mut [0; MAX_DEPTH], 0, &mut budget)
            })
            .collect()
    }
}
