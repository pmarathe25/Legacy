//! Society decides local votes and applies eligibility to foreign-domain judgments.
use crate::institutions::*;
use crate::storage::*;
use crate::voting::VoterFacts;
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeMap;
/// A focused foreign owner already evaluated this exact proposal for this voter.
#[derive(Clone, Copy)]
pub struct ForeignVoteJudgment {
    pub proposal: RuleProposalId,
    pub actor: PersonId,
    pub score: i64,
}
/// Invocation owns the penalty; Society owns when local reputation affects a vote.
#[derive(Clone, Copy)]
pub struct ProposerFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub social_penalty: i64,
}
impl Runtime {
    pub fn local_change_score(
        &self,
        store: &Store,
        voter: &VoterFacts<'_>,
        i: &Institution,
        change: &InstitutionChange,
    ) -> Option<i64> {
        Some(match change {
            InstitutionChange::AddRule(rule) => {
                self.rule_vote_score(store, voter, &i.charter, rule)
            }
            InstitutionChange::ReplaceRule { index, rule } => {
                self.rule_vote_score(store, voter, &i.charter, rule)
                    - self.rule_vote_score(
                        store,
                        voter,
                        &i.charter,
                        i.charter.rules.get(*index as usize)?,
                    )
            }
            InstitutionChange::RemoveRule { index } => -self.rule_vote_score(
                store,
                voter,
                &i.charter,
                i.charter.rules.get(*index as usize)?,
            ),
            InstitutionChange::Appoint { holder, .. } => i64::from(voter.knowledge.trust(*holder)),
            InstitutionChange::Procedure(_) | InstitutionChange::Invite(_) => 0,
            InstitutionChange::Dissolve => -self.charter_vote_score(store, voter, &i.charter),
            _ => return None,
        })
    }
    pub fn proposal_vote_requests(
        &self,
        store: &Store,
        authority: &AuthorityFacts,
        voters: &[VoterFacts<'_>],
    ) -> Result<Vec<(RuleProposalId, PersonId)>, Error> {
        let mut by_voter = BTreeMap::new();
        for voter in voters {
            if by_voter.insert(voter.actor, voter).is_some() {
                return Err(Error::Unavailable);
            }
        }
        let mut requests = Vec::new();
        for (_, p) in proposals(store).filter(|(_, p)| p.status == RuleProposalStatus::Proposed) {
            let i = institution(store, p.institution).ok_or(Error::Unavailable)?;
            for actor in i
                .subjects(&i.charter.procedure.voters, authority)
                .intersection(&i.members)
            {
                if by_voter.get(actor).is_some_and(|voter| {
                    voter.autonomous
                        && !p.votes.contains_key(actor)
                        && voter.local_area == Some(i.charter.jurisdiction)
                }) {
                    requests.push((p.id, *actor));
                }
            }
        }
        Ok(requests)
    }
    pub fn proposal_replies(
        &self,
        store: &Store,
        authority: &AuthorityFacts,
        voters: &[VoterFacts<'_>],
        proposers: &[ProposerFacts],
        foreign: &[ForeignVoteJudgment],
    ) -> Result<Vec<(PersonId, InstitutionAction)>, Error> {
        let mut by_voter = BTreeMap::new();
        for voter in voters {
            if by_voter.insert(voter.actor, voter).is_some() {
                return Err(Error::Unavailable);
            }
        }
        let mut by_proposer = BTreeMap::new();
        for proposer in proposers {
            if by_proposer.insert(proposer.actor, proposer).is_some() {
                return Err(Error::Unavailable);
            }
        }
        let mut judgments = BTreeMap::new();
        for judgment in foreign {
            if judgments
                .insert((judgment.proposal, judgment.actor), judgment.score)
                .is_some()
            {
                return Err(Error::Unavailable);
            }
        }
        let mut replies = Vec::new();
        for (_, p) in proposals(store).filter(|(_, p)| p.status == RuleProposalStatus::Proposed) {
            let i = institution(store, p.institution).ok_or(Error::Unavailable)?;
            for actor in i
                .subjects(&i.charter.procedure.voters, authority)
                .intersection(&i.members)
            {
                let Some(voter) = by_voter.get(actor) else {
                    continue;
                };
                if !voter.autonomous
                    || p.votes.contains_key(actor)
                    || voter.local_area != Some(i.charter.jurisdiction)
                {
                    continue;
                }
                let score = if let Some(score) = self.local_change_score(store, voter, i, &p.change)
                {
                    score
                } else {
                    *judgments.get(&(p.id, *actor)).ok_or(Error::Unavailable)?
                };
                let proposer = by_proposer.get(&p.proposer).ok_or(Error::Unavailable)?;
                let penalty = if proposer.area == voter.local_area {
                    proposer.social_penalty
                } else {
                    0
                };
                replies.push((
                    *actor,
                    InstitutionAction::Vote {
                        proposal: p.id,
                        support: score - penalty >= 0,
                    },
                ));
            }
        }
        Ok(replies)
    }
}
