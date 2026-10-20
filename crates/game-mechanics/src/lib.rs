//! Finite physical apparatus, manual power and personally observed mechanical behavior.
use game_core::*;
pub use game_physics::{
    Config, Effort, Material, Part, Resolution, Task, Transmission, resolve, resolve_loaded,
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
mod catalog;
pub use catalog::{Install, load};
mod operation;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub area: AreaId,
    pub inventory: InventoryId,
    pub maker: PersonId,
    pub parts: Vec<Part>,
    pub broken: BTreeSet<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Work {
    device: Option<u64>,
    task: Task,
    recipe: Option<String>,
    source: Effort,
    since: SimTime,
    unit: SimDuration,
    weights: Option<InventoryId>,
    outlet: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machines {
    pub enabled: bool,
    pub config: Arc<Config>,
    pub devices: Records<u64, Device>,
    work: Table<PersonId, Work>,
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
            self.work
                .values()
                .all(|w| w.unit.0 > 0 && w.task.force > 0 && w.task.travel > 0),
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
}
impl Event for Tested {
    const KEY: &'static str = "mechanics.tested";
}
pub use game_physics::Measurement;
pub fn known(tx: &Transaction<'_>, p: PersonId, device: u64, outlet: usize) -> Option<Measurement> {
    let facts = tx.get::<game_knowledge::Knowledge>().ok()?.0.get(&p)?;
    game_knowledge::known(facts, &format!("mechanics:device:{device}:{outlet}"))
}
fn ready(tx: &Transaction<'_>, person: PersonId) -> Result<AreaId> {
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
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("mechanics", 1)?;
    b.resource::<Machines>()?;
    catalog::register(b)?;
    operation::register(b)?;
    b.validate_save(|h| {
        h.get::<Machines>()?.config.validate_objects(h.get()?)?;
        for (person, w) in &h.get::<Machines>()?.work {
            require(
                h.get::<Actions>()?.0.contains_key(person)
                    && w.device.is_none_or(|id| {
                        h.get::<Machines>()
                            .unwrap()
                            .devices
                            .get(&id)
                            .is_some_and(|d| w.outlet < d.parts.len())
                    }),
                "physical work lacks its actor or apparatus",
            )?;
        }
        for d in h.get::<Machines>()?.devices.values() {
            require(
                h.get::<game_cargo::Cargo>()?
                    .0
                    .get(&d.inventory)
                    .is_some_and(|i| {
                        i.area(h.get::<game_transport::Positions>().unwrap()) == Some(d.area)
                    }),
                "apparatus moved from its site",
            )?;
        }
        Ok(())
    });
    Ok(())
}
