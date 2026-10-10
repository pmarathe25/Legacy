use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageView {
    pub invocation_learning: u64,
    pub invocation_ready: bool,
    pub current: Option<PersonId>,
    pub incarnations: Vec<PersonId>,
    pub waiting_since: Option<SimTime>,
    pub guardians: Vec<GuardianSearchView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianSearchView {
    pub ready: bool,
    pub assessment: String,
    pub explanation: String,
    pub guardian: PersonId,
    pub location: Location,
    pub goal: GuardianGoalView,
    pub target: Option<PersonId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianGoalView {
    Waiting,
    Searching,
    Mentoring,
    Confined,
    Recovering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LessonTopic {
    Invocation,
    Craft,
    Travel,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianRequest {
    Supply {
        good: Good,
        source: InventoryId,
        quantity: Quantity,
    },
    Move {
        destination: AreaId,
    },
    Rest,
    Work {
        site: SiteId,
    },
}
