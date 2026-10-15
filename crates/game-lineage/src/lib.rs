//! Player continuity never implies inherited office, assets, or personal skills.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Life {
    pub person: PersonId,
    pub died: SimTime,
    pub skills: BTreeMap<String, u64>,
    pub memories: BTreeMap<String, game_knowledge::Fact>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lineage {
    pub current: Option<PersonId>,
    pub after_death: Option<SimTime>,
    pub history: BTreeMap<LifeId, Life>,
}
impl Resource for Lineage {
    fn allocated_ids(&self) -> Vec<u64> {
        self.history.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "lineage.continuity";
}
#[derive(Serialize, Deserialize)]
pub struct Begin {
    pub person: PersonId,
}
impl Request for Begin {
    const KEY: &'static str = "lineage.begin";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Successor {
    pub person: PersonId,
    pub area: AreaId,
}
impl Event for Successor {
    const KEY: &'static str = "lineage.successor";
}
pub fn current(host: &Host) -> Result<Option<PersonId>> {
    Ok(host.get::<Lineage>()?.current)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("lineage", 1)?;
    b.resource::<Lineage>()?;
    b.validate_save(|h| {
        let s = h.get::<Lineage>()?;
        if let Some(person) = s.current {
            require(
                h.get::<game_physiology::Bodies>()?
                    .people
                    .get(&person)
                    .is_some_and(|b| b.alive),
                "current ruler is not living",
            )?;
        }
        for life in s.history.values() {
            require(
                h.get::<game_physiology::Bodies>()?
                    .people
                    .get(&life.person)
                    .is_some_and(|b| !b.alive),
                "past life is not dead",
            )?;
        }
        Ok(())
    });
    b.request::<Begin>(|tx, r| {
        require(
            tx.get::<Lineage>()?.current.is_none() && tx.get::<Lineage>()?.history.is_empty(),
            "lineage already started",
        )?;
        game_physiology::capable(tx, r.person)?;
        require(
            tx.get::<game_physiology::Bodies>()?.people[&r.person].kind
                == game_physiology::Kind::Mortal,
            "ruler must be mortal",
        )?;
        tx.edit::<Lineage>()?.current = Some(r.person);
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("lineage.record", |tx, e| {
        if tx.get::<Lineage>()?.current == Some(e.person) {
            let skills = tx
                .get::<game_skills::Skills>()?
                .levels
                .get(&e.person)
                .cloned()
                .unwrap_or_default();
            let memories = tx
                .get::<game_knowledge::Knowledge>()?
                .0
                .get(&e.person)
                .cloned()
                .unwrap_or_default();
            let id = LifeId(tx.allocate()?);
            let state = tx.edit::<Lineage>()?;
            state.history.insert(
                id,
                Life {
                    person: e.person,
                    died: e.at,
                    skills,
                    memories,
                },
            );
            state.current = None;
            state.after_death = Some(e.at);
        }
        Ok(())
    })?;
    b.subscribe::<game_lifecycle::Born>("lineage.select", |tx, e| {
        // This fixture policy chooses the first eligible actual birth in event order.
        let eligible = tx.get::<Lineage>()?.current.is_none()
            && tx.get::<Lineage>()?.after_death.is_some_and(|d| e.at > d);
        if eligible {
            let state = tx.edit::<Lineage>()?;
            state.current = Some(e.person);
            state.after_death = None;
            tx.emit(
                Successor {
                    person: e.person,
                    area: e.area,
                },
                vec![],
            )?;
        }
        Ok(())
    })
}
