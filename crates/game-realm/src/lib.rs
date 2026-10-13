//! Regional commitments reference existing institutions and physical work.
//! Native ratification, communication, agreements, passage and missions are
//! applied by their owners through explicit receipts; this crate owns none of
//! their people, goods or decision procedures.
mod publications;
pub use publications::*;
mod admission;
mod commands;
mod communications;
mod observation;
pub use admission::*;
mod reads;
pub use communications::*;
mod policy;
pub use policy::*;
mod work;
pub use work::*;
mod context;
pub use context::*;
mod storage;
pub use storage::*;
mod types;
pub use types::*;
mod state;
mod validation;
pub use state::{PreparedChange, stage_changes};
mod knowledge;
mod persistence;
pub use persistence::{PendingFacts, TalkActivity};

#[cfg(test)]
mod tests;

mod cursor;
pub use cursor::*;
mod workflow;
pub use workflow::*;

mod newborn;
pub use newborn::*;

pub mod receipt_validation;
