//! Individuals travel on directed, data-defined routes while retaining their custody.
use crate::Person;
use game_lot_runtime::{Phase, Work};
use game_lots::{Place, View};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub source: u64,
    pub destination: u64,
    pub ticks: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journey {
    pub person: u64,
    pub source: u64,
    pub destination: u64,
    pub remaining: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Departure {
    pub person: u64,
    pub destination: u64,
}
/// A directed route is physically impassable during the half-open interval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Closure {
    pub source: u64,
    pub destination: u64,
    pub start: u64,
    pub end: u64,
}

/// The outcome observed by an individual who actually tried a local route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteObservation {
    pub person: u64,
    pub source: u64,
    pub destination: u64,
    pub open: bool,
    pub observed_at: u64,
    /// The individual's estimate; it may differ from the actual travel time.
    #[serde(default)]
    pub estimated_ticks: Option<u64>,
    #[serde(default)]
    pub witness: Option<u64>,
    #[serde(default)]
    pub learned_from: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepartureOutcome {
    pub person: u64,
    pub departed: bool,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Travel {
    pub routes: Vec<Route>,
    #[serde(default)]
    pub journeys: Vec<Journey>,
    #[serde(default)]
    pub closures: Vec<Closure>,
    #[serde(default)]
    pub observations: Vec<RouteObservation>,
    #[serde(default = "default_observation_limit")]
    pub observation_limit: usize,
    /// Food-search path selection uses only the individual's own route reports.
    #[serde(default)]
    pub personal_routing: bool,
}
fn default_observation_limit() -> usize {
    8
}
impl Default for Travel {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            journeys: Vec::new(),
            closures: Vec::new(),
            observations: Vec::new(),
            observation_limit: default_observation_limit(),
            personal_routing: false,
        }
    }
}
/// Compute routes once per reported destination, independently across destinations.
pub fn routes(travel: &Travel, destinations: &[u64]) -> HashMap<(u64, u64), (u64, u64)> {
    let mut reverse: Vec<_> = travel
        .routes
        .par_iter()
        .map(|r| (r.destination, r.source, r.ticks))
        .collect();
    reverse.par_sort_unstable();
    destinations
        .par_iter()
        .flat_map_iter(|&target| {
            let mut best = HashMap::from([(target, (0u64, target))]);
            let mut queue = BinaryHeap::from([Reverse((0u64, target))]);
            while let Some(Reverse((distance, node))) = queue.pop() {
                if best.get(&node).is_none_or(|row| row.0 != distance) {
                    continue;
                }
                let first = reverse.partition_point(|r| r.0 < node);
                let last = reverse.partition_point(|r| r.0 <= node);
                for &(_, source, ticks) in &reverse[first..last] {
                    let Some(cost) = distance.checked_add(ticks) else {
                        continue;
                    };
                    let candidate = (cost, node);
                    if best.get(&source).is_none_or(|current| candidate < *current) {
                        best.insert(source, candidate);
                        queue.push(Reverse((cost, source)));
                    }
                }
            }
            best.into_iter()
                .filter(move |(source, _)| *source != target)
                .map(move |(source, value)| ((source, target), value))
        })
        .collect()
}

/// Find paths over only one person's bounded, dated route reports. A closed
/// report can be rechecked after the person's normal review delay.
pub fn known_paths(
    reports: &[&RouteObservation],
    source: u64,
    now: u64,
    retry_ticks: u64,
) -> HashMap<u64, (u64, u64)> {
    let mut best = HashMap::from([(source, (0u64, source))]);
    let mut queue = BinaryHeap::from([Reverse((0u64, source, source))]);
    while let Some(Reverse((distance, first, node))) = queue.pop() {
        if best.get(&node) != Some(&(distance, first)) {
            continue;
        }
        for report in reports
            .iter()
            .copied()
            .filter(|report| report.source == node)
        {
            if !report.open && now.saturating_sub(report.observed_at) < retry_ticks {
                continue;
            }
            let Some(cost) = report
                .estimated_ticks
                .and_then(|ticks| distance.checked_add(ticks))
            else {
                continue;
            };
            let first_step = if node == source {
                report.destination
            } else {
                first
            };
            let candidate = (cost, first_step);
            if best
                .get(&report.destination)
                .is_none_or(|old| candidate < *old)
            {
                best.insert(report.destination, candidate);
                queue.push(Reverse((cost, first_step, report.destination)));
            }
        }
    }
    best.remove(&source);
    best
}
/// Coarse routes place people together when their recorded legs and progress match.
pub struct Colocation {
    journeys: HashMap<u64, (u64, u64, u64)>,
}
impl Colocation {
    pub fn new(travel: Option<&Travel>) -> Self {
        Self {
            journeys: travel
                .map_or(&[][..], |t| t.journeys.as_slice())
                .par_iter()
                .map(|j| (j.person, (j.source, j.destination, j.remaining)))
                .collect(),
        }
    }
    /// Return the local region or, for travelers, the source-region route label.
    pub fn region(&self, a: (u64, Option<u64>), b: (u64, Option<u64>)) -> Option<u64> {
        match (a.1, b.1) {
            (Some(x), Some(y)) if x == y => Some(x),
            (None, None) => {
                let route = self.journeys.get(&a.0)?;
                (self.journeys.get(&b.0) == Some(route)).then_some(route.0)
            }
            _ => None,
        }
    }
}
impl Clone for Travel {
    fn clone(&self) -> Self {
        Self {
            routes: self.routes.par_iter().cloned().collect(),
            journeys: self.journeys.par_iter().cloned().collect(),
            closures: self.closures.par_iter().cloned().collect(),
            observations: self.observations.par_iter().cloned().collect(),
            observation_limit: self.observation_limit,
            personal_routing: self.personal_routing,
        }
    }
}

