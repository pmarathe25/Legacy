//! Check saved physical custody and production specifications together.
use crate::{Phase, Snapshot};
use game_lot_content::Catalog;
use game_lot_model::{Container, Place};
use game_production_model::{Amount, JobPhase, valid_items};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

fn bounded(actual: &Container, expected: &[Amount], catalog: &Catalog) -> bool {
    let product = |good| {
        catalog
            .maturation(good)
            .map_or(good, |profile| profile.product)
    };
    if expected
        .iter()
        .any(|item| catalog.maturation(item.good).is_some())
    {
        let quantity = |good| {
            actual
                .lots
                .iter()
                .filter(|lot| lot.good == good)
                .map(|lot| u128::from(lot.quantity))
                .sum::<u128>()
        };
        return actual.lots.iter().all(|lot| {
            expected
                .iter()
                .any(|item| item.good == lot.good || product(item.good) == lot.good)
        }) && expected.iter().all(|item| {
            let output = product(item.good);
            let opening = expected
                .iter()
                .filter(|row| product(row.good) == output)
                .map(|row| u128::from(row.quantity))
                .sum::<u128>();
            let remaining = actual
                .lots
                .iter()
                .filter(|lot| product(lot.good) == output)
                .map(|lot| u128::from(lot.quantity))
                .sum::<u128>();
            (output == item.good || quantity(item.good) <= u128::from(item.quantity))
                && remaining <= opening
        });
    }
    actual
        .lots
        .iter()
        .all(|lot| expected.iter().any(|item| item.good == lot.good))
        && expected.iter().all(|item| {
            let quantity: u128 = actual
                .lots
                .iter()
                .filter(|lot| lot.good == item.good)
                .map(|lot| u128::from(lot.quantity))
                .sum();
            quantity <= u128::from(item.quantity)
        })
}
fn multiplier(actual: &[Amount], recipe: &[Amount]) -> Option<u64> {
    if actual.len() != recipe.len() {
        return None;
    }
    let first = recipe.first()?;
    let quantity = actual.iter().find(|item| item.good == first.good)?.quantity;
    if first.quantity == 0 || quantity % first.quantity != 0 {
        return None;
    }
    let units = quantity / first.quantity;
    if units == 0 || units > u64::from(u32::MAX) {
        return None;
    }
    recipe
        .iter()
        .all(|item| {
            actual
                .iter()
                .find(|row| row.good == item.good)
                .is_some_and(|row| item.quantity.checked_mul(units) == Some(row.quantity))
        })
        .then_some(units)
}
pub fn validate(state: &Snapshot, catalog: &Catalog) -> Result<(), String> {
    let work_period = state.work_rate_period_millis.unwrap_or(state.tick_millis);
    if work_period == 0 {
        return Err("invalid work rate period".into());
    }
    if let Some(interval) = &state.work_interval {
        let work: HashMap<_, _> = state
            .work
            .par_iter()
            .map(|row| (row.job.id, row.phase))
            .collect();
        if interval.tick != state.custody.tick
            || interval.custody_revision != state.custody.revision
            || interval.elapsed_millis > state.tick_millis
            || interval
                .rates
                .par_windows(2)
                .any(|pair| pair[0].0 >= pair[1].0)
            || interval
                .rates
                .par_iter()
                .any(|(job, rate)| *rate == 0 || work.get(job) != Some(&Phase::Working))
        {
            return Err("invalid saved work interval".into());
        }
    }
    let goods: HashSet<_> = state.goods.iter().map(|row| row.id).collect();
    if state.recipes.par_iter().any(|recipe| {
        recipe.id == 0
            || recipe.ticks == 0
            || !valid_items(&recipe.inputs, &goods)
            || !valid_items(&recipe.outputs, &goods)
    }) {
        return Err("invalid physical production recipe".into());
    }
    let recipes: HashMap<_, _> = state.recipes.iter().map(|row| (row.id, row)).collect();
    if recipes.len() != state.recipes.len()
        || state
            .work
            .par_windows(2)
            .any(|pair| pair[0].job.id >= pair[1].job.id)
    {
        return Err("duplicate or unordered production identity".into());
    }
    if state.custody.containers.par_iter().any(|container| {
        container.lots.iter().any(|lot| !catalog.contains(lot.good))
            || match container.place {
                Place::Stock { good, .. } => !catalog.contains(good),
                Place::JobInput(id) => state
                    .work
                    .binary_search_by_key(&id, |row| row.job.id)
                    .map_or(true, |index| {
                        let work = &state.work[index];
                        work.phase == Phase::Waiting || !bounded(container, &work.job.reserved, catalog)
                    }),
                Place::JobOutput(id) => state
                    .work
                    .binary_search_by_key(&id, |row| row.job.id)
                    .map_or(true, |index| {
                        let work = &state.work[index];
                        if work.phase == Phase::Waiting {
                            !bounded(container, &work.job.outputs, catalog)
                        } else {
                            container.used != 0
                        }
                    }),
                _ => false,
            }
    }) || state.work.par_iter().any(|work| {
        let job = &work.job;
        let Some(recipe) = recipes.get(&job.recipe) else {
            return true;
        };
        let Some(units) = multiplier(&job.outputs, &recipe.outputs) else {
            return true;
        };
        !valid_items(&job.outputs, &goods)
            || job.remaining > u64::from(recipe.ticks) * units
            || work.progress_remainder >= work_period
            || (job.remaining == 0 && work.progress_remainder != 0)
            || match work.phase {
                Phase::Working | Phase::Returning | Phase::Suspended { .. } => {
                    job.phase != JobPhase::Working
                        || matches!(work.phase, Phase::Suspended { until } if until <= state.custody.tick)
                        || !valid_items(&job.reserved, &goods)
                        || multiplier(&job.reserved, &recipe.inputs) != Some(units)
                }
                Phase::Waiting => {
                    job.phase != JobPhase::Waiting || job.remaining != 0 || !job.reserved.is_empty()
                }
            }
    }) {
        return Err("invalid physical job custody".into());
    }
    Ok(())
}
