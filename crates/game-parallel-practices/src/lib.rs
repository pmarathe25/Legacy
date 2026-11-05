//! Bounded personal invention of conditional action from experienced events.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

pub const MAX_CASES: usize = 16;
pub const MAX_PROPOSALS: usize = 8;

/// Each mind owns its cases and decisions; nothing here is a collective actor.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Practices {
    pub minds: Vec<Mind>,
}
impl Clone for Practices {
    fn clone(&self) -> Self {
        Self {
            minds: self.minds.par_iter().cloned().collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    /// Whether this person may propose a new arrangement.
    pub invent: bool,
    /// Their private approval of receiving a proposal.
    pub approve: bool,
    /// Their current willingness to carry out an adopted clause.
    pub willing: bool,
    /// Asking and giving are separate personal choices.
    #[serde(default)]
    pub ask_willing: bool,
    /// This person may revise a suggested quantity after unanswered requests.
    #[serde(default)]
    pub adjust_after_failure: bool,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    pub shortage_threshold: u32,
    pub minimum_shortages: u32,
    pub review_interval: u64,
    pub proposal_lifetime: u64,
    /// The proposer suggests retaining this many observed portions.
    #[serde(default)]
    pub reserve_portions: u32,
    /// Signed personal weights used to compare the candidate clauses.
    #[serde(default)]
    pub quantity_weight: i64,
    #[serde(default)]
    pub reserve_weight: i64,
    /// Signed personal interpretation of prior giving and shortage experiences.
    #[serde(default)]
    pub giving_experience_weight: i64,
    #[serde(default)]
    pub shortage_experience_weight: i64,
    /// Signed personal weight for the recipient's affinity toward the author.
    #[serde(default)]
    pub proposal_affinity_weight: i64,
    #[serde(default)]
    pub acceptance_threshold: i64,
    /// Optional personal attachment to conditional aid, learned from relevant experience.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_learning: Option<Arc<ValueLearning>>,
    /// Optional interpretation of a relevant personal precedent when evaluating a proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precedent: Option<Arc<PrecedentReasoning>>,
    /// How this person chooses to describe their own current ability to deliver.
    #[serde(default)]
    pub reporting: Reporting,
    /// Their private disposition toward reports they receive.
    #[serde(default)]
    pub report_attitude: ReportAttitude,
    /// Optional personal reassessment after a request to that speaker goes unanswered.
    #[serde(default)]
    pub unanswered_report_belief: Option<Belief>,
    /// Ask under a proposal only after personally believing a readiness claim.
    #[serde(default)]
    pub require_believed_ready: bool,
    #[serde(default)]
    pub next_review: u64,
    #[serde(default)]
    pub sequence: u64,
    #[serde(default)]
    pub cases: Arc<Vec<Case>>,
    #[serde(default)]
    pub authored: Arc<Vec<Proposal>>,
    #[serde(default)]
    pub inbox: Arc<Vec<Received>>,
    #[serde(default)]
    pub sent_reports: Arc<Vec<ReadinessReport>>,
    #[serde(default)]
    pub reports: Arc<Vec<ReceivedReport>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueLearning {
    pub value: i64,
    pub minimum: i64,
    pub maximum: i64,
    #[serde(default)]
    pub received: i64,
    #[serde(default)]
    pub gave: i64,
    #[serde(default)]
    pub unanswered: i64,
}
impl ValueLearning {
    fn valid(self) -> bool {
        self.minimum <= self.value && self.value <= self.maximum
    }
}
/// Signed similarity weights select a case; the person's signed interpretation
/// of that case then contributes to their decision about the new proposal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrecedentReasoning {
    pub same_person: i64,
    pub same_good: i64,
    pub quantity_difference: i64,
    pub received: i64,
    pub gave: i64,
    pub unanswered: i64,
    pub shortage: i64,
}
impl PrecedentReasoning {
    fn judgment(&self, cases: &[Case], proposal: &Proposal) -> i64 {
        cases
            .iter()
            .map(|case| {
                let (other, good, quantity, judgment) = match *case {
                    Case::Received {
                        helper,
                        good,
                        quantity,
                        ..
                    } => (Some(helper), Some(good), Some(quantity), self.received),
                    Case::Gave {
                        recipient,
                        good,
                        quantity,
                        ..
                    } => (Some(recipient), Some(good), Some(quantity), self.gave),
                    Case::Unanswered { target, .. } => (Some(target), None, None, self.unanswered),
                    Case::Shortage { .. } => (None, None, None, self.shortage),
                };
                let mut similarity = 0i128;
                if other == Some(proposal.author) {
                    similarity += i128::from(self.same_person);
                }
                if good == Some(proposal.good) {
                    similarity += i128::from(self.same_good);
                }
                if let Some(quantity) = quantity {
                    similarity = similarity.saturating_add(
                        i128::from(self.quantity_difference)
                            .saturating_mul(i128::from(quantity.abs_diff(proposal.quantity))),
                    );
                }
                (similarity, case.key(), judgment)
            })
            .max_by_key(|(similarity, key, _)| (*similarity, *key))
            .map_or(0, |(_, _, judgment)| judgment)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reporting {
    #[default]
    Silent,
    Truthful,
    Lie,
    Ready,
    Unready,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportAttitude {
    #[default]
    Ignore,
    Trust,
    Doubt,
    /// Believe this speaker only when personal directed trust meets the threshold.
    TrustAtLeast(i32),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Belief {
    #[default]
    Unassessed,
    Believes,
    Disbelieves,
}
/// The statement contains no privileged record of whether it was true.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessReport {
    pub speaker: u64,
    pub listener: u64,
    pub sequence: u64,
    pub claimed_ready: bool,
    pub tick: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceivedReport {
    pub statement: ReadinessReport,
    pub belief: Belief,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Case {
    Shortage {
        tick: u64,
        hunger: u32,
    },
    Received {
        tick: u64,
        helper: u64,
        good: u32,
        quantity: u64,
    },
    Unanswered {
        tick: u64,
        target: u64,
    },
    Gave {
        tick: u64,
        recipient: u64,
        good: u32,
        quantity: u64,
    },
}
impl Case {
    pub fn tick(&self) -> u64 {
        match *self {
            Self::Shortage { tick, .. }
            | Self::Received { tick, .. }
            | Self::Unanswered { tick, .. }
            | Self::Gave { tick, .. } => tick,
        }
    }
    fn key(&self) -> (u64, u8, u64, u32, u64) {
        match *self {
            Self::Shortage { tick, hunger } => (tick, 0, 0, hunger, 0),
            Self::Received {
                tick,
                helper,
                good,
                quantity,
            } => (tick, 1, helper, good, quantity),
            Self::Unanswered { tick, target } => (tick, 2, target, 0, 0),
            Self::Gave {
                tick,
                recipient,
                good,
                quantity,
            } => (tick, 3, recipient, good, quantity),
        }
    }
}
/// A proposal is copied into the addressee's own inbox only through contact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub author: u64,
    pub sequence: u64,
    pub recipient: u64,
    pub good: u32,
    pub quantity: u64,
    pub reserve: u64,
    pub ask_hunger: u32,
    pub created: u64,
    pub expires: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Pending,
    Adopted,
    Declined,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Received {
    pub proposal: Proposal,
    pub decision: Decision,
    /// Outward adoption and private approval are independent.
    pub approved: bool,
    pub delivered: u64,
}
#[derive(Clone, Debug)]
pub struct Observation {
    pub person: u64,
    pub case: Case,
}

impl Mind {
    /// A recipient can make their own explicit decision; it does not bind later actions.
    pub fn decide(
        &mut self,
        author: u64,
        sequence: u64,
        approve: bool,
        now: u64,
    ) -> Result<(), &'static str> {
        let item = Arc::make_mut(&mut self.inbox)
            .iter_mut()
            .find(|item| item.proposal.author == author && item.proposal.sequence == sequence)
            .ok_or("unknown personal proposal")?;
        if item.decision != Decision::Pending || item.delivered > now || item.proposal.expires < now
        {
            return Err("proposal is no longer pending");
        }
        item.approved = approve;
        item.decision = if approve {
            Decision::Adopted
        } else {
            Decision::Declined
        };
        Ok(())
    }
    /// Abandon this person's adopted clause without changing anybody else's records.
    pub fn withdraw(&mut self, author: u64, sequence: u64, now: u64) -> Result<(), &'static str> {
        let item = Arc::make_mut(&mut self.inbox)
            .iter_mut()
            .find(|item| item.proposal.author == author && item.proposal.sequence == sequence)
            .ok_or("unknown personal proposal")?;
        if item.decision != Decision::Adopted || item.proposal.expires < now {
            return Err("proposal is not currently adopted");
        }
        item.decision = Decision::Declined;
        item.approved = false;
        Ok(())
    }
    pub fn assess(
        &mut self,
        speaker: u64,
        sequence: u64,
        belief: Belief,
    ) -> Result<(), &'static str> {
        let report = Arc::make_mut(&mut self.reports)
            .iter_mut()
            .find(|report| {
                report.statement.speaker == speaker && report.statement.sequence == sequence
            })
            .ok_or("unknown personal report")?;
        report.belief = belief;
        Ok(())
    }
}
impl Practices {
    fn speech_indices(
        &self,
        speaker: u64,
        listener: u64,
        sequence: u64,
        tick: u64,
    ) -> Result<(usize, usize), &'static str> {
        let source = self
            .minds
            .iter()
            .position(|mind| mind.person == speaker)
            .ok_or("unknown speaker")?;
        let target = self
            .minds
            .iter()
            .position(|mind| mind.person == listener)
            .ok_or("unknown listener")?;
        if source == target {
            return Err("invalid personal report");
        }
        let speaker_mind = &self.minds[source];
        let listener_mind = &self.minds[target];
        if !speaker_mind.inbox.iter().any(|item| {
            item.decision == Decision::Adopted
                && item.proposal.author == listener
                && item.proposal.sequence == sequence
                && item.proposal.expires >= tick
        }) || !listener_mind
            .authored
            .iter()
            .any(|p| p.recipient == speaker && p.sequence == sequence && p.expires >= tick)
            || speaker_mind
                .sent_reports
                .iter()
                .any(|r| r.listener == listener && r.sequence == sequence)
            || speaker_mind
                .sent_reports
                .iter()
                .filter(|report| {
                    speaker_mind.inbox.iter().any(|item| {
                        item.proposal.author == report.listener
                            && item.proposal.sequence == report.sequence
                            && item.proposal.expires >= tick
                    })
                })
                .count()
                >= MAX_PROPOSALS
            || listener_mind
                .reports
                .iter()
                .filter(|report| {
                    listener_mind.authored.iter().any(|proposal| {
                        proposal.recipient == report.statement.speaker
                            && proposal.sequence == report.statement.sequence
                            && proposal.expires >= tick
                    })
                })
                .count()
                >= MAX_PROPOSALS
        {
            return Err("cannot communicate this report");
        }
        Ok((source, target))
    }
    pub fn check_speech(
        &self,
        speaker: u64,
        listener: u64,
        sequence: u64,
        tick: u64,
    ) -> Result<(), &'static str> {
        self.speech_indices(speaker, listener, sequence, tick)
            .map(|_| ())
    }
    /// Publish a statement only after the host has established actual contact.
    pub fn speak(
        &mut self,
        speaker: u64,
        listener: u64,
        sequence: u64,
        claimed_ready: bool,
        tick: u64,
    ) -> Result<(), &'static str> {
        let (source, target) = self.speech_indices(speaker, listener, sequence, tick)?;
        let report = ReadinessReport {
            speaker,
            listener,
            sequence,
            claimed_ready,
            tick,
        };
        if self.minds[source].sent_reports.len() == MAX_PROPOSALS {
            let inbox = Arc::clone(&self.minds[source].inbox);
            let reports = Arc::make_mut(&mut self.minds[source].sent_reports);
            let obsolete = reports
                .iter()
                .position(|report| {
                    !inbox.iter().any(|item| {
                        item.proposal.author == report.listener
                            && item.proposal.sequence == report.sequence
                            && item.proposal.expires >= tick
                    })
                })
                .expect("speech capacity checked");
            reports.remove(obsolete);
        }
        if self.minds[target].reports.len() == MAX_PROPOSALS {
            let authored = Arc::clone(&self.minds[target].authored);
            let reports = Arc::make_mut(&mut self.minds[target].reports);
            let obsolete = reports
                .iter()
                .position(|report| {
                    !authored.iter().any(|proposal| {
                        proposal.recipient == report.statement.speaker
                            && proposal.sequence == report.statement.sequence
                            && proposal.expires >= tick
                    })
                })
                .expect("speech capacity checked");
            reports.remove(obsolete);
        }
        Arc::make_mut(&mut self.minds[source].sent_reports).push(report.clone());
        Arc::make_mut(&mut self.minds[target].reports).push(ReceivedReport {
            statement: report,
            belief: Belief::Unassessed,
        });
        Ok(())
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        food: &HashSet<u32>,
        now: u64,
    ) -> Result<(), &'static str> {
        let mut ids: Vec<_> = self.minds.par_iter().map(|m| m.person).collect();
        ids.par_sort_unstable();
        if ids.par_windows(2).any(|p| p[0] == p[1])
            || self.minds.par_iter().any(|mind| {
                !people.contains(&mind.person)
                    || mind
                        .value_learning
                        .as_ref()
                        .is_some_and(|value| !value.valid())
                    || mind.minimum_shortages == 0
                    || mind.minimum_shortages as usize > MAX_CASES
                    || mind.review_interval == 0
                    || mind.proposal_lifetime == 0
                    || mind.pressure_threshold == Some(0)
                    || mind.cases.len() > MAX_CASES
                    || mind.authored.len() > MAX_PROPOSALS
                    || mind.inbox.len() > MAX_PROPOSALS
                    || mind.sent_reports.len() > MAX_PROPOSALS
                    || mind.reports.len() > MAX_PROPOSALS
                    || mind
                        .cases
                        .windows(2)
                        .any(|pair| pair[0].tick() > pair[1].tick())
                    || mind.cases.iter().any(|case| match *case {
                        Case::Shortage { tick, .. } => tick > now,
                        Case::Received {
                            tick,
                            helper,
                            good,
                            quantity,
                        } => {
                            tick > now
                                || helper == mind.person
                                || !people.contains(&helper)
                                || !food.contains(&good)
                                || quantity == 0
                        }
                        Case::Unanswered { tick, target } => {
                            tick > now || target == mind.person || !people.contains(&target)
                        }
                        Case::Gave {
                            tick,
                            recipient,
                            good,
                            quantity,
                        } => {
                            tick > now
                                || recipient == mind.person
                                || !people.contains(&recipient)
                                || !food.contains(&good)
                                || quantity == 0
                        }
                    })
                    || mind.authored.iter().any(|p| {
                        !valid_proposal(p, people, food, now)
                            || p.author != mind.person
                            || p.sequence > mind.sequence
                    })
                    || mind.inbox.iter().any(|r| {
                        !valid_proposal(&r.proposal, people, food, now)
                            || r.proposal.recipient != mind.person
                            || r.delivered > now
                            || r.delivered < r.proposal.created
                            || r.delivered > r.proposal.expires
                            || (r.decision != Decision::Adopted && r.approved)
                    })
                    || mind.authored.iter().enumerate().any(|(i, p)| {
                        mind.authored[..i]
                            .iter()
                            .any(|prior| prior.sequence == p.sequence)
                    })
                    || mind.inbox.iter().enumerate().any(|(i, r)| {
                        mind.inbox[..i].iter().any(|prior| {
                            (prior.proposal.author, prior.proposal.sequence)
                                == (r.proposal.author, r.proposal.sequence)
                        })
                    })
                    || mind.sent_reports.iter().any(|report| {
                        !valid_report(report, people, now) || report.speaker != mind.person
                    })
                    || mind.sent_reports.iter().enumerate().any(|(i, report)| {
                        mind.sent_reports[..i].iter().any(|prior| {
                            (prior.listener, prior.sequence) == (report.listener, report.sequence)
                        })
                    })
                    || mind.reports.iter().any(|report| {
                        !valid_report(&report.statement, people, now)
                            || report.statement.listener != mind.person
                    })
                    || mind.reports.iter().enumerate().any(|(i, report)| {
                        mind.reports[..i].iter().any(|prior| {
                            (prior.statement.speaker, prior.statement.sequence)
                                == (report.statement.speaker, report.statement.sequence)
                        })
                    })
            })
        {
            return Err("invalid personal practice state");
        }
        Ok(())
    }

