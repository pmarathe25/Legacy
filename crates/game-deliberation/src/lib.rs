//! Characters construct and evaluate explicit influence arrangements from personal experience.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// An executable agreement about whose expressed support counts and how much.
/// These are literal negotiated quantities, not named government types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRule {
    pub influence: BTreeMap<PersonId, u32>,
    pub required_support: u64,
}
impl DecisionRule {
    pub fn total(&self) -> u64 {
        self.influence.values().map(|w| u64::from(*w)).sum()
    }
    pub fn valid(&self, members: &BTreeSet<PersonId>) -> bool {
        self.required_support > 0
            && self
                .influence
                .iter()
                .all(|(p, w)| *w > 0 && members.contains(p))
    }
    pub fn passed(
        &self,
        members: &BTreeSet<PersonId>,
        opinions: &BTreeMap<PersonId, bool>,
    ) -> bool {
        let support: u64 = self
            .influence
            .iter()
            .filter(|(p, _)| members.contains(p) && opinions.get(p) == Some(&true))
            .map(|(_, w)| u64::from(*w))
            .sum();
        self.required_support > 0 && support >= self.required_support
    }
    pub fn sole_decider(&self) -> Option<PersonId> {
        if self.influence.len() != 1 {
            return None;
        }
        self.influence
            .iter()
            .find(|(_, w)| u64::from(**w) >= self.required_support)
            .map(|(p, _)| *p)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Procedure {
    /// No joint decision procedure: each custodian controls their own commitments.
    Personal,
    Consultation(DecisionRule),
    // Historical wire variants remain readable; autonomous construction uses literal rules.
    Leader(PersonId),
    Majority,
    Unanimity,
}
impl Procedure {
    pub fn sole_decider(&self) -> Option<PersonId> {
        match self {
            Self::Leader(p) => Some(*p),
            Self::Consultation(rule) => rule.sole_decider(),
            _ => None,
        }
    }
    pub fn consults(&self) -> bool {
        matches!(
            self,
            Self::Consultation(_) | Self::Majority | Self::Unanimity
        )
    }
}
pub fn passed(
    procedure: &Procedure,
    members: &BTreeSet<PersonId>,
    votes: &BTreeMap<PersonId, bool>,
) -> bool {
    match procedure {
        Procedure::Consultation(rule) => rule.passed(members, votes),
        Procedure::Personal | Procedure::Leader(_) => false,
        Procedure::Majority => {
            votes
                .iter()
                .filter(|(p, v)| members.contains(p) && **v)
                .count()
                > members.len() / 2
        }
        Procedure::Unanimity => members.iter().all(|p| votes.get(p) == Some(&true)),
    }
}
fn value(tx: &Transaction<'_>, p: PersonId, key: &str) -> i64 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&p)?.get(key))
        .copied()
        .map_or(0, i64::from)
}
/// Bounded invention: construct quantities from the proposer's own values and
/// directed relationships. Other participants still decide whether to accept.
pub fn construct(tx: &Transaction<'_>, p: PersonId, members: &BTreeSet<PersonId>) -> DecisionRule {
    let voice = value(tx, p, "shared-decisions").max(0);
    let social = tx.get::<game_relationships::Social>().ok();
    let authority = social
        .and_then(|s| s.beliefs.get(&p))
        .map_or(0, |b| i64::from(b.authority));
    let mut candidates = vec![];
    for other in members {
        let weight = if *other == p {
            100 + voice * 2
                + value(tx, p, "independence").max(0)
                + game_vitality::disposition(tx, p, "ambition") / 5
        } else {
            let tie = social.and_then(|s| s.relationships.get(&p)?.get(other));
            voice * 2
                + tie.map_or(0, |r| {
                    i64::from(r.trust * 2 + r.respect + r.admiration - r.grievance * 2)
                })
                + authority.max(0) * tie.map_or(0, |r| i64::from(r.respect.max(0))) / 10
        };
        if weight > 0 {
            candidates.push((*other, weight.min(u32::MAX as i64) as u32));
        }
    }
    // Attend to the most salient people, retaining one's own participation.
    candidates.sort_by_key(|(other, weight)| (*other != p, std::cmp::Reverse(*weight), *other));
    let influence: BTreeMap<_, _> = candidates.into_iter().take(32).collect();
    let total: u64 = influence.values().map(|w| u64::from(*w)).sum();
    let caution = (500
        + game_vitality::disposition(tx, p, "conviction") / 4
        + value(tx, p, "independence") * 2
        - game_vitality::disposition(tx, p, "patience") / 4)
        .clamp(1, 1000) as u64;
    DecisionRule {
        influence,
        required_support: total.saturating_mul(caution).div_ceil(1000).max(1),
    }
}
/// Expected influence, trusted delegation and room for independent action. This
/// reads the participant's own impressions, never other people's hidden preferences.
pub fn preference(tx: &Transaction<'_>, p: PersonId, rule: &DecisionRule) -> i64 {
    let total = rule.total().max(1) as i128;
    let own = i128::from(rule.influence.get(&p).copied().unwrap_or(0));
    let voice = i128::from(value(tx, p, "shared-decisions"));
    let independence = i128::from(value(tx, p, "independence"));
    let social = tx.get::<game_relationships::Social>().ok();
    let authority = social
        .and_then(|s| s.beliefs.get(&p))
        .map_or(0, |b| i128::from(b.authority));
    let mut benefit = own
        * (voice * 4
            + independence * 3
            + i128::from(game_vitality::disposition(tx, p, "ambition") / 10));
    for (other, weight) in &rule.influence {
        if *other != p {
            let regard = social.map_or(0, |s| s.regard(p, *other));
            benefit += i128::from(*weight) * (i128::from(regard) / 2 + authority * 3);
        }
    }
    let exposure = total
        .saturating_sub(i128::from(rule.required_support))
        .max(0);
    ((benefit - exposure * independence.max(0)) / total).clamp(i64::MIN as i128, i64::MAX as i128)
        as i64
}

pub fn personal_preference(tx: &Transaction<'_>, p: PersonId) -> i64 {
    let authority = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.beliefs.get(&p))
        .map_or(0, |b| i64::from(b.authority));
    value(tx, p, "independence") * 3 - authority * 3
        + game_vitality::disposition(tx, p, "ambition") / 10
}
