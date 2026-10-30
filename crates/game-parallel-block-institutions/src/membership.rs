//! Publish independent membership decisions with parallel, ordered merging.
use game_parallel::{InstitutionAdmissionEvent, Institutions};
use rayon::prelude::*;

pub fn publish(institutions: &mut Institutions, mut decisions: Vec<InstitutionAdmissionEvent>) {
    if decisions.is_empty() {
        return;
    }
    if decisions
        .par_windows(2)
        .any(|pair| (pair[0].body, pair[0].applicant) > (pair[1].body, pair[1].applicant))
    {
        decisions.par_sort_unstable_by_key(|event| (event.body, event.applicant));
    }
    institutions.bodies.par_iter_mut().for_each(|body| {
        let start = decisions.partition_point(|event| event.body < body.id);
        let end = decisions.partition_point(|event| event.body <= body.id);
        if start != end {
            body.members.par_extend(
                decisions[start..end]
                    .par_iter()
                    .filter(|event| event.accepted)
                    .map(|event| event.applicant),
            );
            if body.members.par_windows(2).any(|pair| pair[0] > pair[1]) {
                body.members.par_sort_unstable();
            }
        }
    });
    let mut requests = std::mem::take(&mut institutions.requests);
    if requests
        .par_windows(2)
        .any(|pair| (pair[0].body, pair[0].applicant) > (pair[1].body, pair[1].applicant))
    {
        requests.par_sort_unstable_by_key(|request| (request.body, request.applicant));
    }
    let view = &decisions;
    institutions.requests = requests
        .into_par_iter()
        .chunks(4096)
        .flat_map_iter(|chunk| {
            let first = &chunk[0];
            let mut index = view.partition_point(|event| {
                (event.body, event.applicant) < (first.body, first.applicant)
            });
            chunk.into_iter().filter(move |request| {
                let key = (request.body, request.applicant);
                while index < view.len() && (view[index].body, view[index].applicant) < key {
                    index += 1;
                }
                index == view.len() || (view[index].body, view[index].applicant) != key
            })
        })
        .collect();
    institutions.admissions.par_extend(decisions);
}

#[cfg(test)]
#[path = "../tests/support/membership.rs"]
mod tests;
