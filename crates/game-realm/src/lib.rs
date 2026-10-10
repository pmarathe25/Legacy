//! Regional commitments reference existing institutions and physical work.
//! Native ratification, communication, agreements, passage and missions are
//! applied by their owners through explicit receipts; this crate owns none of
//! their people, goods or decision procedures.
mod types;
pub use types::*;
mod state;
mod validation;
pub use state::PreparedChange;
mod knowledge;
mod persistence;

#[cfg(test)]
mod tests;
