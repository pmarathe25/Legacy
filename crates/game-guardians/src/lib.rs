//! Guardian connection, physical search, mentorship, and discretionary orders.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guardian {
    pub seeking: bool,
    pub mentoring: bool,
    pub ready: bool,
    pub refusal: Option<String>,
    pub known_danger: BTreeMap<AreaId, u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guardians(pub BTreeMap<PersonId, Guardian>);
impl Resource for Guardians {
    const KEY: &'static str = "guardians.mission";
}
#[derive(Serialize, Deserialize)]
pub struct Register {
    pub person: PersonId,
}
impl Request for Register {
    const KEY: &'static str = "guardians.register";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Seek {
    pub guardian: PersonId,
}
impl Request for Seek {
    const KEY: &'static str = "guardians.seek";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Order {
    pub guardian: PersonId,
    pub destination: AreaId,
    pub danger: u64,
}
impl Request for Order {
    const KEY: &'static str = "guardians.order";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Assess {
    pub guardian: PersonId,
    pub learned: u64,
}
impl Request for Assess {
    const KEY: &'static str = "guardians.assess";
    type Response = bool;
}
fn seek(tx: &mut Transaction<'_>, guardian: PersonId) -> Result<()> {
    let target = tx.get::<game_lineage::Lineage>()?.current;
    let Some(target) = target else {
        return Ok(());
    };
    let from = game_transport::area(tx.get()?, guardian)?;
    let to = game_transport::area(tx.get()?, target)?;
    if from == to {
        let g = tx.edit::<Guardians>()?.0.get_mut(&guardian).unwrap();
        g.seeking = false;
        g.mentoring = true;
        return Ok(());
    }
    let route = game_spatial::route(tx.get::<game_land::Land>()?, from, to)?;
    tx.call(game_transport::Move {
        person: guardian,
        destination: route.areas[1],
    })?;
    Ok(())
}
#[derive(Serialize, Deserialize)]
pub struct KnowDanger {
    pub guardian: PersonId,
    pub area: AreaId,
    pub danger: u64,
}
impl Request for KnowDanger {
    const KEY: &'static str = "guardians.know_danger";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct TravelOrder {
    pub guardian: PersonId,
    pub destination: AreaId,
}
impl Request for TravelOrder {
    const KEY: &'static str = "guardians.travel_order";
    type Response = bool;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("guardians", 1)?;
    b.resource::<Guardians>()?;
    b.request::<KnowDanger>(|tx, r| {
        let g = tx
            .edit::<Guardians>()?
            .0
            .get_mut(&r.guardian)
            .ok_or_else(|| Error::Missing("guardian".into()))?;
        g.known_danger.insert(r.area, r.danger);
        Ok(())
    })?;
    b.request::<TravelOrder>(|tx, r| {
        let g = tx
            .get::<Guardians>()?
            .0
            .get(&r.guardian)
            .ok_or_else(|| Error::Missing("guardian".into()))?;
        let danger = g.known_danger.get(&r.destination).copied().unwrap_or(100);
        tx.call(Order {
            guardian: r.guardian,
            destination: r.destination,
            danger,
        })
    })?;
    b.request::<Register>(|tx, r| {
        require(
            tx.get::<game_physiology::Bodies>()?
                .people
                .get(&r.person)
                .is_some_and(|p| p.kind == game_physiology::Kind::Guardian),
            "not a guardian",
        )?;
        require(
            !tx.get::<Guardians>()?.0.contains_key(&r.person),
            "guardian already registered",
        )?;
        tx.edit::<Guardians>()?.0.insert(
            r.person,
            Guardian {
                seeking: false,
                mentoring: true,
                ready: false,
                refusal: None,
                known_danger: BTreeMap::new(),
            },
        );
        Ok(())
    })?;
    b.request::<Seek>(|tx, r| {
        let g = tx
            .edit::<Guardians>()?
            .0
            .get_mut(&r.guardian)
            .ok_or_else(|| Error::Missing("guardian".into()))?;
        g.seeking = true;
        seek(tx, r.guardian)
    })?;
    b.subscribe::<game_lineage::Successor>("guardians.connection", |tx, _| {
        let people: Vec<_> = tx.get::<Guardians>()?.0.keys().copied().collect();
        for person in people {
            tx.edit::<Guardians>()?.0.get_mut(&person).unwrap().seeking = true;
            if game_physiology::capable(tx, person).is_ok()
                && !tx.get::<Actions>()?.0.contains_key(&person)
            {
                let _ = tx.attempt(Seek { guardian: person });
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("guardians.search", |tx, e| {
        if tx
            .get::<Guardians>()?
            .0
            .get(&e.person)
            .is_some_and(|g| g.seeking)
        {
            let _ = tx.attempt(Seek { guardian: e.person });
        }
        Ok(())
    })?;
    b.request::<Assess>(|tx, r| {
        let ruler = tx
            .get::<game_lineage::Lineage>()?
            .current
            .ok_or_else(|| Error::Rejected("no ruler".into()))?;
        let learned = tx
            .get::<game_skills::Skills>()?
            .levels
            .get(&ruler)
            .map_or(0, |skills| skills.values().copied().sum::<u64>());
        let ready = learned >= r.learned;
        let g = tx
            .edit::<Guardians>()?
            .0
            .get_mut(&r.guardian)
            .ok_or_else(|| Error::Missing("guardian".into()))?;
        g.ready = ready;
        g.mentoring = !ready;
        Ok(ready)
    })?;
    b.request::<Order>(|tx, r| {
        let g = tx
            .get::<Guardians>()?
            .0
            .get(&r.guardian)
            .ok_or_else(|| Error::Missing("guardian".into()))?;
        let accepted = g.ready && r.danger < 50;
        tx.edit::<Guardians>()?
            .0
            .get_mut(&r.guardian)
            .unwrap()
            .refusal = if accepted {
            None
        } else {
            Some("mentorship or unacceptable risk".into())
        };
        if accepted {
            tx.call(game_transport::Move {
                person: r.guardian,
                destination: r.destination,
            })?;
        }
        Ok(accepted)
    })
}
