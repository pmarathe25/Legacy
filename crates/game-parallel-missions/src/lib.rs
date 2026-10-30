//! Accepted personal expeditions over physical journeys.
mod draft;
mod party;
mod phase;
pub use draft::Draft;
pub mod routing;
pub use game_parallel_missions_model::{
    Departure, Event, EventKind, Mission, Objective, Phase, Start,
};
use game_parallel_production_model::{PersonSnapshot, person_index};
use game_parallel_transport::Route;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Missions {
    pub next_id: u64,
    /// Minimum experienced fear plus obligation that can motivate a non-willing companion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_threshold: Option<u64>,
    pub entries: Vec<Mission>,
    #[serde(default)]
    pub events: Vec<Event>,
}
/// Copy independent expedition rows in parallel when staging a world boundary.
pub fn clone_parallel(owner: &Missions) -> Missions {
    Missions {
        next_id: owner.next_id,
        pressure_threshold: owner.pressure_threshold,
        entries: owner.entries.par_iter().cloned().collect(),
        events: owner.events.par_iter().cloned().collect(),
    }
}
impl Missions {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        routes: &[Route],
        tick: u64,
    ) -> Result<(), &'static str> {
        let mut ids: Vec<_> = self.entries.par_iter().map(|mission| mission.id).collect();
        ids.par_sort_unstable();
        let mut actors: Vec<_> = self
            .entries
            .par_iter()
            .filter(|mission| !matches!(mission.phase, Phase::Completed | Phase::Failed))
            .map(|mission| mission.person)
            .collect();
        actors.par_sort_unstable();
        let targets = routing::targets(&self.entries, false);
        let paths = routing::Routing::new(routes, &targets);
        let people_by_mission: HashMap<_, _> = self
            .entries
            .iter()
            .map(|mission| (mission.id, mission.person))
            .collect();
        if self.pressure_threshold == Some(0)
            || !party::valid(&self.entries)
            || ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || actors.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.entries.par_iter().any(|mission| {
                (mission.party_opt_in && mission.party_compliant)
                    || (mission.party_compliant && mission.party.is_none())
                    || mission.objective.is_some_and(|Objective::Meet { person }| {
                        person == mission.person || !people.contains(&person)
                    })
                    || (mission.objective.is_none() && mission.objective_met)
                    || mission.id >= self.next_id
                    || !people.contains(&mission.person)
                    || mission.origin == mission.destination
                    || !regions.contains(&mission.origin)
                    || !regions.contains(&mission.destination)
                    || paths.next(mission.origin, mission.destination).is_none()
                    || paths.next(mission.destination, mission.origin).is_none()
                    || mission.visit_ticks == 0
                    || mission.remaining > mission.visit_ticks
                    || (mission.phase != Phase::Visiting && mission.remaining != 0)
                    || (mission.phase == Phase::Visiting && mission.remaining == 0)
            })
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || event.mission >= self.next_id
                    || !people.contains(&event.person)
                    || people_by_mission.get(&event.mission) != Some(&event.person)
            })
        {
            return Err("invalid expedition state");
        }
        Ok(())
    }
    /// Unaccompanied personal expeditions require biological adulthood when ages are modeled.
    pub fn validate_adults(&self, adults: &HashSet<u64>) -> Result<(), &'static str> {
        if self.entries.par_iter().any(|mission| {
            !matches!(mission.phase, Phase::Completed | Phase::Failed)
                && !adults.contains(&mission.person)
        }) {
            return Err("a child cannot undertake an unaccompanied expedition");
        }
        Ok(())
    }
    /// Decisions read the tick-start location and reserve only an attempted departure.
    pub fn prepare(
        &self,
        people: &[PersonSnapshot],
        busy: &[u64],
        routes: &[Route],
        tick: u64,
    ) -> Option<Draft> {
        self.prepare_with(
            |person| person_index(people, person).map(|i| people[i]),
            busy,
            routes,
            tick,
        )
    }
    /// Read only requested actors from a committed physical view.
    pub fn prepare_with(
        &self,
        person_view: impl Fn(u64) -> Option<PersonSnapshot> + Sync,
        busy: &[u64],
        routes: &[Route],
        tick: u64,
    ) -> Option<Draft> {
        phase::prepare(self, &person_view, busy, routes, tick)
    }
}
#[cfg(test)]
#[path = "../tests/support/owner.rs"]
mod tests;
