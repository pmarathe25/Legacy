//! Personal requests to consider a remembered or reported taking.
use crate::{Outcome, Report, Taking};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Pending,
    Accepted,
    Declined,
    Expired,
}

/// A frozen personal account, which may itself have come from a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub actor: u64,
    pub target: u64,
    pub good: u32,
    pub quantity: u64,
    pub observed_at: u64,
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<u64>,
}

/// An unanswered request is not a judgment, duty, or proof of wrongdoing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Complaint {
    pub author: u64,
    pub recipient: u64,
    pub witness: u64,
    pub account: Account,
    pub created_at: u64,
    pub expires_at: u64,
    pub decision: Decision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<u64>,
    /// This recipient's previous personal case, carried across requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_case: Option<Arc<PersonalCase>>,
    /// The investigator's later received statement; this is not proof of truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Arc<Answer>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub report: Report,
    /// Whether the statement's details match the frozen account.
    pub matches_account: bool,
    /// The recipient's private interpretation, not a finding of fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<i32>,
    /// The earlier personal report used as a precedent, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precedent: Option<Report>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_case: Option<PersonalCase>,
}

/// A bounded private memory of a person's own earlier assessment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalCase {
    pub person: u64,
    pub account: Account,
    pub assessment: i32,
    pub remembered_at: u64,
}

/// A person's own interpretation of a statement, including the choice to
/// withhold judgment. Scores have no prescribed moral meaning or consequences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentPreference {
    pub person: u64,
    #[serde(default)]
    pub matching_believed: Option<i32>,
    #[serde(default)]
    pub matching_doubted: Option<i32>,
    #[serde(default)]
    pub matching_unassessed: Option<i32>,
    #[serde(default)]
    pub different_believed: Option<i32>,
    #[serde(default)]
    pub different_doubted: Option<i32>,
    #[serde(default)]
    pub different_unassessed: Option<i32>,
    /// Optional personally chosen adjustment when an earlier report about the
    /// same actor is known. Its sign and interpretation are up to this person.
    #[serde(default)]
    pub prior_actor_adjustment: Option<i32>,
    /// Restrict the reports this person chooses to consider, if desired.
    #[serde(default)]
    pub prior_belief: Option<crate::ReportBelief>,
    /// A signed personal weight on a remembered judgment about the same actor.
    #[serde(default)]
    pub prior_assessment_scale: Option<i32>,
}

