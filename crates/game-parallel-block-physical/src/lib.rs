//! Parallel physical preparation into reusable next-generation columns.
use game_parallel_block_data::Block;
use game_parallel_people::Rules;
use rayon::prelude::*;
use std::mem;
pub struct Context<'a> {
    pub blocks: &'a mut [Block],
    pub food: &'a [u32],
    pub grown: &'a mut [u32],
    pub claims: &'a mut [usize],
    pub meals: &'a mut [usize],
    pub rules: &'a Rules,
    pub carried_meals: &'a [u64],
}
impl Context<'_> {
    pub fn prepare(&mut self) {
        self.grown
            .par_iter_mut()
            .zip(self.food.par_iter())
            .for_each(|(grown, &food)| {
                *grown = food
                    .saturating_add(self.rules.food_growth)
                    .min(self.rules.food_capacity)
            });
        self.blocks.par_iter_mut().for_each(|block| {
            block.claims = if block.journey.is_some() {
                0
            } else {
                block
                    .health
                    .iter()
                    .zip(&block.hunger)
                    .filter(|(health, hunger)| {
                        **health > 0
                            && hunger.saturating_add(self.rules.hunger_per_tick)
                                >= self.rules.hunger_satisfied
                    })
                    .count()
            };
        });
        self.claims.fill(0);
        let mut overlap = vec![false; self.claims.len()];
        let mut last_id = vec![None; self.claims.len()];
        for block in self.blocks.iter_mut() {
            block.prefix = self.claims[block.region];
            self.claims[block.region] += block.claims;
            if block.journey.is_none() {
                if last_id[block.region].is_some_and(|id| id > block.ids[0]) {
                    overlap[block.region] = true;
                }
                last_id[block.region] = Some(
                    last_id[block.region].map_or(*block.ids.last().unwrap(), |id: u64| {
                        id.max(*block.ids.last().unwrap())
                    }),
                );
            }
        }
        self.meals
            .par_iter_mut()
            .enumerate()
            .for_each(|(region, meals)| {
                *meals = self.claims[region]
                    .min((self.grown[region] / self.rules.food_per_meal) as usize)
            });
        // Blocks can overlap after an arrival or birth. In those regions use a
        // stable identity cutoff so saving and recompiling cannot change who
        // receives contested food. Uncontested and non-overlapping regions use
        // the cheaper block-prefix path.
        let mut cutoffs = vec![None; self.claims.len()];
        if overlap.iter().enumerate().any(|(region, &mixed)| {
            mixed && self.meals[region] > 0 && self.meals[region] < self.claims[region]
        }) {
            let hunger_per_tick = self.rules.hunger_per_tick;
            let hunger_satisfied = self.rules.hunger_satisfied;
            let mut candidates: Vec<(usize, u64)> = self
                .blocks
                .par_iter()
                .flat_map_iter(|block| {
                    let active = block.journey.is_none()
                        && overlap[block.region]
                        && self.meals[block.region] > 0
                        && self.meals[block.region] < self.claims[block.region];
                    block
                        .ids
                        .iter()
                        .enumerate()
                        .filter_map(move |(index, &id)| {
                            (active
                                && block.health[index] > 0
                                && block.hunger[index].saturating_add(hunger_per_tick)
                                    >= hunger_satisfied)
                                .then_some((block.region, id))
                        })
                })
                .collect();
            candidates.par_sort_unstable();
            for (region, cutoff) in cutoffs.iter_mut().enumerate() {
                if overlap[region]
                    && self.meals[region] > 0
                    && self.meals[region] < self.claims[region]
                {
                    let start = candidates.partition_point(|&(candidate, _)| candidate < region);
                    *cutoff = Some(candidates[start + self.meals[region] - 1].1);
                }
            }
        }
        self.blocks.par_iter_mut().for_each(|block| {
            let mut rank = block.prefix;
            let limit = self.meals[block.region];
            for index in 0..block.ids.len() {
                let mut health = block.health[index];
                let mut hunger = block.hunger[index];
                if health > 0 {
                    hunger = hunger.saturating_add(self.rules.hunger_per_tick);
                    if block.journey.is_some()
                        && !self.carried_meals.is_empty()
                        && self.carried_meals.binary_search(&block.ids[index]).is_ok()
                    {
                        hunger = hunger.saturating_sub(self.rules.hunger_satisfied);
                    }
                    if block.journey.is_none() && hunger >= self.rules.hunger_satisfied {
                        if cutoffs[block.region].map_or(rank < limit, |id| block.ids[index] <= id) {
                            hunger = hunger.saturating_sub(self.rules.hunger_satisfied);
                        }
                        rank += 1;
                    }
                    health = if hunger >= self.rules.starvation_threshold {
                        health.saturating_sub(self.rules.starvation_damage)
                    } else {
                        health
                            .saturating_add(self.rules.recovery)
                            .min(self.rules.max_health)
                    };
                }
                block.next_hunger[index] = hunger;
                block.next_health[index] = health;
            }
        });
    }
}
pub fn publish(blocks: &mut [Block]) {
    blocks.par_iter_mut().for_each(|block| {
        mem::swap(&mut block.health, &mut block.next_health);
        mem::swap(&mut block.hunger, &mut block.next_hunger);
    });
}
pub fn advance_journeys(blocks: &mut [Block]) -> usize {
    blocks
        .par_iter_mut()
        .map(|block| {
            let Some(mut journey) = block.journey else {
                return 0;
            };
            journey.remaining -= 1;
            if journey.remaining == 0 {
                block.region = journey.destination_index;
                block.journey = None;
                1
            } else {
                block.journey = Some(journey);
                0
            }
        })
        .sum()
}
