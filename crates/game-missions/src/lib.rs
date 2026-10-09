//! Mission commitments and individual service decisions. World scheduling,
//! communication, carrying and force are executed by the coordinator.
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
