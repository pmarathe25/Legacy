//! Health rules and owned state. No simulation, renderer or world-service dependency.
mod api;
mod clinical;
mod interval;
pub use interval::*;
mod planner;
mod state;
mod storage;
pub use storage::*;
mod validation;
mod water;
pub use planner::*;
mod actions;
pub use actions::*;
pub use api::*;
pub use clinical::*;
pub use state::*;
pub use validation::*;
pub use water::*;
#[cfg(test)]
mod action_tests;
#[cfg(test)]
mod planner_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_tests;

#[cfg(test)]
fn component_store() -> game_ecs::Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_ecs::Store::with_schema(schema)
}

#[cfg(test)]
mod interval_tests;
#[cfg(test)]
mod owner_storage_tests;
#[cfg(test)]
mod storage_tests;
