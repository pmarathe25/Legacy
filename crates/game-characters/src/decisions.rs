//! Personal dispositions and decision evidence, without world access.
use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Personality {
    pub discipline: u16,
    pub ambition: u16,
    pub sociability: u16,
    pub risk_tolerance: u16,
    pub hedonism: u16,
    pub empathy: u16,
    pub aggression: u16,
    pub honesty: u16,
    pub patience: u16,
}

impl Personality {
    pub fn industrious() -> Self {
        Self {
            discipline: 900,
            ambition: 900,
            sociability: 500,
            risk_tolerance: 500,
            hedonism: 100,
            empathy: 500,
            aggression: 100,
            honesty: 800,
            patience: 600,
        }
    }
    pub fn restful() -> Self {
        Self {
            discipline: 100,
            ambition: 100,
            hedonism: 900,
            ..Self::industrious()
        }
    }
    pub fn valid(&self) -> bool {
        [
            self.discipline,
            self.ambition,
            self.sociability,
            self.risk_tolerance,
            self.hedonism,
            self.empathy,
            self.aggression,
            self.honesty,
            self.patience,
        ]
        .iter()
        .all(|v| *v <= 1_000)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dream {
    Independence,
    Comfort,
    SupportOthers,
    Parenthood,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Goal {
    Survival,
    Recovery,
    Practice,
    Rest,
    Care,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub trust: i32,
    pub affection: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub weight: i32,
    pub experiences: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    pub goal: Goal,
    pub need: i64,
    pub benefit: i64,
    pub value: i64,
    pub total: i64,
    pub blocker: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteKnowledge {
    pub site: SiteId,
    pub area: AreaId,
    pub input: InventoryId,
    pub tool: InventoryId,
    pub input_quantity: Quantity,
    pub output: InventoryId,
    pub access: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildSignal {
    pub preference: Option<ChildhoodInterest>,
    pub location: game_core::Location,
    pub urgency: i64,
    pub observed_at: SimTime,
}

#[derive(Clone, Copy, Debug)]
pub struct CandidateScore {
    pub total: i64,
    pub tie: (u8, u64),
    pub resting: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct ChoiceContext {
    pub working: bool,
    pub resting: bool,
    pub urgent: bool,
    pub previous_score: i64,
    pub switching_margin: i64,
}

/// Rank only the alternatives the observer actually knows. Resource feasibility
/// and action execution are the coordinator's responsibility.
pub fn choose(candidates: &[CandidateScore], current: ChoiceContext) -> Option<usize> {
    let (index, candidate) = candidates
        .iter()
        .enumerate()
        .min_by_key(|(index, candidate)| {
            (std::cmp::Reverse(candidate.total), candidate.tie, *index)
        })?;
    if candidate.resting && current.resting
        || current.working
            && !current.urgent
            && i128::from(candidate.total)
                < i128::from(current.previous_score) + i128::from(current.switching_margin)
    {
        return None;
    }
    Some(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn current() -> ChoiceContext {
        ChoiceContext {
            working: false,
            resting: false,
            urgent: false,
            previous_score: 100,
            switching_margin: 10,
        }
    }
    #[test]
    fn equal_scores_use_explicit_logical_keys() {
        let alternatives = [
            CandidateScore {
                total: 100,
                tie: (0, 8),
                resting: false,
            },
            CandidateScore {
                total: 100,
                tie: (0, 3),
                resting: false,
            },
        ];
        assert_eq!(choose(&alternatives, current()), Some(1));
        assert_eq!(choose(&[], current()), None);
    }
    #[test]
    fn switching_respects_commitment_but_an_urgent_need_can_interrupt() {
        let alternatives = [CandidateScore {
            total: 109,
            tie: (0, 3),
            resting: false,
        }];
        let mut current = current();
        current.working = true;
        assert_eq!(choose(&alternatives, current), None);
        current.urgent = true;
        assert_eq!(choose(&alternatives, current), Some(0));
        current.urgent = false;
        current.previous_score = i64::MAX;
        assert_eq!(choose(&alternatives, current), None);
    }
    #[test]
    fn continuing_rest_does_not_restart_a_physical_action() {
        let alternatives = [CandidateScore {
            total: 100,
            tie: (3, 0),
            resting: true,
        }];
        let mut current = current();
        current.resting = true;
        assert_eq!(choose(&alternatives, current), None);
    }
}
