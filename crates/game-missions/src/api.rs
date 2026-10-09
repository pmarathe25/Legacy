//! Objectives are communicated commitments; companions retain their own agency.
use game_core::*;
use game_economy::agreements::AgreementTerm;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionObjective {
    Visit {
        destination: AreaId,
    },
    Deliver {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Rescue {
        person: PersonId,
        return_to: AreaId,
    },
    Defeat {
        person: PersonId,
    },
    Confine {
        person: PersonId,
        until: SimTime,
    },
    Patrol {
        areas: BTreeSet<AreaId>,
        until: SimTime,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionSupply {
    pub carrier: PersonId,
    pub source: InventoryId,
    pub quantity: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionProposal {
    pub leader: PersonId,
    pub recruits: BTreeSet<PersonId>,
    pub objective: MissionObjective,
    pub home: AreaId,
    pub supplies: Vec<MissionSupply>,
    pub compensation: BTreeMap<PersonId, Vec<AgreementTerm>>,
    pub deadline: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionAction {
    Propose {
        proposal: MissionProposal,
    },
    Accept {
        recruitment: RecruitmentId,
    },
    AcceptCounteroffer {
        recruitment: RecruitmentId,
    },
    Refuse {
        recruitment: RecruitmentId,
    },
    Counteroffer {
        recruitment: RecruitmentId,
        compensation: Vec<AgreementTerm>,
    },
    Depart {
        mission: MissionId,
    },
    Objective {
        mission: MissionId,
        objective: MissionObjective,
    },
    Retreat {
        mission: MissionId,
    },
    Leave {
        mission: MissionId,
    },
    Report {
        mission: MissionId,
        recipient: PersonId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionStatus {
    Preparing,
    Traveling,
    Operating,
    Returning,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompanionStatus {
    Offered,
    Accepted,
    Refused,
    Withdrawn,
    Defected,
    Captured,
    Dead,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecruitmentView {
    pub id: RecruitmentId,
    pub person: PersonId,
    pub status: CompanionStatus,
    pub compensation: Vec<AgreementTerm>,
    pub payment_agreement: Option<AgreementId>,
    pub countered: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionView {
    pub person: PersonId,
    pub status: CompanionStatus,
    pub location: Option<Location>,
    pub explanation: String,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionView {
    pub id: MissionId,
    pub sponsor: PersonId,
    pub leader: PersonId,
    pub objective: MissionObjective,
    pub status: MissionStatus,
    pub home: AreaId,
    pub deadline: SimTime,
    pub companions: Vec<CompanionView>,
    pub recruitments: Vec<RecruitmentView>,
    pub supplies: Vec<MissionSupply>,
    pub explanation: String,
    pub observed_at: SimTime,
    pub reported_by: Option<PersonId>,
}
