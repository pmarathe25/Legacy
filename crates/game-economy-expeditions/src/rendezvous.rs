//! Independent departure choices sharing a route and a meeting place.
use game_economy_person::travel::{Departure, DepartureOutcome};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rendezvous {
    pub group: u64,
    pub minimum_companions: u32,
    pub wait_until_millis: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RouteGroup {
    pub group: u64,
    pub source: u64,
    pub first_leg: u64,
    pub destination: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub departure: Departure,
    pub(crate) group: Option<RouteGroup>,
    pub(crate) minimum_companions: u32,
}

pub(crate) fn admit(mut proposals: Vec<Proposal>) -> Vec<Proposal> {
    if proposals.par_iter().any(|row| row.group.is_some()) {
        proposals.par_sort_unstable_by_key(|row| (row.group, row.minimum_companions));
        proposals = proposals
            .par_chunk_by(|a, b| a.group == b.group)
            .flat_map(|group| {
                let count = cohort_size(group);
                group[..count].par_iter().copied()
            })
            .collect();
    }
    proposals.par_sort_unstable_by_key(|row| row.departure.person);
    proposals
}

fn cohort_size(group: &[Proposal]) -> usize {
    if group[0].group.is_none() {
        return group.len();
    }
    // Proposals are sorted by the number of companions each person requires.
    // A prefix is viable when even its most selective member would depart.
    let satisfied = |(index, proposal): (usize, &Proposal)| {
        u64::from(proposal.minimum_companions) <= index as u64
    };
    let last = if group.len() > 1024 {
        group.par_iter().enumerate().position_last(satisfied)
    } else {
        group.iter().enumerate().rposition(satisfied)
    };
    last.map_or(0, |index| index + 1)
}

pub(crate) fn companions(
    proposals: &[Proposal],
    outcomes: &[DepartureOutcome],
) -> Vec<(u64, u64, u64)> {
    let mut traveled: Vec<_> = proposals
        .par_iter()
        .filter_map(|row| {
            let group = row.group?;
            let person = row.departure.person;
            let outcome = outcomes
                .binary_search_by_key(&person, |outcome| outcome.person)
                .ok()?;
            outcomes[outcome].departed.then_some((group, person))
        })
        .collect();
    traveled.par_sort_unstable();
    let mut together: Vec<_> = traveled
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter(|group| group.len() > 1)
        .flat_map(|group| {
            let count = (group.len() - 1) as u64;
            group
                .par_iter()
                .map(move |(key, person)| (*person, key.group, count))
        })
        .collect();
    together.par_sort_unstable_by_key(|row| row.0);
    together
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_and_concentrated_rendezvous_have_identical_admission_across_workers() {
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut proposals: Vec<_> = (1..=12288)
                        .map(|person| {
                            let group = if person <= 8192 {
                                1
                            } else {
                                2 + (person - 8193) / 4
                            };
                            let minimum_companions = if person > 4096 && person <= 8192 {
                                8192
                            } else {
                                3
                            };
                            Proposal {
                                departure: Departure {
                                    person,
                                    destination: 2,
                                },
                                group: Some(RouteGroup {
                                    group,
                                    source: 1,
                                    first_leg: 2,
                                    destination: 3,
                                }),
                                minimum_companions,
                            }
                        })
                        .collect();
                    proposals.reverse();
                    let admitted = admit(proposals);
                    assert_eq!(admitted.len(), 8192);
                    assert_eq!(admitted[4095].departure.person, 4096);
                    assert_eq!(admitted[4096].departure.person, 8193);
                    admitted
                })
        };
        assert_eq!(run(1), run(6));
    }
}
