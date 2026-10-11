//! Personal institutional judgments and local founding responses.
use crate::institutions::*;
use game_agreements::AgreementTerm;
use game_core::*;
use std::collections::BTreeMap;

/// These ports read facts. The Society owner supplies every judgment and threshold.
pub trait VotingKnowledge {
    fn stock(&self, inventory: InventoryId) -> Option<&StockKnowledge>;
    fn custodian(&self, inventory: InventoryId) -> Option<PersonId>;
    fn trust(&self, person: PersonId) -> i32;
}
#[derive(Clone, Copy)]
pub struct VoterFacts<'a> {
    pub actor: PersonId,
    pub local_area: Option<AreaId>,
    pub autonomous: bool,
    pub empathy: u16,
    pub supports_others: bool,
    pub knowledge: &'a dyn VotingKnowledge,
}
impl World {
    pub fn learned_rule_score(&self, actor: PersonId, rule: &InstitutionalRule) -> i64 {
        let Some(memory) = self.memories.get(&actor) else {
            return 0;
        };
        memory
            .values
            .iter()
            .rev()
            .take(32)
            .map(|(id, value)| {
                let old = &self.concepts[id];
                let same = match (&old.effect, &rule.effect) {
                    (
                        SocialEffect::Access {
                            inventory: a,
                            allowed: aa,
                        },
                        SocialEffect::Access {
                            inventory: b,
                            allowed: ba,
                        },
                    ) => a == b && aa == ba,
                    (SocialEffect::Labor { site: a, .. }, SocialEffect::Labor { site: b, .. }) => {
                        a == b
                    }
                    (
                        SocialEffect::Delivery {
                            source: a,
                            destination: ad,
                            ..
                        },
                        SocialEffect::Delivery {
                            source: b,
                            destination: bd,
                            ..
                        },
                    ) => a == b && ad == bd,
                    _ => false,
                };
                if same { i64::from(value.weight) } else { 0 }
            })
            .sum::<i64>()
            .clamp(-4_000, 4_000)
    }
    pub fn rule_vote_score(
        &self,
        voter: &VoterFacts<'_>,
        charter: &InstitutionCharter,
        rule: &InstitutionalRule,
    ) -> i64 {
        let actor = voter.actor;
        let recipient = rule.subjects.members
            || rule.subjects.named.contains(&actor)
            || rule.subjects.offices.iter().any(|slot| {
                charter
                    .offices
                    .iter()
                    .any(|o| o.slot == *slot && o.holder == actor)
            });
        let mut score = i64::from(voter.empathy) - 500;
        match rule.effect {
            SocialEffect::Access { inventory, allowed } => {
                if recipient
                    && voter
                        .knowledge
                        .stock(inventory)
                        .is_some_and(|k| k.quantity.0 >= 1_000)
                {
                    score += if allowed { 1_200 } else { -1_200 };
                }
                if voter.knowledge.custodian(inventory) == Some(actor) && allowed {
                    score -= 300;
                }
            }
            SocialEffect::Labor {
                millis,
                ref beneficiary,
                ..
            } => {
                if recipient {
                    score -= (millis / 120).min(100_000) as i64;
                }
                if beneficiary == &Beneficiary::Person(actor)
                    || matches!(beneficiary, Beneficiary::Office(slot) if charter.offices.iter().any(|o| o.slot == *slot && o.holder == actor))
                {
                    score += 700;
                }
                if voter.supports_others {
                    score += 300;
                }
            }
            SocialEffect::Delivery {
                quantity,
                ref beneficiary,
                ..
            } => {
                if recipient {
                    score -= quantity.0.min(100_000) as i64;
                }
                if beneficiary == &Beneficiary::Person(actor)
                    || matches!(beneficiary, Beneficiary::Office(slot) if charter.offices.iter().any(|o| o.slot == *slot && o.holder == actor))
                {
                    score += quantity.0.min(100_000) as i64;
                }
            }
        }
        score + self.learned_rule_score(actor, rule)
    }
    pub fn charter_vote_score(&self, voter: &VoterFacts<'_>, charter: &InstitutionCharter) -> i64 {
        let actor = voter.actor;
        let mut score: i64 = charter
            .rules
            .iter()
            .map(|rule| self.rule_vote_score(voter, charter, rule))
            .sum();
        for office in &charter.offices {
            score += i64::from(voter.knowledge.trust(office.holder));
        }
        for contribution in &charter.contributions {
            if contribution.parties()[0] == actor {
                score -= match contribution {
                    AgreementTerm::Labor { millis, .. } => (millis / 120).min(100_000) as i64,
                    _ => contribution.amount().min(100_000) as i64,
                };
            } else if contribution.parties()[1] == actor {
                score += contribution.amount().min(100_000) as i64;
            }
        }
        score
    }

    /// Prepare every response against the same opening state before any command runs.
    pub fn formation_replies(
        &self,
        voters: &[VoterFacts<'_>],
    ) -> Result<Vec<(PersonId, InstitutionAction)>, Error> {
        let mut by_actor = BTreeMap::new();
        for voter in voters {
            if by_actor.insert(voter.actor, voter).is_some() {
                return Err(Error::Unavailable);
            }
        }
        let mut replies = Vec::new();
        for institution in self
            .institutions
            .values()
            .filter(|row| row.status == InstitutionStatus::Proposed)
        {
            for actor in &institution.charter.founders {
                let Some(voter) = by_actor.get(actor) else {
                    continue;
                };
                if !voter.autonomous
                    || voter.local_area != Some(institution.charter.jurisdiction)
                    || institution.members.contains(actor)
                    || institution.declined.contains(actor)
                {
                    continue;
                }
                let action = if self.charter_vote_score(voter, &institution.charter) >= 0 {
                    InstitutionAction::Join {
                        institution: institution.id,
                    }
                } else {
                    InstitutionAction::Decline {
                        institution: institution.id,
                    }
                };
                replies.push((*actor, action));
            }
        }
        Ok(replies)
    }
}
#[cfg(test)]
mod tests;
