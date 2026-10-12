//! Institutional state and deterministic decisions from explicitly supplied facts.
pub mod case_commands;
pub mod deliberation;
pub mod enforcement;
pub mod institutions;
pub mod perception;
#[cfg(test)]
mod policy_fixtures;
pub mod reactions;
pub mod voting;

pub mod storage;
pub use storage::*;

pub mod proposal_voting;
pub mod save_validation;

#[cfg(test)]
mod storage_tests;

#[cfg(test)]
mod proposal_voting_tests;