pub fn working(work: &[Work]) -> Vec<u64> {
    let mut ids: Vec<_> = work
        .par_iter()
        .filter(|row| row.phase == Phase::Working)
        .map(|row| row.job.worker)
        .collect();
    ids.par_sort_unstable();
    ids
}
pub fn validate_world(
    travel: &Travel,
    people: &[Person],
    view: &View,
    work: &[Work],
    tick: u64,
) -> Result<(), String> {
    let regions = view
        .containers
        .par_iter()
        .filter_map(|row| match row.place {
            Place::Stock { region, .. } => Some(region),
            _ => None,
        })
        .collect();
    travel.validate(people, &regions, &working(work), tick)
}

impl Travel {
    /// Build the current physical admission index without updating anyone's beliefs.
    pub fn closed_routes(&self, tick: u64) -> Vec<(u64, u64)> {
        let mut closed: Vec<_> = self
            .closures
            .par_iter()
            .filter(|closure| closure.start <= tick && tick < closure.end)
            .map(|closure| (closure.source, closure.destination))
            .collect();
        closed.par_sort_unstable();
        closed.dedup();
        closed
    }
    pub fn validate(
        &self,
        people: &[Person],
        regions: &HashSet<u64>,
        working: &[u64],
        tick: u64,
    ) -> Result<(), String> {
        let mut routes: Vec<_> = self.routes.par_iter().collect();
        routes.par_sort_unstable_by_key(|r| (r.source, r.destination));
        let mut journeys: Vec<_> = self.journeys.par_iter().map(|j| j.person).collect();
        journeys.par_sort_unstable();
        let mut observations: Vec<_> = self
            .observations
            .par_iter()
            .map(|o| (o.person, o.source, o.destination))
            .collect();
        observations.par_sort_unstable();
        if routes
            .par_windows(2)
            .any(|r| (r[0].source, r[0].destination) == (r[1].source, r[1].destination))
            || journeys.par_windows(2).any(|r| r[0] == r[1])
            || observations.par_windows(2).any(|pair| pair[0] == pair[1])
            || observations
                .par_chunk_by(|a, b| a.0 == b.0)
                .any(|group| group.len() > self.observation_limit)
            || ((!self.closures.is_empty() || self.personal_routing)
                && !(1..=64).contains(&self.observation_limit))
            || self.closures.par_iter().any(|c| {
                c.start >= c.end
                    || routes
                        .binary_search_by_key(&(c.source, c.destination), |r| {
                            (r.source, r.destination)
                        })
                        .is_err()
            })
            || self.observations.par_iter().any(|o| {
                o.observed_at > tick
                    || o.estimated_ticks == Some(0)
                    || o.witness
                        .is_some_and(|id| people.binary_search_by_key(&id, |p| p.id).is_err())
                    || o.learned_from
                        .is_some_and(|id| people.binary_search_by_key(&id, |p| p.id).is_err())
                    || people.binary_search_by_key(&o.person, |p| p.id).is_err()
                    || routes
                        .binary_search_by_key(&(o.source, o.destination), |r| {
                            (r.source, r.destination)
                        })
                        .is_err()
            })
            || self.routes.par_iter().any(|r| {
                r.source == r.destination
                    || r.ticks == 0
                    || !regions.contains(&r.source)
                    || !regions.contains(&r.destination)
            })
            || self.journeys.par_iter().any(|j| {
                let route = routes
                    .binary_search_by_key(&(j.source, j.destination), |r| (r.source, r.destination))
                    .ok()
                    .map(|i| routes[i]);
                route.is_none_or(|r| j.remaining == 0 || j.remaining > r.ticks)
                    || people
                        .binary_search_by_key(&j.person, |p| p.id)
                        .ok()
                        .is_none_or(|i| people[i].region.is_some())
                    || working.binary_search(&j.person).is_ok()
            })
            || people
                .par_iter()
                .any(|p| p.region.is_none() != journeys.binary_search(&p.id).is_ok())
            || people
                .par_iter()
                .any(|p| p.region.is_some_and(|id| !regions.contains(&id)))
        {
            return Err("invalid travel state".into());
        }
        Ok(())
    }
    /// Validate an entire departure batch before changing people or journeys.
    fn plan_departures(
        &self,
        people: &[Person],
        requests: &[Departure],
        working: &[u64],
    ) -> Result<Vec<Journey>, String> {
        let mut requests: Vec<_> = requests.par_iter().copied().collect();
        requests.par_sort_unstable_by_key(|r| r.person);
        if requests.par_windows(2).any(|r| r[0].person == r[1].person) {
            return Err("duplicate departure".into());
        }
        let mut routes: Vec<_> = self.routes.par_iter().collect();
        routes.par_sort_unstable_by_key(|r| (r.source, r.destination));
        requests
            .par_iter()
            .map(|request| {
                let person = people
                    .binary_search_by_key(&request.person, |p| p.id)
                    .ok()
                    .map(|i| &people[i])
                    .ok_or("unknown traveler")?;
                let source = person.region.ok_or("person is already traveling")?;
                if person.health == 0 || working.binary_search(&person.id).is_ok() {
                    return Err("person cannot depart");
                }
                let route = routes
                    .binary_search_by_key(&(source, request.destination), |r| {
                        (r.source, r.destination)
                    })
                    .ok()
                    .map(|i| routes[i])
                    .ok_or("unknown route")?;
                Ok(Journey {
                    person: person.id,
                    source,
                    destination: route.destination,
                    remaining: route.ticks,
                })
            })
            .collect::<Result<_, &str>>()
            .map_err(str::to_owned)
    }
    fn publish_departures(&self, people: &[Person], journeys: Vec<Journey>) -> (Self, Vec<Person>) {
        let mut next_people: Vec<_> = people.par_iter().cloned().collect();
        next_people.par_iter_mut().for_each(|person| {
            if journeys
                .binary_search_by_key(&person.id, |journey| journey.person)
                .is_ok()
            {
                person.region = None;
            }
        });
        let mut next = self.clone();
        next.journeys.par_extend(journeys);
        next.journeys.par_sort_unstable_by_key(|j| j.person);
        (next, next_people)
    }
    /// Validate a batch as a whole, then resolve physical route closures per
    /// traveler. A blocked attempt does not cancel unrelated departures.
    pub fn attempt(
        &self,
        people: &[Person],
        requests: &[Departure],
        working: &[u64],
        tick: u64,
    ) -> Result<(Self, Vec<Person>, Vec<DepartureOutcome>), String> {
        let journeys = self.plan_departures(people, requests, working)?;
        if self.closures.is_empty() && !self.personal_routing {
            let (all_travel, all_people) = self.publish_departures(people, journeys);
            return Ok((
                all_travel,
                all_people,
                requests
                    .par_iter()
                    .map(|r| DepartureOutcome {
                        person: r.person,
                        departed: true,
                    })
                    .collect(),
            ));
        }
        let closed = self.closed_routes(tick);
        // A route attempt reveals whether passage is currently possible, not
        // the true duration of a journey the person has not yet completed.
        let mut estimates: Vec<_> = self
            .observations
            .par_iter()
            .filter_map(|observation| {
                observation.estimated_ticks.map(|ticks| {
                    (
                        (
                            observation.person,
                            observation.source,
                            observation.destination,
                        ),
                        ticks,
                    )
                })
            })
            .collect();
        if estimates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            estimates.par_sort_unstable_by_key(|row| row.0);
        }
        let results: Vec<_> = requests
            .par_iter()
            .map(|request| {
                let index = people
                    .binary_search_by_key(&request.person, |p| p.id)
                    .unwrap();
                let source = people[index].region.unwrap();
                (
                    DepartureOutcome {
                        person: request.person,
                        departed: closed
                            .binary_search(&(source, request.destination))
                            .is_err(),
                    },
                    RouteObservation {
                        person: request.person,
                        source,
                        destination: request.destination,
                        open: closed
                            .binary_search(&(source, request.destination))
                            .is_err(),
                        observed_at: tick,
                        estimated_ticks: estimates
                            .binary_search_by_key(
                                &(request.person, source, request.destination),
                                |row| row.0,
                            )
                            .ok()
                            .map(|index| estimates[index].1),
                        witness: Some(request.person),
                        learned_from: None,
                    },
                )
            })
            .collect();
        let accepted: Vec<_> = journeys
            .into_par_iter()
            .filter(|journey| {
                closed
                    .binary_search(&(journey.source, journey.destination))
                    .is_err()
            })
            .collect();
        let (mut next, next_people) = self.publish_departures(people, accepted);
        let incoming: Vec<_> = results
            .par_iter()
            .map(|(_, observation)| observation.clone())
            .collect();
        next.receive_reports(&incoming);
        Ok((
            next,
            next_people,
            results
                .into_par_iter()
                .map(|(outcome, _)| outcome)
                .collect(),
        ))
    }
    /// Merge validated personal reports in parallel, retaining bounded history.
    pub fn receive_reports(&mut self, incoming: &[RouteObservation]) {
        if incoming.is_empty() {
            return;
        }
        // Independent recipients with no prior reports need neither conflict
        // arbitration nor retention sorting.
        if self.observations.is_empty()
            && self.observation_limit > 0
            && incoming
                .par_windows(2)
                .all(|pair| pair[0].person < pair[1].person)
        {
            self.observations = incoming.par_iter().cloned().collect();
            return;
        }
        let mut observations = std::mem::take(&mut self.observations);
        observations.par_extend(incoming.par_iter().cloned());
        // For an equal timestamp, a newly received result supersedes the
        // previous report. Stable sorting preserves that publication order.
        if observations.par_windows(2).any(|pair| {
            (
                pair[0].person,
                pair[0].source,
                pair[0].destination,
                pair[0].observed_at,
            ) > (
                pair[1].person,
                pair[1].source,
                pair[1].destination,
                pair[1].observed_at,
            )
        }) {
            observations.par_sort_by_key(|o| (o.person, o.source, o.destination, o.observed_at));
        }
        observations = observations
            .par_chunk_by(|a, b| {
                (a.person, a.source, a.destination) == (b.person, b.source, b.destination)
            })
            .map(|group| group.last().unwrap().clone())
            .collect();
        observations.par_sort_unstable_by_key(|o| {
            (o.person, Reverse(o.observed_at), o.source, o.destination)
        });
        self.observations = observations
            .par_chunk_by(|a, b| a.person == b.person)
            .flat_map_iter(|group| group.iter().take(self.observation_limit).cloned())
            .collect();
        self.observations
            .par_sort_unstable_by_key(|o| (o.person, o.source, o.destination));
    }
    /// Arrival occurs after this interval's personal and physical decisions.
    pub fn advance(&self, people: &mut [Person]) -> Self {
        let mut journeys: Vec<_> = self
            .journeys
            .par_iter()
            .map(|j| {
                let mut j = j.clone();
                let incapacitated_immortal = people
                    .binary_search_by_key(&j.person, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health == 0 && people[i].immortal.is_some());
                if !incapacitated_immortal {
                    j.remaining -= 1;
                }
                j
            })
            .collect();
        journeys.par_sort_unstable_by_key(|j| j.person);
        people.par_iter_mut().for_each(|person| {
            if let Ok(index) = journeys.binary_search_by_key(&person.id, |j| j.person) {
                if person.health == 0 && person.immortal.is_none() {
                    // Coarse routes have no intermediate location; the source retains the body.
                    person.region = Some(journeys[index].source);
                } else if journeys[index].remaining == 0 {
                    person.region = Some(journeys[index].destination);
                }
            }
        });
        let journeys = journeys
            .into_par_iter()
            .filter(|j| {
                j.remaining > 0
                    && people
                        .binary_search_by_key(&j.person, |p| p.id)
                        .ok()
                        .is_some_and(|i| people[i].health > 0 || people[i].immortal.is_some())
            })
            .collect();
        Self {
            routes: self.routes.clone(),
            journeys,
            closures: self.closures.par_iter().cloned().collect(),
            observations: self.observations.par_iter().cloned().collect(),
            observation_limit: self.observation_limit,
            personal_routing: self.personal_routing,
        }
    }
    /// Stop travelers who died in a later lifecycle phase of this boundary.
    pub fn settle_deaths(&mut self, people: &mut [Person]) {
        if self.journeys.is_empty()
            || !people
                .par_iter()
                .any(|p| p.health == 0 && p.immortal.is_none() && p.region.is_none())
        {
            return;
        }
        people
            .par_iter_mut()
            .filter(|p| p.health == 0 && p.immortal.is_none() && p.region.is_none())
            .for_each(|p| {
                if let Ok(i) = self.journeys.binary_search_by_key(&p.id, |j| j.person) {
                    p.region = Some(self.journeys[i].source);
                }
            });
        self.journeys = std::mem::take(&mut self.journeys)
            .into_par_iter()
            .filter(|j| {
                people
                    .binary_search_by_key(&j.person, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0 || people[i].immortal.is_some())
            })
            .collect();
    }
}
