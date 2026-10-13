//! Chosen enforcement vocabulary; this module performs no physical force.
use crate::institutions::Subjects;
use game_agreements::AgreementTerm;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EnforcementTopic {
    Taking,
    Assault,
    Default,
    Exclusion,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RemedyKind {
    Dismiss,
    Settlement,
    Confinement,
    Deposition,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnforcementPolicy {
    pub topics: BTreeSet<EnforcementTopic>,
    pub areas: BTreeSet<AreaId>,
    pub subjects: Subjects,
    pub enforcer_office: u16,
    pub remedies: BTreeSet<RemedyKind>,
    /// Maximum promised quantities per case, scoped to actual office mandates.
    pub spending: BTreeMap<InventoryId, Quantity>,
    /// Provisions and equipment must still be physically collected by volunteers.
    pub supplies: BTreeMap<InventoryId, Quantity>,
    pub maximum_confinement: SimDuration,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnforcementRemedy {
    Dismiss,
    Settlement { terms: Vec<AgreementTerm> },
    Confine { until: SimTime },
    Depose { office: u16, replacement: PersonId },
}
impl EnforcementRemedy {
    pub fn kind(&self) -> RemedyKind {
        match self {
            Self::Dismiss => RemedyKind::Dismiss,
            Self::Settlement { .. } => RemedyKind::Settlement,
            Self::Confine { .. } => RemedyKind::Confinement,
            Self::Depose { .. } => RemedyKind::Deposition,
        }
    }
}

/// The complaint has not reached its officer, so interruption withdraws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplaintInterruption {
    pub changed_at: SimTime,
    pub explanation: &'static str,
}
pub fn complaint_interruption(now: SimTime, filing: bool) -> Option<ComplaintInterruption> {
    filing.then_some(ComplaintInterruption {
        changed_at: now,
        explanation: "The complaint was interrupted before the officer received it",
    })
}

/// Promised spending and supplies are requests, not available balances.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DispatchBudget {
    pub spending: BTreeMap<InventoryId, u128>,
    pub supplies: BTreeMap<InventoryId, u128>,
}
impl DispatchBudget {
    pub fn validate(&self, policy: &EnforcementPolicy) -> Result<(), Error> {
        if self.spending.iter().any(|(id, q)| {
            policy
                .spending
                .get(id)
                .is_none_or(|cap| *q > u128::from(cap.0))
        }) || self.supplies.iter().any(|(id, q)| {
            policy
                .supplies
                .get(id)
                .is_none_or(|cap| *q > u128::from(cap.0))
        }) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_an_unreceived_filing_is_withdrawn() {
        assert_eq!(complaint_interruption(SimTime(9), false), None);
        let change = complaint_interruption(SimTime(9), true).unwrap();
        assert_eq!(change.changed_at, SimTime(9));
        assert_eq!(
            change.explanation,
            "The complaint was interrupted before the officer received it"
        );
    }
    #[test]
    fn several_promises_share_one_chosen_cap_without_creating_funds() {
        let policy = EnforcementPolicy {
            topics: BTreeSet::from([EnforcementTopic::Taking]),
            areas: BTreeSet::from([AreaId(1)]),
            subjects: Subjects::members(),
            enforcer_office: 1,
            remedies: BTreeSet::from([RemedyKind::Confinement]),
            spending: BTreeMap::from([(InventoryId(2), Quantity(2_000))]),
            supplies: BTreeMap::from([(InventoryId(3), Quantity(1_000))]),
            maximum_confinement: SimDuration(600_000),
        };
        let okay = DispatchBudget {
            spending: BTreeMap::from([(InventoryId(2), 2_000)]),
            supplies: BTreeMap::from([(InventoryId(3), 1_000)]),
        };
        assert_eq!(okay.validate(&policy), Ok(()));
        let excess = DispatchBudget {
            spending: BTreeMap::from([(InventoryId(2), 2_001)]),
            ..okay.clone()
        };
        assert_eq!(excess.validate(&policy), Err(Error::Unavailable));
        let foreign = DispatchBudget {
            supplies: BTreeMap::from([(InventoryId(4), 1)]),
            ..okay
        };
        assert_eq!(foreign.validate(&policy), Err(Error::Unavailable));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EnforcementEvidenceRef {
    Incident(IncidentId),
    Assault(EventId),
    Default(ObligationId),
    Social(CaseId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessedAssault {
    pub event: EventId,
    pub attacker: PersonId,
    pub target: PersonId,
    pub area: AreaId,
    pub damage: u64,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnforcementEvidence {
    Incident(game_property::IncidentView),
    Assault(WitnessedAssault),
    Default(game_agreements::ObligationView),
    Social(crate::institutions::SocialCaseView),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportedContact {
    pub person: PersonId,
    pub area: AreaId,
    pub observed_at: SimTime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnforcementStatus {
    Filing,
    Pending,
    Dismissed,
    Decided,
    Offered,
    Enforcing,
    Completed,
    Refused,
    Failed,
    Withdrawn,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnforcementJudgmentView {
    pub id: JudgmentId,
    pub proposer: PersonId,
    pub proposal: RuleProposalId,
    pub remedy: EnforcementRemedy,
    pub adopted_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnforcementCaseView {
    pub id: EnforcementCaseId,
    pub institution: InstitutionId,
    pub office: u16,
    pub filer: PersonId,
    pub target: PersonId,
    pub topic: EnforcementTopic,
    /// This is evidence someone communicated, not a universal finding of guilt.
    pub evidence: EnforcementEvidence,
    pub contact: Option<ReportedContact>,
    pub judgment: Option<EnforcementJudgmentView>,
    pub mission: Option<MissionId>,
    pub settlement: Option<AgreementId>,
    pub status: EnforcementStatus,
    pub explanation: String,
    pub observed_at: SimTime,
    pub learned_at: SimTime,
    pub source: KnowledgeSource,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnforcementPolicyView {
    pub institution: InstitutionId,
    pub office: u16,
    pub policy: EnforcementPolicy,
    pub adopted_at: SimTime,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnforcementView {
    pub policies: Vec<EnforcementPolicyView>,
    pub cases: Vec<EnforcementCaseView>,
    pub assaults: Vec<WitnessedAssault>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnforcementAction {
    File {
        institution: InstitutionId,
        office: u16,
        evidence: EnforcementEvidenceRef,
    },
    Report {
        case: EnforcementCaseId,
        recipient: PersonId,
    },
    Respond {
        case: EnforcementCaseId,
        comply: bool,
    },
    Dispatch {
        case: EnforcementCaseId,
        proposal: game_missions::MissionProposal,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub id: EnforcementCaseId,
    pub institution: InstitutionId,
    pub office: u16,
    pub filer: PersonId,
    pub target: PersonId,
    pub topic: EnforcementTopic,
    pub evidence: EnforcementEvidence,
    pub contact: Option<ReportedContact>,
    pub policy: EnforcementPolicy,
    pub judgment: Option<EnforcementJudgmentView>,
    pub mission: Option<MissionId>,
    pub settlement: Option<AgreementId>,
    pub status: EnforcementStatus,
    pub explanation: String,
    pub changed_at: SimTime,
    pub notice_area: Option<AreaId>,
    pub witnesses: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conversation {
    File,
    Report {
        snapshot: Box<EnforcementCaseView>,
        institution: Box<crate::institutions::InstitutionView>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Talk {
    pub case: EnforcementCaseId,
    pub recipient: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub conversation: Conversation,
}

#[path = "enforcement_storage.rs"]
mod storage;
pub use storage::*;
#[path = "enforcement_systems.rs"]
mod systems;
pub use systems::{
    ActivityEffect, Event, MindFacts, PersonFacts, PhysicalFacts, Port, Strike, StrikeRecord,
    System,
};

pub fn allocation_bound(runtime: Option<&World>, people: usize) -> u64 {
    runtime.map_or(0, |_| people as u64 * 128)
}

pub fn validate_runtime_presence(
    store: &game_ecs::Store,
    runtime: Option<&World>,
) -> Result<(), Error> {
    if runtime.is_none()
        && (cases(store).next().is_some()
            || talks(store).next().is_some()
            || policies(store).next().is_some()
            || memories(store).next().is_some())
    {
        return Err(Error::InvalidSave(
            "enforcement components have no runtime".into(),
        ));
    }
    Ok(())
}
#[path = "enforcement_adoption.rs"]
mod adoption;
pub use adoption::{
    AdoptionObservationFacts, AdoptionPublication, ObservationPublication, PreparedAdoption,
    adoption_observer_areas, prepare_adoption, stage_adoption_observation,
};

pub(super) fn view_of_case(
    case: &Case,
    actor: PersonId,
    now: SimTime,
    source: KnowledgeSource,
) -> EnforcementCaseView {
    EnforcementCaseView {
        id: case.id,
        institution: case.institution,
        office: case.office,
        filer: case.filer,
        target: case.target,
        topic: case.topic,
        evidence: case.evidence.clone(),
        contact: case.contact.clone(),
        judgment: case.judgment.clone(),
        mission: case.mission,
        settlement: case.settlement,
        status: case.status,
        explanation: case.explanation.clone(),
        observed_at: case.changed_at,
        learned_at: now,
        source: if matches!(source, KnowledgeSource::Observed(_)) {
            KnowledgeSource::Observed(actor)
        } else {
            source
        },
    }
}

#[path = "enforcement_observation.rs"]
mod observation;
pub use observation::{EnforcementObservationPublication, stage_enforcement_observation};
