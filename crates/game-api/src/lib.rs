//! Renderer-independent commands and committed observations. Time is in milliseconds;
//! quantities use 1,000 fixed-point units per whole good.
use serde::{Deserialize, Serialize};

macro_rules! id {
    ($($name:ident),*) => {$(
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);
    )*};
}
id!(
    PersonId,
    AreaId,
    InventoryId,
    SiteId,
    ConceptId,
    EventId,
    SimTime,
    SimDuration,
    Quantity
);

pub const MINUTE: SimDuration = SimDuration(60_000);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Move {
        destination: AreaId,
    },
    Rest,
    Eat {
        source: InventoryId,
    },
    Load {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Work {
        site: SiteId,
    },
    Cancel,
    Tell {
        recipient: PersonId,
        inventory: InventoryId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    pub caller_sequence: u64,
    pub command: Command,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub caller_sequence: u64,
    pub authoritative_sequence: u64,
    pub accepted_at: SimTime,
}

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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Ruler,
    Guardian,
    Citizen,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    AtArea(AreaId),
    InTransit {
        from: AreaId,
        to: AreaId,
        departed: SimTime,
        arrives: SimTime,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaView {
    pub id: AreaId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    pub revision: u64,
    pub time: SimTime,
    pub ruler: PersonId,
    pub location: Location,
    pub known_areas: Vec<AreaView>,
    pub available_commands: Vec<Command>,
    pub physical: Option<PhysicalView>,
    pub known_stocks: Vec<StockKnowledge>,
    pub nearby_people: Vec<(PersonId, Role)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Good {
    Food,
    Timber,
    Tool,
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
pub enum Activity {
    Idle,
    Rest,
    Eat {
        source: InventoryId,
        completes: SimTime,
    },
    Load {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    },
    Work {
        site: SiteId,
    },
    Tell {
        recipient: PersonId,
        fact: StockKnowledge,
        completes: SimTime,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockView {
    pub id: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
    pub can_access: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalView {
    pub vitals: Vitals,
    pub activity: Activity,
    pub local_stocks: Vec<StockView>,
    pub local_sites: Vec<SiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnowledgeSource {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Departed {
        actor: PersonId,
        from: AreaId,
        to: AreaId,
        arrives: SimTime,
    },
    Arrived {
        actor: PersonId,
        area: AreaId,
    },
    Transferred {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    TransferFailed {
        actor: PersonId,
        source: InventoryId,
    },
    Consumed {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Ate {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Spoiled {
        inventory: InventoryId,
        quantity: Quantity,
    },
    Produced {
        actor: PersonId,
        site: SiteId,
        output: InventoryId,
        quantity: Quantity,
    },
    Died {
        actor: PersonId,
    },
    Interrupted {
        actor: PersonId,
    },
    Recovered {
        actor: PersonId,
    },
    Reported {
        sender: PersonId,
        recipient: PersonId,
        inventory: InventoryId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub time: SimTime,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    TargetReached,
    Paused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvanceResult {
    pub requested: SimTime,
    pub reached: SimTime,
    pub revision: u64,
    pub reason: StopReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioInfo {
    pub id: String,
    pub version: u32,
    pub debug_fixture: bool,
}
