//! Personal statements about remembered events; belief is owned by the listener.
use crate::{Interpretation, Outcome, Taking};
use game_opinions::{PersonalChange, SubjectiveReaction};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{cmp::Reverse, collections::HashSet};

const MAX_REPORTS: usize = 64;

fn same_statement(a: &Report, b: &Report) -> bool {
    (
        a.target,
        a.good,
        a.quantity,
        a.observed_at,
        a.claimed_outcome,
        a.source_speaker,
    ) == (
        b.target,
        b.good,
        b.quantity,
        b.observed_at,
        b.claimed_outcome,
        b.source_speaker,
    )
}

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
    #[serde(default)]
    pub source: ReportSource,
    /// Optional immediate speaker to select when forwarding a received account.
    #[serde(default)]
    pub heard_from: Option<u64>,
    /// Optional changes to the account, whether observed or relayed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<ReportDetails>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportDetails {
    #[serde(default)]
    pub target: Option<u64>,
    #[serde(default)]
    pub good: Option<u32>,
    #[serde(default)]
    pub quantity: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportSource {
    #[default]
    Observed,
    Heard,
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
    /// This listener's reaction to a statement matching their own memory.
    #[serde(default)]
    pub matching_toward_speaker: Option<SubjectiveReaction>,
    /// This listener's reaction when the statement differs from their memory.
    #[serde(default)]
    pub different_toward_speaker: Option<SubjectiveReaction>,
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
    /// Immediate source of a relayed statement; absent for a direct observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_speaker: Option<u64>,
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
        .iter()
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
                || p.heard_from
                    .is_some_and(|id| id == p.person || !people.contains(&id))
                || (p.source == ReportSource::Observed && p.heard_from.is_some())
                || p.details.as_ref().is_some_and(|d| {
                    d.target
                        .is_some_and(|id| id == p.actor || !people.contains(&id))
                        || d.good.is_some_and(|id| !goods.contains(&id))
                        || d.quantity == Some(0)
                })
                || p.pressure_threshold == Some(0)
        })
        && !owner
            .receptions
            .par_iter()
            .any(|p| !people.contains(&p.person))
        && !owner.reports.iter().any(|r| {
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
                || r.source_speaker
                    .is_some_and(|id| id == r.speaker || !people.contains(&id))
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
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("taking.reports.index");
        let ordered = (contacts.par_iter().any(|(a, b)| a > b)
            || contacts.par_windows(2).any(|pair| pair[0] > pair[1]))
        .then(|| {
            let mut rows: Vec<_> = contacts.par_iter().copied().collect();
            rows.par_iter_mut().for_each(|(a, b)| {
                if *a > *b {
                    std::mem::swap(a, b);
                }
            });
            rows.par_sort_unstable();
            rows
        });
        let contacts = ordered.as_deref().unwrap_or(contacts);
        let contact_base = contacts.first().map(|pair| pair.0).filter(|_| {
            contacts
                .par_windows(2)
                .all(|p| p[0].0.checked_add(1) == Some(p[1].0))
        });
        let mut witnesses: Vec<_> = if previous
            .report_policies
            .par_iter()
            .any(|p| p.source == ReportSource::Observed)
            || previous.receptions.par_iter().any(|r| {
                r.matching_toward_speaker.is_some() || r.different_toward_speaker.is_some()
            }) {
            previous.witnesses.par_iter().collect()
        } else {
            Vec::new()
        };
        witnesses.par_sort_unstable_by_key(|w| (w.person, w.actor));
        let witness_base = witnesses.first().map(|w| w.person).filter(|_| {
            witnesses
                .par_windows(2)
                .all(|p| p[0].person.checked_add(1) == Some(p[1].person))
        });
        let witness_for = |person: u64, actor: u64| {
            witness_base
                .and_then(|base| person.checked_sub(base))
                .and_then(|offset| usize::try_from(offset).ok())
                .and_then(|index| witnesses.get(index).copied())
                .filter(|w| (w.person, w.actor) == (person, actor))
                .or_else(|| {
                    witnesses
                        .binary_search_by_key(&(person, actor), |w| (w.person, w.actor))
                        .ok()
                        .map(|index| witnesses[index])
                })
        };
        let mut receptions: Vec<_> = previous.receptions.par_iter().collect();
        receptions.par_sort_unstable_by_key(|r| r.person);
        let dense = (previous.report_policies.len() >= 4096
            && previous.report_policies.len().saturating_mul(8) >= previous.reports.len())
        .then(|| previous.reports.dense_lookup())
        .flatten();
        let reports_for = |person| {
            dense
                .as_ref()
                .map_or_else(|| previous.reports.get(person), |index| index.get(person))
        };
        // Accepted inquiries can request a fresh statement even when an
        // identical account was heard earlier. Build keyed lookups once;
        // never search all inquiries for every reporting policy.
        let mut inquiry_keys: Vec<_> = previous
            .complaints
            .par_iter()
            .filter(|c| {
                c.decision == crate::ComplaintDecision::Accepted
                    && c.answer.is_none()
                    && tick < c.expires_at
                    && c.decided_at.is_some_and(|at| at < tick)
            })
            .map(|c| {
                (
                    c.recipient,
                    c.witness,
                    c.account.actor,
                    c.account.observed_at,
                )
            })
            .collect();
        inquiry_keys.par_sort_unstable();
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("taking.reports.select");
        }
        let make_report = |policy: &ReportPolicy| {
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
            let (target, good, quantity, observed_at, outcome, source_speaker) = match policy.source
            {
                ReportSource::Observed => {
                    let witness = witness_for(policy.person, policy.actor)?;
                    let seen = witness.last_seen.filter(|seen| seen.tick < tick)?;
                    (
                        seen.target,
                        seen.good,
                        seen.quantity,
                        seen.tick,
                        seen.outcome,
                        None,
                    )
                }
                ReportSource::Heard => {
                    let report = reports_for(policy.person)
                        .iter()
                        .filter(|r| {
                            r.actor == policy.actor
                                && r.received_at < tick
                                && policy.heard_from.is_none_or(|speaker| speaker == r.speaker)
                        })
                        .max_by_key(|r| (r.received_at, Reverse(r.speaker)))?;
                    (
                        report.target,
                        report.good,
                        report.quantity,
                        report.observed_at,
                        report.claimed_outcome,
                        Some(report.speaker),
                    )
                }
            };
            let target = policy
                .details
                .as_ref()
                .and_then(|d| d.target)
                .unwrap_or(target);
            let good = policy.details.as_ref().and_then(|d| d.good).unwrap_or(good);
            let quantity = policy
                .details
                .as_ref()
                .and_then(|d| d.quantity)
                .unwrap_or(quantity);
            let claimed_outcome = policy.claimed_outcome.unwrap_or(outcome);
            let prior = reports_for(policy.recipient);
            let repeated = prior
                .binary_search_by_key(&(policy.person, policy.actor), |r| (r.speaker, r.actor))
                .ok()
                .is_some_and(|i| {
                    let old = &prior[i];
                    (
                        old.target,
                        old.good,
                        old.quantity,
                        old.observed_at,
                        old.claimed_outcome,
                        old.source_speaker,
                    ) == (
                        target,
                        good,
                        quantity,
                        observed_at,
                        claimed_outcome,
                        source_speaker,
                    )
                });
            if repeated
                && inquiry_keys
                    .binary_search(&(policy.recipient, policy.person, policy.actor, observed_at))
                    .is_err()
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
                target,
                good,
                quantity,
                observed_at,
                received_at: tick,
                claimed_outcome,
                belief,
                source_speaker,
            })
        };
        let policies = &previous.report_policies;
        // A large ordered group can retain at most 64 identities. Evaluate
        // chunks in parallel and discard candidates dominated within each
        // chunk before allocating the complete incoming batch.
        let grouped = policies.len() >= 4096
            && policies
                .windows(2)
                .take(64)
                .filter(|p| p[0].recipient == p[1].recipient)
                .count()
                >= 32
            && policies.par_windows(2).all(|p| {
                (p[0].recipient, p[0].person, p[0].actor)
                    < (p[1].recipient, p[1].person, p[1].actor)
            });
        let incoming: Vec<_> = if grouped {
            policies
                .par_chunk_by(|a, b| a.recipient == b.recipient)
                .flat_map_iter(|group| {
                    let chunks: Vec<Vec<_>> = group
                        .par_chunks(2048)
                        .map(|chunk| {
                            chunk
                                .iter()
                                .filter_map(&make_report)
                                .take(MAX_REPORTS)
                                .collect()
                        })
                        .collect();
                    chunks
                        .into_iter()
                        .flatten()
                        .take(MAX_REPORTS)
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            policies.par_iter().filter_map(&make_report).collect()
        };
        if incoming.is_empty() {
            return Vec::new();
        }
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("taking.reports.update");
        }
        let (admitted, candidates) = previous.reports.updated(incoming);
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("taking.reports.reactions");
        }
        let changes = if receptions.is_empty() {
            Vec::new()
        } else {
            candidates
                .par_iter()
                .filter(|r| {
                    let key = (r.speaker, r.actor);
                    let before = reports_for(r.recipient);
                    let after = admitted.get(r.recipient);
                    let new = before
                        .binary_search_by_key(&key, |row| (row.speaker, row.actor))
                        .ok()
                        .is_none_or(|i| {
                            before[i].received_at < r.received_at && !same_statement(&before[i], r)
                        });
                    new && after
                        .binary_search_by_key(&key, |row| (row.speaker, row.actor))
                        .ok()
                        .is_some_and(|i| &after[i] == *r)
                })
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
                    let comparison = rule.and_then(|rule| {
                        if rule.matching_toward_speaker.is_none()
                            && rule.different_toward_speaker.is_none()
                        {
                            return None;
                        }
                        let seen = witness_for(report.recipient, report.actor)?.last_seen?;
                        if seen.tick != report.observed_at {
                            return None;
                        }
                        if (seen.target, seen.good, seen.quantity, seen.outcome)
                            == (
                                report.target,
                                report.good,
                                report.quantity,
                                report.claimed_outcome,
                            )
                        {
                            rule.matching_toward_speaker
                        } else {
                            rule.different_toward_speaker
                        }
                    });
                    let make = |target, value: Option<SubjectiveReaction>| {
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
                        make(report.speaker, comparison),
                    ]
                    .into_iter()
                    .flatten()
                })
                .collect()
        };
        self.reports = admitted;
        #[cfg(feature = "profile")]
        drop(phase);
        changes
    }
}
