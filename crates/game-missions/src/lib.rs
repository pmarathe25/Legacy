//! Native service commitments, personal choices, local perception and progression.
//! The assembly binds factual rows and applies owner-prepared physical effects.
mod api;
use game_core::PersonId;
mod state;
pub use api::*;
pub use state::*;
mod decisions;
pub use decisions::*;

impl MissionObjective {
    pub fn target_person(&self) -> Option<PersonId> {
        match self {
            Self::Defeat { person }
            | Self::Rescue { person, .. }
            | Self::Confine { person, .. } => Some(*person),
            _ => None,
        }
    }
    pub fn requires_force(&self) -> bool {
        matches!(self, Self::Defeat { .. } | Self::Confine { .. })
    }
}

mod transitions;
pub use transitions::*;

#[cfg(test)]
mod transition_tests;

mod outcomes;
pub use outcomes::*;

mod observations;
pub use observations::*;

#[cfg(test)]
mod outcome_tests;

mod control;
pub use control::*;

mod engagement;
pub use engagement::*;

mod tactics;
pub use tactics::*;

mod reinforcements;
pub use reinforcements::*;

mod storage;
pub use storage::*;

mod validation;

#[cfg(test)]
mod fixture;

#[cfg(test)]
mod component_tests;

mod cancellation;
pub use cancellation::*;

mod context;
pub use context::*;

mod boundaries;
mod presence;
pub use presence::*;
mod availability;
pub use availability::*;
mod declarations;
pub use declarations::*;
mod communications;
pub use communications::*;
#[cfg(test)]
mod native_flow_tests;
