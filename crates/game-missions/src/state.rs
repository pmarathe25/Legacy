use crate::*;
use game_agreements::AgreementTerm;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Companion {
    pub confinement: Option<ConfinementObservation>,
    pub person: PersonId,
    pub recruitment: RecruitmentId,
    pub status: CompanionStatus,
    pub directive: MissionObjective,
    pub received_at: SimTime,
    pub visited: BTreeSet<AreaId>,
    pub delivered: u64,
    pub cargo: BTreeMap<InventoryId, u64>,
    /// Actual supply receipts, keyed by promised source then carried compartment.
    /// These tags qualify existing goods and cannot be spent as inventory.
    pub supply_cargo: BTreeMap<InventoryId, BTreeMap<InventoryId, u64>>,
    pub contacts: BTreeMap<PersonId, AreaId>,
    pub last_leg_from: Option<AreaId>,
    pub explanation: String,
    pub declared_explanation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfinementObservation {
    pub person: PersonId,
    pub captured_at: SimTime,
    pub released_at: Option<SimTime>,
}
impl ConfinementObservation {
    pub fn finished(&self, person: PersonId, until: SimTime, now: SimTime) -> bool {
        self.person == person
            && self.captured_at < until
            && self.released_at.is_some_and(|at| at >= until && at <= now)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mission {
    pub id: MissionId,
    pub sponsor: PersonId,
    pub proposal: MissionProposal,
    pub status: MissionStatus,
    pub members: BTreeMap<PersonId, Companion>,
    pub created_at: SimTime,
    pub departed_at: Option<SimTime>,
    pub explanation: String,
    pub succeeded: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recruitment {
    pub id: RecruitmentId,
    pub mission: MissionId,
    pub person: PersonId,
    pub compensation: Vec<AgreementTerm>,
    pub agreement: Option<AgreementId>,
    pub countered: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conversation {
    Objective {
        mission: MissionId,
        leader: PersonId,
        objective: MissionObjective,
    },
    Report {
        recipient: PersonId,
        snapshot: Box<MissionView>,
    },
    Retreat {
        mission: MissionId,
        leader: PersonId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTalk {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub conversation: Conversation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub missions: BTreeMap<MissionId, Mission>,
    pub recruitments: BTreeMap<RecruitmentId, Recruitment>,
    pub known: BTreeMap<PersonId, BTreeMap<MissionId, MissionView>>,
    pub talks: BTreeMap<PersonId, PendingTalk>,
    pub last_event: u64,
    pub last_review: SimTime,
}
