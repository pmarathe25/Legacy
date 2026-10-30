//! Reusable tools are physical possessions, checked before work is admitted.
use game_parallel_lots::{Place, View};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub recipe: u32,
    pub good: u32,
    pub quantity: u64,
}

/// The host keeps this table sorted by recipe and good.
pub fn validate(
    rows: &[Requirement],
    recipes: &HashSet<u32>,
    goods: &HashSet<u32>,
) -> Result<(), String> {
    if rows
        .par_windows(2)
        .any(|p| (p[0].recipe, p[0].good) >= (p[1].recipe, p[1].good))
        || rows
            .par_iter()
            .any(|r| r.quantity == 0 || !recipes.contains(&r.recipe) || !goods.contains(&r.good))
    {
        return Err("invalid work tool requirement".into());
    }
    Ok(())
}

/// Each worker must actually carry the tools; they are checked but never consumed.
pub fn available(rows: &[Requirement], view: &View, worker: u64, recipe: u32) -> bool {
    let first = rows.partition_point(|r| r.recipe < recipe);
    let last = rows.partition_point(|r| r.recipe <= recipe);
    if first == last {
        return true;
    }
    view.container(Place::Person(worker)).is_some_and(|c| {
        rows[first..last]
            .iter()
            .all(|r| c.quantity(r.good) >= r.quantity)
    })
}
