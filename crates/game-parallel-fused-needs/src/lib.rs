//! Fused local work progress and physical-needs preparation.
use game_parallel_fused_data::{Block, NONE};
pub mod environment;
pub mod fatigue;
pub mod food;
use food::Allocation;
use game_parallel_people::Rules;
use rayon::prelude::*;
pub fn prepare(
    blocks: &mut [Block],
    rules: &Rules,
    max_work_hunger: u32,
    fatigue: Option<&game_parallel_fatigue::Rules>,
    busy: &[u64],
    food_choices: (&[u64], Option<&[u64]>, &[u64]),
) {
    prepare_with_rate(
        blocks,
        rules,
        max_work_hunger,
        fatigue,
        busy,
        food_choices,
        |_, _| 1,
    );
}
/// Read committed personal rates; the caller supplies the active job handle and person ID.
pub fn prepare_with_rate(
    blocks: &mut [Block],
    rules: &Rules,
    max_work_hunger: u32,
    fatigue_rules: Option<&game_parallel_fatigue::Rules>,
    busy: &[u64],
    food_choices: (&[u64], Option<&[u64]>, &[u64]),
    rate: impl Fn(usize, u64) -> u64 + Sync,
) {
    let (carried_meals, seekers, care_fetch) = food_choices;
    blocks.par_iter_mut().for_each(|block| {
        let mut claims = 0usize;
        block.finished.clear();
        let has_carried_meals = !carried_meals.is_empty();
        if !has_carried_meals {
            block.carried_meal.fill(false);
        }
        let mut busy_index = block
            .ids
            .first()
            .map_or(0, |id| busy.partition_point(|other| other < id));
        let mut meal_index = block
            .ids
            .first()
            .map_or(0, |id| carried_meals.partition_point(|other| other < id));
        let mut seeker_index = block.ids.first().map_or(0, |id| {
            seekers.map_or(0, |s| s.partition_point(|other| other < id))
        });
        let mut care_index = block
            .ids
            .first()
            .map_or(0, |id| care_fetch.partition_point(|other| other < id));
        for index in 0..block.ids.len() {
            let health = block.health[index];
            let hunger = block.hunger[index];
            let ate_carried = if has_carried_meals {
                while meal_index < carried_meals.len()
                    && carried_meals[meal_index] < block.ids[index]
                {
                    meal_index += 1;
                }
                let ate = carried_meals.get(meal_index) == Some(&block.ids[index]);
                block.carried_meal[index] = ate;
                ate
            } else {
                false
            };
            let updated = if health > 0 {
                hunger.saturating_add(rules.hunger_per_tick)
            } else {
                hunger
            };
            let updated = if ate_carried {
                updated.saturating_sub(rules.hunger_satisfied)
            } else {
                updated
            };
            block.next_hunger[index] = updated;
            let seeking = seekers.is_none_or(|seekers| {
                while seeker_index < seekers.len() && seekers[seeker_index] < block.ids[index] {
                    seeker_index += 1;
                }
                seekers.get(seeker_index) == Some(&block.ids[index])
            });
            let claim = block.journey.is_none()
                && health > 0
                && !ate_carried
                && updated >= rules.hunger_satisfied
                && seeking;
            block.food_claim[index] = claim;
            while care_index < care_fetch.len() && care_fetch[care_index] < block.ids[index] {
                care_index += 1;
            }
            block.care_food_claim[index] =
                claim && care_fetch.get(care_index) == Some(&block.ids[index]);
            claims += usize::from(claim);
            let handle = block.work[index];
            let active = handle != NONE
                && health > 0
                && hunger <= max_work_hunger
                && block.work_local[index]
                && block
                    .fatigue
                    .as_ref()
                    .is_none_or(|columns| columns.flags[index] & 2 == 0);
            let progress = if active {
                rate(handle, block.ids[index])
            } else {
                0
            };
            block.next_remaining[index] = block.remaining[index].saturating_sub(progress);
            if let Some(rules) = fatigue_rules {
                let columns = block.fatigue.as_mut().expect("fatigue columns");
                let mut value = columns.values[index];
                let mut resting = columns.flags[index] & 2 != 0;
                if health > 0 {
                    value = value
                        .saturating_add(rules.baseline_gain)
                        .min(rules.capacity);
                    while busy_index < busy.len() && busy[busy_index] < block.ids[index] {
                        busy_index += 1;
                    }
                    let engaged = busy.get(busy_index) == Some(&block.ids[index]);
                    if progress > 0 || engaged {
                        value = value.saturating_add(rules.work_gain).min(rules.capacity);
                    } else if resting {
                        if block.journey.is_none() && !engaged {
                            value = value.saturating_sub(rules.rest_recovery);
                            if value <= rules.resume_at {
                                resting = false;
                            }
                        }
                    } else if block.journey.is_none() && !engaged {
                        value = value.saturating_sub(rules.idle_recovery);
                    }
                    if value >= rules.rest_at {
                        resting = true;
                    }
                }
                columns.next_values[index] = value;
                columns.next_flags[index] =
                    u8::from(value != 0 || resting) | (u8::from(resting) << 1);
            }
            if active && block.next_remaining[index] == 0 {
                block.finished.push(handle);
            }
        }
        block.claims = claims;
    });
}
pub fn finalize(
    blocks: &mut [Block],
    rules: &Rules,
    allocation: &Allocation,
    fatigue_rules: Option<&game_parallel_fatigue::Rules>,
    disease_rules: Option<&game_parallel_disease::Disease>,
    tick: u64,
    personal_access: bool,
) {
    // Finalize the same owned blocks only after shared grants are resolved.
    blocks.par_iter_mut().for_each(|block| {
        let mut rank = block.prefix;
        for index in 0..block.ids.len() {
            let mut health = block.health[index];
            let mut hunger = block.next_hunger[index];
            if health > 0 {
                if block.food_claim[index] {
                    let granted = allocation.cutoffs[block.region]
                        .map_or(rank < allocation.meals[block.region], |cutoff| {
                            cutoff.allows(block.ids[index])
                        });
                    if granted && (!personal_access || block.care_food_claim[index]) {
                        hunger = hunger.saturating_sub(rules.hunger_satisfied);
                    }
                    rank += 1;
                }
                health = if hunger >= rules.starvation_threshold {
                    health.saturating_sub(rules.starvation_damage)
                } else {
                    health.saturating_add(rules.recovery).min(rules.max_health)
                };
            }
            if let Some(rules) = fatigue_rules
                && block.health[index] > 0
                && block.fatigue.as_ref().expect("fatigue columns").next_values[index]
                    >= rules.exhaustion_at
            {
                health = health.saturating_sub(rules.exhaustion_damage);
            }
            if let Some(disease) = disease_rules
                && block.health[index] > 0
            {
                let columns = block.disease.as_ref().expect("disease columns");
                if columns.flags[index] & 1 != 0 {
                    let strain = game_parallel_fused_disease::find_strain(
                        &disease.strains,
                        columns.strains[index],
                    );
                    if strain.contagious(tick - columns.infected_at[index]) {
                        health = health.saturating_sub(strain.damage_per_tick);
                    }
                }
            }
            block.next_health[index] = health;
            block.next_hunger[index] = hunger;
            let handle = block.work[index];
            if handle != NONE && block.next_remaining[index] == 0 {
                block.next_work[index] = NONE;
                block.next_remaining[index] = 0;
            } else {
                block.next_work[index] = handle;
            }
        }
    });
}
