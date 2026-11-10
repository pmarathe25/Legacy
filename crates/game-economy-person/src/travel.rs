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
    /// Milliseconds already traveled toward the next whole travel tick.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub progress_millis: u64,
    /// Travel has finished, but arrival has not yet been published.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pending_arrival: bool,
}
fn is_zero(value: &u64) -> bool {
    *value == 0
}
fn is_false(value: &bool) -> bool {
    !*value
}

impl Journey {
    /// Time until arrival; the caller publishes arrival and other world effects.
    pub fn arrival_after(&self, quantum_millis: u64) -> Result<u128, &'static str> {
        if quantum_millis == 0
            || self.progress_millis >= quantum_millis
            || (self.remaining == 0) != self.pending_arrival
            || (self.pending_arrival && self.progress_millis != 0)
        {
            return Err("invalid journey progress");
        }
        Ok(u128::from(self.remaining) * u128::from(quantum_millis)
            - u128::from(self.progress_millis))
    }

    /// Advance the counter only, returning elapsed milliseconds consumed before arrival.
    pub fn advance_elapsed(
        &mut self,
        elapsed: u64,
        quantum_millis: u64,
    ) -> Result<u64, &'static str> {
        let consumed = self.arrival_after(quantum_millis)?.min(u128::from(elapsed)) as u64;
        let progress = u128::from(self.progress_millis) + u128::from(consumed);
        self.remaining -= (progress / u128::from(quantum_millis)) as u64;
        self.progress_millis = (progress % u128::from(quantum_millis)) as u64;
        self.pending_arrival = self.remaining == 0;
        Ok(consumed)
    }
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

