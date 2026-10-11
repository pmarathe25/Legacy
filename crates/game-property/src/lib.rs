//! Property assertions, provenance, and personal knowledge of incidents.
mod api;
mod state;
mod transitions;
pub use transitions::*;
mod receipts;
pub use api::*;
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
