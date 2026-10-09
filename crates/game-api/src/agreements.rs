//! Agreements describe promises and chosen procedures, never physical transfers.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementTerm {
    ConstructionLabor {
        debtor: PersonId,
        creditor: PersonId,
        project: ProjectId,
        millis: u64,
        deadline: SimTime,
    },
    Delivery {
        debtor: PersonId,
        creditor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        deadline: SimTime,
    },
    Labor {
        debtor: PersonId,
        creditor: PersonId,
        site: SiteId,
        millis: u64,
        deadline: SimTime,
    },
    Access {
        grantor: PersonId,
        grantee: PersonId,
        inventory: InventoryId,
        allowed: bool,
    },
}
impl AgreementTerm {
    pub fn parties(&self) -> [PersonId; 2] {
        match *self {
            Self::ConstructionLabor {
                debtor, creditor, ..
            }
            | Self::Delivery {
                debtor, creditor, ..
            }
            | Self::Labor {
                debtor, creditor, ..
            } => [debtor, creditor],
            Self::Access {
                grantor, grantee, ..
            } => [grantor, grantee],
        }
    }
    pub fn amount(&self) -> u64 {
        match *self {
            Self::Delivery { quantity, .. } => quantity.0,
            Self::Labor { millis, .. } | Self::ConstructionLabor { millis, .. } => millis,
            Self::Access { .. } => 1,
        }
    }
    pub fn deadline(&self) -> Option<SimTime> {
        match *self {
            Self::Delivery { deadline, .. }
            | Self::Labor { deadline, .. }
            | Self::ConstructionLabor { deadline, .. } => Some(deadline),
            Self::Access { .. } => None,
        }
    }
}

/// Parties choose this procedure when accepting the original agreement.
/// Absence leaves amendments unresolved; it does not invent a fallback law.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendmentProcedure {
    pub eligible_proposers: BTreeSet<PersonId>,
    pub approvers: BTreeSet<PersonId>,
    pub threshold: usize,
    /// Named approvers required for particular term indices, in addition to quorum.
    pub term_approvers: BTreeMap<u16, BTreeSet<PersonId>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementAction {
    Report {
        agreement: AgreementId,
        recipient: PersonId,
    },
    Propose {
        parties: BTreeSet<PersonId>,
        terms: Vec<AgreementTerm>,
        procedure: Option<AmendmentProcedure>,
    },
    Counteroffer {
        agreement: AgreementId,
        terms: Vec<AgreementTerm>,
        procedure: Option<AmendmentProcedure>,
    },
    Accept {
        agreement: AgreementId,
    },
    Reject {
        agreement: AgreementId,
    },
    Withdraw {
        agreement: AgreementId,
    },
    Amend {
        agreement: AgreementId,
        replacements: BTreeMap<u16, AgreementTerm>,
    },
    Vote {
        amendment: AmendmentId,
        support: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementStatus {
    Proposed,
    Active,
    Fulfilled,
    Rejected,
    Withdrawn,
    Breached,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentStatus {
    Proposed,
    Adopted,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationView {
    pub id: ObligationId,
    pub term: AgreementTerm,
    pub performed: u64,
    pub defaulted: bool,
    #[serde(default)]
    pub cargo: BTreeMap<InventoryId, u64>,
    #[serde(default)]
    pub observed_at: SimTime,
    #[serde(default)]
    pub learned_at: SimTime,
    #[serde(default)]
    pub source: Option<KnowledgeSource>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgreementView {
    pub id: AgreementId,
    pub proposer: PersonId,
    pub parties: BTreeSet<PersonId>,
    pub accepted: BTreeSet<PersonId>,
    pub status: AgreementStatus,
    pub terms: Vec<AgreementTerm>,
    pub obligations: Vec<ObligationView>,
    pub procedure: Option<AmendmentProcedure>,
    pub amendments: Vec<AmendmentView>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub observed_at: SimTime,
    #[serde(default)]
    pub learned_at: SimTime,
    #[serde(default)]
    pub source: Option<KnowledgeSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendmentView {
    pub id: AmendmentId,
    pub proposer: PersonId,
    pub replacements: BTreeMap<u16, AgreementTerm>,
    pub votes: BTreeMap<PersonId, bool>,
    pub status: AmendmentStatus,
    #[serde(default)]
    pub base_revision: u64,
}