/// One individual's choice to share or alter a particular route report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteSharing {
    pub person: u64,
    pub source: u64,
    pub destination: u64,
    #[serde(default)]
    pub target: Option<u64>,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub fear_at_least: Option<u64>,
    #[serde(default)]
    pub obligation_at_least: Option<u64>,
    #[serde(default)]
    pub claimed_open: Option<bool>,
    #[serde(default)]
    pub claimed_ticks: Option<u64>,
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
    #[serde(default)]
    pub sharing: Vec<RouteSharing>,
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
            sharing: Vec::new(),
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
    journeys: HashMap<u64, (u64, u64, u64, u64)>,
}
impl Colocation {
    pub fn new(travel: Option<&Travel>) -> Self {
        Self {
            journeys: travel
                .map_or(&[][..], |t| t.journeys.as_slice())
                .par_iter()
                .map(|j| {
                    (
                        j.person,
                        (j.source, j.destination, j.remaining, j.progress_millis),
                    )
                })
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
            sharing: self.sharing.par_iter().cloned().collect(),
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
    quantum_millis: u64,
) -> Result<(), String> {
    if travel
        .journeys
        .par_iter()
        .any(|journey| journey.arrival_after(quantum_millis).is_err())
    {
        return Err("invalid journey progress".into());
    }
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
    /// Progress existing journeys without publishing arrival. The caller owns
    /// the world interval, generation and any later settlement. All input is
    /// validated before mutation; incapacitated travelers remain in place.
    pub fn progress_elapsed(
        &mut self,
        people: &[Person],
        elapsed_millis: u64,
        quantum_millis: u64,
    ) -> Result<(), &'static str> {
        if quantum_millis == 0
            || people.par_windows(2).any(|pair| pair[0].id >= pair[1].id)
            || self
                .journeys
                .par_windows(2)
                .any(|pair| pair[0].person >= pair[1].person)
            || self.journeys.par_iter().any(|journey| {
                journey.arrival_after(quantum_millis).is_err()
                    || people
                        .binary_search_by_key(&journey.person, |p| p.id)
                        .ok()
                        .is_none_or(|index| people[index].region.is_some())
            })
        {
            return Err("invalid journey progress batch");
        }
        self.journeys.par_iter_mut().for_each(|journey| {
            let index = people
                .binary_search_by_key(&journey.person, |p| p.id)
                .expect("validated traveler");
            if people[index].health > 0 {
                journey
                    .advance_elapsed(elapsed_millis, quantum_millis)
                    .expect("validated journey progress");
            }
        });
        Ok(())
    }

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
        let mut speakers: Vec<_> = self
            .sharing
            .par_iter()
            .map(|policy| policy.person)
            .collect();
        speakers.par_sort_unstable();
        if routes
            .par_windows(2)
            .any(|r| (r[0].source, r[0].destination) == (r[1].source, r[1].destination))
            || journeys.par_windows(2).any(|r| r[0] == r[1])
            || observations.par_windows(2).any(|pair| pair[0] == pair[1])
            || speakers.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.sharing.par_iter().any(|policy| {
                people
                    .binary_search_by_key(&policy.person, |person| person.id)
                    .is_err()
                    || policy.target.is_some_and(|id| {
                        id == policy.person
                            || people
                                .binary_search_by_key(&id, |person| person.id)
                                .is_err()
                    })
                    || policy.claimed_ticks == Some(0)
                    || routes
                        .binary_search_by_key(&(policy.source, policy.destination), |route| {
                            (route.source, route.destination)
                        })
                        .is_err()
            })
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
                route.is_none_or(|r| {
                    (j.remaining == 0) != j.pending_arrival
                        || (j.pending_arrival && j.progress_millis != 0)
                        || j.remaining > r.ticks
                }) || people
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
                    progress_millis: 0,
                    pending_arrival: false,
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
        // If each recipient already has exactly this route, replacements are
        // independent and need no regrouping or retention arbitration.
        if self.observations.len() == incoming.len()
            && self
                .observations
                .par_windows(2)
                .all(|pair| pair[0].person < pair[1].person)
            && self
                .observations
                .par_iter()
                .zip(incoming.par_iter())
                .all(|(old, new)| {
                    (old.person, old.source, old.destination)
                        == (new.person, new.source, new.destination)
                })
        {
            self.observations
                .par_iter_mut()
                .zip(incoming.par_iter())
                .for_each(|(old, new)| {
                    if new.observed_at >= old.observed_at {
                        *old = new.clone();
                    }
                });
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
        let journeys: Vec<_> = self
            .journeys
            .par_iter()
            .map(|j| {
                let mut j = j.clone();
                let incapacitated_immortal = people
                    .binary_search_by_key(&j.person, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health == 0 && people[i].immortal.is_some());
                if !incapacitated_immortal && !j.pending_arrival {
                    j.remaining -= 1;
                }
                j
            })
            .collect();
        self.settle_journeys(people, journeys)
    }

    /// Publish pending arrivals and place mortal bodies without adding travel time.
    pub fn settle_elapsed(&self, people: &mut [Person]) -> Self {
        self.settle_journeys(people, self.journeys.par_iter().cloned().collect())
    }

    /// Publish living travelers whose elapsed journey has completed, without
    /// settling deaths or changing any other journey.
    /// People and journeys must be ordered by person ID as in a validated world.
    pub fn publish_arrivals(&mut self, people: &mut [Person]) -> Vec<u64> {
        let arrivals: Vec<_> = self
            .journeys
            .par_iter()
            .filter_map(|journey| {
                let person = people
                    .binary_search_by_key(&journey.person, |p| p.id)
                    .ok()?;
                (journey.pending_arrival && people[person].health > 0).then_some(journey.person)
            })
            .collect();
        if arrivals.is_empty() {
            return arrivals;
        }
        people.par_iter_mut().for_each(|person| {
            if arrivals.binary_search(&person.id).is_ok() {
                let index = self
                    .journeys
                    .binary_search_by_key(&person.id, |j| j.person)
                    .expect("arrival came from journey");
                person.region = Some(self.journeys[index].destination);
            }
        });
        self.journeys = std::mem::take(&mut self.journeys)
            .into_par_iter()
            .filter(|journey| arrivals.binary_search(&journey.person).is_err())
            .collect();
        arrivals
    }

    fn settle_journeys(&self, people: &mut [Person], mut journeys: Vec<Journey>) -> Self {
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
            sharing: self.sharing.par_iter().cloned().collect(),
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

#[cfg(test)]
mod elapsed_tests {
    use super::*;

    fn journey(person: u64) -> Journey {
        Journey {
            person,
            source: 1,
            destination: 2,
            remaining: 2,
            progress_millis: 0,
            pending_arrival: false,
        }
    }

    #[test]
    fn early_arrivals_do_not_settle_dead_or_unfinished_travelers() {
        let person = |id, health| Person {
            id,
            region: None,
            health,
            hunger: 0,
            hunger_remainder: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        };
        let mut people = vec![person(1, 100), person(2, 0), person(3, 100)];
        let mut rows = vec![journey(1), journey(2), journey(3)];
        rows[0].advance_elapsed(2000, 1000).unwrap();
        rows[1].advance_elapsed(2000, 1000).unwrap();
        let mut travel = Travel {
            journeys: rows,
            ..Travel::default()
        };
        assert_eq!(travel.publish_arrivals(&mut people), vec![1]);
        assert_eq!(
            people.iter().map(|p| p.region).collect::<Vec<_>>(),
            vec![Some(2), None, None]
        );
        assert_eq!(
            travel.journeys.iter().map(|j| j.person).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(travel.publish_arrivals(&mut people).is_empty());
    }

    #[test]
    fn split_progress_preserves_arrival_deadline_and_colocation() {
        let mut first = journey(1);
        let mut second = journey(2);
        first.advance_elapsed(333, 1000).unwrap();
        let travel = Travel {
            journeys: vec![first.clone(), second.clone()],
            ..Travel::default()
        };
        assert_eq!(
            Colocation::new(Some(&travel)).region((1, None), (2, None)),
            None
        );
        first.advance_elapsed(667, 1000).unwrap();
        second.advance_elapsed(1000, 1000).unwrap();
        assert_eq!(first.arrival_after(1000).unwrap(), 1000);
        let travel = Travel {
            journeys: vec![first.clone(), second],
            ..Travel::default()
        };
        assert_eq!(
            Colocation::new(Some(&travel)).region((1, None), (2, None)),
            Some(1)
        );
        assert_eq!(first.advance_elapsed(1500, 1000).unwrap(), 1000);
        assert_eq!((first.remaining, first.progress_millis), (0, 0));
    }

    #[test]
    fn invalid_progress_is_inert() {
        let mut row = journey(1);
        row.progress_millis = 1000;
        let before = row.clone();
        assert!(row.advance_elapsed(1, 1000).is_err());
        assert_eq!(row, before);
        let mut row = journey(1);
        row.pending_arrival = true;
        assert!(row.advance_elapsed(1, 1000).is_err());
    }

    #[test]
    fn progress_round_trips_and_old_saves_default_to_zero() {
        let mut row = journey(1);
        row.advance_elapsed(123, 1000).unwrap();
        let saved = serde_json::to_string(&row).unwrap();
        assert_eq!(serde_json::from_str::<Journey>(&saved).unwrap(), row);
        let mut old = serde_json::to_value(row).unwrap();
        old.as_object_mut().unwrap().remove("progress_millis");
        let restored = serde_json::from_value::<Journey>(old).unwrap();
        assert_eq!(restored.progress_millis, 0);
        assert!(!restored.pending_arrival);
    }

    #[test]
    fn whole_boundaries_retain_fraction_and_publish_arrival() {
        let mut row = journey(1);
        row.advance_elapsed(250, 1000).unwrap();
        let travel = Travel {
            journeys: vec![row],
            ..Travel::default()
        };
        let mut people = vec![Person {
            id: 1,
            region: None,
            health: 100,
            hunger: 0,
            hunger_remainder: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        }];
        let travel = travel.advance(&mut people);
        assert_eq!(
            (
                travel.journeys[0].remaining,
                travel.journeys[0].progress_millis
            ),
            (1, 250)
        );
        assert_eq!(people[0].region, None);
        let travel = travel.advance(&mut people);
        assert!(travel.journeys.is_empty());
        assert_eq!(people[0].region, Some(2));
    }

    #[test]
    fn finished_journey_can_be_saved_before_arrival_is_published() {
        let mut row = journey(1);
        assert_eq!(row.advance_elapsed(2500, 1000).unwrap(), 2000);
        assert!(row.pending_arrival);
        assert_eq!(row.arrival_after(1000).unwrap(), 0);
        let saved = serde_json::to_string(&row).unwrap();
        let row: Journey = serde_json::from_str(&saved).unwrap();
        let travel = Travel {
            routes: vec![Route {
                source: 1,
                destination: 2,
                ticks: 2,
            }],
            journeys: vec![row],
            ..Travel::default()
        };
        let mut people = vec![Person {
            id: 1,
            region: None,
            health: 100,
            hunger: 0,
            hunger_remainder: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        }];
        travel
            .validate(&people, &HashSet::from([1, 2]), &[], 0)
            .unwrap();
        assert_eq!(people[0].region, None);
        let settled = travel.settle_elapsed(&mut people);
        assert!(settled.journeys.is_empty());
        assert_eq!(people[0].region, Some(2));
    }

    #[test]
    fn settlement_does_not_add_travel_progress() {
        let mut row = journey(1);
        row.advance_elapsed(500, 1000).unwrap();
        let travel = Travel {
            journeys: vec![row.clone()],
            ..Travel::default()
        };
        let mut people = vec![Person {
            id: 1,
            region: None,
            health: 100,
            hunger: 0,
            hunger_remainder: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        }];
        let settled = travel.settle_elapsed(&mut people);
        assert_eq!(settled.journeys, vec![row]);
        assert_eq!(people[0].region, None);
    }

    #[test]
    fn split_progress_and_settlement_match_whole_boundary() {
        let people: Vec<_> = (1..=3)
            .map(|id| Person {
                id,
                region: None,
                health: if id == 2 { 0 } else { 100 },
                hunger: 0,
                hunger_remainder: 0,
                immortal: (id == 2).then_some(crate::Immortal {
                    recovery_per_tick: 1,
                }),
                willing_to_work: false,
                seek_meals: false,
                controlled: false,
            })
            .collect();
        let mut first = journey(1);
        first.remaining = 1;
        let initial = Travel {
            journeys: vec![first, journey(2), journey(3)],
            ..Travel::default()
        };
        let mut whole_people = people.clone();
        let expected = initial.advance(&mut whole_people);
        let mut split_people = people;
        let mut split = initial;
        split.progress_elapsed(&split_people, 333, 1000).unwrap();
        split.progress_elapsed(&split_people, 667, 1000).unwrap();
        let actual = split.settle_elapsed(&mut split_people);
        assert_eq!(actual, expected);
        assert_eq!(split_people, whole_people);
    }

    #[test]
    fn batched_journeys_pause_incapacitated_and_match_across_workers() {
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut people: Vec<_> = (1..=4096)
                        .map(|id| Person {
                            id,
                            region: None,
                            health: if id == 1 { 0 } else { 100 },
                            hunger: 0,
                            hunger_remainder: 0,
                            immortal: (id == 1).then_some(crate::Immortal {
                                recovery_per_tick: 1,
                            }),
                            willing_to_work: false,
                            seek_meals: false,
                            controlled: false,
                        })
                        .collect();
                    let mut travel = Travel {
                        journeys: (1..=4096).map(journey).collect(),
                        ..Travel::default()
                    };
                    let before = travel.clone();
                    assert!(travel.progress_elapsed(&people, 1, 0).is_err());
                    assert_eq!(travel, before);
                    people[100].region = Some(1);
                    assert!(travel.progress_elapsed(&people, 1, 1000).is_err());
                    assert_eq!(travel, before);
                    people[100].region = None;
                    travel.progress_elapsed(&people, 333, 1000).unwrap();
                    travel.progress_elapsed(&people, 1667, 1000).unwrap();
                    assert_eq!(travel.journeys[0], journey(1));
                    assert!(
                        travel.journeys[1..]
                            .iter()
                            .all(|row| row.pending_arrival && row.remaining == 0)
                    );
                    travel
                })
        };
        assert_eq!(run(1), run(4));
    }
}
