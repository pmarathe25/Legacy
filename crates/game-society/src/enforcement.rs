//! Chosen enforcement vocabulary; this module performs no physical force.
use crate::institutions::Subjects;
use game_core::*;
use game_economy::agreements::AgreementTerm;
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
