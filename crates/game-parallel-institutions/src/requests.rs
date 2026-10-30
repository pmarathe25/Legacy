//! Voluntary applications and their recorded institutional decisions.
use crate::Institutions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipRequest {
    pub body: u64,
    pub applicant: u64,
    pub requested_at: u64,
    #[serde(default)]
    pub votes: Vec<crate::Vote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionEvent {
    pub tick: u64,
    pub body: u64,
    pub applicant: u64,
    pub accepted: bool,
    #[serde(default)]
    pub yes: u64,
    #[serde(default)]
    pub no: u64,
}

impl Institutions {
    pub(crate) fn validate_requests(&self, people: &HashSet<u64>, now: u64) -> Result<(), String> {
        let bodies: std::collections::HashMap<_, _> =
            self.bodies.iter().map(|body| (body.id, body)).collect();
        let mut memberships: Vec<_> = if self.requests.is_empty() {
            Vec::new()
        } else {
            self.bodies
                .par_iter()
                .flat_map_iter(|body| body.members.iter().map(move |&person| (body.id, person)))
                .collect()
        };
        if memberships.par_windows(2).any(|pair| pair[0] > pair[1]) {
            memberships.par_sort_unstable();
        }
        let mut keys: Vec<_> = self
            .requests
            .par_iter()
            .map(|row| (row.body, row.applicant))
            .collect();
        keys.par_sort_unstable();
        if keys.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.requests.par_iter().any(|request| {
                let mut voters: Vec<_> = request.votes.iter().map(|vote| vote.voter).collect();
                voters.sort_unstable();
                request.requested_at > now
                    || !people.contains(&request.applicant)
                    || !bodies.contains_key(&request.body)
                    || memberships
                        .binary_search(&(request.body, request.applicant))
                        .is_ok()
                    || voters.windows(2).any(|pair| pair[0] == pair[1])
                    || voters
                        .iter()
                        .any(|&voter| memberships.binary_search(&(request.body, voter)).is_err())
            })
            || self.admissions.par_iter().any(|event| {
                event.tick > now
                    || !people.contains(&event.applicant)
                    || !bodies.contains_key(&event.body)
            })
        {
            return Err("invalid institutional membership request".into());
        }
        Ok(())
    }
}
