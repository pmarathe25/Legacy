//! Health rules and owned state. No simulation, renderer or world-service dependency.
mod api;
mod clinical;
mod state;
mod validation;
mod water;
pub use api::*;
pub use clinical::*;
pub use state::*;
pub use validation::*;
pub use water::*;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_tests;
