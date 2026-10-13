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

pub mod projected_publication;

pub mod personal;
pub mod review;

#[cfg(test)]
mod institution_enforcement_tests;

pub mod rule_publication;
#[cfg(test)]
mod rule_publication_tests;

pub mod personal_receipts;

pub mod personal_completion;

#[cfg(test)]
mod social_atomic_tests;

pub mod receipt_validation;
