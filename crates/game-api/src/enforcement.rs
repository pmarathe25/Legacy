//! Accusations, chosen remedies, and funded requests remain distinct from force.
use crate::*;

pub use game_society::enforcement::{
    EnforcementPolicy, EnforcementRemedy, EnforcementTopic, RemedyKind,
};
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
    Incident(IncidentView),
    Assault(WitnessedAssault),
    Default(ObligationView),
    Social(SocialCaseView),
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
        proposal: MissionProposal,
    },
}