    /// Stage personal choices using prior cases and real contacts, then record new experiences.
    /// A recipient evaluates a delivered proposal no earlier than the following boundary.
    pub fn prepare<F>(
        &self,
        contacts: &[(u64, u64)],
        observations: &[Observation],
        tick: u64,
        pressure: F,
    ) -> Result<Self, &'static str>
    where
        F: Fn(u64, u64) -> u64 + Sync,
    {
        self.prepare_with_agency(contacts, observations, tick, pressure, |_| true)
    }
    pub fn prepare_with_agency<F, G>(
        &self,
        contacts: &[(u64, u64)],
        observations: &[Observation],
        tick: u64,
        pressure: F,
        can_decide: G,
    ) -> Result<Self, &'static str>
    where
        F: Fn(u64, u64) -> u64 + Sync,
        G: Fn(u64) -> bool + Sync,
    {
        self.prepare_with_evidence(
            contacts,
            observations,
            tick,
            pressure,
            can_decide,
            |_, _, _| false,
        )
    }
    /// Readiness is evaluated only from the speaker's own current custody.
    /// This convenience form uses neutral trust (zero); hosts with personal
    /// opinions should use `prepare_with_judgments` to supply directed trust.
    pub fn prepare_with_evidence<F, G, H>(
        &self,
        contacts: &[(u64, u64)],
        observations: &[Observation],
        tick: u64,
        pressure: F,
        can_decide: G,
        ready: H,
    ) -> Result<Self, &'static str>
    where
        F: Fn(u64, u64) -> u64 + Sync,
        G: Fn(u64) -> bool + Sync,
        H: Fn(u64, u32, u64) -> bool + Sync,
    {
        self.prepare_with_judgments(
            contacts,
            observations,
            tick,
            (pressure, |_, _| 0),
            can_decide,
            ready,
        )
    }
    /// Supply pressure and trust; affinity defaults to zero in this convenience form.
    pub fn prepare_with_judgments<F, G, H, J>(
        &self,
        contacts: &[(u64, u64)],
        observations: &[Observation],
        tick: u64,
        impressions: (F, J),
        can_decide: G,
        ready: H,
    ) -> Result<Self, &'static str>
    where
        F: Fn(u64, u64) -> u64 + Sync,
        G: Fn(u64) -> bool + Sync,
        H: Fn(u64, u32, u64) -> bool + Sync,
        J: Fn(u64, u64) -> i32 + Sync,
    {
        let (pressure, trust) = impressions;
        self.prepare_with_relationships(
            contacts,
            observations,
            tick,
            (pressure, trust, |_, _| 0),
            can_decide,
            ready,
        )
    }
    /// Supply directed pressure, trust, and affinity from the person's own relationships.
    pub fn prepare_with_relationships<F, G, H, J, K>(
        &self,
        contacts: &[(u64, u64)],
        observations: &[Observation],
        tick: u64,
        impressions: (F, J, K),
        can_decide: G,
        ready: H,
    ) -> Result<Self, &'static str>
    where
        F: Fn(u64, u64) -> u64 + Sync,
        G: Fn(u64) -> bool + Sync,
        H: Fn(u64, u32, u64) -> bool + Sync,
        J: Fn(u64, u64) -> i32 + Sync,
        K: Fn(u64, u64) -> i32 + Sync,
    {
        let (pressure, trust, affinity) = impressions;
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("practices.setup");
        let mut contacts: Vec<_> = contacts
            .par_iter()
            .flat_map_iter(|&(a, b)| [(a, b), (b, a)])
            .collect();
        if contacts.par_windows(2).any(|pair| pair[0] > pair[1]) {
            contacts.par_sort_unstable();
        }
        let mut minds: Vec<_> = self.minds.par_iter().collect();
        if minds
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            minds.par_sort_unstable_by_key(|m| m.person);
        }
        // A person who chooses to revise waits for the newly observed unanswered
        // request to enter their own committed memory before constructing terms.
        let newly_unanswered: HashSet<_> = observations
            .par_iter()
            .filter(|observation| matches!(observation.case, Case::Unanswered { .. }))
            .map(|observation| observation.person)
            .collect();
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("practices.offers");
        }
        let offers: Vec<_> = minds
            .par_iter()
            .filter_map(|mind| {
                if !mind.invent || !can_decide(mind.person) || tick < mind.next_review
                    || (mind.adjust_after_failure && newly_unanswered.contains(&mind.person)) {
                    return None;
                }
                let shortages = mind
                    .cases
                    .iter()
                    .filter(|case| matches!(case, Case::Shortage { .. }))
                    .count();
                if shortages < mind.minimum_shortages as usize {
                    return None;
                }
                let candidates = mind.cases.iter().filter_map(|case| match *case {
                    Case::Received { tick: observed, helper, good, quantity }
                        if contacts.binary_search(&(mind.person, helper)).is_ok()
                            && minds.binary_search_by_key(&helper, |other| other.person).is_ok()
                            && !mind.authored.iter().any(|p| p.recipient == helper && p.expires >= tick) => {
                        let failures = if mind.adjust_after_failure {
                            mind.cases.iter().filter(|case| matches!(case, Case::Unanswered { tick, target } if *target == helper && *tick > observed)).count() as u64
                        } else { 0 };
                        Some((observed, helper, good, quantity.saturating_sub(failures).max(1)))
                    }
                    _ => None,
                }).flat_map(|(observed, helper, good, amount)| {
                    [1, amount].into_iter().flat_map(move |quantity| {
                        [0, quantity.saturating_mul(u64::from(mind.reserve_portions))].into_iter().map(move |reserve| {
                            let score = i128::from(mind.quantity_weight).saturating_mul(i128::from(quantity))
                                .saturating_add(i128::from(mind.reserve_weight).saturating_mul(i128::from(reserve)));
                            (score, observed, quantity, reserve, std::cmp::Reverse(helper), std::cmp::Reverse(good))
                        })
                    })
                });
                let (_, _, quantity, reserve, std::cmp::Reverse(helper), std::cmp::Reverse(good)) = candidates.max()?;
                Some((mind, helper, good, quantity, reserve))
            })
            .map(|(mind, recipient, good, quantity, reserve)| {
                Ok(Proposal {
                    author: mind.person,
                    sequence: mind
                        .sequence
                        .checked_add(1)
                        .ok_or("practice sequence overflow")?,
                    recipient,
                    good,
                    quantity,
                    reserve,
                    ask_hunger: mind.shortage_threshold,
                    created: tick,
                    expires: tick
                        .checked_add(mind.proposal_lifetime)
                        .ok_or("practice expiry overflow")?,
                })
            })
            .collect::<Result<_, &'static str>>()?;
        let mut offers = offers;
        if offers.par_windows(2).any(|pair| {
            (pair[0].recipient, pair[0].author, pair[0].sequence)
                > (pair[1].recipient, pair[1].author, pair[1].sequence)
        }) {
            offers.par_sort_unstable_by_key(|p| (p.recipient, p.author, p.sequence));
        }
        // An author records only proposals that actually fit in the addressee's inbox.
        let offers: Vec<_> = offers
            .par_chunk_by(|a, b| a.recipient == b.recipient)
            .flat_map_iter(|group| {
                let target = minds[minds
                    .binary_search_by_key(&group[0].recipient, |m| m.person)
                    .expect("known recipient")];
                let retained = target
                    .inbox
                    .iter()
                    .filter(|r| r.proposal.expires >= tick)
                    .count();
                group
                    .iter()
                    .take(MAX_PROPOSALS.saturating_sub(retained))
                    .cloned()
            })
            .collect();
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("practices.reports");
        }
        // A readiness claim is a statement by its speaker, not an inventory fact.
        let mut reports: Vec<_> = if minds
            .par_iter()
            .any(|mind| mind.reporting != Reporting::Silent)
        {
            minds
                .par_iter()
                .filter_map(|mind| {
                    if !can_decide(mind.person) || mind.reporting == Reporting::Silent {
                        return None;
                    }
                    let item = mind.inbox.iter().find(|item| {
                        let proposal = &item.proposal;
                        item.decision == Decision::Adopted
                            && proposal.expires >= tick
                            && contacts
                                .binary_search(&(mind.person, proposal.author))
                                .is_ok()
                            && !mind.sent_reports.iter().any(|r| {
                                r.listener == proposal.author && r.sequence == proposal.sequence
                            })
                            && minds
                                .binary_search_by_key(&proposal.author, |other| other.person)
                                .ok()
                                .is_some_and(|i| {
                                    minds[i].authored.iter().any(|p| {
                                        p.sequence == proposal.sequence
                                            && p.recipient == mind.person
                                            && p.expires >= tick
                                    })
                                })
                    })?;
                    let proposal = &item.proposal;
                    let actual = proposal
                        .quantity
                        .checked_add(proposal.reserve)
                        .is_some_and(|amount| ready(mind.person, proposal.good, amount));
                    let claimed_ready = match mind.reporting {
                        Reporting::Silent => return None,
                        Reporting::Truthful => actual,
                        Reporting::Lie => !actual,
                        Reporting::Ready => true,
                        Reporting::Unready => false,
                    };
                    Some(ReadinessReport {
                        speaker: mind.person,
                        listener: proposal.author,
                        sequence: proposal.sequence,
                        claimed_ready,
                        tick,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        if reports.par_windows(2).any(|pair| {
            (pair[0].listener, pair[0].speaker, pair[0].sequence)
                > (pair[1].listener, pair[1].speaker, pair[1].sequence)
        }) {
            reports.par_sort_unstable_by_key(|r| (r.listener, r.speaker, r.sequence));
        }
        let reports: Vec<_> = reports
            .par_chunk_by(|a, b| a.listener == b.listener)
            .flat_map_iter(|group| {
                let listener = minds[minds
                    .binary_search_by_key(&group[0].listener, |mind| mind.person)
                    .expect("known listener")];
                let retained = listener
                    .reports
                    .iter()
                    .filter(|r| {
                        listener.authored.iter().any(|p| {
                            p.sequence == r.statement.sequence
                                && p.recipient == r.statement.speaker
                                && p.expires >= tick
                        })
                    })
                    .count();
                group
                    .iter()
                    .take(MAX_PROPOSALS.saturating_sub(retained))
                    .cloned()
            })
            .collect();
        let mut reports_by_speaker: Vec<_> = reports.par_iter().collect();
        if reports_by_speaker
            .par_windows(2)
            .any(|pair| pair[0].speaker > pair[1].speaker)
        {
            reports_by_speaker.par_sort_unstable_by_key(|r| r.speaker);
        }
        let mut by_author: Vec<_> = offers.par_iter().collect();
        if by_author
            .par_windows(2)
            .any(|pair| pair[0].author > pair[1].author)
        {
            by_author.par_sort_unstable_by_key(|p| p.author);
        }
        let mut observed: Vec<_> = observations.par_iter().collect();
        observed.par_sort_unstable_by_key(|o| (o.person, o.case.key()));
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("practices.update");
        }
        let next: Vec<_> = minds
            .par_iter()
            .map(|old| -> Result<Mind, &'static str> {
                let mut mind = (*old).clone();
                if mind.value_learning.as_ref().is_some_and(|value| !value.valid()) {
                    return Err("invalid personal practice value");
                }
                let can_act = can_decide(mind.person);
                if can_act && mind.inbox.iter().any(|item| item.decision == Decision::Pending && item.delivered < tick && item.proposal.expires >= tick) {
                    for item in Arc::make_mut(&mut mind.inbox) {
                        if item.decision != Decision::Pending || item.delivered >= tick || item.proposal.expires < tick { continue; }
                        let given = mind.cases.iter().filter(|case| matches!(case, Case::Gave { recipient, .. } if *recipient == item.proposal.author)).count() as i128;
                        let shortages = mind.cases.iter().filter(|case| matches!(case, Case::Shortage { .. })).count() as i128;
                        let affinity_score = if mind.proposal_affinity_weight == 0 { 0 } else {
                            i128::from(mind.proposal_affinity_weight).saturating_mul(i128::from(affinity(mind.person, item.proposal.author)))
                        };
                        let score = i128::from(mind.quantity_weight).saturating_mul(i128::from(item.proposal.quantity))
                            .saturating_add(i128::from(mind.reserve_weight).saturating_mul(i128::from(item.proposal.reserve)))
                            .saturating_add(i128::from(mind.giving_experience_weight).saturating_mul(given))
                            .saturating_add(i128::from(mind.shortage_experience_weight).saturating_mul(shortages))
                            .saturating_add(affinity_score)
                            .saturating_add(i128::from(mind.value_learning.as_ref().map_or(0, |v| v.value)))
                            .saturating_add(i128::from(mind.precedent.as_ref().map_or(0, |p| p.judgment(&mind.cases, &item.proposal))));
                        item.approved = mind.approve && score >= i128::from(mind.acceptance_threshold);
                        let pressured = mind.pressure_threshold.is_some_and(|threshold| {
                            pressure(mind.person, item.proposal.author) >= threshold
                        });
                        item.decision = if item.approved || pressured {
                            Decision::Adopted
                        } else {
                            Decision::Declined
                        };
                    }
                }
                if can_act && mind.report_attitude != ReportAttitude::Ignore && mind.reports.iter().any(|r| r.belief == Belief::Unassessed && r.statement.tick < tick) {
                    for report in Arc::make_mut(&mut mind.reports) {
                        if report.belief != Belief::Unassessed || report.statement.tick >= tick { continue; }
                        report.belief = match mind.report_attitude {
                            ReportAttitude::Ignore => Belief::Unassessed,
                            ReportAttitude::Trust => Belief::Believes,
                            ReportAttitude::Doubt => Belief::Disbelieves,
                            ReportAttitude::TrustAtLeast(threshold) => {
                                if trust(mind.person, report.statement.speaker) >= threshold {
                                    Belief::Believes
                                } else {
                                    Belief::Disbelieves
                                }
                            }
                        };
                    }
                }
                if mind.authored.iter().any(|p| p.expires < tick) { Arc::make_mut(&mut mind.authored).retain(|p| p.expires >= tick); }
                if mind.inbox.iter().any(|p| p.proposal.expires < tick) { Arc::make_mut(&mut mind.inbox).retain(|p| p.proposal.expires >= tick); }
                if let Ok(i) = reports_by_speaker.binary_search_by_key(&mind.person, |r| r.speaker)
                {
                    let inbox = &mind.inbox;
                    let sent = Arc::make_mut(&mut mind.sent_reports);
                    if sent.len() == MAX_PROPOSALS
                        && let Some(obsolete) = sent.iter().position(|r| !inbox.iter().any(|item| item.proposal.author == r.listener && item.proposal.sequence == r.sequence)) {
                        sent.remove(obsolete);
                    }
                    if sent.len() < MAX_PROPOSALS { sent.push(reports_by_speaker[i].clone()); }
                }
                let report_rows = reports.partition_point(|r| r.listener < mind.person)..reports.partition_point(|r| r.listener <= mind.person);
                if !report_rows.is_empty() {
                    let authored = &mind.authored;
                    let inbox = Arc::make_mut(&mut mind.reports);
                    for report in &reports[report_rows] {
                        if inbox.len() == MAX_PROPOSALS {
                            let Some(obsolete) = inbox.iter().position(|r| !authored.iter().any(|p| p.sequence == r.statement.sequence && p.recipient == r.statement.speaker)) else { break; };
                            inbox.remove(obsolete);
                        }
                        inbox.push(ReceivedReport { statement: report.clone(), belief: Belief::Unassessed });
                    }
                }
                if let Ok(i) = by_author.binary_search_by_key(&mind.person, |p| p.author) {
                    let proposal = by_author[i];
                    mind.sequence = proposal.sequence;
                    mind.next_review = tick
                        .checked_add(mind.review_interval)
                        .ok_or("practice review overflow")?;
                    let authored = Arc::make_mut(&mut mind.authored);
                    if authored.len() == MAX_PROPOSALS {
                        authored.remove(0);
                    }
                    authored.push(proposal.clone());
                }
                let offer_rows = offers.partition_point(|p| p.recipient < mind.person)..offers.partition_point(|p| p.recipient <= mind.person);
                if !offer_rows.is_empty() {
                    let inbox = Arc::make_mut(&mut mind.inbox);
                    for proposal in &offers[offer_rows] {
                        if inbox.len() == MAX_PROPOSALS { break; }
                        inbox.push(Received {
                            proposal: proposal.clone(),
                            decision: Decision::Pending,
                            approved: false,
                            delivered: tick,
                        });
                    }
                }
                let observation_rows = observed.partition_point(|o| o.person < mind.person)..observed.partition_point(|o| o.person <= mind.person);
                if !observation_rows.is_empty() {
                    if let Some(value) = mind.value_learning.as_ref() {
                        let change = observed[observation_rows.clone()].iter().fold(0i128, |total, observation| {
                            let rate = match &observation.case {
                                Case::Received { tick, helper, good, .. } if old.authored.iter().any(|p| p.recipient == *helper && p.good == *good && p.created <= *tick && p.expires >= *tick) => value.received,
                                Case::Gave { tick, recipient, good, .. } if old.inbox.iter().any(|r| r.proposal.author == *recipient && r.proposal.good == *good && r.proposal.created <= *tick && r.proposal.expires >= *tick) => value.gave,
                                Case::Unanswered { tick, target } if old.authored.iter().any(|p| p.recipient == *target && p.created <= *tick && p.expires >= *tick) => value.unanswered,
                                _ => 0,
                            };
                            total.saturating_add(i128::from(rate))
                        });
                        if change != 0 {
                            let value = Arc::make_mut(mind.value_learning.as_mut().expect("configured value"));
                            value.value = i128::from(value.value).saturating_add(change)
                                .clamp(i128::from(value.minimum), i128::from(value.maximum)) as i64;
                        }
                    }
                    if let Some(reassessment) = mind.unanswered_report_belief.filter(|_| can_act)
                        && mind.reports.iter().any(|report| report.statement.claimed_ready && observed[observation_rows.clone()].iter().any(|observation| matches!(observation.case, Case::Unanswered { target, .. } if target == report.statement.speaker))) {
                        for report in Arc::make_mut(&mut mind.reports) {
                            if report.statement.claimed_ready && observed[observation_rows.clone()].iter().any(|observation| matches!(observation.case, Case::Unanswered { target, .. } if target == report.statement.speaker)) {
                                report.belief = reassessment;
                            }
                        }
                    }
                    let cases = Arc::make_mut(&mut mind.cases);
                    for observation in &observed[observation_rows] {
                        if cases.len() == MAX_CASES { cases.remove(0); }
                        cases.push(observation.case.clone());
                    }
                }
                Ok(mind)
            })
            .collect::<Result<_, _>>()?;
        #[cfg(feature = "profile")]
        drop(phase);
        Ok(Self { minds: next })
    }
}
fn valid_proposal(p: &Proposal, people: &HashSet<u64>, food: &HashSet<u32>, now: u64) -> bool {
    p.author != p.recipient
        && people.contains(&p.author)
        && people.contains(&p.recipient)
        && food.contains(&p.good)
        && p.sequence > 0
        && p.quantity > 0
        && p.created <= now
        && p.expires > p.created
}
fn valid_report(report: &ReadinessReport, people: &HashSet<u64>, now: u64) -> bool {
    report.speaker != report.listener
        && people.contains(&report.speaker)
        && people.contains(&report.listener)
        && report.sequence > 0
        && report.tick <= now
}
