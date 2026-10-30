//! Optional personal voting preferences, evaluated against committed local ballots.
use game_parallel_institutions::{Action, Cast, Institutions};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
mod founding;
mod membership;
pub use founding::FoundingRules;
pub use membership::AdmissionDraft;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub enabled: bool,
    /// None means abstain on trade law; otherwise support the selected state.
    #[serde(default)]
    pub prefer_open: Option<bool>,
    /// When set, support open trade only while this person's trust meets the threshold.
    #[serde(default)]
    pub trade_trust_target: Option<u64>,
    #[serde(default)]
    pub trade_trust_threshold: i32,
    /// None means abstain in elections. Missing personal opinions use neutral trust zero.
    #[serde(default)]
    pub minimum_candidate_trust: Option<i32>,
    /// When enabled, propose a change to the preferred trade law when no ballot is open.
    #[serde(default)]
    pub propose_trade: bool,
    #[serde(default = "default_duration")]
    pub proposal_duration: u64,
    /// Voluntarily leave a local institution whose trade policy conflicts with this preference.
    #[serde(default)]
    pub leave_on_trade_disagreement: bool,
    /// Seek membership in a local institution with a compatible trade policy.
    #[serde(default)]
    pub seek_membership: bool,
}
fn default_duration() -> u64 {
    3
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivicAuto {
    pub policies: Vec<Policy>,
    /// When enabled, consenting local proponents may establish a new body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub founding: Option<FoundingRules>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub body: u64,
    pub proposer: u64,
    pub open: bool,
    pub duration: u64,
}
pub struct Draft {
    pub votes: Vec<Cast>,
    pub proposals: Vec<Proposal>,
    pub actors: Vec<u64>,
}
impl CivicAuto {
    pub fn validate(&self, people: &HashSet<u64>) -> Result<(), &'static str> {
        let mut ids: Vec<_> = self
            .policies
            .par_iter()
            .map(|policy| policy.person)
            .collect();
        ids.par_sort_unstable();
        if self.founding.as_ref().is_some_and(|rules| !rules.valid())
            || ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || ids.par_iter().any(|id| !people.contains(id))
            || self.policies.par_iter().any(|policy| {
                (policy.propose_trade && policy.proposal_duration == 0)
                    || policy
                        .trade_trust_target
                        .is_some_and(|target| target == policy.person || !people.contains(&target))
            })
        {
            return Err("invalid civic preference");
        }
        Ok(())
    }
    pub fn prepare(
        &self,
        institutions: &Institutions,
        opinions: Option<&Opinions>,
        people: &[PersonSnapshot],
        busy: &[u64],
    ) -> Option<Draft> {
        if self.policies.is_empty() {
            return None;
        }
        let mut policies: Vec<_> = self
            .policies
            .par_iter()
            .filter(|policy| policy.enabled)
            .collect();
        if policies
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            policies.par_sort_unstable_by_key(|policy| policy.person);
        }
        let mut busy = busy.to_vec();
        if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
            busy.par_sort_unstable();
        }
        let bodies: HashMap<_, _> = institutions
            .bodies
            .iter()
            .map(|body| (body.id, body))
            .collect();
        let needs_trust = policies
            .par_iter()
            .any(|policy| policy.trade_trust_target.is_some())
            || institutions
                .ballots
                .par_iter()
                .any(|ballot| matches!(ballot.action, Action::Elect { .. }))
                && policies
                    .par_iter()
                    .any(|policy| policy.minimum_candidate_trust.is_some());
        let index = opinions.filter(|_| needs_trust).map(Opinions::index);
        let trade_preference = |policy: &Policy| {
            policy
                .trade_trust_target
                .map_or(policy.prefer_open, |target| {
                    Some(
                        index
                            .as_ref()
                            .map_or(0, |index| index.trust(policy.person, target))
                            >= policy.trade_trust_threshold,
                    )
                })
        };
        let batches: Vec<Vec<Cast>> = institutions
            .ballots
            .par_iter()
            .map(|ballot| {
                let body = bodies[&ballot.body];
                if body.members.len() < body.quorum as usize {
                    return Vec::new();
                }
                let mut voted: Vec<_> = ballot.votes.par_iter().map(|vote| vote.voter).collect();
                if voted.par_windows(2).any(|pair| pair[0] > pair[1]) {
                    voted.par_sort_unstable();
                }
                body.members
                    .par_iter()
                    .filter_map(|&person| {
                        let policy = policies
                            .binary_search_by_key(&person, |policy| policy.person)
                            .ok()
                            .map(|i| policies[i])?;
                        let (_, region, health, _) =
                            people[*person_index(people, person).as_ref()?];
                        if health == 0
                            || region != Some(body.region)
                            || busy.binary_search(&person).is_ok()
                            || voted.binary_search(&person).is_ok()
                        {
                            return None;
                        }
                        let yes = match ballot.action {
                            Action::Trade { open } => {
                                trade_preference(policy).map(|preference| preference == open)?
                            }
                            Action::Elect { candidate } => {
                                let minimum = policy.minimum_candidate_trust?;
                                index
                                    .as_ref()
                                    .map_or(0, |index| index.trust(person, candidate))
                                    >= minimum
                            }
                        };
                        Some(Cast {
                            ballot: ballot.id,
                            voter: person,
                            yes,
                        })
                    })
                    .collect()
            })
            .collect();
        let mut votes: Vec<_> = batches
            .into_par_iter()
            .flat_map_iter(|batch| batch)
            .collect();
        if votes
            .par_windows(2)
            .any(|pair| (pair[0].voter, pair[0].ballot) > (pair[1].voter, pair[1].ballot))
        {
            votes.par_sort_unstable_by_key(|vote| (vote.voter, vote.ballot));
        }
        if votes
            .par_windows(2)
            .any(|pair| pair[0].voter == pair[1].voter)
        {
            votes = votes
                .par_chunk_by(|a, b| a.voter == b.voter)
                .map(|group| group[0])
                .collect();
        }
        let voting: Vec<_> = votes.par_iter().map(|vote| vote.voter).collect();
        let active: HashSet<_> = institutions
            .ballots
            .iter()
            .map(|ballot| ballot.body)
            .collect();
        let mut proposals: Vec<_> = institutions
            .bodies
            .par_iter()
            .filter(|body| !active.contains(&body.id) && body.members.len() >= body.quorum as usize)
            .filter_map(|body| {
                body.members
                    .par_iter()
                    .filter_map(|&person| {
                        let policy = policies
                            .binary_search_by_key(&person, |policy| policy.person)
                            .ok()
                            .map(|i| policies[i])?;
                        let open = trade_preference(policy)?;
                        let (_, region, health, _) =
                            people[*person_index(people, person).as_ref()?];
                        (policy.propose_trade
                            && open != body.trade_open
                            && health > 0
                            && region == Some(body.region)
                            && busy.binary_search(&person).is_err()
                            && voting.binary_search(&person).is_err())
                        .then_some(Proposal {
                            body: body.id,
                            proposer: person,
                            open,
                            duration: policy.proposal_duration,
                        })
                    })
                    .min_by_key(|proposal| proposal.proposer)
            })
            .collect();
        if proposals
            .par_windows(2)
            .any(|pair| pair[0].body > pair[1].body)
        {
            proposals.par_sort_unstable_by_key(|proposal| proposal.body);
        }
        let mut actors: Vec<_> = voting
            .into_par_iter()
            .chain(proposals.par_iter().map(|proposal| proposal.proposer))
            .collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        if votes.is_empty() && proposals.is_empty() {
            return None;
        }

        if votes
            .par_windows(2)
            .any(|pair| (pair[0].ballot, pair[0].voter) > (pair[1].ballot, pair[1].voter))
        {
            votes.par_sort_unstable_by_key(|vote| (vote.ballot, vote.voter));
        }
        Some(Draft {
            votes,
            proposals,
            actors,
        })
    }
}
#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
