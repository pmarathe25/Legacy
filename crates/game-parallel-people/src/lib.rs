//! Region-owned ecology, personal need updates and deterministic meal arbitration.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub food_growth: u32,
    pub food_capacity: u32,
    pub food_per_meal: u32,
    pub hunger_per_tick: u32,
    pub hunger_satisfied: u32,
    pub starvation_threshold: u32,
    pub starvation_damage: u32,
    pub recovery: u32,
    pub max_health: u32,
}

impl Rules {
    pub fn validate(&self) -> Result<(), String> {
        if self.food_per_meal == 0 || self.hunger_satisfied == 0 || self.max_health == 0 {
            return Err("meal size, satisfaction and max health must be positive".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: u64,
    pub hunger: u32,
    pub health: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub id: u64,
    pub food: u32,
    pub people: Vec<Person>,
}

pub mod ecology {
    use super::{Region, Rules};
    pub fn grow(region: &mut Region, rules: &Rules) {
        region.food = region
            .food
            .saturating_add(rules.food_growth)
            .min(rules.food_capacity);
    }
}

pub mod people {
    use super::{Region, Rules};
    use rayon::prelude::*;
    // A counter-based priority is independent of thread scheduling and input order.
    fn priority(seed: u64, tick: u64, id: u64) -> u64 {
        let mut x = seed ^ tick.wrapping_mul(0x9e3779b97f4a7c15) ^ id;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    pub fn advance(region: &mut Region, rules: &Rules, seed: u64, tick: u64) {
        if region.people.len() >= 4096 {
            advance_large(region, rules, seed, tick);
            return;
        }
        for person in &mut region.people {
            if person.health > 0 {
                person.hunger = person.hunger.saturating_add(rules.hunger_per_tick);
            }
        }
        // The resource owner resolves competing meal claims in a stable order.
        let mut claims: Vec<usize> = region
            .people
            .iter()
            .enumerate()
            .filter(|(_, p)| p.health > 0 && p.hunger >= rules.hunger_satisfied)
            .map(|(i, _)| i)
            .collect();
        claims.sort_unstable_by_key(|&i| {
            (
                priority(seed, tick, region.people[i].id),
                region.people[i].id,
            )
        });
        for i in claims {
            if region.food < rules.food_per_meal {
                break;
            }
            region.food -= rules.food_per_meal;
            region.people[i].hunger = region.people[i]
                .hunger
                .saturating_sub(rules.hunger_satisfied);
        }
        for person in &mut region.people {
            if person.health == 0 {
                continue;
            }
            if person.hunger >= rules.starvation_threshold {
                person.health = person.health.saturating_sub(rules.starvation_damage);
            } else {
                person.health = person
                    .health
                    .saturating_add(rules.recovery)
                    .min(rules.max_health);
            }
        }
    }
    fn advance_large(region: &mut Region, rules: &Rules, seed: u64, tick: u64) {
        let claims: usize = region
            .people
            .par_chunks_mut(4096)
            .map(|chunk| {
                let mut count = 0;
                for p in chunk {
                    if p.health > 0 {
                        p.hunger = p.hunger.saturating_add(rules.hunger_per_tick);
                        count += usize::from(p.hunger >= rules.hunger_satisfied);
                    }
                }
                count
            })
            .sum();
        let meals = (region.food / rules.food_per_meal) as usize;
        let awarded = claims.min(meals);
        let cutoff = if awarded == 0 || awarded == claims {
            None
        } else {
            Some(select_cutoff(
                &region.people,
                rules,
                seed,
                tick,
                awarded - 1,
            ))
        };
        region.people.par_iter_mut().for_each(|p| {
            if p.health == 0 {
                return;
            }
            if p.hunger >= rules.hunger_satisfied
                && (awarded == claims
                    || cutoff.is_some_and(|limit| priority(seed, tick, p.id) <= limit))
            {
                p.hunger = p.hunger.saturating_sub(rules.hunger_satisfied);
            }
            if p.hunger >= rules.starvation_threshold {
                p.health = p.health.saturating_sub(rules.starvation_damage);
            } else {
                p.health = p
                    .health
                    .saturating_add(rules.recovery)
                    .min(rules.max_health);
            }
        });
        region.food -= (awarded as u32) * rules.food_per_meal;
    }

    // A parallel histogram finds the only bucket whose internal order matters.
    // The small boundary bucket is selected in place; all other grants are
    // independent comparisons against the resulting cutoff.
    fn select_cutoff(
        people: &[super::Person],
        rules: &Rules,
        seed: u64,
        tick: u64,
        mut rank: usize,
    ) -> u64 {
        let bins = people
            .par_chunks(4096)
            .map(|chunk| {
                let mut bins = [0usize; 256];
                for p in chunk {
                    if p.health > 0 && p.hunger >= rules.hunger_satisfied {
                        bins[(priority(seed, tick, p.id) >> 56) as usize] += 1;
                    }
                }
                bins
            })
            .reduce(
                || [0usize; 256],
                |mut a, b| {
                    for (x, y) in a.iter_mut().zip(b) {
                        *x += y;
                    }
                    a
                },
            );
        let mut bucket = 0;
        while rank >= bins[bucket] {
            rank -= bins[bucket];
            bucket += 1;
        }
        let mut boundary: Vec<u64> = people
            .par_iter()
            .filter_map(|p| {
                if p.health == 0 || p.hunger < rules.hunger_satisfied {
                    return None;
                }
                let key = priority(seed, tick, p.id);
                ((key >> 56) as usize == bucket).then_some(key)
            })
            .collect();
        *boundary.select_nth_unstable(rank).1
    }
}
