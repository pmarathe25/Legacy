//! Named strategic areas and travel links.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Area {
    pub name: String,
    pub links: BTreeMap<AreaId, SimDuration>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Land(pub Records<AreaId, Area>);
impl Resource for Land {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "land.areas";
    fn validate(&self) -> Result<()> {
        for (id, area) in &self.0 {
            require(id.0 > 0 && !area.name.is_empty(), "invalid area")?;
            for (to, duration) in &area.links {
                require(
                    to != id && duration.0 > 0 && self.0.contains_key(to),
                    "invalid link",
                )?;
            }
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
pub struct CreateArea {
    pub name: String,
}
impl Request for CreateArea {
    const KEY: &'static str = "land.create";
    type Response = AreaId;
}
#[derive(Serialize, Deserialize)]
pub struct Link {
    pub from: AreaId,
    pub to: AreaId,
    pub duration: SimDuration,
}
impl Request for Link {
    const KEY: &'static str = "land.link";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct CloseRoad {
    pub from: AreaId,
    pub to: AreaId,
}
impl Request for CloseRoad {
    const KEY: &'static str = "land.close_road";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("land", 1)?;
    b.resource::<Land>()?;
    b.request::<CreateArea>(|tx, r| {
        require(!r.name.is_empty(), "area needs a name")?;
        let id = AreaId(tx.allocate()?);
        tx.edit::<Land>()?.0.insert(
            id,
            Area {
                name: r.name,
                links: BTreeMap::new(),
            },
        );
        Ok(id)
    })?;
    b.request::<CloseRoad>(|tx, r| {
        require(
            tx.get::<Land>()?
                .0
                .get(&r.from)
                .is_some_and(|a| a.links.contains_key(&r.to)),
            "unknown road",
        )?;
        tx.edit::<Land>()?
            .0
            .get_mut(&r.from)
            .unwrap()
            .links
            .remove(&r.to);
        tx.edit::<Land>()?
            .0
            .get_mut(&r.to)
            .unwrap()
            .links
            .remove(&r.from);
        Ok(())
    })?;
    b.request::<Link>(|tx, r| {
        require(r.from != r.to && r.duration.0 > 0, "invalid road")?;
        require(
            tx.get::<Land>()?.0.contains_key(&r.from) && tx.get::<Land>()?.0.contains_key(&r.to),
            "unknown area",
        )?;
        let land = tx.edit::<Land>()?;
        land.0
            .get_mut(&r.from)
            .unwrap()
            .links
            .insert(r.to, r.duration);
        land.0
            .get_mut(&r.to)
            .unwrap()
            .links
            .insert(r.from, r.duration);
        Ok(())
    })
}
