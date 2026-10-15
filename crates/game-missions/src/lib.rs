//! Parties travel through the same world as workers and return with persistent consequences.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Preparing,
    Outbound,
    AtSite,
    Returning,
    Complete,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mission {
    pub leader: PersonId,
    pub members: BTreeSet<PersonId>,
    pub accepted: BTreeSet<PersonId>,
    pub home: AreaId,
    pub destination: AreaId,
    pub arrived: BTreeSet<PersonId>,
    pub phase: Phase,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Missions(pub Records<MissionId, Mission>);
impl Resource for Missions {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "missions.parties";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Prepare {
    pub leader: PersonId,
    pub members: BTreeSet<PersonId>,
    pub destination: AreaId,
}
impl Request for Prepare {
    const KEY: &'static str = "missions.prepare";
    type Response = MissionId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Join {
    pub person: PersonId,
    pub mission: MissionId,
}
impl Request for Join {
    const KEY: &'static str = "missions.join";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Depart {
    pub person: PersonId,
    pub mission: MissionId,
}
impl Request for Depart {
    const KEY: &'static str = "missions.depart";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Return {
    pub person: PersonId,
    pub mission: MissionId,
}
impl Request for Return {
    const KEY: &'static str = "missions.return";
    type Response = ();
}
fn onward(tx: &mut Transaction<'_>, person: PersonId, mission: MissionId) -> Result<()> {
    let m = tx.get::<Missions>()?.0[&mission].clone();
    let to = if m.phase == Phase::Returning {
        m.home
    } else {
        m.destination
    };
    let from = game_transport::area(tx.get()?, person)?;
    if from == to {
        let m = tx.edit::<Missions>()?.0.get_mut(&mission).unwrap();
        m.arrived.insert(person);
        if m.arrived == m.accepted {
            m.phase = if m.phase == Phase::Returning {
                Phase::Complete
            } else {
                Phase::AtSite
            };
        }
    } else {
        let route = game_spatial::route(tx.get::<game_land::Land>()?, from, to)?;
        tx.call(game_transport::Move {
            person,
            destination: route.areas[1],
        })?;
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
struct Continue {
    person: PersonId,
    mission: MissionId,
}
impl Request for Continue {
    const KEY: &'static str = "missions.continue";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("missions", 1)?;
    b.resource::<Missions>()?;
    b.request::<Prepare>(|tx, r| {
        game_physiology::capable(tx, r.leader)?;
        require(r.members.contains(&r.leader), "leader not in party")?;
        let home = game_transport::area(tx.get()?, r.leader)?;
        game_spatial::route(tx.get::<game_land::Land>()?, home, r.destination)?;
        let id = MissionId(tx.allocate()?);
        tx.edit::<Missions>()?.0.insert(
            id,
            Mission {
                leader: r.leader,
                members: r.members,
                accepted: BTreeSet::from([r.leader]),
                home,
                destination: r.destination,
                arrived: BTreeSet::new(),
                phase: Phase::Preparing,
            },
        );
        Ok(id)
    })?;
    b.request::<Join>(|tx, r| {
        let m = tx
            .edit::<Missions>()?
            .0
            .get_mut(&r.mission)
            .ok_or_else(|| Error::Missing("mission".into()))?;
        require(
            m.phase == Phase::Preparing && m.members.contains(&r.person),
            "cannot join mission",
        )?;
        m.accepted.insert(r.person);
        Ok(())
    })?;
    b.request::<Depart>(|tx, r| {
        let m = tx
            .get::<Missions>()?
            .0
            .get(&r.mission)
            .ok_or_else(|| Error::Missing("mission".into()))?
            .clone();
        require(
            m.leader == r.person && m.phase == Phase::Preparing && m.accepted == m.members,
            "party not ready",
        )?;
        tx.edit::<Missions>()?.0.get_mut(&r.mission).unwrap().phase = Phase::Outbound;
        for p in m.accepted {
            onward(tx, p, r.mission)?;
        }
        Ok(())
    })?;
    b.request::<Return>(|tx, r| {
        let m = tx
            .get::<Missions>()?
            .0
            .get(&r.mission)
            .ok_or_else(|| Error::Missing("mission".into()))?
            .clone();
        require(
            m.leader == r.person && m.phase == Phase::AtSite,
            "cannot return",
        )?;
        let state = tx.edit::<Missions>()?.0.get_mut(&r.mission).unwrap();
        state.phase = Phase::Returning;
        state.arrived.clear();
        for p in m.accepted {
            onward(tx, p, r.mission)?;
        }
        Ok(())
    })?;
    b.request::<Continue>(|tx, r| onward(tx, r.person, r.mission))?;
    b.subscribe::<game_transport::Arrived>("missions.progress", |tx, e| {
        let ids: Vec<_> = tx
            .get::<Missions>()?
            .0
            .iter()
            .filter(|(_, m)| {
                m.accepted.contains(&e.person)
                    && matches!(m.phase, Phase::Outbound | Phase::Returning)
            })
            .map(|(id, _)| *id)
            .collect();
        for mission in ids {
            if tx
                .attempt(Continue {
                    person: e.person,
                    mission,
                })
                .is_err()
            {
                tx.edit::<Missions>()?.0.get_mut(&mission).unwrap().phase = Phase::Failed;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("missions.casualty", |tx, e| {
        let affected: Vec<_> = tx
            .get::<Missions>()?
            .0
            .iter()
            .filter(|(_, m)| m.accepted.contains(&e.person) && m.phase != Phase::Complete)
            .map(|(id, _)| *id)
            .collect();
        for id in affected {
            let m = tx.edit::<Missions>()?.0.get_mut(&id).unwrap();
            if m.accepted.contains(&e.person) && m.phase != Phase::Complete {
                m.phase = Phase::Failed;
            }
        }
        Ok(())
    })
}
