//! Combine disjoint bulk and ordinary agreement patches.
use crate::Draft;
use rayon::prelude::*;
pub fn combine(
    bulk: Option<game_parallel_agreement_bulk::Draft>,
    ordinary: Option<Draft>,
) -> Option<Draft> {
    let Some(bulk) = bulk else {
        return ordinary;
    };
    let Some(mut result) = ordinary else {
        return Some(Draft {
            actors: bulk.actors,
            updates: bulk.updates,
            events: bulk.events,
            holdings: bulk.holdings,
        });
    };
    result.actors.par_extend(bulk.actors);
    result.updates.par_extend(bulk.updates);
    result.events.par_extend(bulk.events);
    result.holdings.par_extend(bulk.holdings);
    result.actors.par_sort_unstable();
    result.updates.par_sort_unstable_by_key(|row| row.id);
    result
        .events
        .par_sort_unstable_by_key(|event| event.contract);
    result.holdings.par_sort_unstable_by_key(|row| row.0);
    Some(result)
}
