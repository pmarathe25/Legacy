//! Active recollection, borrowed mastery, and explicit consent for invocation costs.
use crate::*;
use std::collections::BTreeSet;

pub use game_invocation::{InvocationCost, InvocationWarning};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvocationAction {
    Activate {
        lives: BTreeSet<PersonId>,
    },
    Dismiss,
    AcceptWarning {
        warning: WarningId,
    },
    RecordMemory {
        life: PersonId,
        inventory: InventoryId,
        material: InventoryId,
        tool: InventoryId,
    },
    RecordRoute {
        life: PersonId,
        origin: AreaId,
        destination: AreaId,
        material: InventoryId,
        tool: InventoryId,
    },
    ReadRecord {
        record: RecordId,
    },
    MoveRecord {
        record: RecordId,
        destination: StockPosition,
    },
    TeachMemory {
        life: PersonId,
        inventory: InventoryId,
        recipient: PersonId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordedMemory {
    Stock {
        life: PersonId,
        fact: StockKnowledge,
    },
    Route {
        life: PersonId,
        origin: AreaId,
        destination: AreaId,
        duration: SimDuration,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecordView {
    pub id: RecordId,
    pub inventory: InventoryId,
    pub content: Option<RecordedMemory>,
    pub written_at: SimTime,
    pub author: PersonId,
    pub position: StockPosition,
    pub can_read: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvokedLifeView {
    pub person: PersonId,
    pub died_at: SimTime,
    pub skills: Vec<SkillView>,
    pub stocks: Vec<StockKnowledge>,
    pub routes: Vec<(AreaId, AreaId, SimDuration)>,
    pub institutions: Vec<InstitutionView>,
    pub cases: Vec<SocialCaseView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationPreview {
    pub lives: BTreeSet<PersonId>,
    pub burden_rate: u64,
    pub activation_burden: u64,
    pub activation_fatigue: u64,
    pub borrowed_skills: Vec<(Skill, u16)>,
    pub next_cost_in: Option<SimDuration>,
}
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
