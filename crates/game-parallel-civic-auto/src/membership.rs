//! Local members evaluate applications using their own recorded trust.
use crate::CivicAuto;
use game_parallel_institutions::{AdmissionEvent, Institutions};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::HashMap;

pub struct AdmissionDraft {
    pub actors: Vec<u64>,
    decisions: Vec<AdmissionEvent>,
}

impl AdmissionDraft {
    /// Consume a prepared decision batch for publication by the institution owner.
    pub fn into_decisions(self) -> Vec<AdmissionEvent> {
        self.decisions
    }
}

impl CivicAuto {
    /// Allocate disjoint groups of available members to simultaneous applications.
    /// Each group meets quorum and its members vote at most once in this tick.
    pub fn admissions(
        &self,
        institutions: &Institutions,
        opinions: Option<&Opinions>,
        people: &[PersonSnapshot],
        busy: &[u64],
        tick: u64,
    ) -> Option<AdmissionDraft> {
        if institutions.requests.is_empty() {
            return None;
        }
        let bodies: HashMap<_, _> = institutions
            .bodies
            .iter()
            .map(|body| (body.id, body))
            .collect();
        let mut policies: Vec<_> = self
            .policies
            .par_iter()
            .filter(|row| row.enabled && row.minimum_candidate_trust.is_some())
            .collect();
        if policies
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            policies.par_sort_unstable_by_key(|row| row.person);
        }
        let dense_policy = policies.first().and_then(|first| {
            (policies.last()?.person.checked_sub(first.person) == Some((policies.len() - 1) as u64))
                .then_some(first.person)
        });
        let mut busy = busy.to_vec();
        if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
            busy.par_sort_unstable();
        }
        let opinions = opinions.map(Opinions::index);
        let mut requests: Vec<_> = institutions.requests.par_iter().collect();
        if requests.par_windows(2).any(|pair| {
            (pair[0].body, pair[0].requested_at, pair[0].applicant)
                > (pair[1].body, pair[1].requested_at, pair[1].applicant)
        }) {
            requests.par_sort_unstable_by_key(|row| (row.body, row.requested_at, row.applicant));
        }
        let batches: Vec<_> = requests
            .par_chunk_by(|a, b| a.body == b.body)
            .filter_map(|group| {
                let body = bodies.get(&group[0].body)?;
                // A reduced body can recruit enough people to restore its normal quorum.
                let quorum = (body.quorum as usize).min(body.members.len());
                if quorum == 0 {
                    return None;
                }
                let candidates: Vec<_> = group
                    .par_iter()
                    .filter(|request| {
                        person_index(people, request.applicant).is_some_and(|index| {
                            people[index].2 > 0 && people[index].1 == Some(body.region)
                        })
                    })
                    .copied()
                    .collect();
                let mut voters: Vec<_> = body
                    .members
                    .par_iter()
                    .filter_map(|&person| {
                        let index = dense_policy
                            .and_then(|first| person.checked_sub(first))
                            .and_then(|offset| usize::try_from(offset).ok())
                            .filter(|&index| index < policies.len())
                            .or_else(|| {
                                dense_policy
                                    .is_none()
                                    .then(|| {
                                        policies
                                            .binary_search_by_key(&person, |row| row.person)
                                            .ok()
                                    })
                                    .flatten()
                            })?;
                        let policy = policies[index];
                        let (_, region, health, _) =
                            people[*person_index(people, person).as_ref()?];
                        if health == 0
                            || region != Some(body.region)
                            || busy.binary_search(&person).is_ok()
                        {
                            return None;
                        }
                        Some((person, policy.minimum_candidate_trust?))
                    })
                    .collect();
                if voters.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
                    voters.par_sort_unstable_by_key(|row| row.0);
                }
                if candidates
                    .par_iter()
                    .all(|request| request.votes.is_empty())
                {
                    let count = candidates.len().min(voters.len() / quorum);
                    if count == 0 {
                        return None;
                    }
                    let base = voters.len() / count;
                    let extra = voters.len() % count;
                    let decisions = candidates[..count]
                        .par_iter()
                        .enumerate()
                        .map(|(i, request)| {
                            let start = i * base + i.min(extra);
                            let assigned = &voters[start..start + base + usize::from(i < extra)];
                            let supports = |&&(person, threshold): &&(u64, i32)| {
                                opinions
                                    .as_ref()
                                    .map_or(0, |index| index.trust(person, request.applicant))
                                    >= threshold
                            };
                            let yes = if assigned.len() < 256 {
                                assigned.iter().filter(supports).count()
                            } else {
                                assigned.par_iter().filter(supports).count()
                            };
                            AdmissionEvent {
                                tick,
                                body: body.id,
                                applicant: request.applicant,
                                accepted: yes > assigned.len() - yes,
                                yes: yes as u64,
                                no: (assigned.len() - yes) as u64,
                            }
                        })
                        .collect();
                    let actors = voters.into_par_iter().map(|row| row.0).collect();
                    return Some((decisions, actors));
                }
                let candidates: Vec<_> = candidates
                    .into_par_iter()
                    .map(|request| {
                        let mut previous: Vec<_> = request
                            .votes
                            .iter()
                            .map(|vote| (vote.voter, vote.yes))
                            .collect();
                        if previous.windows(2).any(|pair| pair[0].0 > pair[1].0) {
                            previous.sort_unstable_by_key(|row| row.0);
                        }
                        (request, previous)
                    })
                    .collect();
                let completed: Vec<_> = candidates
                    .par_iter()
                    .filter(|(_, votes)| votes.len() >= quorum)
                    .map(|(request, votes)| {
                        let yes = votes.iter().filter(|vote| vote.1).count();
                        AdmissionEvent {
                            tick,
                            body: body.id,
                            applicant: request.applicant,
                            accepted: yes > votes.len() - yes,
                            yes: yes as u64,
                            no: (votes.len() - yes) as u64,
                        }
                    })
                    .collect();
                let pending: Vec<_> = candidates
                    .par_iter()
                    .filter(|(_, votes)| votes.len() < quorum)
                    .collect();
                let capacity = voters.len() / quorum;
                let count = if capacity > 0 {
                    pending.len().min(capacity)
                } else if pending.first().is_some_and(|(_, votes)| !votes.is_empty()) {
                    1
                } else {
                    0
                };
                if count == 0 && completed.is_empty() {
                    return None;
                }
                if count == 0 {
                    return Some((completed, Vec::new()));
                }
                let base = voters.len() / count;
                let extra = voters.len() % count;
                let decisions: Vec<_> = pending[..count]
                    .par_iter()
                    .enumerate()
                    .map(|(i, (request, previous))| {
                        let start = i * base + i.min(extra);
                        let end = start + base + usize::from(i < extra);
                        let (new_count, new_yes) = voters[start..end]
                            .par_iter()
                            .filter_map(|&(person, threshold)| {
                                if previous.binary_search_by_key(&person, |row| row.0).is_ok() {
                                    return None;
                                }
                                let yes = opinions
                                    .as_ref()
                                    .map_or(0, |index| index.trust(person, request.applicant))
                                    >= threshold;
                                Some((1usize, usize::from(yes)))
                            })
                            .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
                        let total = previous.len() + new_count;
                        if total < quorum {
                            return None;
                        }
                        let yes = previous.iter().filter(|vote| vote.1).count() + new_yes;
                        Some(AdmissionEvent {
                            tick,
                            body: body.id,
                            applicant: request.applicant,
                            accepted: yes > total - yes,
                            yes: yes as u64,
                            no: (total - yes) as u64,
                        })
                    })
                    .collect();
                let split = (base + 1) * extra;
                let actors: Vec<_> = voters
                    .par_iter()
                    .enumerate()
                    .filter_map(|(i, &(person, _))| {
                        let group = if i < split {
                            i / (base + 1)
                        } else {
                            extra + (i - split) / base
                        };
                        let (_, previous) = pending[group];
                        (decisions[group].is_some()
                            && previous.binary_search_by_key(&person, |row| row.0).is_err())
                        .then_some(person)
                    })
                    .collect();
                let mut result = completed;
                result.par_extend(decisions.into_par_iter().flatten());
                if result.is_empty() {
                    None
                } else {
                    Some((result, actors))
                }
            })
            .collect();
        if batches.is_empty() {
            return None;
        }
        let mut decisions: Vec<_> = batches
            .par_iter()
            .flat_map(|batch| batch.0.par_iter().cloned())
            .collect();
        decisions.par_sort_unstable_by_key(|row| (row.body, row.applicant));
        let actors = batches
            .into_par_iter()
            .flat_map(|batch| batch.1.into_par_iter())
            .collect();
        Some(AdmissionDraft { actors, decisions })
    }
}
