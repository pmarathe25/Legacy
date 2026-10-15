//! A delivery is loading, actual travel, then local unloading.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub pack: InventoryId,
    pub destination: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
    pub area: AreaId,
    pub blocked: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Logistics(pub Table<PersonId, Delivery>);
impl Resource for Logistics {
    const KEY: &'static str = "logistics.deliveries";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Haul {
    pub carrier: PersonId,
    pub from: InventoryId,
    pub to: InventoryId,
    pub pack: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Haul {
    const KEY: &'static str = "logistics.haul";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Continue {
    pub carrier: PersonId,
}
impl Request for Continue {
    const KEY: &'static str = "logistics.continue";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Cancel {
    pub carrier: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "logistics.cancel";
    type Response = ();
}
fn onward(tx: &mut Transaction<'_>, carrier: PersonId) -> Result<()> {
    let work = tx.get::<Logistics>()?.0[&carrier].clone();
    let from = game_transport::area(tx.get()?, carrier)?;
    if from == work.area {
        tx.call(game_cargo::Transfer {
            from: work.pack,
            to: work.destination,
            good: work.good,
            quantity: work.quantity,
        })?;
        tx.edit::<Logistics>()?.0.remove(&carrier);
    } else {
        let route = game_spatial::route(tx.get::<game_land::Land>()?, from, work.area)?;
        tx.call(game_transport::Move {
            person: carrier,
            destination: route.areas[1],
        })?;
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("logistics", 1)?;
    b.resource::<Logistics>()?;
    b.request::<Haul>(|tx, r| {
        game_physiology::capable(tx, r.carrier)?;
        require(
            !tx.get::<Logistics>()?.0.contains_key(&r.carrier),
            "already delivering",
        )?;
        require(
            tx.get::<game_cargo::Cargo>()?
                .0
                .get(&r.pack)
                .is_some_and(|p| p.custody == game_cargo::Custody::Carried(r.carrier)),
            "not the carrier's pack",
        )?;
        let area = game_cargo::locality(tx, r.to)?;
        tx.call(game_property::Take {
            actor: r.carrier,
            transfer: game_cargo::Transfer {
                from: r.from,
                to: r.pack,
                good: r.good,
                quantity: r.quantity,
            },
            unauthorized: false,
        })?;
        tx.edit::<Logistics>()?.0.insert(
            r.carrier,
            Delivery {
                pack: r.pack,
                destination: r.to,
                good: r.good,
                quantity: r.quantity,
                area,
                blocked: false,
            },
        );
        onward(tx, r.carrier)
    })?;
    b.request::<Continue>(|tx, r| {
        require(
            tx.get::<Logistics>()?.0.contains_key(&r.carrier),
            "no delivery",
        )?;
        tx.edit::<Logistics>()?
            .0
            .get_mut(&r.carrier)
            .unwrap()
            .blocked = false;
        onward(tx, r.carrier)
    })?;
    b.request::<Cancel>(|tx, r| {
        tx.cancel_action(r.carrier)?;
        tx.edit::<Logistics>()?.0.remove(&r.carrier);
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("logistics.delivery", |tx, e| {
        if tx.get::<Logistics>()?.0.contains_key(&e.person)
            && tx.attempt(Continue { carrier: e.person }).is_err()
        {
            tx.edit::<Logistics>()?
                .0
                .get_mut(&e.person)
                .unwrap()
                .blocked = true;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("logistics.release", |tx, e| {
        tx.edit::<Logistics>()?.0.remove(&e.person);
        Ok(())
    })
}
