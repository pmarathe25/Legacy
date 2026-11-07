//! A worker's bounded, subjective expectations about terms they actually heard.
use super::{GiftObservation, Proposal};
use crate::{Decision, MAX_PROPOSALS, Practices};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub wait_ticks: u64,
    #[serde(default)]
    pub missing_trust: i32,
    #[serde(default)]
    pub missing_affinity: i32,
    /// Signed personal reactions after the full quantity was received.
    #[serde(default)]
    pub received_trust: i32,
    #[serde(default)]
    pub received_affinity: i32,
    /// Optional overrides when some, but not all, of the expected quantity arrived.
    #[serde(default)]
    pub partial_trust: Option<i32>,
    #[serde(default)]
    pub partial_affinity: Option<i32>,
    #[serde(default)]
    pub missing_fear: i64,
    #[serde(default)]
    pub missing_obligation: i64,
    #[serde(default)]
    pub received_fear: i64,
    #[serde(default)]
    pub received_obligation: i64,
    #[serde(default)]
    pub partial_fear: Option<i64>,
    #[serde(default)]
    pub partial_obligation: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Experience {
    pub proposal: Proposal,
    pub request: u64,
    pub job: u64,
    pub delivered: u64,
    pub started: u64,
    pub completed: Option<u64>,
    pub received: u64,
    pub assessed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    pub requester: u64,
    pub worker: u64,
    pub recipe: u32,
    pub request: u64,
    pub job: u64,
}

/// This person's interpretation, not an objective default or proof of intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reaction {
    pub observer: u64,
    pub target: u64,
    pub trust: i32,
    pub affinity: i32,
    pub fear: i64,
    pub obligation: i64,
}

