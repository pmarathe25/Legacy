//! Renderer-independent commands and immutable player observations.
pub use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Neighbor {
    pub person: PersonId,
    pub name: String,
    pub action: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Move {
        destination: AreaId,
    },
    Eat {
        inventory: InventoryId,
    },
    Rest,
    ReadWork {
        work: u64,
    },
    PresentWork {
        work: u64,
        listener: PersonId,
    },
    Observe {
        inventory: InventoryId,
    },
    ObserveSurroundings,
    ObservePatch {
        patch: u64,
    },
    Tell {
        recipient: PersonId,
        key: String,
    },
    Talk {
        other: PersonId,
    },
    Take {
        from: InventoryId,
        to: InventoryId,
        good: GoodId,
        quantity: u64,
        unauthorized: bool,
    },
    Sow {
        plot: PlotId,
        seed: InventoryId,
    },
    Harvest {
        plot: PlotId,
        destination: InventoryId,
    },
    Craft {
        recipe: String,
        inventory: InventoryId,
    },
    Plant {
        crop: String,
        seed: InventoryId,
    },
    Pursue {
        activity: String,
        inventory: InventoryId,
    },
    Treat {
        patient: PersonId,
        supplies: InventoryId,
    },
    Gather {
        patch: u64,
        destination: InventoryId,
        quantity: u64,
    },
    Haul {
        from: InventoryId,
        to: InventoryId,
        pack: InventoryId,
        good: GoodId,
        quantity: u64,
    },
    Cancel,
    Accept {
        agreement: AgreementId,
    },
    Reject {
        agreement: AgreementId,
    },
    Fulfill {
        agreement: AgreementId,
        term: usize,
    },
    Vote {
        proposal: u64,
        support: bool,
    },
    Decide {
        proposal: u64,
        support: bool,
    },
    Build {
        project: ProjectId,
    },
    GuardianOrder {
        guardian: PersonId,
        destination: AreaId,
    },
    Invoke {
        lives: std::collections::BTreeSet<LifeId>,
    },
    Consent {
        step: usize,
        accept: bool,
    },
    Dismiss,
    Combat {
        battle: u64,
        action: game_conflict::OrderKind,
    },
    Depart {
        mission: MissionId,
    },
    Return {
        mission: MissionId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    pub time: SimTime,
    pub revision: u64,
    pub person: Option<PersonId>,
    pub body: Option<game_physiology::BodyView>,
    pub location: Option<game_transport::Location>,
    pub action: Option<String>,
    pub facts: game_knowledge::Facts,
    pub warning: Option<game_invocation::Warning>,
    pub control_until: Option<SimTime>,
    pub past_lives: Vec<LifeId>,
    pub borrowed_memories: BTreeMap<String, game_lineage::Life>,
    pub battles: Vec<game_conflict::BattleView>,
    pub neighbors: Vec<Neighbor>,
    pub communities: Vec<game_communities::CommunityView>,
    pub pastimes: BTreeMap<String, u64>,
    #[serde(default)]
    pub lexicon: BTreeMap<String, Vec<game_communication::Association>>,
    #[serde(default)]
    pub works: Vec<game_works::WorkView>,
    pub events: Vec<PlayerEvent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerEvent {
    pub sequence: u64,
    pub at: SimTime,
    pub kind: String,
    pub data: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub time: SimTime,
    pub revision: u64,
    pub result: serde_json::Value,
}
