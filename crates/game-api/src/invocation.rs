//! Active recollection, borrowed mastery, and explicit consent for invocation costs.
use crate::*;
use std::collections::BTreeSet;

pub use game_invocation::{InvocationCost, InvocationWarning};

pub use game_invocation::{InvocationAction, InvokedLifeView, MemoryRecordView, RecordedMemory};

pub use game_invocation::InvocationPreview;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationView {
    pub ready: bool,
    pub available_lives: Vec<PersonId>,
    pub active_lives: BTreeSet<PersonId>,
    pub burden: u64,
    pub exposure: SimDuration,
    pub uninterrupted: SimDuration,
    pub temporary_identity: u16,
    pub emotional_disruption: u16,
    pub permanent_identity: u16,
    pub lifespan_loss: SimDuration,
    pub player_control: bool,
    pub loss_ends: Option<SimTime>,
    pub warning: Option<InvocationWarning>,
    pub preview: InvocationPreview,
    pub memories: Vec<InvokedLifeView>,
    pub records: Vec<MemoryRecordView>,
}
