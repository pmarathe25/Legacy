//! Temporary dense location keys keep route groups separate from stationary people.
use game_parallel_economy_person::{Person, travel::Travel};
use game_parallel_social::Encounter;
use rayon::prelude::*;

pub struct Locality {
    regions: Vec<u64>,
    routes: Vec<(u64, u64, u64)>,
    cells: Vec<(u64, u64, u64)>,
    people: Vec<(u64, usize)>,
}
impl Locality {
    pub fn new(
        people: &[Person],
        travel: Option<&Travel>,
        partition: Option<&(dyn Fn(u64) -> (u64, u64) + Sync)>,
    ) -> Option<Self> {
        let journeys = travel.map_or(&[][..], |travel| travel.journeys.as_slice());
        if journeys.is_empty() && partition.is_none() {
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
        let cell_people: Vec<_> = partition.map_or_else(Vec::new, |partition| {
            people
                .par_iter()
                .filter_map(|person| {
                    let (before, after) = partition(person.id);
                    (before != 0 || after != 0)
                        .then_some((person.id, (person.region?, before, after)))
                })
                .collect()
        });
        let mut cells: Vec<_> = cell_people.par_iter().map(|row| row.1).collect();
        if cells.par_windows(2).any(|pair| pair[0] > pair[1]) {
            cells.par_sort_unstable();
        }
        cells = cells.par_chunk_by(|a, b| a == b).map(|g| g[0]).collect();
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
        if !cell_people.is_empty() {
            locations.par_extend(cell_people.par_iter().filter_map(|&(person, key)| {
                let slot = regions.len() + routes.len() + cells.binary_search(&key).ok()?;
                Some((person, slot))
            }));
        }
        if locations.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            locations.par_sort_unstable_by_key(|row| row.0);
        }
        Some(Self {
            regions,
            routes,
            cells,
            people: locations,
        })
    }
    pub fn key(&self, person: &Person) -> Option<u64> {
        let slot = if let Ok(index) = self.people.binary_search_by_key(&person.id, |row| row.0) {
            self.people[index].1
        } else if let Some(region) = person.region {
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
            } else if slot < self.regions.len() + self.routes.len() {
                self.routes[slot - self.regions.len()].0
            } else {
                self.cells[slot - self.regions.len() - self.routes.len()].0
            };
        }
    }
}
