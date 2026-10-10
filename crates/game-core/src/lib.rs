//! Stable identifiers and shared value types. No simulation or client dependency.
use serde::{Deserialize, Serialize};

macro_rules! id {
    ($($name:ident),*) => {$(
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);
    )*};
}
id!(
    RealmCommitmentId,
    EnforcementCaseId,
    JudgmentId,
    ShelterId,
    ResourceId,
    MissionId,
    RecruitmentId,
    PersonId,
    AreaId,
    InventoryId,
    SiteId,
    ConceptId,
    AgreementId,
    WarningId,
    RecordId,
    ProjectId,
    EnterpriseId,
    InstitutionId,
    RuleProposalId,
    CaseId,
    ClaimId,
    IncidentId,
    AmendmentId,
    ObligationId,
    PregnancyId,
    EncounterId,
    CombatMapId,
    EventId,
    SimTime,
    SimDuration,
    Quantity
);

pub const MINUTE: SimDuration = SimDuration(60_000);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Error {
    UnknownArea,
    NoRoute,
    AlreadyThere,
    SequenceConflict,
    InvalidConfiguration(String),
    InvalidFixture(String),
    InvalidSave(String),
    Overflow,
    WorkerFailure,
    HostDisconnected,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Ruler,
    Guardian,
    Citizen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoadPosition {
    pub from: AreaId,
    pub to: AreaId,
    pub progress: SimDuration,
    pub duration: SimDuration,
}
impl RoadPosition {
    pub fn valid(&self) -> bool {
        self.from != self.to && self.duration.0 > 0 && self.progress <= self.duration
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    AtArea(AreaId),
    InTransit {
        from: AreaId,
        to: AreaId,
        departed: SimTime,
        arrives: SimTime,
        road_start: Option<RoadPosition>,
    },
    StoppedOnRoad {
        from: AreaId,
        to: AreaId,
        progress: SimDuration,
        duration: SimDuration,
    },
    Carried {
        carrier: PersonId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Good {
    Herb,
    Food,
    Timber,
    Tool,
    Binding,
    Seed,
    Water,
    Record,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vitals {
    /// Milliseconds of fixture hunger/fatigue exposure, not setting biology.
    pub hunger: u64,
    pub fatigue: u64,
    pub health: u64,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StockPosition {
    Area(AreaId),
    Carried(PersonId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnowledgeSource {
    Recorded(RecordId),
    Observed(PersonId),
    Reported(PersonId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockKnowledge {
    pub inventory: InventoryId,
    pub area: AreaId,
    pub good: Good,
    pub quantity: Quantity,
    pub accessible: bool,
    pub observed_at: SimTime,
    pub learned_at: SimTime,
    pub source: KnowledgeSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifeStage {
    Infant,
    Child,
    Adult,
    Elder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChildhoodInterest {
    Food,
    Company,
    Movement,
    Craft,
}

/// Continuous placement in millimetres, independent of any rendering grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub mod skills;
pub use skills::*;