/// Observe started and finished jobs and directly received gifts. Choices read
/// prior personal knowledge; the caller publishes this detached generation atomically.
pub fn advance(
    prior: &Practices,
    next: &mut Practices,
    starts: &[Start],
    finished: &[u64],
    failed: &[u64],
    gifts: &[GiftObservation],
    tick: u64,
) -> Vec<Reaction> {
    if starts.is_empty()
        && finished.is_empty()
        && failed.is_empty()
        && gifts.is_empty()
        && !prior.minds.par_iter().any(|person| {
            person.work.as_ref().is_some_and(|work| {
                work.experiences.iter().any(|experience| {
                    !experience.assessed
                        && experience.completed.is_some_and(|completed| {
                            experience.received >= experience.proposal.quantity
                                || (tick > experience.started
                                    && work.outcome_policy.is_some_and(|policy| {
                                        tick.saturating_sub(completed) >= policy.wait_ticks
                                    }))
                        })
                })
            })
        })
    {
        return Vec::new();
    }
    let mut sorted_starts;
    let starts = if starts.par_windows(2).any(|pair| {
        (pair[0].worker, pair[0].request, pair[0].job)
            > (pair[1].worker, pair[1].request, pair[1].job)
    }) {
        sorted_starts = starts.par_iter().copied().collect::<Vec<_>>();
        sorted_starts.par_sort_unstable_by_key(|row| (row.worker, row.request, row.job));
        sorted_starts.as_slice()
    } else {
        starts
    };
    let mut sorted_finished;
    let finished = if finished.par_windows(2).any(|pair| pair[0] > pair[1]) {
        sorted_finished = finished.par_iter().copied().collect::<Vec<_>>();
        sorted_finished.par_sort_unstable();
        sorted_finished.as_slice()
    } else {
        finished
    };
    let mut sorted_failed;
    let failed = if failed.par_windows(2).any(|pair| pair[0] > pair[1]) {
        sorted_failed = failed.par_iter().copied().collect::<Vec<_>>();
        sorted_failed.par_sort_unstable();
        sorted_failed.as_slice()
    } else {
        failed
    };

    let mut gifts: Vec<_> = gifts
        .par_iter()
        .filter(|row| !row.gift.given)
        .copied()
        .collect();
    gifts.par_sort_unstable_by_key(|row| {
        (
            row.person,
            row.action,
            row.gift.other,
            row.gift.good,
            row.gift.quantity,
        )
    });
    let gifts: Vec<_> = gifts
        .par_chunk_by(|a, b| (a.person, a.action) == (b.person, b.action))
        .map(|rows| rows[0])
        .collect();
    let aligned = !starts.is_empty()
        && prior.minds.len() == next.minds.len()
        && prior
            .minds
            .par_iter()
            .zip(next.minds.par_iter())
            .all(|(a, b)| a.person == b.person);
    let mut old: Vec<_> = if starts.is_empty() || aligned {
        Vec::new()
    } else {
        prior.minds.par_iter().collect()
    };
    if old
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        old.par_sort_unstable_by_key(|person| person.person);
    }
    next.minds
        .par_iter_mut()
        .enumerate()
        .flat_map_iter(|(index, person)| {
            let Some(existing) = person.work.as_ref() else {
                return Vec::new();
            };
            let start = starts
                .binary_search_by_key(&person.person, |row| row.worker)
                .ok()
                .map(|i| starts[i]);
            let proposed = start.and_then(|start| {
                let old_work = if aligned {
                    prior.minds[index].work.as_ref()
                } else {
                    old.binary_search_by_key(&person.person, |row| row.person)
                        .ok()
                        .and_then(|i| old[i].work.as_ref())
                }?;
                old_work.outcome_policy?;
                if existing.experiences.iter().any(|experience| {
                    experience.request == start.request || experience.job == start.job
                }) {
                    return None;
                }
                let received = old_work
                    .inbox
                    .iter()
                    .filter(|received| {
                        received.decision == Decision::Adopted
                            && received.delivered < tick
                            && received.proposal.expires >= tick
                            && received.proposal.author == start.requester
                            && received.proposal.worker == start.worker
                            && received.proposal.recipe == start.recipe
                    })
                    .max_by_key(|received| (received.delivered, received.proposal.sequence))?;
                Some(Experience {
                    proposal: received.proposal.clone(),
                    request: start.request,
                    job: start.job,
                    delivered: received.delivered,
                    started: tick,
                    completed: None,
                    received: 0,
                    assessed: false,
                })
            });
            let g0 = gifts.partition_point(|row| row.person < person.person);
            let g1 = gifts.partition_point(|row| row.person <= person.person);
            let needs_update = proposed.is_some()
                || existing.experiences.iter().any(|experience| {
                    (experience.completed.is_none()
                        && finished.binary_search(&experience.job).is_ok())
                        || (experience.completed.is_none()
                            && failed.binary_search(&experience.job).is_ok())
                        || (!experience.assessed
                            && experience.completed.is_some()
                            && experience.received >= experience.proposal.quantity)
                        || (!experience.assessed
                            && tick > experience.started
                            && existing.outcome_policy.is_some_and(|policy| {
                                experience
                                    .completed
                                    .is_some_and(|at| tick.saturating_sub(at) >= policy.wait_ticks)
                            }))
                        || (!experience.assessed
                            && experience.started < tick
                            && gifts[g0..g1].iter().any(|row| {
                                row.gift.other == experience.proposal.author
                                    && row.gift.good == experience.proposal.good
                            }))
                });
            if !needs_update {
                return Vec::new();
            }
            let work = std::sync::Arc::make_mut(person.work.as_mut().expect("work mind"));
            if let Some(experience) = proposed {
                work.experiences.push(experience);
                work.experiences
                    .sort_unstable_by_key(|row| (row.started, row.request, row.job));
                if work.experiences.len() > MAX_PROPOSALS {
                    work.experiences.remove(0);
                }
            }
            work.experiences.retain(|experience| {
                experience.completed.is_some() || failed.binary_search(&experience.job).is_err()
            });
            for experience in &mut work.experiences {
                if experience.completed.is_none() && finished.binary_search(&experience.job).is_ok()
                {
                    experience.completed = Some(tick);
                }
            }
            for gift in &gifts[g0..g1] {
                let mut remaining = gift.gift.quantity;
                for experience in &mut work.experiences {
                    if remaining == 0 {
                        break;
                    }
                    if experience.assessed
                        || experience.started >= tick
                        || experience.proposal.author != gift.gift.other
                        || experience.proposal.good != gift.gift.good
                    {
                        continue;
                    }
                    let amount = remaining.min(
                        experience
                            .proposal
                            .quantity
                            .saturating_sub(experience.received),
                    );
                    experience.received += amount;
                    remaining -= amount;
                }
            }
            let mut reactions = Vec::new();
            for experience in &mut work.experiences {
                let Some(completed) = experience.completed else {
                    continue;
                };
                if experience.assessed {
                    continue;
                }
                let reaction = if experience.received >= experience.proposal.quantity {
                    experience.assessed = true;
                    work.outcome_policy.map(|policy| {
                        (
                            policy.received_trust,
                            policy.received_affinity,
                            policy.received_fear,
                            policy.received_obligation,
                        )
                    })
                } else if let Some(policy) = work.outcome_policy.filter(|policy| {
                    tick > experience.started && tick.saturating_sub(completed) >= policy.wait_ticks
                }) {
                    experience.assessed = true;
                    Some(if experience.received > 0 {
                        (
                            policy.partial_trust.unwrap_or(policy.missing_trust),
                            policy.partial_affinity.unwrap_or(policy.missing_affinity),
                            policy.partial_fear.unwrap_or(policy.missing_fear),
                            policy
                                .partial_obligation
                                .unwrap_or(policy.missing_obligation),
                        )
                    } else {
                        (
                            policy.missing_trust,
                            policy.missing_affinity,
                            policy.missing_fear,
                            policy.missing_obligation,
                        )
                    })
                } else {
                    None
                };
                if let Some((trust, affinity, fear, obligation)) =
                    reaction.filter(|&(trust, affinity, fear, obligation)| {
                        trust != 0 || affinity != 0 || fear != 0 || obligation != 0
                    })
                {
                    reactions.push(Reaction {
                        observer: person.person,
                        target: experience.proposal.author,
                        trust,
                        affinity,
                        fear,
                        obligation,
                    });
                }
            }
            reactions
        })
        .collect()
}
