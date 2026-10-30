//! Individuals travel on directed, data-defined routes while retaining their custody.
use crate::Person;
use game_parallel_lot_runtime::{Phase, Work};
use game_parallel_lots::{Place, View};
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
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Travel {
    pub routes: Vec<Route>,
    #[serde(default)]
    pub journeys: Vec<Journey>,
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
) -> Result<(), String> {
    let regions = view
        .containers
        .par_iter()
        .filter_map(|row| match row.place {
            Place::Stock { region, .. } => Some(region),
            _ => None,
        })
        .collect();
    travel.validate(people, &regions, &working(work))
}

impl Travel {
    pub fn validate(
        &self,
        people: &[Person],
        regions: &HashSet<u64>,
        working: &[u64],
    ) -> Result<(), String> {
        let mut routes: Vec<_> = self.routes.par_iter().collect();
        routes.par_sort_unstable_by_key(|r| (r.source, r.destination));
        let mut journeys: Vec<_> = self.journeys.par_iter().map(|j| j.person).collect();
        journeys.par_sort_unstable();
        if routes
            .par_windows(2)
            .any(|r| (r[0].source, r[0].destination) == (r[1].source, r[1].destination))
            || journeys.par_windows(2).any(|r| r[0] == r[1])
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
    pub fn depart(
        &self,
        people: &[Person],
        requests: &[Departure],
        working: &[u64],
    ) -> Result<(Self, Vec<Person>), String> {
        let mut requests: Vec<_> = requests.par_iter().copied().collect();
        requests.par_sort_unstable_by_key(|r| r.person);
        if requests.par_windows(2).any(|r| r[0].person == r[1].person) {
            return Err("duplicate departure".into());
        }
        let mut routes: Vec<_> = self.routes.par_iter().collect();
        routes.par_sort_unstable_by_key(|r| (r.source, r.destination));
        let journeys: Vec<_> = requests
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
            .map_err(str::to_owned)?;
        let mut next_people: Vec<_> = people.par_iter().cloned().collect();
        next_people.par_iter_mut().for_each(|person| {
            if requests
                .binary_search_by_key(&person.id, |r| r.person)
                .is_ok()
            {
                person.region = None;
            }
        });
        let mut next = self.clone();
        next.journeys.par_extend(journeys);
        next.journeys.par_sort_unstable_by_key(|j| j.person);
        Ok((next, next_people))
    }
    /// Arrival occurs after this interval's personal and physical decisions.
    pub fn advance(&self, people: &mut [Person]) -> Self {
        let mut journeys: Vec<_> = self
            .journeys
            .par_iter()
            .map(|j| {
                let mut j = j.clone();
                j.remaining -= 1;
                j
            })
            .collect();
        journeys.par_sort_unstable_by_key(|j| j.person);
        people.par_iter_mut().for_each(|person| {
            if let Ok(index) = journeys.binary_search_by_key(&person.id, |j| j.person) {
                if person.health == 0 {
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
                        .is_some_and(|i| people[i].health > 0)
            })
            .collect();
        Self {
            routes: self.routes.clone(),
            journeys,
        }
    }
    /// Stop travelers who died in a later lifecycle phase of this boundary.
    pub fn settle_deaths(&mut self, people: &mut [Person]) {
        if self.journeys.is_empty()
            || !people
                .par_iter()
                .any(|p| p.health == 0 && p.region.is_none())
        {
            return;
        }
        people
            .par_iter_mut()
            .filter(|p| p.health == 0 && p.region.is_none())
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
                    .is_some_and(|i| people[i].health > 0)
            })
            .collect();
    }
}
