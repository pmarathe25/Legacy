//! Executable, bounded social arrangements people can propose and recognize.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subjects {
    pub members: bool,
    pub named: BTreeSet<PersonId>,
    pub offices: BTreeSet<u16>,
}
impl Subjects {
    pub fn members() -> Self {
        Self {
            members: true,
            named: BTreeSet::new(),
            offices: BTreeSet::new(),
        }
    }
    pub fn named(actor: PersonId) -> Self {
        Self {
            members: false,
            named: BTreeSet::from([actor]),
            offices: BTreeSet::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionProcedure {
    pub proposers: Subjects,
    pub voters: Subjects,
    pub minimum_support: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialCondition {
    Local,
    Member,
    HungerAtLeast(u64),
    KnownStockAtLeast {
        inventory: InventoryId,
        quantity: Quantity,
    },
    Excluded {
        inventory: InventoryId,
    },
    KnownCooperationAtLeast(u16),
    KnownDefaultsAtLeast(u16),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleExpression {
    Fact(SocialCondition),
    All(Vec<RuleExpression>),
    Any(Vec<RuleExpression>),
    Not(Box<RuleExpression>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Beneficiary {
    Person(PersonId),
    Office(u16),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialEffect {
    Access {
        inventory: InventoryId,
        allowed: bool,
    },
    Labor {
        site: SiteId,
        millis: u64,
        beneficiary: Beneficiary,
        due_after: SimDuration,
    },
    Delivery {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        beneficiary: Beneficiary,
        due_after: SimDuration,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstitutionalRule {
    pub subjects: Subjects,
    pub when: RuleExpression,
    pub effect: SocialEffect,
    pub authority: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OfficePower {
    Access,
    Labor,
    Delivery,
    HearAccessCases,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeMandate {
    pub slot: u16,
    pub holder: PersonId,
    pub inventories: BTreeSet<InventoryId>,
    pub sites: BTreeSet<SiteId>,
    pub powers: BTreeSet<OfficePower>,
    pub minimum_support: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstitutionCharter {
    pub purpose: String,
    pub jurisdiction: AreaId,
    pub founders: BTreeSet<PersonId>,
    pub open_membership: bool,
    pub procedure: DecisionProcedure,
    pub offices: Vec<OfficeMandate>,
    /// Ordered policies: the last applicable access effect wins within this
    /// institution. Other institutions and independent permissions retain theirs.
    pub rules: Vec<InstitutionalRule>,
    pub contributions: Vec<AgreementTerm>,
    #[serde(default)]
    pub evidence: BTreeSet<CaseId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstitutionChange {
    AddRule(InstitutionalRule),
    ReplaceRule { index: u16, rule: InstitutionalRule },
    RemoveRule { index: u16 },
    Procedure(DecisionProcedure),
    Appoint { office: u16, holder: PersonId },
    Invite(PersonId),
    Dissolve,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstitutionAction {
    Found {
        charter: InstitutionCharter,
        origin: Option<InstitutionId>,
    },
    Join {
        institution: InstitutionId,
    },
    Decline {
        institution: InstitutionId,
    },
    Leave {
        institution: InstitutionId,
    },
    Propose {
        institution: InstitutionId,
        change: InstitutionChange,
    },
    Vote {
        proposal: RuleProposalId,
        support: bool,
    },
    SupportOffice {
        institution: InstitutionId,
        office: u16,
        support: bool,
    },
    HearCase {
        institution: InstitutionId,
        office: u16,
        case: CaseId,
        share: bool,
    },
    TellCase {
        recipient: PersonId,
        case: CaseId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstitutionStatus {
    Proposed,
    Active,
    Rejected,
    Dissolved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleProposalStatus {
    Proposed,
    Adopted,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialCaseKind {
    Exclusion,
    Cooperation,
    Default,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialOutcome {
    Unresolved,
    Shared,
    Contributed,
    Defaulted,
    Refused,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialCaseView {
    pub id: CaseId,
    pub kind: SocialCaseKind,
    pub subject: PersonId,
    pub counterpart: PersonId,
    pub area: AreaId,
    pub inventory: Option<InventoryId>,
    pub good: Good,
    pub need_band: u8,
    pub reciprocal: bool,
    pub outcome: SocialOutcome,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeView {
    pub mandate: OfficeMandate,
    pub supporters: BTreeSet<PersonId>,
    pub effective: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleProposalView {
    pub id: RuleProposalId,
    pub proposer: PersonId,
    pub change: InstitutionChange,
    pub votes: BTreeMap<PersonId, bool>,
    pub status: RuleProposalStatus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstitutionView {
    pub id: InstitutionId,
    pub origin: Option<InstitutionId>,
    pub charter: InstitutionCharter,
    pub members: BTreeSet<PersonId>,
    pub declined: BTreeSet<PersonId>,
    pub status: InstitutionStatus,
    pub offices: Vec<OfficeView>,
    pub proposals: Vec<RuleProposalView>,
    pub funding: Vec<AgreementId>,
    pub duties: Vec<AgreementId>,
    pub observed_at: SimTime,
}
