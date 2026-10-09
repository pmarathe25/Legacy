//! Property assertions and personally known incidents are distinct from goods.
use crate::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ClaimRight {
    Use,
    Consume,
    Transfer,
    GrantAccess,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyAction {
    RequestAccess {
        inventory: InventoryId,
    },
    Claim {
        inventory: InventoryId,
        rights: BTreeSet<ClaimRight>,
        evidence: Vec<IncidentId>,
    },
    Recognize {
        claim: ClaimId,
    },
    Contest {
        claim: ClaimId,
    },
    Share {
        inventory: InventoryId,
        recipient: PersonId,
        allowed: bool,
    },
    Take {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    ReportIncident {
        recipient: PersonId,
        incident: IncidentId,
    },
    ResistTaking {
        thief: PersonId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentOutcome {
    Attempt,
    Taken,
    Interrupted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentSource {
    Observed,
    Reported { sender: PersonId },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimView {
    pub id: ClaimId,
    pub claimant: PersonId,
    pub inventory: InventoryId,
    pub rights: BTreeSet<ClaimRight>,
    pub evidence: Vec<IncidentId>,
    pub recognized_by: BTreeSet<PersonId>,
    pub opposed_by: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentView {
    pub id: IncidentId,
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub area: AreaId,
    pub started_at: SimTime,
    pub changed_at: SimTime,
    pub outcome: IncidentOutcome,
    pub learned_at: SimTime,
    pub source_of_knowledge: IncidentSource,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyView {
    pub claims: Vec<ClaimView>,
    pub incidents: Vec<IncidentView>,
}
