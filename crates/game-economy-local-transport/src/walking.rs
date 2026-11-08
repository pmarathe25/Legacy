//! Optional within-region travel between physical logistics endpoints.
use super::{Endpoint, LocalTransport, Store};
use game_economy_person::Person;
use game_lots::{Place, View};
use game_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashSet, sync::Arc};

/// A directed, data-defined walking time in simulation boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub region: u64,
    pub from: Endpoint,
    pub to: Endpoint,
    pub ticks: u64,
}

/// A person in transit has not yet reached either endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Location {
    At {
        region: u64,
        endpoint: Endpoint,
    },
    Walking {
        region: u64,
        from: Endpoint,
        to: Endpoint,
        remaining: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub person: u64,
    pub location: Location,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub person: u64,
    pub region: u64,
    pub destination: Endpoint,
}

/// A person's own standing inclination to visit a local endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub region: u64,
    pub destination: Endpoint,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Progressed,
    Arrived,
    AlreadyThere,
    NoRoute,
    DifferentLeg,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub destination: Endpoint,
    pub outcome: Outcome,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Walking {
    pub routes: Arc<Vec<Route>>,
    #[serde(default)]
    pub preferences: Arc<Vec<Preference>>,
    #[serde(default)]
    pub positions: Vec<Position>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Clone for Walking {
    fn clone(&self) -> Self {
        Self {
            routes: self.routes.clone(),
            preferences: self.preferences.clone(),
            positions: self.positions.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
impl Walking {
    pub fn normalize(&mut self) {
        Arc::make_mut(&mut self.routes).par_sort_unstable_by_key(|r| (r.region, r.from, r.to));
        Arc::make_mut(&mut self.preferences).par_sort_unstable_by_key(|p| p.person);
        self.positions.par_sort_unstable_by_key(|p| p.person);
    }

    pub fn validate(
        &self,
        people: &[Person],
        stores: &[Store],
        regions: &HashSet<u64>,
        tick: u64,
    ) -> Result<(), String> {
        let endpoint = |region, end| match end {
            Endpoint::Stock => regions.contains(&region),
            Endpoint::Store { id } => stores
                .binary_search_by_key(&id, |s| s.id)
                .ok()
                .is_some_and(|i| stores[i].region == region),
        };
        if self
            .routes
            .par_windows(2)
            .any(|p| (p[0].region, p[0].from, p[0].to) >= (p[1].region, p[1].from, p[1].to))
            || self.routes.par_iter().any(|r| {
                r.from == r.to
                    || r.ticks == 0
                    || !endpoint(r.region, r.from)
                    || !endpoint(r.region, r.to)
            })
            || self
                .preferences
                .par_windows(2)
                .any(|p| p[0].person >= p[1].person)
            || self.preferences.par_iter().any(|p| {
                people
                    .binary_search_by_key(&p.person, |row| row.id)
                    .is_err()
                    || !endpoint(p.region, p.destination)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by.is_some_and(|id| {
                        id == p.person || people.binary_search_by_key(&id, |row| row.id).is_err()
                    })
            })
            || self
                .positions
                .par_windows(2)
                .any(|p| p[0].person >= p[1].person)
            || self.positions.par_iter().any(|p| {
                let region = match p.location {
                    Location::At { region, .. } | Location::Walking { region, .. } => region,
                };
                people
                    .binary_search_by_key(&p.person, |p| p.id)
                    .ok()
                    .is_none_or(|i| people[i].region != Some(region))
                    || match p.location {
                        Location::At { endpoint: end, .. } => !endpoint(region, end),
                        Location::Walking {
                            from,
                            to,
                            remaining,
                            ..
                        } => self
                            .route(region, from, to)
                            .is_none_or(|r| remaining == 0 || remaining >= r.ticks),
                    }
            })
            || self
                .events
                .par_iter()
                .any(|e| e.tick > tick || people.binary_search_by_key(&e.person, |p| p.id).is_err())
        {
            return Err("invalid local walking state".into());
        }
        Ok(())
    }

    fn route(&self, region: u64, from: Endpoint, to: Endpoint) -> Option<&Route> {
        self.routes
            .binary_search_by_key(&(region, from, to), |r| (r.region, r.from, r.to))
            .ok()
            .map(|i| &self.routes[i])
    }

    /// Check whether an autonomous choice can advance toward this endpoint.
    pub fn can_advance(&self, person: u64, region: u64, destination: Endpoint) -> bool {
        match self.location(person, region) {
            Location::At { endpoint, .. } => {
                endpoint != destination && self.route(region, endpoint, destination).is_some()
            }
            Location::Walking { to, .. } => to == destination,
        }
    }

    /// People without an explicit position are at their region's stock node.
    pub fn location(&self, person: u64, region: u64) -> Location {
        self.positions
            .binary_search_by_key(&person, |p| p.person)
            .ok()
            .map_or(
                Location::At {
                    region,
                    endpoint: Endpoint::Stock,
                },
                |i| self.positions[i].location,
            )
    }

    pub fn present(&self, person: u64, region: u64, endpoint: Endpoint) -> bool {
        self.location(person, region) == Location::At { region, endpoint }
    }

    /// Stage independent personal walking choices; absence pauses a leg.
    pub fn prepare(
        &self,
        people: &[Person],
        requests: &[Request],
        tick: u64,
    ) -> Result<Walking, String> {
        if requests.is_empty() {
            let mut next = self.clone();
            next.events.clear();
            return Ok(next);
        }
        let ordered = if requests
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            let mut ordered = requests.to_vec();
            ordered.par_sort_unstable_by_key(|r| r.person);
            Cow::Owned(ordered)
        } else {
            Cow::Borrowed(requests)
        };
        let requests = ordered.as_ref();
        if requests.par_windows(2).any(|p| p[0].person == p[1].person)
            || requests.par_chunks(4096).any(|chunk| {
                let mut at = people.partition_point(|person| person.id < chunk[0].person);
                chunk.iter().any(|request| {
                    while at < people.len() && people[at].id < request.person {
                        at += 1;
                    }
                    people.get(at).is_none_or(|person| {
                        person.id != request.person
                            || person.health == 0
                            || person.region != Some(request.region)
                    })
                })
            })
        {
            return Err("invalid local walking request".into());
        }
        let updates: Vec<_> = requests
            .par_chunks(4096)
            .flat_map_iter(|chunk| {
                let mut at = self
                    .positions
                    .partition_point(|position| position.person < chunk[0].person);
                chunk.iter().map(move |request| {
                    while at < self.positions.len() && self.positions[at].person < request.person {
                        at += 1;
                    }
                    let current = self
                        .positions
                        .get(at)
                        .filter(|position| position.person == request.person)
                        .map_or(
                            Location::At {
                                region: request.region,
                                endpoint: Endpoint::Stock,
                            },
                            |position| position.location,
                        );
                    let (location, outcome) = match current {
                        Location::At { endpoint, .. } if endpoint == request.destination => {
                            (current, Outcome::AlreadyThere)
                        }
                        Location::At { region, endpoint } => {
                            match self.route(region, endpoint, request.destination) {
                                None => (current, Outcome::NoRoute),
                                Some(route) if route.ticks == 1 => (
                                    Location::At {
                                        region,
                                        endpoint: route.to,
                                    },
                                    Outcome::Arrived,
                                ),
                                Some(route) => (
                                    Location::Walking {
                                        region,
                                        from: endpoint,
                                        to: route.to,
                                        remaining: route.ticks - 1,
                                    },
                                    Outcome::Progressed,
                                ),
                            }
                        }
                        Location::Walking {
                            region,
                            from,
                            to,
                            remaining,
                        } if to == request.destination => {
                            if remaining == 1 {
                                (
                                    Location::At {
                                        region,
                                        endpoint: to,
                                    },
                                    Outcome::Arrived,
                                )
                            } else {
                                (
                                    Location::Walking {
                                        region,
                                        from,
                                        to,
                                        remaining: remaining - 1,
                                    },
                                    Outcome::Progressed,
                                )
                            }
                        }
                        Location::Walking { .. } => (current, Outcome::DifferentLeg),
                    };
                    (
                        Position {
                            person: request.person,
                            location,
                        },
                        Event {
                            tick,
                            person: request.person,
                            destination: request.destination,
                            outcome,
                        },
                    )
                })
            })
            .collect();
        let (mut positions, events): (Vec<_>, Vec<_>) = updates.into_par_iter().unzip();
        if !self.positions.is_empty()
            && !(self.positions.len() == requests.len()
                && self
                    .positions
                    .par_iter()
                    .zip(requests.par_iter())
                    .all(|(p, r)| p.person == r.person))
        {
            positions.par_extend(self.positions.par_chunks(4096).flat_map_iter(|chunk| {
                let mut at = requests.partition_point(|request| request.person < chunk[0].person);
                chunk
                    .iter()
                    .filter(move |position| {
                        while at < requests.len() && requests[at].person < position.person {
                            at += 1;
                        }
                        requests
                            .get(at)
                            .is_none_or(|request| request.person != position.person)
                    })
                    .cloned()
            }));
        }
        if positions
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            positions.par_sort_unstable_by_key(|p| p.person);
        }
        Ok(Walking {
            routes: self.routes.clone(),
            preferences: self.preferences.clone(),
            positions,
            events,
        })
    }
}

/// Choose independently motivated destinations without inspecting remote stock.
pub fn personal_requests(
    owner: &LocalTransport,
    walking: &Walking,
    people: &[Person],
    view: &View,
    (busy, visit_blocked): (&[u64], &[u64]),
    opinions: Option<&Opinions>,
    max_hunger: u32,
) -> Vec<Request> {
    let feelings = opinions
        .filter(|_| {
            owner
                .policies
                .par_iter()
                .any(|p| p.pressure_threshold.is_some())
                || walking
                    .preferences
                    .par_iter()
                    .any(|p| p.pressure_threshold.is_some())
        })
        .map(Opinions::index);
    let mut requests: Vec<_> = owner
        .policies
        .par_iter()
        .filter_map(|policy| {
            let person = &people[people.binary_search_by_key(&policy.person, |p| p.id).ok()?];
            if person.controlled
                || person.health == 0
                || person.region != Some(policy.region)
                || person.hunger > max_hunger
                || busy.binary_search(&person.id).is_ok()
            {
                return None;
            }
            let pressured = policy
                .pressured_by
                .zip(policy.pressure_threshold)
                .is_some_and(|(other, threshold)| {
                    feelings.as_ref().is_some_and(|index| {
                        let (fear, obligation) = index.feelings(person.id, other);
                        fear.saturating_add(obligation) >= threshold
                    })
                });
            if !policy.willing && !pressured {
                return None;
            }
            let held = view
                .container(Place::Person(person.id))?
                .quantity(policy.good);
            let destination = if held == 0 {
                policy.source
            } else {
                policy.destination
            };
            match walking.location(person.id, policy.region) {
                Location::At { endpoint, .. } if endpoint == destination => None,
                Location::At { endpoint, .. }
                    if walking
                        .route(policy.region, endpoint, destination)
                        .is_none() =>
                {
                    None
                }
                Location::Walking { to, .. } if to != destination => None,
                _ => Some(Request {
                    person: person.id,
                    region: policy.region,
                    destination,
                }),
            }
        })
        .collect();
    let visits: Vec<_> = walking
        .preferences
        .par_iter()
        .filter_map(|preference| {
            let person = &people[people
                .binary_search_by_key(&preference.person, |p| p.id)
                .ok()?];
            if person.controlled
                || person.health == 0
                || person.region != Some(preference.region)
                || busy.binary_search(&person.id).is_ok()
                || visit_blocked.binary_search(&person.id).is_ok()
                || requests
                    .binary_search_by_key(&person.id, |request| request.person)
                    .is_ok()
            {
                return None;
            }
            let pressured = preference
                .pressured_by
                .zip(preference.pressure_threshold)
                .is_some_and(|(other, threshold)| {
                    feelings.as_ref().is_some_and(|index| {
                        let (fear, obligation) = index.feelings(person.id, other);
                        fear.saturating_add(obligation) >= threshold
                    })
                });
            if !preference.willing && !pressured {
                return None;
            }
            match walking.location(person.id, preference.region) {
                Location::At { endpoint, .. } if endpoint == preference.destination => None,
                Location::At { endpoint, .. }
                    if walking
                        .route(preference.region, endpoint, preference.destination)
                        .is_none() =>
                {
                    None
                }
                Location::Walking { to, .. } if to != preference.destination => None,
                _ => Some(Request {
                    person: person.id,
                    region: preference.region,
                    destination: preference.destination,
                }),
            }
        })
        .collect();
    if !visits.is_empty() {
        requests.par_extend(visits);
        requests.par_sort_unstable_by_key(|request| request.person);
    }
    requests
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people(count: u64) -> Vec<Person> {
        (1..=count)
            .map(|id| Person {
                id,
                region: Some(1),
                health: 100,
                hunger: 0,
                immortal: None,
                willing_to_work: false,
                seek_meals: false,
                controlled: false,
            })
            .collect()
    }
    fn model() -> Walking {
        Walking {
            preferences: vec![].into(),
            routes: vec![Route {
                region: 1,
                from: Endpoint::Stock,
                to: Endpoint::Store { id: 7 },
                ticks: 3,
            }]
            .into(),
            positions: vec![],
            events: vec![],
        }
    }

    #[test]
    fn walking_requires_elapsed_boundaries_and_can_pause_or_resume() {
        let people = people(2);
        let request = Request {
            person: 1,
            region: 1,
            destination: Endpoint::Store { id: 7 },
        };
        let run = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| {
                    let start = model();
                    let first = start.prepare(&people, &[request], 1).unwrap();
                    assert_eq!(
                        first.location(1, 1),
                        Location::Walking {
                            region: 1,
                            from: Endpoint::Stock,
                            to: request.destination,
                            remaining: 2
                        }
                    );
                    let paused = first.prepare(&people, &[], 2).unwrap();
                    assert_eq!(paused.positions, first.positions);
                    let mut restored: Walking =
                        serde_json::from_str(&serde_json::to_string(&paused).unwrap()).unwrap();
                    restored.normalize();
                    let stores = [Store {
                        id: 7,
                        region: 1,
                        name: "store".into(),
                    }];
                    restored
                        .validate(&people, &stores, &HashSet::from([1]), 2)
                        .unwrap();
                    let second = restored.prepare(&people, &[request], 3).unwrap();
                    assert!(!second.present(1, 1, request.destination));
                    let final_state = second.prepare(&people, &[request], 4).unwrap();
                    assert!(final_state.present(1, 1, request.destination));
                    assert_eq!(final_state.events[0].outcome, Outcome::Arrived);
                    assert!(start.prepare(&people, &[request, request], 1).is_err());
                    final_state
                })
        };
        assert_eq!(run(1), run(4));
    }

    #[test]
    fn missing_routes_and_changed_destinations_do_not_teleport_people() {
        let people = people(1);
        let request = Request {
            person: 1,
            region: 1,
            destination: Endpoint::Store { id: 7 },
        };
        let start = model();
        let missing = start
            .prepare(
                &people,
                &[Request {
                    destination: Endpoint::Store { id: 8 },
                    ..request
                }],
                1,
            )
            .unwrap();
        assert_eq!(missing.events[0].outcome, Outcome::NoRoute);
        assert!(missing.present(1, 1, Endpoint::Stock));
        let underway = start.prepare(&people, &[request], 1).unwrap();
        let changed = underway
            .prepare(
                &people,
                &[Request {
                    destination: Endpoint::Stock,
                    ..request
                }],
                2,
            )
            .unwrap();
        assert_eq!(changed.events[0].outcome, Outcome::DifferentLeg);
        assert_eq!(changed.positions, underway.positions);
    }

    #[test]
    #[ignore = "performance measurement"]
    fn walking_batch_benchmark() {
        use std::hash::{Hash, Hasher};
        let count: u64 = std::env::var("LEGACY_PEOPLE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100_000);
        let workers = std::env::var("LEGACY_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let people = people(count);
        let requests: Vec<_> = (1..=count)
            .map(|person| Request {
                person,
                region: 1,
                destination: Endpoint::Store { id: 7 },
            })
            .collect();
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let initial = model();
                let mut samples = Vec::new();
                let mut fingerprint = None;
                for _ in 0..4 {
                    let start = std::time::Instant::now();
                    let next = initial.prepare(&people, &requests, 1).unwrap();
                    samples.push(start.elapsed().as_secs_f64());
                    assert_eq!(next.positions.len(), count as usize);
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&next).unwrap().hash(&mut hash);
                    let actual = hash.finish();
                    if let Some(expected) = fingerprint {
                        assert_eq!(actual, expected);
                    }
                    fingerprint = Some(actual);
                }
                samples.remove(0);
                samples.sort_by(f64::total_cmp);
                eprintln!(
                    "walkers={count} workers={workers} elapsed_median={:.6} fingerprint={:016x}",
                    samples[1],
                    fingerprint.unwrap()
                );
            });
    }
}
