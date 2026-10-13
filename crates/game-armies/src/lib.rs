//! Army command relationships refer to existing missions. Physical recruitment,
//! provisioning, travel, combat and individual judgment stay with their owners.
mod state;
mod types;
mod validation;
pub use state::PreparedChange;
mod storage;
pub use storage::*;
mod systems;
pub use systems::*;
pub use types::*;
pub use validation::ValidationCatalog;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod system_tests;

mod interruption;
pub use interruption::*;
