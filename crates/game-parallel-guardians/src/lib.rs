//! Persistent guardian bodies, physical journeys and local mentorship.
use game_parallel_people::Region;
use game_parallel_transport::Route;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guardians {
    pub recovery_per_tick: u32,
    pub max_health: u32,
    pub mentorship_ticks: u64,
    pub guardians: Vec<Guardian>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guardian {
    pub id: u64,
    pub region: u64,
    pub health: u32,
    #[serde(default)]
    pub confined: bool,
    #[serde(default)]
    pub journey: Option<Journey>,
    #[serde(default)]
    pub student: Option<u64>,
    #[serde(default)]
    pub mentorship: u64,
    #[serde(default)]
    pub released: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseDecision {
    Accepted,
    NotPresent,
    NotReady,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journey {
    pub destination: u64,
    pub remaining: u32,
}
impl Guardians {
    pub fn validate_world(
        &self,
        has_lineage: bool,
        regions: &[Region],
        routes: &[Route],
    ) -> Result<(), String> {
        if !has_lineage {
            return Err("guardians require lineage".into());
        }
        self.validate(&regions.iter().map(|r| r.id).collect::<Vec<_>>(), routes)
    }
    pub fn validate(&self, regions: &[u64], routes: &[Route]) -> Result<(), String> {
        let regions: HashSet<_> = regions.iter().copied().collect();
        let mut ids = HashSet::new();
        if self.max_health == 0
            || self.recovery_per_tick == 0
            || self.mentorship_ticks == 0
            || self.guardians.iter().any(|g| {
                g.id == 0
                    || !ids.insert(g.id)
                    || !regions.contains(&g.region)
                    || g.health > self.max_health
                    || g.journey.as_ref().is_some_and(|j| {
                        g.confined
                            || j.remaining == 0
                            || !regions.contains(&j.destination)
                            || !routes.iter().any(|r| {
                                r.source == g.region
                                    && r.destination == j.destination
                                    && j.remaining <= r.ticks
                            })
                    })
            })
        {
            return Err("invalid guardian state".into());
        }
        Ok(())
    }
    /// The target is sensed at the beginning of a tick. Traveling targets have no
    /// local region until arrival; sensing never adds ordinary personal knowledge.
    pub fn advance(&self, target: Option<(u64, Option<u64>)>, routes: &[Route]) -> Self {
        let goal = target.and_then(|(_, region)| region);
        let routing = goal
            .filter(|&goal| {
                self.guardians.iter().any(|guardian| {
                    !guardian.confined
                        && guardian.journey.is_none()
                        && guardian.region != goal
                        && guardian
                            .health
                            .saturating_add(self.recovery_per_tick)
                            .min(self.max_health)
                            > 0
                })
            })
            .map(|goal| game_parallel_transport::routing::Routing::new(routes, &[goal]));
        let guardians = self
            .guardians
            .par_iter()
            .map(|old| {
                let mut guardian = old.clone();
                guardian.health = guardian
                    .health
                    .saturating_add(self.recovery_per_tick)
                    .min(self.max_health);
                if guardian.student != target.map(|(id, _)| id) {
                    guardian.student = target.map(|(id, _)| id);
                    guardian.mentorship = 0;
                    guardian.released = false;
                }
                if let Some(journey) = &mut guardian.journey {
                    journey.remaining -= 1;
                    if journey.remaining == 0 {
                        guardian.region = journey.destination;
                        guardian.journey = None;
                    }
                } else if !guardian.confined
                    && guardian.health > 0
                    && let Some(goal) = goal
                {
                    if guardian.region == goal {
                        guardian.mentorship = guardian.mentorship.saturating_add(1);
                    } else if let Some((destination, ticks)) = routing
                        .as_ref()
                        .and_then(|routing| routing.next_leg(guardian.region, goal))
                    {
                        guardian.journey = Some(Journey {
                            destination,
                            remaining: ticks,
                        });
                    }
                }
                guardian
            })
            .collect();
        Self {
            guardians,
            recovery_per_tick: self.recovery_per_tick,
            max_health: self.max_health,
            mentorship_ticks: self.mentorship_ticks,
        }
    }
    pub fn ready(&self, guardian: &Guardian) -> bool {
        guardian.student.is_some() && guardian.mentorship >= self.mentorship_ticks
    }
    /// A request changes only the guardian's own judgment, not legal status or office.
    pub fn request_release(
        &mut self,
        id: u64,
        target: Option<(u64, Option<u64>)>,
    ) -> Result<ReleaseDecision, String> {
        let (student, region) = target.ok_or("there is no current ruler")?;
        let threshold = self.mentorship_ticks;
        let guardian = self
            .guardians
            .iter_mut()
            .find(|guardian| guardian.id == id)
            .ok_or("unknown guardian")?;
        if guardian.student != Some(student)
            || guardian.journey.is_some()
            || guardian.health == 0
            || region != Some(guardian.region)
        {
            return Ok(ReleaseDecision::NotPresent);
        }
        if guardian.mentorship < threshold {
            return Ok(ReleaseDecision::NotReady);
        }
        guardian.released = true;
        Ok(ReleaseDecision::Accepted)
    }
}
pub fn target_in_regions(current: Option<u64>, regions: &[Region]) -> Option<(u64, Option<u64>)> {
    current.map(|id| {
        (
            id,
            regions
                .par_iter()
                .find_any(|region| region.people.iter().any(|person| person.id == id))
                .map(|region| region.id),
        )
    })
}
pub fn prepare_world(
    guardians: Option<&Guardians>,
    has_lineage: bool,
    current: Option<u64>,
    regions: &[Region],
    routes: &[Route],
) -> Result<Option<Guardians>, String> {
    guardians
        .map(|guardians| {
            guardians.validate_world(has_lineage, regions, routes)?;
            Ok(guardians.advance(target_in_regions(current, regions), routes))
        })
        .transpose()
}
#[cfg(test)]
#[path = "../tests/support/routing.rs"]
mod routing_tests;
