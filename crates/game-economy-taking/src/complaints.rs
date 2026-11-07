//! Personal requests to consider a remembered or reported taking.
use crate::{Outcome, Taking};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
        let mut pairs: Vec<_> = contacts
            .par_iter()
            .map(|&(a, b)| (a.min(b), a.max(b)))
            .collect();
        pairs.par_sort_unstable();
        let mut preferences: Vec<_> = previous.complaint_preferences.par_iter().collect();
        preferences.par_sort_unstable_by_key(|row| row.person);
        self.complaints = previous
            .complaints
            .par_iter()
            .cloned()
            .map(|mut complaint| {
                if complaint.decision != Decision::Pending {
                    return complaint;
                }
                if tick >= complaint.expires_at {
                    complaint.decision = Decision::Expired;
                    complaint.decided_at = Some(tick);
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
                        complaint.decision = if accepts {
                            Decision::Accepted
                        } else {
                            Decision::Declined
                        };
                        complaint.decided_at = Some(tick);
                    }
                }
                complaint
            })
            .collect::<Vec<_>>()
            .into();
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
    !recipients.par_windows(2).any(|p| p[0] == p[1])
        && !preferences.par_windows(2).any(|p| p[0] == p[1])
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
                && match c.decision {
                    Decision::Pending => c.decided_at.is_none() && c.expires_at > now,
                    _ => c
                        .decided_at
                        .is_some_and(|at| at > c.created_at && at <= now),
                }
        })
}
