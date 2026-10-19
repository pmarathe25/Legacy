//! Remembered routes support finite searches; no remote position query finds the target.
mod supplies;
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub use supplies::{FindGoods, GoodsTrails, find};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trail {
    pub seen: Option<SimTime>,
    pub visited: BTreeSet<AreaId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Searches(pub Table<PersonId, Table<PersonId, Trail>>);
impl Resource for Searches {
    const KEY: &'static str = "search.people";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Seek {
    pub person: PersonId,
    pub other: PersonId,
    pub last_known: AreaId,
}
impl Request for Seek {
    const KEY: &'static str = "search.seek-person";
    type Response = Option<AreaId>;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("search", 1)?;
    b.resource::<Searches>()?;
    supplies::register(b)?;
    b.request::<Seek>(|tx, r| {
        let here = game_transport::area(tx.get()?, r.person)?;
        if game_transport::together(tx.get()?, r.person, r.other) {
            tx.edit::<Searches>()?
                .0
                .entry(r.person)
                .or_default()
                .remove(&r.other);
            return Ok(Some(here));
        }
        let memory = tx
            .get::<game_signals::Signals>()
            .ok()
            .and_then(|s| s.perceptions.get(&r.person)?.get(&r.other));
        let seen = memory.map(|m| m.at);
        let last = memory.and_then(|m| m.area).unwrap_or(r.last_known);
        let trail = tx
            .edit::<Searches>()?
            .0
            .entry(r.person)
            .or_default()
            .entry(r.other)
            .or_default();
        if seen > trail.seen {
            trail.visited.clear();
            trail.seen = seen;
        }
        trail.visited.insert(here);
        // The search budget follows acquired evidence, rather than generating routes.
        if trail.visited.len() >= 64 {
            return Ok(None);
        }
        let visited = trail.visited.clone();
        let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&r.person) else {
            return Ok(None);
        };
        let mut candidates = BTreeSet::from([last]);
        for (_, fact) in game_knowledge::with_prefix(facts, "roads:") {
            if let Some(area) = fact.value.decode::<game_land::Area>() {
                candidates.extend(area.links.keys().copied());
            }
        }
        let next = candidates
            .into_iter()
            .filter(|a| !visited.contains(a))
            .filter_map(|area| {
                let route = game_knowledge::route(facts, here, area).ok()?;
                Some((
                    (area != last, route.duration, area),
                    route.areas.get(1).copied()?,
                ))
            })
            .min_by_key(|(score, _)| *score);
        let Some(((.., target), destination)) = next else {
            return Ok(None);
        };
        tx.call(game_transport::Move {
            person: r.person,
            destination,
        })?;
        Ok(Some(target))
    })
}
