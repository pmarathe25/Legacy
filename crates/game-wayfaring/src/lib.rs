//! Voluntary journeys connect places through remembered roads and actual visits.
use game_core::*;
mod choices;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trip {
    pub home: AreaId,
    pub destination: AreaId,
    pub returning: bool,
    pub leave_at: Option<SimTime>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wayfaring {
    pub enabled: bool,
    pub trips: Table<PersonId, Trip>,
    pub next: Table<PersonId, SimTime>,
    pub visits: Table<PersonId, Table<AreaId, SimTime>>,
    pub completed: u64,
}
impl Resource for Wayfaring {
    const KEY: &'static str = "wayfaring.journeys";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "wayfaring.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Consider {
    pub person: PersonId,
}
impl Request for Consider {
    const KEY: &'static str = "wayfaring.consider";
    type Response = bool;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Visited {
    pub person: PersonId,
    pub area: AreaId,
}
impl Event for Visited {
    const KEY: &'static str = "wayfaring.visited";
}
fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<bool> {
    if tx.get::<Actions>()?.0.contains_key(&person) {
        return Ok(false);
    }
    let offers = choices::options(tx, person)?;
    if let Some(o) = offers
        .into_iter()
        .filter(|o| o.motive.score() > 0)
        .max_by_key(|o| o.motive.score())
    {
        tx.invoke(o.request)?;
        return Ok(true);
    }
    Ok(false)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("wayfaring", 1)?;
    b.resource::<Wayfaring>()?;
    choices::register(b)?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Wayfaring>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Consider>(|tx, r| consider(tx, r.person))?;
    game_choice::provider(b, "journeys.explore", |tx, e| {
        if game_choice::collecting(tx, e.person) {
            for o in choices::options(tx, e.person)? {
                game_choice::submit(tx, e.person, o)?;
            }
            return Ok(());
        }
        tx.call(Consider { person: e.person })?;
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("wayfaring.arrived", |tx, e| {
        if !tx.get::<Wayfaring>()?.enabled {
            return Ok(());
        }
        tx.call(game_knowledge::ObserveSurroundings { person: e.person })?;
        let now = tx.now();
        let first = tx
            .edit::<Wayfaring>()?
            .visits
            .entry(e.person)
            .or_default()
            .insert(e.area, now)
            .is_none();
        if first {
            tx.call(game_relationships::LearnValue {
                person: e.person,
                concept: "discovery".into(),
                outcome: 1,
            })?;
            tx.emit(
                Visited {
                    person: e.person,
                    area: e.area,
                },
                vec![e.person],
            )?;
        }
        let until = now;
        if let Some(t) = tx.edit::<Wayfaring>()?.trips.get_mut(&e.person)
            && !t.returning
            && e.area == t.destination
        {
            t.leave_at = Some(until);
        }
        let home = tx
            .get::<Wayfaring>()?
            .trips
            .get(&e.person)
            .is_some_and(|t| t.returning && t.home == e.area);
        if home {
            let s = tx.edit::<Wayfaring>()?;
            s.trips.remove(&e.person);
            s.completed = s.completed.saturating_add(1);
        }
        tx.edit::<Wayfaring>()?.next.insert(e.person, now);
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("wayfaring.death", |tx, e| {
        let s = tx.edit::<Wayfaring>()?;
        s.trips.remove(&e.person);
        s.next.remove(&e.person);
        Ok(())
    })?;
    b.validate_save(|h| {
        for (p, t) in &h.get::<Wayfaring>()?.trips {
            require(
                h.get::<game_physiology::Bodies>()?
                    .people
                    .get(p)
                    .is_some_and(|b| b.alive)
                    && t.home != t.destination
                    && h.get::<game_knowledge::Knowledge>()?.0.contains_key(p),
                "invalid journey",
            )?;
        }
        Ok(())
    });
    Ok(())
}
