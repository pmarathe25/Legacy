//! Add empty custody to an already planned physical boundary.
use game_parallel_lot_model::{Container, State};
use rayon::prelude::*;

pub fn add(
    state: &State,
    additions: &mut Vec<Container>,
    changes: &mut [(usize, Container)],
    mut new: Vec<Container>,
) -> Result<(), String> {
    if new.is_empty() {
        return Ok(());
    }
    new.par_sort_unstable_by_key(|row| row.place);
    let mut prior: Vec<_> = state
        .containers
        .par_iter()
        .chain(additions.par_iter())
        .map(|row| row.place)
        .collect();
    prior.par_sort_unstable();
    if new
        .par_windows(2)
        .any(|rows| rows[0].place == rows[1].place)
        || new.par_iter().any(|row| {
            row.validate().is_err() || row.used != 0 || prior.binary_search(&row.place).is_ok()
        })
        || changes.par_iter().any(|(index, _)| *index >= prior.len())
    {
        return Err("invalid or existing new custody".into());
    }
    changes.par_iter_mut().for_each(|(index, _)| {
        let place = prior[*index];
        *index += new.partition_point(|row| row.place < place);
    });
    additions.extend(new);
    additions.par_sort_unstable_by_key(|row| row.place);
    Ok(())
}
