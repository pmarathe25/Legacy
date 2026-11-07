//! Personal statements about remembered events; belief is owned by the listener.
use crate::{Interpretation, Outcome, Taking};
use game_parallel_opinions::PersonalChange;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{cmp::Reverse, collections::HashSet};

const MAX_REPORTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportPolicy {
    pub person: u64,
    pub recipient: u64,
    pub actor: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    /// None repeats the memory; a selected outcome is a statement, not a fact.
    #[serde(default)]
    pub claimed_outcome: Option<Outcome>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Belief {
    #[default]
    Unassessed,
    Believes,
    Doubts,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    #[default]
    Unassessed,
    Believe,
    Doubt,
    TrustAtLeast(i32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reception {
    pub person: u64,
    #[serde(default)]
    pub stance: Stance,
    #[serde(default)]
    pub toward_actor: Interpretation,
    #[serde(default)]
    pub toward_target: Interpretation,
    #[serde(default)]
    pub doubted_toward_actor: Interpretation,
    #[serde(default)]
    pub doubted_toward_target: Interpretation,
    #[serde(default)]
    pub unassessed_toward_actor: Interpretation,
    #[serde(default)]
    pub unassessed_toward_target: Interpretation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub speaker: u64,
    pub recipient: u64,
    pub actor: u64,
    pub target: u64,
    pub good: u32,
    pub quantity: u64,
    pub observed_at: u64,
    pub received_at: u64,
    pub claimed_outcome: Outcome,
    pub belief: Belief,
}

pub fn valid(owner: &Taking, people: &HashSet<u64>, goods: &HashSet<u32>, now: u64) -> bool {
    let mut policies: Vec<_> = owner
        .report_policies
        .par_iter()
        .map(|p| (p.person, p.recipient, p.actor))
        .collect();
    policies.par_sort_unstable();
    let mut receptions: Vec<_> = owner.receptions.par_iter().map(|p| p.person).collect();
    receptions.par_sort_unstable();
    let mut reports: Vec<_> = owner
        .reports
        .par_iter()
        .map(|r| (r.recipient, r.speaker, r.actor))
        .collect();
    reports.par_sort_unstable();
    !policies.par_windows(2).any(|p| p[0] == p[1])
        && !receptions.par_windows(2).any(|p| p[0] == p[1])
        && !reports.par_windows(2).any(|p| p[0] == p[1])
        && !owner.report_policies.par_iter().any(|p| {
            !people.contains(&p.person)
                || !people.contains(&p.recipient)
                || !people.contains(&p.actor)
                || p.person == p.recipient
                || p.pressure_threshold == Some(0)
        })
        && !owner
            .receptions
            .par_iter()
            .any(|p| !people.contains(&p.person))
        && !owner.reports.par_iter().any(|r| {
            !people.contains(&r.speaker)
                || !people.contains(&r.recipient)
                || !people.contains(&r.actor)
                || !people.contains(&r.target)
                || !goods.contains(&r.good)
                || r.speaker == r.recipient
                || r.actor == r.target
                || r.quantity == 0
                || r.observed_at > r.received_at
                || r.received_at > now
        })
        && !reports
            .par_chunk_by(|a, b| a.0 == b.0)
            .any(|group| group.len() > MAX_REPORTS)
}

impl Taking {
    /// Share only a previously remembered event during an actual encounter.
    /// Feelings returns (listener's trust, speaker's fear, speaker's obligation).
    pub fn receive_reports(
        &mut self,
        previous: &Taking,
        contacts: &[(u64, u64)],
        tick: u64,
        feelings: &(impl Fn(u64, u64) -> (i32, u64, u64) + Sync),
    ) -> Vec<PersonalChange> {
        if previous.report_policies.is_empty() || contacts.is_empty() {
            return Vec::new();
        }
        let mut contacts: Vec<_> = contacts.par_iter().copied().collect();
        contacts.par_iter_mut().for_each(|(a, b)| {
            if *a > *b {
                std::mem::swap(a, b);
            }
        });
        contacts.par_sort_unstable();
        let contact_base = contacts.first().map(|pair| pair.0).filter(|_| {
            contacts
                .par_windows(2)
                .all(|p| p[0].0.checked_add(1) == Some(p[1].0))
        });
        let mut witnesses: Vec<_> = previous.witnesses.par_iter().collect();
        witnesses.par_sort_unstable_by_key(|w| (w.person, w.actor));
        let witness_base = witnesses.first().map(|w| w.person).filter(|_| {
            witnesses
                .par_windows(2)
                .all(|p| p[0].person.checked_add(1) == Some(p[1].person))
        });
        let mut receptions: Vec<_> = previous.receptions.par_iter().collect();
        receptions.par_sort_unstable_by_key(|r| r.person);
        let mut prior: Vec<_> = previous.reports.par_iter().collect();
        prior.par_sort_unstable_by_key(|r| (r.recipient, r.speaker, r.actor));
        let incoming: Vec<_> = previous
            .report_policies
            .par_iter()
            .filter_map(|policy| {
                let pair = (
                    policy.person.min(policy.recipient),
                    policy.person.max(policy.recipient),
                );
                let contact_found = contact_base
                    .and_then(|base| policy.person.checked_sub(base))
                    .and_then(|offset| usize::try_from(offset).ok())
                    .and_then(|index| contacts.get(index))
                    .is_some_and(|contact| *contact == pair)
                    || contacts.binary_search(&pair).is_ok();
                if !contact_found {
                    return None;
                }
                let (trust, fear, obligation) = feelings(policy.recipient, policy.person);
                if !policy.willing
                    && !policy
                        .pressure_threshold
                        .is_some_and(|v| fear.saturating_add(obligation) >= v)
                {
                    return None;
                }
                let witness = witness_base
                    .and_then(|base| policy.person.checked_sub(base))
                    .and_then(|offset| usize::try_from(offset).ok())
                    .and_then(|index| witnesses.get(index).copied())
                    .filter(|w| (w.person, w.actor) == (policy.person, policy.actor))
                    .or_else(|| {
                        witnesses
                            .binary_search_by_key(&(policy.person, policy.actor), |w| {
                                (w.person, w.actor)
                            })
                            .ok()
                            .map(|i| witnesses[i])
                    })?;
                let seen = witness.last_seen.filter(|seen| seen.tick < tick)?;
                let claimed_outcome = policy.claimed_outcome.unwrap_or(seen.outcome);
                let key = (policy.recipient, policy.person, policy.actor);
                if prior
                    .binary_search_by_key(&key, |r| (r.recipient, r.speaker, r.actor))
                    .ok()
                    .is_some_and(|i| {
                        let old = prior[i];
                        (
                            old.target,
                            old.good,
                            old.quantity,
                            old.observed_at,
                            old.claimed_outcome,
                        ) == (
                            seen.target,
                            seen.good,
                            seen.quantity,
                            seen.tick,
                            claimed_outcome,
                        )
                    })
                {
                    return None;
                }
                let stance = receptions
                    .binary_search_by_key(&policy.recipient, |r| r.person)
                    .ok()
                    .map_or(Stance::Unassessed, |i| receptions[i].stance);
                let belief = match stance {
                    Stance::Unassessed => Belief::Unassessed,
                    Stance::Believe => Belief::Believes,
                    Stance::Doubt => Belief::Doubts,
                    Stance::TrustAtLeast(v) => {
                        if trust >= v {
                            Belief::Believes
                        } else {
                            Belief::Doubts
                        }
                    }
                };
                Some(Report {
                    speaker: policy.person,
                    recipient: policy.recipient,
                    actor: policy.actor,
                    target: seen.target,
                    good: seen.good,
                    quantity: seen.quantity,
                    observed_at: seen.tick,
                    received_at: tick,
                    claimed_outcome,
                    belief,
                })
            })
            .collect();
        if incoming.is_empty() {
            return Vec::new();
        }
        let mut combined: Vec<_> = previous
            .reports
            .par_iter()
            .cloned()
            .chain(incoming.into_par_iter())
            .collect();
        combined.par_sort_unstable_by_key(|r| {
            (r.recipient, r.speaker, r.actor, Reverse(r.received_at))
        });
        let mut latest: Vec<_> = if combined.par_windows(2).all(|p| {
            (p[0].recipient, p[0].speaker, p[0].actor) != (p[1].recipient, p[1].speaker, p[1].actor)
        }) {
            combined
        } else {
            combined
                .par_chunk_by(|a, b| {
                    (a.recipient, a.speaker, a.actor) == (b.recipient, b.speaker, b.actor)
                })
                .map(|group| group[0].clone())
                .collect()
        };
        latest.par_sort_unstable_by_key(|r| {
            (r.recipient, Reverse(r.received_at), r.speaker, r.actor)
        });
        let mut admitted: Vec<_> = if latest
            .par_windows(2)
            .all(|p| p[0].recipient != p[1].recipient)
        {
            latest
        } else {
            latest
                .par_chunk_by(|a, b| a.recipient == b.recipient)
                .flat_map_iter(|group| group.iter().take(MAX_REPORTS).cloned())
                .collect()
        };
        let changes = if receptions.is_empty() {
            Vec::new()
        } else {
            admitted
                .par_iter()
                .filter(|r| r.received_at == tick)
                .flat_map_iter(|report| {
                    let rule = receptions
                        .binary_search_by_key(&report.recipient, |r| r.person)
                        .ok()
                        .map(|i| receptions[i]);
                    let interpretations = rule.map(|r| match report.belief {
                        Belief::Believes => (r.toward_actor, r.toward_target),
                        Belief::Doubts => (r.doubted_toward_actor, r.doubted_toward_target),
                        Belief::Unassessed => {
                            (r.unassessed_toward_actor, r.unassessed_toward_target)
                        }
                    });
                    let make =
                        |target, value: Option<game_parallel_opinions::SubjectiveReaction>| {
                            value.map(|v| PersonalChange {
                                observer: report.recipient,
                                target,
                                trust: v.trust,
                                affinity: v.affinity,
                                fear: v.fear,
                                obligation: v.obligation,
                            })
                        };
                    [
                        make(
                            report.actor,
                            interpretations
                                .and_then(|(actor, _)| actor.get(report.claimed_outcome)),
                        ),
                        make(
                            report.target,
                            interpretations
                                .and_then(|(_, target)| target.get(report.claimed_outcome)),
                        ),
                    ]
                    .into_iter()
                    .flatten()
                })
                .collect()
        };
        admitted.par_sort_unstable_by_key(|r| (r.recipient, r.speaker, r.actor));
        self.reports = admitted.into();
        changes
    }
}
