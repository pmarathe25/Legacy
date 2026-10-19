//! Built food stores, local temperatures, and deliberate physical handling.
mod climate;
pub use climate::temperature;
mod decisions;
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Sealed,
    Cellar,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store {
    pub owner: PersonId,
    pub area: AreaId,
    pub kind: Kind,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Work {
    Build {
        job: ScheduleId,
        area: AreaId,
        kind: Kind,
        capacity: u64,
    },
    Handle {
        job: ScheduleId,
        transfer: game_cargo::Transfer,
        reservation: u64,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stores {
    pub enabled: bool,
    pub sites: Table<InventoryId, Store>,
    work: Table<PersonId, Work>,
    pub failed: u64,
}
impl Resource for Stores {
    const KEY: &'static str = "food-storage.sites";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "food-storage.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Build {
    pub person: PersonId,
    pub supplies: InventoryId,
    pub kit: GoodId,
}
impl Request for Build {
    const KEY: &'static str = "food-storage.build";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PutAway {
    pub person: PersonId,
    pub transfer: game_cargo::Transfer,
}
impl Request for PutAway {
    const KEY: &'static str = "food-storage.put-away";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    person: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "food-storage.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "food-storage.cancel";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Built {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub kind: Kind,
}
impl Event for Built {
    const KEY: &'static str = "food-storage.built";
}
fn observe(tx: &mut Transaction<'_>, person: PersonId, inventory: InventoryId) -> Result<()> {
    if tx.registered::<game_knowledge::ObserveStore>() {
        tx.call(game_knowledge::ObserveStore { person, inventory })?;
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("food-storage", 1)?;
    b.resource::<Stores>()?;
    climate::register(b)?;
    b.request::<Enable>(|tx, _| {
        if !tx.get::<Stores>()?.enabled {
            tx.edit::<Stores>()?.enabled = true;
            climate::start(tx)?;
        }
        Ok(())
    })?;
    b.request::<Build>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let area = game_transport::area(tx.get()?, r.person)?;
        require(
            game_cargo::locality(tx, r.supplies)? == area,
            "storage materials are elsewhere",
        )?;
        let tags = &tx.get::<game_objects::Objects>()?.by_id(r.kit)?.tags;
        let kind = if tags.contains("storage:cellar") {
            Kind::Cellar
        } else if tags.contains("storage:sealed") {
            Kind::Sealed
        } else {
            return Err(Error::Rejected("not a storage kit".into()));
        };
        let capacity = tags
            .iter()
            .find_map(|t| t.strip_prefix("storage-capacity:")?.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::Rejected("storage kit has no capacity".into()))?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.supplies,
            purpose: "storage construction".into(),
        })?;
        tx.call(game_cargo::Consume {
            inventory: r.supplies,
            good: r.kit,
            quantity: 1,
        })?;
        let until = tx.now().after(HOUR)?;
        tx.start_action(
            r.person,
            "build-storage",
            Some(until),
            Cancel { person: r.person },
        )?;
        let job = tx.schedule(until, 10, Finish { person: r.person })?;
        tx.edit::<Stores>()?.work.insert(
            r.person,
            Work::Build {
                job,
                area,
                kind,
                capacity,
            },
        );
        Ok(until)
    })?;
    b.request::<PutAway>(|tx, r| {
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.transfer.to,
            purpose: "food storage".into(),
        })?;
        game_physiology::capable(tx, r.person)?;
        let area = game_transport::area(tx.get()?, r.person)?;
        require(
            game_cargo::locality(tx, r.transfer.from)? == area
                && game_cargo::locality(tx, r.transfer.to)? == area,
            "storage handling is not local",
        )?;
        require(
            tx.get::<Stores>()?.sites.contains_key(&r.transfer.to),
            "destination is not a food store",
        )?;
        require(
            tx.get::<game_objects::Objects>()?
                .by_id(r.transfer.good)?
                .food
                .is_some(),
            "store an edible good",
        )?;
        let reservation = tx.call(game_cargo::ReserveSpace {
            inventory: r.transfer.to,
            good: r.transfer.good,
            quantity: r.transfer.quantity,
        })?;
        let until = tx.now().after(SimDuration(2 * MINUTE.0))?;
        tx.start_action(
            r.person,
            "store-food",
            Some(until),
            Cancel { person: r.person },
        )?;
        let job = tx.schedule(until, 10, Finish { person: r.person })?;
        tx.edit::<Stores>()?.work.insert(
            r.person,
            Work::Handle {
                job,
                transfer: r.transfer,
                reservation,
            },
        );
        Ok(until)
    })?;
    b.request::<Finish>(|tx, r| {
        let work = tx
            .edit::<Stores>()?
            .work
            .remove(&r.person)
            .ok_or_else(|| Error::Missing("storage work".into()))?;
        match work {
            Work::Build {
                area,
                kind,
                capacity,
                ..
            } => {
                require(
                    game_transport::area(tx.get()?, r.person)? == area,
                    "builder left the site",
                )?;
                let inventory = tx.call(game_cargo::Create {
                    custody: game_cargo::Custody::Area(area),
                    capacity,
                })?;
                tx.call(game_property::ClaimInventory {
                    claimant: r.person,
                    inventory,
                    recognized_by: [r.person].into(),
                })?;
                tx.call(game_property::Permit {
                    issuer: r.person,
                    person: r.person,
                    inventory,
                    allow: true,
                })?;
                tx.edit::<Stores>()?.sites.insert(
                    inventory,
                    Store {
                        owner: r.person,
                        area,
                        kind,
                    },
                );
                observe(tx, r.person, inventory)?;
                tx.emit(
                    Built {
                        person: r.person,
                        inventory,
                        kind,
                    },
                    vec![r.person],
                )?;
            }
            Work::Handle {
                transfer,
                reservation,
                ..
            } => {
                tx.call(game_cargo::ReleaseSpace {
                    inventory: transfer.to,
                    reservation,
                })?;
                let result = tx.attempt(game_property::Take {
                    actor: r.person,
                    transfer: transfer.clone(),
                    unauthorized: true,
                });
                if result.is_err() {
                    tx.edit::<Stores>()?.failed = tx.get::<Stores>()?.failed.saturating_add(1);
                } else {
                    observe(tx, r.person, transfer.from)?;
                    observe(tx, r.person, transfer.to)?;
                }
            }
        }
        tx.finish_action(
            r.person,
            if tx.get::<Actions>()?.0[&r.person].kind == "build-storage" {
                "build-storage"
            } else {
                "store-food"
            },
        )?;
        Ok(())
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(work) = tx.edit::<Stores>()?.work.remove(&r.person) {
            let job = match work {
                Work::Build { job, .. } => job,
                Work::Handle {
                    job,
                    transfer,
                    reservation,
                } => {
                    tx.call(game_cargo::ReleaseSpace {
                        inventory: transfer.to,
                        reservation,
                    })?;
                    job
                }
            };
            tx.unschedule(job);
        }
        Ok(())
    })?;
    game_choice::provider(b, "c.storage.prepare", |tx, e| {
        decisions::consider(tx, e.person)
    })?;
    Ok(())
}
