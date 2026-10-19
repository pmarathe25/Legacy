//! Physical confinement is independent of mortality, regeneration, and office.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub area: AreaId,
    pub custodian: PersonId,
    pub resistance: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confinement(pub Table<PersonId, Cell>);
impl Resource for Confinement {
    const KEY: &'static str = "confinement.cells";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Confine {
    pub custodian: PersonId,
    pub person: PersonId,
    pub consent: bool,
    pub resistance: u64,
}
impl Request for Confine {
    const KEY: &'static str = "confinement.confine";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Release {
    pub custodian: PersonId,
    pub person: PersonId,
}
impl Request for Release {
    const KEY: &'static str = "confinement.release";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Escape {
    pub person: PersonId,
    pub capability: u64,
}
impl Request for Escape {
    const KEY: &'static str = "confinement.escape";
    type Response = bool;
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Barriers(pub Table<AreaId, u64>);
impl Resource for Barriers {
    const KEY: &'static str = "confinement.barriers";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct BuildBarrier {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub kit: GoodId,
}
impl Request for BuildBarrier {
    const KEY: &'static str = "confinement.build-barrier";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Install {
    person: PersonId,
    area: AreaId,
    resistance: u64,
}
impl Request for Install {
    const KEY: &'static str = "confinement.install";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Watch {
    pub custodian: PersonId,
    pub person: PersonId,
}
impl Request for Watch {
    const KEY: &'static str = "confinement.watch";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct WatchEnd;
impl Request for WatchEnd {
    const KEY: &'static str = "confinement.watch-end";
    type Response = ();
}
fn assents(tx: &Transaction<'_>, p: PersonId, custodian: PersonId) -> bool {
    let social = tx.get::<game_relationships::Social>().ok();
    let tie = social.and_then(|s| s.relationships.get(&p)?.get(&custodian));
    let regard = tie.map_or(0, |r| {
        i64::from(r.trust + r.affection + r.fear - r.grievance)
    });
    let independence = social
        .and_then(|s| s.values.get(&p)?.get("independence"))
        .copied()
        .unwrap_or(0);
    regard
        > i64::from(independence)
            + 25
            + game_vitality::ability(tx, p, "foresight").min(16000) as i64 / 50
}
fn capability(tx: &Transaction<'_>, person: PersonId) -> u64 {
    let bodies = tx.get::<game_physiology::Bodies>().unwrap();
    let b = &bodies.people[&person];
    (game_vitality::ability(tx, person, "strength") / 10
        + game_vitality::ability(tx, person, "agility") / 20)
        .saturating_mul(b.health)
        / b.maximum(&bodies.rules).max(1)
}
fn resistance(tx: &Transaction<'_>, cell: &Cell) -> u64 {
    let barrier = tx
        .get::<Barriers>()
        .ok()
        .and_then(|b| b.0.get(&cell.area))
        .copied()
        .unwrap_or(0);
    let guard = if game_transport::area(tx.get().unwrap(), cell.custodian).ok() == Some(cell.area)
        && game_physiology::capable(tx, cell.custodian).is_ok()
        && tx
            .get::<Actions>()
            .is_ok_and(|a| a.0.get(&cell.custodian).is_some_and(|a| a.kind == "guard"))
    {
        capability(tx, cell.custodian)
    } else {
        0
    };
    barrier.saturating_add(guard)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("confinement", 1)?;
    b.resource::<Confinement>()?;
    b.resource::<Barriers>()?;
    b.request::<WatchEnd>(|_, _| Ok(()))?;
    b.request::<Watch>(|tx, r| {
        let cell = tx
            .get::<Confinement>()?
            .0
            .get(&r.person)
            .ok_or_else(|| Error::Missing("cell".into()))?;
        require(
            cell.custodian == r.custodian
                && game_transport::area(tx.get()?, r.custodian)? == cell.area,
            "guard needs contact",
        )?;
        game_physiology::capable(tx, r.custodian)?;
        tx.call(game_choice::Undertake {
            person: r.custodian,
            kind: "guard".into(),
            duration: HOUR,
            request: Pending::new(WatchEnd)?,
        })
    })?;
    b.request::<Install>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.person)? == r.area,
            "builder left the site",
        )?;
        let total = tx.edit::<Barriers>()?.0.entry(r.area).or_default();
        *total = total.saturating_add(r.resistance);
        Ok(())
    })?;
    b.request::<BuildBarrier>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let area = game_transport::area(tx.get()?, r.person)?;
        require(
            game_cargo::locality(tx, r.inventory)? == area,
            "materials are elsewhere",
        )?;
        let resistance = tx
            .get::<game_objects::Objects>()?
            .by_id(r.kit)?
            .tags
            .iter()
            .find_map(|s| s.strip_prefix("barrier-resistance:")?.parse().ok())
            .ok_or_else(|| Error::Rejected("not barrier materials".into()))?;
        tx.call(game_property::Use {
            actor: r.person,
            inventory: r.inventory,
            purpose: "barrier construction".into(),
        })?;
        tx.call(game_cargo::Consume {
            inventory: r.inventory,
            good: r.kit,
            quantity: 1,
        })?;
        tx.call(game_choice::Undertake {
            person: r.person,
            kind: "build-barrier".into(),
            duration: SimDuration(2 * HOUR.0),
            request: Pending::new(Install {
                person: r.person,
                area,
                resistance,
            })?,
        })
    })?;
    b.request::<game_physiology::Restrained>(|tx, r| {
        if game_transport::area(tx.get()?, r.actor)
            .ok()
            .is_none_or(|a| game_transport::area(tx.get().unwrap(), r.patient).ok() != Some(a))
        {
            return Ok(false);
        }
        Ok(tx
            .get::<game_physiology::Bodies>()?
            .people
            .get(&r.patient)
            .is_some_and(|b| b.alive && b.health == 0)
            || tx
                .get::<Confinement>()?
                .0
                .get(&r.patient)
                .is_some_and(|cell| {
                    cell.custodian == r.actor && resistance(tx, cell) > capability(tx, r.patient)
                }))
    })?;
    game_choice::provider(b, "confinement.escape-choice", |tx, e| {
        if game_choice::collecting(tx, e.person)
            && tx.get::<Confinement>()?.0.contains_key(&e.person)
        {
            let desire = game_vitality::disposition(tx, e.person, "risk-tolerance") / 10
                + game_vitality::ability(tx, e.person, "foresight").min(16000) as i64 / 20;
            game_choice::offer(
                tx,
                e.person,
                "confinement:escape",
                game_choice::motive(0, desire, 0, 10, 0),
                game_choice::Undertake {
                    person: e.person,
                    kind: "escape".into(),
                    duration: HOUR,
                    request: Pending::new(Escape {
                        person: e.person,
                        capability: 0,
                    })?,
                },
            )?;
        }
        if game_choice::collecting(tx, e.person) {
            let cells: Vec<_> = tx
                .get::<Confinement>()?
                .0
                .iter()
                .filter(|(_, c)| c.custodian == e.person)
                .take(8)
                .map(|(p, _)| *p)
                .collect();
            let holding = !cells.is_empty();
            for person in cells {
                let interest = game_vitality::disposition(tx, e.person, "ambition") / 10
                    + game_vitality::disposition(tx, e.person, "aggression") / 10;
                game_choice::offer(
                    tx,
                    e.person,
                    format!("confinement:watch:{}", person.0),
                    game_choice::motive(0, 0, interest, 10, 0),
                    Watch {
                        custodian: e.person,
                        person,
                    },
                )?;
            }
            if holding {
                let kits: Vec<_> = tx
                    .get::<game_cargo::Cargo>()?
                    .carried(e.person)
                    .flat_map(|i| {
                        tx.get::<game_cargo::Cargo>().unwrap().0[&i]
                            .goods
                            .iter()
                            .map(move |(g, q)| (i, *g, *q))
                    })
                    .filter(|(_, g, q)| {
                        *q > 0
                            && tx
                                .get::<game_objects::Objects>()
                                .unwrap()
                                .by_id(*g)
                                .is_ok_and(|o| {
                                    o.tags.iter().any(|t| t.starts_with("barrier-resistance:"))
                                })
                    })
                    .take(8)
                    .collect();
                for (inventory, kit, _) in kits {
                    game_choice::offer(
                        tx,
                        e.person,
                        format!("confinement:build:{}:{}", inventory.0, kit.0),
                        game_choice::motive(
                            0,
                            0,
                            game_vitality::ability(tx, e.person, "foresight").min(16000) as i64 / 5,
                            20,
                            0,
                        ),
                        BuildBarrier {
                            person: e.person,
                            inventory,
                            kit,
                        },
                    )?;
                }
            }
        }
        Ok(())
    })?;
    b.before::<game_transport::Move>("confinement.barrier", |tx, r| {
        if let Some(cell) = tx.get::<Confinement>()?.0.get(&r.person) {
            require(resistance(tx, cell) == 0, "actor is confined")?;
            tx.edit::<Confinement>()?.0.remove(&r.person);
        }
        Ok(())
    })?;
    b.before::<game_transport::Carry>("confinement.passenger-barrier", |tx, r| {
        if let Some(cell) = tx.get::<Confinement>()?.0.get(&r.passenger) {
            require(resistance(tx, cell) == 0, "passenger is confined")?;
            tx.edit::<Confinement>()?.0.remove(&r.passenger);
        }
        Ok(())
    })?;
    b.request::<Confine>(|tx, r| {
        game_physiology::capable(tx, r.custodian)?;
        let area = game_transport::area(tx.get()?, r.custodian)?;
        require(
            game_transport::area(tx.get()?, r.person)? == area,
            "target is elsewhere",
        )?;
        let target = tx
            .get::<game_physiology::Bodies>()?
            .people
            .get(&r.person)
            .ok_or_else(|| Error::Missing("person".into()))?;
        require(
            r.consent && assents(tx, r.person, r.custodian) || target.health == 0,
            "target must first be subdued",
        )?;
        tx.cancel_action(r.person)?;
        tx.edit::<Confinement>()?.0.insert(
            r.person,
            Cell {
                area,
                custodian: r.custodian,
                resistance: r.resistance,
            },
        );
        tx.call(Watch {
            custodian: r.custodian,
            person: r.person,
        })?;
        Ok(())
    })?;
    b.request::<Release>(|tx, r| {
        let cell = tx
            .get::<Confinement>()?
            .0
            .get(&r.person)
            .ok_or_else(|| Error::Missing("cell".into()))?;
        require(cell.custodian == r.custodian, "not the custodian")?;
        require(
            game_transport::area(tx.get()?, r.custodian)? == cell.area,
            "custodian is elsewhere",
        )?;
        game_physiology::capable(tx, r.custodian)?;
        tx.edit::<Confinement>()?.0.remove(&r.person);
        Ok(())
    })?;
    b.request::<Escape>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let cell = tx
            .get::<Confinement>()?
            .0
            .get(&r.person)
            .ok_or_else(|| Error::Missing("cell".into()))?;
        // Callers cannot manufacture strength. Tools, condition and build determine escape.
        let tool = tx
            .get::<game_cargo::Cargo>()
            .ok()
            .into_iter()
            .flat_map(|c| c.carried(r.person))
            .flat_map(|i| tx.get::<game_cargo::Cargo>().unwrap().0[&i].goods.iter())
            .filter(|(_, q)| **q > 0)
            .filter_map(|(g, _)| tx.get::<game_objects::Objects>().ok()?.by_id(*g).ok())
            .flat_map(|o| o.tags.iter())
            .filter_map(|s| s.strip_prefix("escape-bonus:")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        let escaped = capability(tx, r.person).saturating_add(tool) > resistance(tx, cell);
        if escaped {
            tx.edit::<Confinement>()?.0.remove(&r.person);
        }
        Ok(escaped)
    })
}
