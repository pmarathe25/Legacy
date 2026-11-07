//! Snapshot planning for physical custody.
mod changes;
mod prepare;
pub use changes::Changes;
pub use game_lot_allocation::prefixes;
use game_lot_model::{Container, OutcomeBatch, Spoilage, TransactionOutcome};
pub use prepare::prepare;
use std::ops::Range;

/// Transaction membership aligned with the caller's flattened operation arrays.
pub struct Batch<'a> {
    pub groups: &'a [(u64, Range<usize>, Range<usize>)],
    pub move_owners: &'a [u64],
    pub creation_owners: &'a [u64],
    pub additions: &'a [Container],
    /// Sorted transaction IDs whose consuming movement needs acceptance only.
    pub compact: &'a [u64],
}

pub struct Draft {
    pub base_instance: u64,
    pub base_tick: u64,
    pub base_revision: u64,
    pub next_tick: u64,
    pub changes: Changes,
    pub outcomes: OutcomeBatch,
    pub spoiled: Vec<Spoilage>,
    pub creations: Vec<(u64, bool)>,
    pub transactions: Vec<TransactionOutcome>,
}
