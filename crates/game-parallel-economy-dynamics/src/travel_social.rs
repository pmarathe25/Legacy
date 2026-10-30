//! Temporary dense location keys keep route groups separate from stationary people.
use game_parallel_economy_person::{Person, travel::Travel};
use game_parallel_social::Encounter;
use rayon::prelude::*;

pub struct Locality {
    regions: Vec<u64>,
    routes: Vec<(u64, u64, u64)>,
    people: Vec<(u64, usize)>,
}
impl Locality {
    pub fn new(people: &[Person], travel: Option<&Travel>) -> Option<Self> {
        let journeys = &travel?.journeys;
        if journeys.is_empty() {
            return None;
        }
        let mut regions: Vec<_> = people
            .par_iter()
            .filter_map(|p| p.region)
            .chain(journeys.par_iter().map(|j| j.source))
            .collect();
        regions.par_sort_unstable();
        regions = regions.par_chunk_by(|a, b| a == b).map(|g| g[0]).collect();
        let mut routes: Vec<_> = journeys
            .par_iter()
            .map(|j| (j.source, j.destination, j.remaining))
            .collect();
        routes.par_sort_unstable();
        routes = routes.par_chunk_by(|a, b| a == b).map(|g| g[0]).collect();
        let mut locations: Vec<_> = journeys
            .par_iter()
            .map(|j| {
                let key = (j.source, j.destination, j.remaining);
                (
                    j.person,
                    regions.len() + routes.binary_search(&key).expect("known route"),
                )
            })
            .collect();
        locations.par_sort_unstable_by_key(|row| row.0);
        Some(Self {
            regions,
            routes,
            people: locations,
        })
    }
    pub fn key(&self, person: &Person) -> Option<u64> {
        let slot = if let Some(region) = person.region {
            self.regions.binary_search(&region).ok()?
        } else {
            self.people
                .binary_search_by_key(&person.id, |row| row.0)
                .ok()
                .map(|i| self.people[i].1)?
        };
        u64::try_from(slot).ok()?.checked_add(1)
    }
    pub fn restore(&self, event: &mut Encounter) {
        if let Some(slot) = event
            .region
            .checked_sub(1)
            .and_then(|v| usize::try_from(v).ok())
        {
            event.region = if slot < self.regions.len() {
                self.regions[slot]
            } else {
                self.routes[slot - self.regions.len()].0
            };
        }
    }
}
