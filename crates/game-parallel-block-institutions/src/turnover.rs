//! Remove deceased members from offices and live civic work at a committed boundary.
use game_parallel::{InstitutionAction, InstitutionAdmissionEvent, InstitutionEvent, Institutions};
use rayon::prelude::*;
use std::collections::HashSet;

pub fn remove_dead(institutions: &mut Option<Institutions>, deaths: &[u64], tick: u64) {
    let Some(institutions) = institutions else {
        return;
    };
    if deaths.is_empty() {
        return;
    }
    let mut deaths = deaths.to_vec();
    deaths.par_sort_unstable();
    deaths.dedup();
    let vacant: HashSet<_> = institutions
        .bodies
        .par_iter_mut()
        .filter_map(|body| {
            let previous = body.members.len();
            if body
                .members
                .par_iter()
                .any(|person| deaths.binary_search(person).is_ok())
            {
                body.members = std::mem::take(&mut body.members)
                    .into_par_iter()
                    .filter(|person| deaths.binary_search(person).is_err())
                    .collect();
            }
            if body
                .chair
                .is_some_and(|person| deaths.binary_search(&person).is_ok())
            {
                body.chair = None;
            }
            (previous > 0 && body.members.is_empty()).then_some(body.id)
        })
        .collect();
    let (ballots, mut cancelled): (Vec<_>, Vec<_>) = std::mem::take(&mut institutions.ballots)
        .into_par_iter().fold(|| (Vec::new(), Vec::new()), |(mut ballots, mut events), mut ballot| {
            let cancel = deaths.binary_search(&ballot.proposer).is_ok()
                || matches!(ballot.action, InstitutionAction::Elect { candidate } if deaths.binary_search(&candidate).is_ok());
            if cancel {
                events.push(InstitutionEvent { tick, ballot: ballot.id, body: ballot.body, adopted: false });
            } else {
                ballot.votes.retain(|vote| deaths.binary_search(&vote.voter).is_err());
                ballots.push(ballot);
            }
            (ballots, events)
        }).reduce(|| (Vec::new(), Vec::new()), |(mut ballots, mut events), (more_ballots, more_events)| {
            ballots.extend(more_ballots);
            events.extend(more_events);
            (ballots, events)
        });
    institutions.ballots = ballots;
    institutions
        .ballots
        .par_sort_unstable_by_key(|ballot| ballot.id);
    cancelled.par_sort_unstable_by_key(|event| event.ballot);
    institutions.events.extend(cancelled);
    let (requests, mut rejected): (Vec<_>, Vec<_>) = std::mem::take(&mut institutions.requests)
        .into_par_iter()
        .fold(
            || (Vec::new(), Vec::new()),
            |(mut requests, mut events), request| {
                if vacant.contains(&request.body)
                    || deaths.binary_search(&request.applicant).is_ok()
                {
                    events.push(InstitutionAdmissionEvent {
                        tick,
                        body: request.body,
                        applicant: request.applicant,
                        accepted: false,
                        yes: 0,
                        no: 0,
                    });
                } else {
                    requests.push(request);
                }
                (requests, events)
            },
        )
        .reduce(
            || (Vec::new(), Vec::new()),
            |(mut requests, mut events), (more_requests, more_events)| {
                requests.extend(more_requests);
                events.extend(more_events);
                (requests, events)
            },
        );
    institutions.requests = requests;
    institutions.requests.par_iter_mut().for_each(|request| {
        request
            .votes
            .retain(|vote| deaths.binary_search(&vote.voter).is_err());
    });
    institutions.requests.par_sort_unstable_by_key(|request| {
        (request.body, request.requested_at, request.applicant)
    });
    rejected.par_sort_unstable_by_key(|event| (event.body, event.applicant));
    institutions.admissions.par_extend(rejected);
}

#[cfg(test)]
#[path = "../tests/support/turnover.rs"]
mod tests;

/// Publish voluntary departures from specific bodies without affecting other memberships.
pub fn remove_departures(institutions: &mut Institutions, departures: &[(u64, u64)], tick: u64) {
    if departures.is_empty() {
        return;
    }
    let mut departures = departures.to_vec();
    departures.par_sort_unstable();
    departures.dedup();
    let vacant: HashSet<_> = institutions
        .bodies
        .par_iter_mut()
        .filter_map(|body| {
            let start = departures.partition_point(|row| row.0 < body.id);
            let end = departures.partition_point(|row| row.0 <= body.id);
            if start == end {
                return None;
            }
            let prior = body.members.len();
            let gone = &departures[start..end];
            body.members = std::mem::take(&mut body.members)
                .into_par_iter()
                .filter(|person| gone.binary_search(&(body.id, *person)).is_err())
                .collect();
            if body
                .chair
                .is_some_and(|person| gone.binary_search(&(body.id, person)).is_ok())
            {
                body.chair = None;
            }
            (prior > 0 && body.members.is_empty()).then_some(body.id)
        })
        .collect();
    let (mut ballots, mut cancelled): (Vec<_>, Vec<_>) = std::mem::take(&mut institutions.ballots)
        .into_par_iter().fold(|| (Vec::new(), Vec::new()), |(mut ballots, mut events), mut ballot| {
            let cancel = departures.binary_search(&(ballot.body, ballot.proposer)).is_ok()
                || matches!(ballot.action, InstitutionAction::Elect { candidate } if departures.binary_search(&(ballot.body, candidate)).is_ok());
            if cancel {
                events.push(InstitutionEvent { tick, ballot: ballot.id, body: ballot.body, adopted: false });
            } else {
                ballot.votes.retain(|vote| departures.binary_search(&(ballot.body, vote.voter)).is_err());
                ballots.push(ballot);
            }
            (ballots, events)
        }).reduce(|| (Vec::new(), Vec::new()), |(mut ballots, mut events), (more_ballots, more_events)| {
            ballots.extend(more_ballots); events.extend(more_events); (ballots, events)
        });
    ballots.par_sort_unstable_by_key(|ballot| ballot.id);
    cancelled.par_sort_unstable_by_key(|event| event.ballot);
    institutions.ballots = ballots;
    institutions.events.extend(cancelled);
    let (mut requests, mut rejected): (Vec<_>, Vec<_>) = std::mem::take(&mut institutions.requests)
        .into_par_iter()
        .fold(
            || (Vec::new(), Vec::new()),
            |(mut requests, mut events), request| {
                if vacant.contains(&request.body) {
                    events.push(InstitutionAdmissionEvent {
                        tick,
                        body: request.body,
                        applicant: request.applicant,
                        accepted: false,
                        yes: 0,
                        no: 0,
                    });
                } else {
                    requests.push(request);
                }
                (requests, events)
            },
        )
        .reduce(
            || (Vec::new(), Vec::new()),
            |(mut requests, mut events), (more_requests, more_events)| {
                requests.extend(more_requests);
                events.extend(more_events);
                (requests, events)
            },
        );
    requests.par_sort_unstable_by_key(|request| {
        (request.body, request.requested_at, request.applicant)
    });
    rejected.par_sort_unstable_by_key(|event| (event.body, event.applicant));
    institutions.requests = requests;
    institutions.requests.par_iter_mut().for_each(|request| {
        request.votes.retain(|vote| {
            departures
                .binary_search(&(request.body, vote.voter))
                .is_err()
        });
    });
    institutions.admissions.par_extend(rejected);
}
