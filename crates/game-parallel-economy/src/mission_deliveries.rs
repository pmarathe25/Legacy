//! Select physical promise deliveries that a person independently attempted.
use game_parallel_lot_projection::agreement::Plan;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::Place;
use rayon::prelude::*;

pub(crate) fn accepted_promises(
    plan: Option<&Plan>,
    events: &Events,
) -> Vec<(u64, u64, u64, u32, u64)> {
    let Some(plan) = plan.filter(|plan| !plan.actions().is_empty()) else {
        return Vec::new();
    };
    let mut accepted: Vec<_> = events
        .physical_actions
        .par_iter()
        .filter(|outcome| outcome.accepted)
        .map(|outcome| outcome.id)
        .collect();
    accepted.par_sort_unstable();
    plan.actions()
        .par_iter()
        .filter(|action| accepted.binary_search(&action.id).is_ok())
        .flat_map_iter(|action| {
            action.movements.iter().filter_map(move |movement| {
                let (Place::Person(giver), Some(Place::Person(recipient))) =
                    (movement.from, movement.to)
                else {
                    return None;
                };
                Some((
                    action.id,
                    giver,
                    recipient,
                    movement.good,
                    movement.quantity,
                ))
            })
        })
        .collect()
}
