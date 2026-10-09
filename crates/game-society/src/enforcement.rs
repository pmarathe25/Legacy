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
