//! Property assertions, provenance, and personal knowledge of incidents.
mod api;
mod state;
mod transitions;
pub use transitions::*;
mod cancellation;
pub use cancellation::*;
mod receipt_validation;
mod receipts;
pub use api::*;
pub use receipt_validation::*;
pub use receipts::*;
mod planning;
pub use planning::*;
#[cfg(test)]
mod tests;
mod validation;
pub use state::*;
pub use validation::validate_runtime;

mod storage;
use game_ecs::{Batch, StableId, Store};
pub use storage::*;
mod references;
pub use references::*;

#[cfg(test)]
mod component_tests;

mod menu;
pub use menu::MenuFacts;

#[cfg(test)]
mod menu_tests;

mod custody;
pub use custody::*;

mod material;
pub use material::*;

mod activity_effects;
pub use activity_effects::*;

mod initialization;
pub use initialization::*;
