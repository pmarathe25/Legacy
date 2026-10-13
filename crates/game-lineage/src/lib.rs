//! Lineage identity and guardian judgment, independent of the world coordinator.
mod api;
mod canonical;
mod evaluation;
mod learning;
mod state;
pub use canonical::{History, Runtime, guardian, guardians};
use state::PolicyState as World;
mod canonical_transitions;
pub use canonical_transitions::{PreparedLineage, ProvisionFacts, StagedLineage, Succession};
mod canonical_judgment;
mod gameplay;
pub use gameplay::*;
mod canonical_delivery;
pub use canonical_delivery::*;
mod canonical_references;
#[cfg(test)]
mod canonical_tests;
pub use api::*;
pub use canonical_judgment::{
    AssistanceFacts, RequestDecision, RequestFacts, ReviewAction, ReviewDecision,
    supply_step_can_start,
};
pub use canonical_references::{LineageReferences, PersonReference, StockReference};
pub use evaluation::*;
pub use learning::*;
pub use state::{
    Assessment, Assignment, CareProvision, Guardian, GuardianGoal, Incarnation, MentorshipRules,
    RequestedDelivery, Waiting,
};
#[cfg(test)]
mod tests;

mod delivery;
pub use delivery::*;

#[cfg(test)]
mod delivery_tests;

mod service;
pub use service::*;

mod rescue;
pub use rescue::*;

mod review;
pub use review::*;
