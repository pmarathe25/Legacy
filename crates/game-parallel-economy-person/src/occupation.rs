//! Decide whether existing work progresses or the individual abandons it.
use crate::Person;
use game_parallel_lot_runtime::{Phase, Work};
use rayon::prelude::*;
pub type WorkChoices = (Vec<(u64, u64)>, Vec<(u64, u64)>);
pub fn continuing(
    work: &[Work],
    people: &[Person],
    pressured: &[u64],
    meals: &[(u64, u32)],
    max_hunger: u32,
) -> WorkChoices {
    let rows: Vec<_> = work
        .par_iter()
        .filter(|row| row.phase == Phase::Working)
        .filter_map(|row| {
            let person = people
                .binary_search_by_key(&row.job.worker, |p| p.id)
                .ok()
                .map(|i| &people[i])?;
            let willing = person.willing_to_work || pressured.binary_search(&person.id).is_ok();
            if person.health == 0 || (!person.controlled && !willing) {
                Some((row.job.id, person.id, false))
            } else if !person.controlled
                && willing
                && person.hunger <= max_hunger
                && meals.binary_search_by_key(&person.id, |m| m.0).is_err()
            {
                Some((row.job.id, person.id, true))
            } else {
                None
            }
        })
        .collect();
    let rates = rows
        .par_iter()
        .filter(|row| row.2)
        .map(|row| (row.0, 1))
        .collect();
    let abandon = rows
        .par_iter()
        .filter(|row| !row.2)
        .map(|row| (row.0, row.1))
        .collect();
    (rates, abandon)
}
