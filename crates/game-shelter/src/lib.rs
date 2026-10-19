//! Built rooms use ordinary areas, roads, inventories, and local observation.
mod climate;
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub outside: AreaId,
    pub owner: PersonId,
    pub stores: InventoryId,
    pub insulation: i32,
    pub heat: i32,
    pub burning_until: SimTime,
    pub fire: Option<ScheduleId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Construction {
    outside: AreaId,
    job: ScheduleId,
    insulation: i32,
    heat: i32,
    name: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shelters {
    pub enabled: bool,
    pub rooms: Table<AreaId, Room>,
    work: Table<PersonId, Construction>,
    pub fuel_used: u64,
    pub fuel_for: Table<PersonId, AreaId>,
}
impl Resource for Shelters {
    const KEY: &'static str = "shelter.rooms";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "shelter.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Build {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub kit: GoodId,
}
impl Request for Build {
    const KEY: &'static str = "shelter.build";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Stoke {
    pub person: PersonId,
    pub room: AreaId,
    pub inventory: InventoryId,
    pub fuel: GoodId,
    pub quantity: u64,
}
impl Request for Stoke {
    const KEY: &'static str = "shelter.stoke";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    person: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "shelter.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "shelter.cancel";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Built {
    pub person: PersonId,
    pub room: AreaId,
    pub outside: AreaId,
    pub name: String,
}
impl Event for Built {
    const KEY: &'static str = "shelter.built";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FireLit {
    pub person: PersonId,
    pub room: AreaId,
    pub until: SimTime,
    pub quantity: u64,
}
impl Event for FireLit {
    const KEY: &'static str = "shelter.fire-lit";
}
pub fn protection(tx: &Transaction<'_>, area: AreaId) -> i32 {
    tx.get::<Shelters>()
        .ok()
        .and_then(|s| s.rooms.get(&area))
        .map_or(0, |r| r.insulation)
}
pub fn outside(tx: &Transaction<'_>, mut area: AreaId) -> AreaId {
    if let Ok(s) = tx.get::<Shelters>() {
        for _ in 0..128 {
            let Some(r) = s.rooms.get(&area) else {
                break;
            };
            area = r.outside;
        }
    }
    area
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("shelter", 1)?;
    b.resource::<Shelters>()?;
    climate::register(b)?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Shelters>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Build>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let area = game_transport::area(tx.get()?, r.person)?;
        require(
            game_cargo::locality(tx, r.inventory)? == area,
            "shelter materials are elsewhere",
        )?;
        let object = tx.get::<game_objects::Objects>()?.by_id(r.kit)?;
        let insulation = object
            .tags
            .iter()
            .find_map(|t| t.strip_prefix("shelter-insulation:")?.parse::<i32>().ok())
            .filter(|n| *n > 0)
            .ok_or_else(|| Error::Rejected("not a shelter kit".into()))?;
        let heat = object
            .tags
            .iter()
            .find_map(|t| t.strip_prefix("hearth-heat:")?.parse().ok())
            .unwrap_or(15);
        let label = if insulation >= 15 { "hall" } else { "hearth" };
        let name = tx
            .get::<game_social_life::SocialLife>()
            .ok()
            .and_then(|s| s.people.get(&r.person))
            .map(|p| format!("{}'s {label}", p.name))
            .unwrap_or(format!("{}'s {label}", r.person.0));
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "shelter construction".into(),
        })?;
        tx.call(game_cargo::Consume {
            inventory: r.inventory,
            good: r.kit,
            quantity: 1,
        })?;
        let until = tx.now().after(SimDuration(6 * HOUR.0))?;
        tx.start_action(r.person, "build", Some(until), Cancel { person: r.person })?;
        let job = tx.schedule(until, 10, Finish { person: r.person })?;
        tx.edit::<Shelters>()?.work.insert(
            r.person,
            Construction {
                outside: area,
                job,
                insulation,
                heat,
                name,
            },
        );
        Ok(until)
    })?;
    b.request::<Finish>(|tx, r| {
        let w = tx
            .edit::<Shelters>()?
            .work
            .remove(&r.person)
            .ok_or_else(|| Error::Missing("shelter work".into()))?;
        require(
            game_transport::area(tx.get()?, r.person)? == w.outside,
            "builder left the site",
        )?;
        let room = tx.call(game_land::CreateArea {
            name: w.name.clone(),
        })?;
        tx.call(game_land::Link {
            from: w.outside,
            to: room,
            duration: MINUTE,
        })?;
        let stores = tx.call(game_cargo::Create {
            custody: game_cargo::Custody::Area(room),
            capacity: 100_000,
        })?;
        tx.call(game_property::ClaimInventory {
            claimant: r.person,
            inventory: stores,
            recognized_by: [r.person].into(),
        })?;
        tx.call(game_property::Permit {
            issuer: r.person,
            person: r.person,
            inventory: stores,
            allow: true,
        })?;
        tx.edit::<Shelters>()?.rooms.insert(
            room,
            Room {
                outside: w.outside,
                owner: r.person,
                stores,
                insulation: w.insulation,
                heat: w.heat,
                burning_until: SimTime(0),
                fire: None,
            },
        );
        climate::refresh(tx, room)?;
        tx.finish_action(r.person, "build")?;
        tx.call(game_knowledge::ObserveSurroundings { person: r.person })?;
        tx.emit(
            Built {
                person: r.person,
                room,
                outside: w.outside,
                name: w.name,
            },
            vec![r.person],
        )
    })?;
    b.request::<Stoke>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(
            game_transport::area(tx.get()?, r.person)? == r.room
                && game_cargo::locality(tx, r.inventory)? == r.room,
            "hearth or fuel is elsewhere",
        )?;
        let room = tx
            .get::<Shelters>()?
            .rooms
            .get(&r.room)
            .cloned()
            .ok_or_else(|| Error::Missing("shelter".into()))?;
        let hours = tx
            .get::<game_objects::Objects>()?
            .by_id(r.fuel)?
            .tags
            .iter()
            .find_map(|t| t.strip_prefix("fuel-hours:")?.parse::<u64>().ok())
            .filter(|h| *h > 0)
            .ok_or_else(|| Error::Rejected("not fuel".into()))?;
        require(r.quantity > 0, "empty fire")?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: room.stores,
            purpose: "heating".into(),
        })?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "burning fuel".into(),
        })?;
        tx.call(game_cargo::Consume {
            inventory: r.inventory,
            good: r.fuel,
            quantity: r.quantity,
        })?;
        let duration = SimDuration(
            hours
                .checked_mul(r.quantity)
                .and_then(|v| v.checked_mul(HOUR.0))
                .ok_or(Error::Overflow)?,
        );
        let until = room.burning_until.max(tx.now()).after(duration)?;
        if let Some(job) = room.fire {
            tx.unschedule(job);
        }
        let job = tx.schedule(until, 4, climate::Extinguish { room: r.room })?;
        let s = tx.edit::<Shelters>()?;
        let room = s.rooms.get_mut(&r.room).unwrap();
        room.burning_until = until;
        room.fire = Some(job);
        s.fuel_used = s.fuel_used.saturating_add(r.quantity);
        climate::refresh(tx, r.room)?;
        tx.emit(
            FireLit {
                person: r.person,
                room: r.room,
                until,
                quantity: r.quantity,
            },
            vec![r.person],
        )?;
        Ok(until)
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(w) = tx.edit::<Shelters>()?.work.remove(&r.person) {
            tx.unschedule(w.job);
        }
        Ok(())
    })?;
    Ok(())
}
