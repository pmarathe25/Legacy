//! Personal dispositions and decision evidence, without world access.
pub use crate::planning::*;
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
    pub fn accepts_risk(&self, risk: Risk) -> bool {
        match risk {
            Risk::Safe => true,
            Risk::PhysicalDanger => self.risk_tolerance >= 300,
            Risk::UnconsentedTaking => self.honesty < 250 && self.risk_tolerance >= 700,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Aspiration {
    Comfort,
    Independence,
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
    pub hydration_urgency: i64,
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

/// A receiving relationship reaction reads and changes only affected fields.
#[derive(Clone, Copy, Debug)]
pub struct TrustDelta {
    pub observer: PersonId,
    pub subject: PersonId,
    pub delta: i32,
}
pub struct PreparedTrustReactions {
    changes: Vec<(PersonId, PersonId, Option<Relationship>, Relationship)>,
}
impl PreparedTrustReactions {
    pub fn prepare<A: game_ecs::Component>(
        store: &game_ecs::Store,
        effects: impl IntoIterator<Item = TrustDelta>,
    ) -> Result<Self, Error> {
        let mut changes: Vec<(PersonId, PersonId, Option<Relationship>, Relationship)> = Vec::new();
        for effect in effects {
            let mind = mind::<A>(store, effect.observer).ok_or(Error::Unavailable)?;
            if let Some((_, _, _, after)) = changes.iter_mut().find(|(observer, subject, _, _)| {
                *observer == effect.observer && *subject == effect.subject
            }) {
                after.trust = (after.trust + effect.delta).clamp(-1000, 1000);
                continue;
            }
            let before = mind.relationships.get(&effect.subject).cloned();
            let mut after = before.clone().unwrap_or(Relationship {
                trust: 0,
                affection: 0,
            });
            after.trust = (after.trust + effect.delta).clamp(-1000, 1000);
            changes.push((effect.observer, effect.subject, before, after));
        }
        Ok(Self { changes })
    }
    pub fn check<A: game_ecs::Component>(&self, store: &game_ecs::Store) -> Result<(), Error> {
        for (observer, subject, before, _) in &self.changes {
            if mind::<A>(store, *observer)
                .ok_or(Error::Unavailable)?
                .relationships
                .get(subject)
                != before.as_ref()
            {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub fn check_batch<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        self.check::<A>(store)?;
        if self
            .changes
            .iter()
            .any(|(observer, _, _, _)| batch.touches::<Mind<A>>(game_ecs::StableId(observer.0)))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn commit<A: game_ecs::Component>(self, store: &mut game_ecs::Store) -> Result<(), Error> {
        self.check::<A>(store)?;
        for (observer, subject, _, after) in self.changes {
            mind_mut::<A>(store, observer)
                .unwrap()
                .relationships
                .insert(subject, after);
        }
        Ok(())
    }
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
