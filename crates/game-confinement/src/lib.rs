//! Physical confinement is independent of mortality, regeneration, and office.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub area: AreaId,
    pub custodian: PersonId,
    pub resistance: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confinement(pub BTreeMap<PersonId, Cell>);
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
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("confinement", 1)?;
    b.resource::<Confinement>()?;
    b.before::<game_transport::Move>("confinement.barrier", |tx, r| {
        require(
            !tx.get::<Confinement>()?.0.contains_key(&r.person),
            "actor is confined",
        )
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
            r.consent || target.health == 0,
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
        let escaped = r.capability > cell.resistance;
        if escaped {
            tx.edit::<Confinement>()?.0.remove(&r.person);
        }
        Ok(escaped)
    })
}
