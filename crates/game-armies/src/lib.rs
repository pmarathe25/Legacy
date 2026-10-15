//! Recruitment and units preserve each member's identity and actual supplies.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub commander: PersonId,
    pub members: BTreeSet<PersonId>,
    pub supply: InventoryId,
    pub deployed: Option<MissionId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Armies(pub Records<ArmyId, Unit>);
impl Resource for Armies {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "armies.units";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Create {
    pub commander: PersonId,
    pub supply: InventoryId,
}
impl Request for Create {
    const KEY: &'static str = "armies.create";
    type Response = ArmyId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Recruit {
    pub army: ArmyId,
    pub person: PersonId,
    pub willing: bool,
}
impl Request for Recruit {
    const KEY: &'static str = "armies.recruit";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Deploy {
    pub army: ArmyId,
    pub person: PersonId,
    pub destination: AreaId,
}
impl Request for Deploy {
    const KEY: &'static str = "armies.deploy";
    type Response = MissionId;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("armies", 1)?;
    b.resource::<Armies>()?;
    b.request::<Create>(|tx, r| {
        game_physiology::capable(tx, r.commander)?;
        require(
            tx.get::<game_cargo::Cargo>()?.0.contains_key(&r.supply),
            "unknown supply",
        )?;
        let id = ArmyId(tx.allocate()?);
        tx.edit::<Armies>()?.0.insert(
            id,
            Unit {
                commander: r.commander,
                members: BTreeSet::from([r.commander]),
                supply: r.supply,
                deployed: None,
            },
        );
        Ok(id)
    })?;
    b.request::<Recruit>(|tx, r| {
        require(r.willing, "recruit refuses")?;
        game_physiology::capable(tx, r.person)?;
        require(
            !tx.get::<Armies>()?
                .0
                .values()
                .any(|u| u.members.contains(&r.person)),
            "already in a unit",
        )?;
        let u = tx
            .edit::<Armies>()?
            .0
            .get_mut(&r.army)
            .ok_or_else(|| Error::Missing("army".into()))?;
        require(u.deployed.is_none(), "unit deployed")?;
        u.members.insert(r.person);
        Ok(())
    })?;
    b.request::<Deploy>(|tx, r| {
        let u = tx
            .get::<Armies>()?
            .0
            .get(&r.army)
            .ok_or_else(|| Error::Missing("army".into()))?
            .clone();
        require(
            u.commander == r.person && u.deployed.is_none(),
            "cannot deploy unit",
        )?;
        require(
            game_transport::area(tx.get()?, r.person)? == game_cargo::locality(tx, u.supply)?,
            "unit supplies are elsewhere",
        )?;
        require(
            tx.get::<game_property::Property>()?
                .allowed(r.person, u.supply),
            "no unit supply access",
        )?;
        let hungry = u
            .members
            .iter()
            .filter(|p| {
                tx.get::<game_physiology::Bodies>().unwrap().people[p].kind
                    == game_physiology::Kind::Mortal
            })
            .count() as u64;
        if hungry > 0 {
            let (good, food) = tx
                .get::<game_objects::Objects>()?
                .meal(&tx.get::<game_cargo::Cargo>()?.0[&u.supply].goods)
                .ok_or_else(|| Error::Rejected("no edible unit supplies".into()))?;
            let quantity = food.portion.checked_mul(hungry).ok_or(Error::Overflow)?;
            tx.call(game_cargo::Consume {
                inventory: u.supply,
                good,
                quantity,
            })?;
        }
        let mission = tx.call(game_missions::Prepare {
            leader: r.person,
            members: u.members.clone(),
            destination: r.destination,
        })?;
        for person in u.members {
            if person != r.person {
                tx.call(game_missions::Join { person, mission })?;
            }
        }
        tx.call(game_missions::Depart {
            person: r.person,
            mission,
        })?;
        tx.edit::<Armies>()?.0.get_mut(&r.army).unwrap().deployed = Some(mission);
        Ok(mission)
    })
}
