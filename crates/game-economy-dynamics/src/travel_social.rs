//! Temporary dense location keys keep route groups separate from stationary people.
use game_economy_person::{Person, travel::Travel};
use game_social::Encounter;
use rayon::prelude::*;

fn sorted_unique<T: Copy + Ord + Send + Sync>(mut rows: Vec<T>) -> Vec<T> {
    let Some(&first) = rows.first() else {
        return rows;
    };
    if rows.par_iter().all(|row| *row == first) {
        rows.truncate(1);
        return rows;
    }
    if rows.par_windows(2).any(|pair| pair[0] > pair[1]) {
        rows.par_sort_unstable();
    }
    rows.par_chunk_by(|a, b| a == b)
        .map(|group| group[0])
        .collect()
}

pub struct Locality {
    regions: Vec<u64>,
    routes: Vec<(u64, u64, u64, u64)>,
    cells: Vec<(u64, u64, u64, u8, u64)>,
    people: Vec<(u64, usize)>,
}
impl Locality {
    pub fn new(
        people: &[Person],
        travel: Option<&Travel>,
        partition: Option<&(dyn Fn(u64) -> (u64, u64) + Sync)>,
        local: Option<&(dyn Fn(u64) -> (u8, u64) + Sync)>,
    ) -> Option<Self> {
        let journeys = travel.map_or(&[][..], |travel| travel.journeys.as_slice());
        if journeys.is_empty() && partition.is_none() && local.is_none() {
            return None;
        }
        let regions = sorted_unique(
            people
                .par_iter()
                .filter_map(|p| p.region)
                .chain(journeys.par_iter().map(|j| j.source))
                .collect(),
        );
        let routes = sorted_unique(
            journeys
                .par_iter()
                .map(|j| (j.source, j.destination, j.remaining, j.progress_millis))
                .collect(),
        );
        let cell_people: Vec<_> = if partition.is_some() || local.is_some() {
            people
                .par_iter()
                .filter_map(|person| {
                    let (before, after) =
                        partition.map_or((0, 0), |partition| partition(person.id));
                    let (kind, endpoint) = local.map_or((0, 0), |local| local(person.id));
                    (before != 0 || after != 0 || kind != 0)
                        .then_some((person.id, (person.region?, before, after, kind, endpoint)))
                })
                .collect()
        } else {
            Vec::new()
        };
        let cells = if let Some(first) = cell_people.first().map(|row| row.1)
            && cell_people.par_iter().all(|row| row.1 == first)
        {
            vec![first]
        } else {
            sorted_unique(cell_people.par_iter().map(|row| row.1).collect())
        };
        let mut locations: Vec<_> = journeys
            .par_iter()
            .map(|j| {
                let key = (j.source, j.destination, j.remaining, j.progress_millis);
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
        Self::slot_key(slot)
    }
    /// Use direct paired access when every person already has an explicit slot.
    pub fn aligned<'a>(&'a self, people: &[Person]) -> Option<&'a [(u64, usize)]> {
        (self.people.len() == people.len()
            && self
                .people
                .par_iter()
                .zip(people.par_iter())
                .all(|(row, person)| row.0 == person.id))
        .then_some(self.people.as_slice())
    }
    pub fn slot_key(slot: usize) -> Option<u64> {
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
