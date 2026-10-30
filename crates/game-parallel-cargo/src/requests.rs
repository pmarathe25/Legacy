//! Validate a bounded inbox per requester without prescribing a response.
use crate::Cargo;
use rayon::prelude::*;
pub fn validate(cargo: &Cargo, exists: impl Fn(u64) -> bool + Sync) -> Result<(), &'static str> {
    if (!cargo.food_requests.is_empty() && !cargo.personal_food_access)
        || cargo.food_requests.par_iter().any(|r| {
            r.requester == r.giver || r.amount == 0 || !exists(r.requester) || !exists(r.giver)
        })
    {
        return Err("invalid food request");
    }
    if (!cargo.food_askers.is_empty() && !cargo.personal_food_access)
        || cargo.food_askers.par_iter().any(|a| {
            a.person == a.giver
                || a.amount == 0
                || a.interval == 0
                || a.lifetime == 0
                || !exists(a.person)
                || !exists(a.giver)
        })
        || cargo
            .food_requests
            .par_iter()
            .any(|r| r.id >= cargo.next_food_request)
    {
        return Err("invalid food request policy or sequence");
    }
    if (!cargo.food_enrollments.is_empty() && !cargo.personal_food_access)
        || cargo.food_enrollments.par_iter().any(|e| !exists(e.person))
    {
        return Err("invalid food enrollment");
    }
    let mut enrollments: Vec<_> = cargo
        .food_enrollments
        .par_iter()
        .map(|e| e.person)
        .collect();
    enrollments.par_sort_unstable();
    if enrollments.par_windows(2).any(|p| p[0] == p[1])
        || cargo
            .acquire_when_hungry
            .par_iter()
            .any(|id| enrollments.binary_search(id).is_ok())
    {
        return Err("duplicate food enrollment");
    }
    let mut policies: Vec<_> = cargo.food_askers.par_iter().map(|a| a.person).collect();
    policies.par_sort_unstable();
    if policies.par_windows(2).any(|p| p[0] == p[1]) {
        return Err("duplicate food requester policy");
    }
    let (mut ids, mut requesters): (Vec<_>, Vec<_>) = rayon::join(
        || cargo.food_requests.par_iter().map(|r| r.id).collect(),
        || {
            cargo
                .food_requests
                .par_iter()
                .map(|r| r.requester)
                .collect()
        },
    );
    ids.par_sort_unstable();
    requesters.par_sort_unstable();
    if ids.par_windows(2).any(|p| p[0] == p[1]) || requesters.par_windows(2).any(|p| p[0] == p[1]) {
        return Err("duplicate food request or requester");
    }
    Ok(())
}
