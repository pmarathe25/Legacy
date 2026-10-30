//! Independent applications to compatible local institutions.
use game_parallel::{InstitutionMembershipRequest, World};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn prepare(
    world: &World,
    people: &[PersonSnapshot],
    busy: &[u64],
    tick: u64,
) -> Vec<InstitutionMembershipRequest> {
    let (Some(civic), Some(institutions)) = (&world.civic_auto, &world.institutions) else {
        return Vec::new();
    };
    if civic
        .policies
        .par_iter()
        .all(|policy| !policy.enabled || !policy.seek_membership)
    {
        return Vec::new();
    }
    let bodies: HashMap<_, _> = institutions
        .bodies
        .iter()
        .filter(|body| !body.members.is_empty())
        .map(|body| (body.region, body))
        .collect();
    if bodies.is_empty() {
        return Vec::new();
    }
    let mut members: Vec<_> = institutions
        .bodies
        .par_iter()
        .flat_map(|body| {
            body.members
                .par_iter()
                .map(move |&person| (body.id, person))
        })
        .collect();
    if members.par_windows(2).any(|pair| pair[0] > pair[1]) {
        members.par_sort_unstable();
    }
    let pending: HashSet<_> = institutions
        .requests
        .par_iter()
        .map(|request| (request.body, request.applicant))
        .collect();
    let mut busy = busy.to_vec();
    if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
        busy.par_sort_unstable();
    }
    let needs_trust = civic.policies.par_iter().any(|policy| {
        policy.enabled && policy.seek_membership && policy.trade_trust_target.is_some()
    });
    let opinions = world
        .opinions
        .as_ref()
        .filter(|_| needs_trust)
        .map(|owner| owner.index());
    let mut requests: Vec<_> = civic
        .policies
        .par_iter()
        .filter_map(|policy| {
            if !policy.enabled
                || !policy.seek_membership
                || busy.binary_search(&policy.person).is_ok()
            {
                return None;
            }
            let (_, region, health, _) = people[*person_index(people, policy.person).as_ref()?];
            let body = bodies.get(&region?)?;
            if health == 0
                || members.binary_search(&(body.id, policy.person)).is_ok()
                || pending.contains(&(body.id, policy.person))
            {
                return None;
            }
            let preferred = policy
                .trade_trust_target
                .map_or(policy.prefer_open, |target| {
                    Some(
                        opinions
                            .as_ref()
                            .map_or(0, |index| index.trust(policy.person, target))
                            >= policy.trade_trust_threshold,
                    )
                });
            if preferred.is_some_and(|open| open != body.trade_open) {
                return None;
            }
            Some(InstitutionMembershipRequest {
                body: body.id,
                applicant: policy.person,
                requested_at: tick,
                votes: vec![],
            })
        })
        .collect();
    requests.par_sort_unstable_by_key(|row| (row.body, row.applicant));
    requests
}

/// Publish requests only if the institution still has members at this boundary.
pub fn commit(
    institutions: &mut game_parallel::Institutions,
    requests: Vec<InstitutionMembershipRequest>,
) {
    if requests.is_empty() {
        return;
    }
    let active: HashSet<_> = institutions
        .bodies
        .iter()
        .filter(|body| !body.members.is_empty())
        .map(|body| body.id)
        .collect();
    let (mut accepted, mut rejected): (Vec<_>, Vec<_>) = requests
        .into_par_iter()
        .fold(
            || (Vec::new(), Vec::new()),
            |(mut accepted, mut rejected), request| {
                if active.contains(&request.body) {
                    accepted.push(request);
                } else {
                    rejected.push(game_parallel::InstitutionAdmissionEvent {
                        tick: request.requested_at,
                        body: request.body,
                        applicant: request.applicant,
                        accepted: false,
                        yes: 0,
                        no: 0,
                    });
                }
                (accepted, rejected)
            },
        )
        .reduce(
            || (Vec::new(), Vec::new()),
            |(mut accepted, mut rejected), (more_accepted, more_rejected)| {
                accepted.extend(more_accepted);
                rejected.extend(more_rejected);
                (accepted, rejected)
            },
        );
    accepted.par_sort_unstable_by_key(|row| (row.body, row.applicant));
    rejected.par_sort_unstable_by_key(|row| (row.body, row.applicant));
    institutions.requests.par_extend(accepted);
    institutions.admissions.par_extend(rejected);
}

#[cfg(test)]
#[path = "../tests/support/application.rs"]
mod tests;