impl AssessmentPreference {
    fn assess(self, matches: bool, belief: crate::ReportBelief) -> Option<i32> {
        match (matches, belief) {
            (true, crate::ReportBelief::Believes) => self.matching_believed,
            (true, crate::ReportBelief::Doubts) => self.matching_doubted,
            (true, crate::ReportBelief::Unassessed) => self.matching_unassessed,
            (false, crate::ReportBelief::Believes) => self.different_believed,
            (false, crate::ReportBelief::Doubts) => self.different_doubted,
            (false, crate::ReportBelief::Unassessed) => self.different_unassessed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub willing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_threshold: Option<u64>,
}

impl Taking {
    /// Independent, optional intentions; the social owner still arbitrates contact.
    pub fn inquiry_meetings(&self, tick: u64) -> Vec<(u64, u64)> {
        if self.inquiry_pursuit.is_empty() || self.complaints.is_empty() {
            return Vec::new();
        }
        let ordered = self
            .inquiry_pursuit
            .par_windows(2)
            .any(|pair| pair[0] > pair[1])
            .then(|| {
                let mut rows: Vec<_> = self.inquiry_pursuit.par_iter().copied().collect();
                rows.par_sort_unstable();
                rows
            });
        let pursuing = ordered
            .as_deref()
            .unwrap_or(self.inquiry_pursuit.as_slice());
        self.complaints
            .par_iter()
            .filter_map(|c| {
                let (actor, target) = match c.decision {
                    Decision::Pending if c.created_at < tick && tick < c.expires_at => {
                        (c.author, c.recipient)
                    }
                    Decision::Accepted
                        if c.answer.is_none()
                            && c.decided_at.is_some_and(|at| at < tick)
                            && tick < c.expires_at =>
                    {
                        (c.recipient, c.witness)
                    }
                    _ => return None,
                };
                pursuing
                    .binary_search(&actor)
                    .is_ok()
                    .then_some((actor, target))
            })
            .collect()
    }
    /// Freeze only an account already in the author's own memory or reports.
    pub fn complaint_account(&self, author: u64, actor: u64) -> Option<Account> {
        if let Some(seen) = self
            .witnesses
            .iter()
            .find(|w| w.person == author && w.actor == actor)
            .and_then(|w| w.last_seen)
        {
            return Some(Account {
                actor,
                target: seen.target,
                good: seen.good,
                quantity: seen.quantity,
                observed_at: seen.tick,
                outcome: seen.outcome,
                source: None,
            });
        }
        self.reports
            .get(author)
            .iter()
            .filter(|r| r.actor == actor)
            .max_by_key(|r| (r.received_at, std::cmp::Reverse(r.speaker)))
            .map(|r| Account {
                actor,
                target: r.target,
                good: r.good,
                quantity: r.quantity,
                observed_at: r.observed_at,
                outcome: r.claimed_outcome,
                source: Some(r.speaker),
            })
    }

    /// Evaluate each recipient's single pending request against actual contacts.
    pub fn advance_complaints(
        &mut self,
        previous: &Taking,
        contacts: &[(u64, u64)],
        tick: u64,
        pressure: &(impl Fn(u64, u64) -> u64 + Sync),
    ) {
        if previous.complaints.is_empty() {
            return;
        }
        let ordered = (contacts.par_iter().any(|(a, b)| a > b)
            || contacts.par_windows(2).any(|pair| pair[0] > pair[1]))
        .then(|| {
            let mut rows: Vec<_> = contacts
                .par_iter()
                .map(|&(a, b)| (a.min(b), a.max(b)))
                .collect();
            rows.par_sort_unstable();
            rows
        });
        let pairs = ordered.as_deref().unwrap_or(contacts);
        let ordered_preferences = previous
            .complaint_preferences
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
            .then(|| {
                let mut rows: Vec<_> = previous.complaint_preferences.par_iter().copied().collect();
                rows.par_sort_unstable_by_key(|row| row.person);
                rows
            });
        let preferences = ordered_preferences
            .as_deref()
            .unwrap_or(previous.complaint_preferences.as_slice());
        let ordered_assessments = previous
            .assessment_preferences
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
            .then(|| {
                let mut rows: Vec<_> = previous
                    .assessment_preferences
                    .par_iter()
                    .copied()
                    .collect();
                rows.par_sort_unstable_by_key(|row| row.person);
                rows
            });
        let assessments = ordered_assessments
            .as_deref()
            .unwrap_or(previous.assessment_preferences.as_slice());
        let reports = &self.reports;
        let mut updates: Vec<_> = previous
            .complaints
            .par_iter()
            .enumerate()
            .filter_map(|(index, complaint)| {
                if complaint.decision == Decision::Accepted
                    && complaint.answer.is_none()
                    && tick < complaint.expires_at
                    && complaint.decided_at.is_some_and(|at| at < tick)
                    && let Some(report) = reports.get(complaint.recipient).iter().find(|report| {
                        report.speaker == complaint.witness
                            && report.actor == complaint.account.actor
                            && report.observed_at == complaint.account.observed_at
                            && report.received_at == tick
                    })
                {
                    let mut next = complaint.clone();
                    let matches_account = report.target == complaint.account.target
                        && report.good == complaint.account.good
                        && report.quantity == complaint.account.quantity
                        && report.claimed_outcome == complaint.account.outcome;
                    let preference = assessments
                        .binary_search_by_key(&complaint.recipient, |p| p.person)
                        .ok()
                        .map(|i| assessments[i]);
                    let mut assessment =
                        preference.and_then(|p| p.assess(matches_account, report.belief));
                    let precedent = preference.and_then(|p| {
                        let adjustment = p.prior_actor_adjustment?;
                        let score = assessment.as_mut()?;
                        let prior = previous
                            .reports
                            .get(complaint.recipient)
                            .iter()
                            .filter(|prior| {
                                prior.actor == complaint.account.actor
                                    && prior.observed_at < complaint.account.observed_at
                                    && p.prior_belief.is_none_or(|belief| prior.belief == belief)
                            })
                            .max_by_key(|prior| {
                                (
                                    prior.observed_at,
                                    prior.received_at,
                                    std::cmp::Reverse(prior.speaker),
                                )
                            })?;
                        *score = score.saturating_add(adjustment);
                        Some(prior.clone())
                    });
                    let prior_case = preference.and_then(|p| {
                        let scale = p.prior_assessment_scale?;
                        let score = assessment.as_mut()?;
                        let prior = complaint.previous_case.as_deref().filter(|prior| {
                            prior.person == complaint.recipient
                                && prior.account.actor == complaint.account.actor
                                && prior.account.observed_at < complaint.account.observed_at
                        })?;
                        *score = score.saturating_add(prior.assessment.saturating_mul(scale));
                        Some(*prior)
                    });
                    next.answer = Some(Arc::new(Answer {
                        matches_account,
                        assessment,
                        precedent,
                        prior_case,
                        report: report.clone(),
                    }));
                    return Some((index, next));
                }
                if complaint.decision != Decision::Pending {
                    return None;
                }
                if tick >= complaint.expires_at {
                    let mut next = complaint.clone();
                    next.decision = Decision::Expired;
                    next.decided_at = Some(tick);
                    return Some((index, next));
                } else if complaint.created_at < tick
                    && pairs
                        .binary_search(&(
                            complaint.author.min(complaint.recipient),
                            complaint.author.max(complaint.recipient),
                        ))
                        .is_ok()
                {
                    let preference = preferences
                        .binary_search_by_key(&complaint.recipient, |p| p.person)
                        .ok()
                        .map(|i| preferences[i]);
                    if let Some(preference) = preference {
                        let accepts = preference.willing
                            || preference.pressure_threshold.is_some_and(|threshold| {
                                pressure(complaint.recipient, complaint.author) >= threshold
                            });
                        let mut next = complaint.clone();
                        next.decision = if accepts {
                            Decision::Accepted
                        } else {
                            Decision::Declined
                        };
                        next.decided_at = Some(tick);
                        return Some((index, next));
                    }
                }
                None
            })
            .collect();
        if updates.is_empty() {
            self.complaints = std::sync::Arc::clone(&previous.complaints);
            return;
        }
        if updates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            updates.par_sort_unstable_by_key(|row| row.0);
        }
        let mut next: Vec<_> = previous.complaints.par_iter().cloned().collect();
        next.par_chunks_mut(1024)
            .enumerate()
            .for_each(|(chunk_index, chunk)| {
                let start = chunk_index * 1024;
                let first = updates.partition_point(|row| row.0 < start);
                let last = updates.partition_point(|row| row.0 < start + chunk.len());
                for (index, replacement) in &updates[first..last] {
                    chunk[*index - start] = replacement.clone();
                }
            });
        self.complaints = next.into();
    }
}

pub(crate) fn valid(owner: &Taking, people: &HashSet<u64>, goods: &HashSet<u32>, now: u64) -> bool {
    let mut recipients: Vec<_> = owner
        .complaints
        .par_iter()
        .map(|row| row.recipient)
        .collect();
    recipients.par_sort_unstable();
    let mut preferences: Vec<_> = owner
        .complaint_preferences
        .par_iter()
        .map(|row| row.person)
        .collect();
    preferences.par_sort_unstable();
    let mut pursuing: Vec<_> = owner.inquiry_pursuit.par_iter().copied().collect();
    pursuing.par_sort_unstable();
    let mut assessments: Vec<_> = owner
        .assessment_preferences
        .par_iter()
        .map(|p| p.person)
        .collect();
    assessments.par_sort_unstable();
    !recipients.par_windows(2).any(|p| p[0] == p[1])
        && !preferences.par_windows(2).any(|p| p[0] == p[1])
        && !pursuing.par_windows(2).any(|p| p[0] == p[1])
        && !assessments.par_windows(2).any(|p| p[0] == p[1])
        && assessments.par_iter().all(|id| people.contains(id))
        && pursuing.par_iter().all(|id| people.contains(id))
        && owner
            .complaint_preferences
            .par_iter()
            .all(|p| people.contains(&p.person))
        && owner.complaints.par_iter().all(|c| {
            people.contains(&c.author)
                && people.contains(&c.recipient)
                && people.contains(&c.witness)
                && c.author != c.recipient
                && c.witness != c.recipient
                && people.contains(&c.account.actor)
                && people.contains(&c.account.target)
                && c.account.actor != c.account.target
                && goods.contains(&c.account.good)
                && c.account.quantity > 0
                && c.account.observed_at <= c.created_at
                && c.account
                    .source
                    .is_none_or(|source| source != c.author && people.contains(&source))
                && c.created_at <= now
                && c.expires_at > c.created_at
                && c.previous_case.as_ref().is_none_or(|prior| {
                    prior.person == c.recipient
                        && prior.remembered_at <= c.created_at
                        && prior.account.observed_at <= prior.remembered_at
                        && people.contains(&prior.account.actor)
                        && people.contains(&prior.account.target)
                        && prior.account.actor != prior.account.target
                        && goods.contains(&prior.account.good)
                        && prior.account.quantity > 0
                        && prior
                            .account
                            .source
                            .is_none_or(|source| people.contains(&source))
                })
                && c.answer.as_ref().is_none_or(|answer| {
                    let report = &answer.report;
                    c.decision == Decision::Accepted
                        && c.decided_at.is_some_and(|at| at < report.received_at)
                        && report.received_at < c.expires_at
                        && report.received_at <= now
                        && report.recipient == c.recipient
                        && report.speaker == c.witness
                        && report.actor == c.account.actor
                        && report.observed_at == c.account.observed_at
                        && people.contains(&report.target)
                        && report.target != report.actor
                        && goods.contains(&report.good)
                        && report.quantity > 0
                        && report.source_speaker.is_none_or(|source| {
                            source != report.speaker && people.contains(&source)
                        })
                        && answer.precedent.as_ref().is_none_or(|prior| {
                            prior.recipient == c.recipient
                                && prior.actor == c.account.actor
                                && prior.observed_at < c.account.observed_at
                                && prior.observed_at <= prior.received_at
                                && prior.received_at < report.received_at
                                && people.contains(&prior.speaker)
                                && prior.speaker != prior.recipient
                                && people.contains(&prior.target)
                                && prior.target != prior.actor
                                && goods.contains(&prior.good)
                                && prior.quantity > 0
                                && prior.source_speaker.is_none_or(|source| {
                                    source != prior.speaker && people.contains(&source)
                                })
                        })
                        && answer.prior_case.is_none_or(|prior| {
                            Some(prior) == c.previous_case.as_deref().copied()
                                && prior.account.actor == c.account.actor
                                && prior.account.observed_at < c.account.observed_at
                        })
                        && answer.matches_account
                            == (report.target == c.account.target
                                && report.good == c.account.good
                                && report.quantity == c.account.quantity
                                && report.claimed_outcome == c.account.outcome)
                })
                && match c.decision {
                    Decision::Pending => c.decided_at.is_none() && c.expires_at > now,
                    _ => c
                        .decided_at
                        .is_some_and(|at| at > c.created_at && at <= now),
                }
        })
}
