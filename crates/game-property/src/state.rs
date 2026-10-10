use crate::*;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    pub claimant: PersonId,
    pub inventory: InventoryId,
    pub rights: BTreeSet<ClaimRight>,
    pub evidence: Vec<IncidentId>,
    /// Marker zero is unclaimed output. Positive markers identify snapshots
    /// taken when somebody first asserted a claim, not a second stock ledger.
    pub portions: BTreeSet<ClaimId>,
    pub recognized_by: BTreeSet<PersonId>,
    pub opposed_by: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: IncidentId,
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub area: AreaId,
    pub started_at: SimTime,
    pub completes: SimTime,
    pub changed_at: SimTime,
    pub outcome: IncidentOutcome,
    pub portions: BTreeMap<ClaimId, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub area: AreaId,
    pub started_at: SimTime,
    pub completes: SimTime,
    pub snapshot: IncidentKnowledge,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentKnowledge {
    pub incident: Incident,
    pub learned_at: SimTime,
    pub source: IncidentSource,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attitude {
    pub exclusive_claims: u16,
    pub sharing: u16,
    pub willingness_to_defend: u16,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub claims: BTreeMap<ClaimId, Claim>,
    pub incidents: BTreeMap<IncidentId, Incident>,
    pub known_claims: BTreeMap<PersonId, BTreeMap<ClaimId, ClaimView>>,
    pub knowledge: BTreeMap<PersonId, BTreeMap<IncidentId, IncidentKnowledge>>,
    pub attitudes: BTreeMap<PersonId, Attitude>,
    /// This partition records provenance, and must sum to physical quantity.
    pub portions: BTreeMap<InventoryId, BTreeMap<ClaimId, u64>>,
    pub pending: BTreeMap<PersonId, IncidentId>,
    pub reports: BTreeMap<PersonId, Report>,
    pub last_event: u64,
    pub next_search: BTreeMap<PersonId, SimTime>,
}
