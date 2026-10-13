use crate::*;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Incarnation {
    pub person: PersonId,
    pub bound_at: SimTime,
    pub died_at: Option<SimTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waiting {
    pub predecessor: PersonId,
    pub died_at: SimTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianGoal {
    Waiting,
    Searching,
    Mentoring,
    Confined,
    Recovering,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MentorshipRules {
    pub lesson_duration: SimDuration,
    pub lesson_gain: u64,
    pub invocation_access_threshold: u64,
}
impl Default for MentorshipRules {
    fn default() -> Self {
        Self {
            lesson_duration: MINUTE,
            lesson_gain: 100,
            invocation_access_threshold: 200,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub independence: u64,
    pub competence: u64,
    pub judgment: u64,
    pub risks: u64,
    pub observed_stage: Option<LifeStage>,
    pub observed_at: SimTime,
    pub ready: bool,
    pub explanation: String,
}
impl Default for Assessment {
    fn default() -> Self {
        Self {
            independence: 0,
            competence: 0,
            judgment: 0,
            risks: 0,
            observed_stage: None,
            observed_at: SimTime(0),
            ready: false,
            explanation: "No independent conduct has been observed".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub request: GuardianRequest,
    pub accepted_at: SimTime,
}

/// A remembered care intention, rather than knowledge of a distant body's needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CareProvision {
    pub delivery: game_logistics::Delivery,
    pub observed_at: SimTime,
}

/// A personally accepted, source-constrained request. The cargo map contains
/// provenance tags clipped by actual stock; it is never a spendable inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedDelivery {
    pub delivery: game_logistics::Delivery,
    pub accepted_at: SimTime,
    pub cargo: BTreeMap<InventoryId, u64>,
    pub last_event: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guardian {
    pub target: Option<PersonId>,
    /// This connection conveys location only. It grants no stock, permissions,
    /// relationships, private intentions, or knowledge to other people.
    pub sensed_location: Option<Location>,
    pub sensed_at: SimTime,
    pub goal: GuardianGoal,
    pub explanation: String,
    pub assessments: BTreeMap<PersonId, Assessment>,
    pub assignment: Option<Assignment>,
    pub requested_delivery: Option<RequestedDelivery>,
    pub provision: Option<CareProvision>,
    pub knows_invocation: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PolicyState {
    pub incarnations: Vec<Incarnation>,
    pub waiting: Option<Waiting>,
    pub guardians: BTreeMap<PersonId, Guardian>,
}

impl World {
    pub fn current(&self) -> Option<PersonId> {
        self.incarnations
            .last()
            .filter(|life| life.died_at.is_none())
            .map(|life| life.person)
    }
}
