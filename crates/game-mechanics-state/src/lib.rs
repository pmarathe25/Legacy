//! Finite physical apparatus, manual power and personally observed mechanical behavior.
use game_core::*;
pub use game_physics::{
    Config, Effort, Material, Mechanism, Part, Resolution, Task, Transmission, resolve,
    resolve_loaded,
};
use game_sim::*;
use game_storage::{Records, Table};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
macro_rules! request {
    ($ty:ty,$key:literal,$response:ty) => {
        impl game_sim::Request for $ty {
            const KEY: &'static str = $key;
            type Response = $response;
        }
    };
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub area: AreaId,
    pub inventory: InventoryId,
    pub maker: PersonId,
    pub parts: Vec<Part>,
    pub broken: BTreeSet<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub device: Option<u64>,
    pub task: Task,
    pub recipe: Option<String>,
    pub source: Effort,
    pub since: SimTime,
    pub unit: SimDuration,
    pub weights: Option<InventoryId>,
    pub outlet: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Session>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machines {
    pub enabled: bool,
    pub config: Arc<Config>,
    pub devices: Records<u64, Device>,
    pub work: Table<PersonId, Work>,
    pub attempts: Table<PersonId, u64>,
    pub designs: Records<PersonId, BTreeSet<u64>>,
    pub waste: BTreeMap<GoodId, u64>,
}
impl Machines {
    pub fn trial(&self, person: PersonId) -> Option<Experiment> {
        let w = self.work.get(&person).filter(|w| w.recipe.is_none())?;
        Some(Experiment {
            person,
            device: w.device?,
            weights: w.weights?,
            load: w.task.force,
            outlet: w.outlet,
        })
    }
}
impl Resource for Machines {
    const KEY: &'static str = "mechanics.devices";
    fn allocated_ids(&self) -> Vec<u64> {
        self.devices.keys().copied().collect()
    }
    fn validate(&self) -> Result<()> {
        self.config.validate()?;
        require(
            self.work.values().all(|w| {
                w.unit.0 > 0
                    && w.task.force > 0
                    && w.task.travel > 0
                    && w.session.as_ref().is_none_or(|s| {
                        w.device.is_some()
                            && w.weights.is_some()
                            && w.recipe.is_none()
                            && s.until >= w.since
                            && s.trial <= 4
                            && s.limits.hunger_below > 0
                            && s.limits.fatigue_below > 0
                    })
            }),
            "invalid physical work",
        )?;
        for d in self.devices.values() {
            game_physics::validate(&self.config, &d.parts)?;
            require(
                d.broken.iter().all(|i| *i < d.parts.len()),
                "invalid broken component",
            )?;
        }
        Ok(())
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        if !Arc::ptr_eq(&self.config, &old.config) {
            return self.validate();
        }
        for (_, d) in self.devices.changed(&old.devices) {
            game_physics::validate(&self.config, &d.parts)?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Assemble {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub parts: Vec<Part>,
}
request!(Assemble, "mechanics.assemble", u64);
#[derive(Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub person: PersonId,
    pub device: u64,
    pub weights: InventoryId,
    pub load: u64,
    pub outlet: usize,
}
request!(Experiment, "mechanics.experiment", SimTime);
#[derive(Clone, Serialize, Deserialize)]
pub struct Dismantle {
    pub person: PersonId,
    pub device: u64,
    pub inventory: InventoryId,
}
request!(Dismantle, "mechanics.dismantle", ());
#[derive(Clone, Serialize, Deserialize)]
pub struct PrepareLabor {
    pub person: PersonId,
    pub label: String,
    pub task: Task,
    pub baseline: SimDuration,
}
request!(PrepareLabor, "mechanics.prepare-labor", SimDuration);
/// An optional perception-based technique provider chooses an aid; physics revalidates it.
#[derive(Clone, Serialize, Deserialize)]
pub struct SelectAid(pub PrepareLabor);
request!(SelectAid, "mechanics.select-aid", Option<(u64, usize)>);
#[derive(Clone, Serialize, Deserialize)]
pub struct FinishLabor {
    pub person: PersonId,
}
request!(FinishLabor, "mechanics.finish-labor", bool);
#[derive(Clone, Serialize, Deserialize)]
pub struct Built {
    pub person: PersonId,
    pub device: u64,
    pub inventory: InventoryId,
}
impl Event for Built {
    const KEY: &'static str = "mechanics.built";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tested {
    pub person: PersonId,
    pub device: Option<u64>,
    pub lifted: u64,
    pub travel: u64,
    pub broken: Vec<usize>,
    pub source: Effort,
    pub outlet: usize,
    pub task: Option<(String, Task)>,
    /// The tested arrangement survives later listeners dismantling the device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<Part>>,
}
impl Event for Tested {
    const KEY: &'static str = "mechanics.tested";
}
pub use game_physics::Measurement;
pub fn known(tx: &Transaction<'_>, p: PersonId, device: u64, outlet: usize) -> Option<Measurement> {
    let facts = tx.get::<game_knowledge::Knowledge>().ok()?.0.get(&p)?;
    game_knowledge::known(facts, &format!("mechanics:device:{device}:{outlet}"))
}
pub fn ready(tx: &Transaction<'_>, person: PersonId) -> Result<AreaId> {
    game_physiology::capable(tx, person)?;
    require(
        game_vitality::can_manipulate(tx, person),
        "mechanical work exceeds motor capacity",
    )?;
    game_transport::area(tx.get()?, person)
}
pub fn busy(tx: &Transaction<'_>, device: u64) -> bool {
    tx.get::<Machines>()
        .unwrap()
        .work
        .values()
        .any(|w| w.device == Some(device))
}

/// Persistent owner work; worker threads receive only immutable physical inputs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub action: u64,
    pub until: SimTime,
    pub health: u64,
    pub trial: u32,
    pub limits: SessionLimits,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLimits {
    pub hunger_below: u64,
    pub fatigue_below: u64,
}
/// Character-owned admission after request hooks and before reading physics.
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckSessions(pub Vec<PersonId>);
request!(
    CheckSessions,
    "characters.check-mechanics-sessions",
    Option<SessionLimits>
);
#[derive(Clone, Serialize, Deserialize)]
pub struct InterruptSession(pub PersonId);
request!(InterruptSession, "mechanics.interrupt-session", ());
pub fn session_active(tx: &Transaction<'_>, person: PersonId) -> bool {
    tx.get::<Machines>()
        .is_ok_and(|m| m.work.get(&person).is_some_and(|w| w.session.is_some()))
}
pub fn source(tx: &Transaction<'_>, person: PersonId) -> Effort {
    Effort {
        force: game_vitality::factor(game_vitality::ability(tx, person, "strength")),
        travel: game_vitality::factor(game_vitality::ability(tx, person, "agility")),
    }
}
