//! Data-defined medical treatment and physically reserved supplies.
use game_parallel_production_model::{Inventory, PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Care {
    pub medicine: u32,
    pub dose: u64,
    pub duration: u32,
    pub healing: u32,
    pub next_id: u64,
    #[serde(default)]
    pub jobs: Vec<Job>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: u64,
    pub caregiver: u64,
    pub patient: u64,
    pub region: u64,
    pub remaining: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct Start {
    pub id: u64,
    pub caregiver: u64,
    pub patient: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub id: u64,
    pub caregiver: u64,
    pub patient: u64,
    pub healed: u32,
    pub interrupted: bool,
}
impl Care {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        goods: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), String> {
        if self.dose == 0
            || self.duration == 0
            || self.healing == 0
            || self.next_id == 0
            || !goods.contains(&self.medicine)
        {
            return Err("invalid care rules".into());
        }
        let mut ids = HashSet::new();
        let mut actors = HashSet::new();
        if self.jobs.iter().any(|job| {
            job.id >= self.next_id
                || !ids.insert(job.id)
                || !actors.insert(job.caregiver)
                || !people.contains(&job.caregiver)
                || !people.contains(&job.patient)
                || !regions.contains(&job.region)
                || job.remaining == 0
                || job.remaining > self.duration
        }) || self.events.iter().any(|event| {
            event.tick > tick
                || event.id >= self.next_id
                || !people.contains(&event.caregiver)
                || !people.contains(&event.patient)
        }) {
            return Err("invalid care state".into());
        }
        Ok(())
    }
    pub fn actors(&self) -> Vec<u64> {
        let mut actors: Vec<_> = self.jobs.par_iter().map(|job| job.caregiver).collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        actors
    }
    /// Reserve all medicine and caregiver slots or leave the entire batch unchanged.
    pub fn start(
        &mut self,
        inventories: &mut [Inventory],
        people: &[PersonSnapshot],
        busy: &[u64],
        requests: &[Start],
    ) -> Result<(), String> {
        if requests.is_empty() {
            return Ok(());
        }
        let mut ids: Vec<_> = requests.par_iter().map(|request| request.id).collect();
        ids.par_sort_unstable();
        let mut actors = self.actors();
        actors.extend_from_slice(busy);
        actors.par_sort_unstable();
        let mut caregivers: Vec<_> = requests
            .par_iter()
            .map(|request| request.caregiver)
            .collect();
        caregivers.par_sort_unstable();
        if ids.par_windows(2).any(|p| p[0] == p[1])
            || caregivers.par_windows(2).any(|p| p[0] == p[1])
            || ids[0] < self.next_id
        {
            return Err("duplicate or stale care identity".into());
        }
        let next = ids
            .last()
            .unwrap()
            .checked_add(1)
            .ok_or("care identity overflow")?;
        let jobs: Vec<_> = requests
            .par_iter()
            .map(|request| -> Result<Job, String> {
                if actors.binary_search(&request.caregiver).is_ok() {
                    return Err("caregiver is busy".into());
                }
                let caregiver =
                    people[person_index(people, request.caregiver).ok_or("unknown caregiver")?];
                let patient =
                    people[person_index(people, request.patient).ok_or("unknown patient")?];
                let region = caregiver.1.ok_or("caregiver must be local")?;
                if caregiver.2 == 0 || patient.2 == 0 || patient.1 != Some(region) {
                    return Err("caregiver and patient must be living and local".into());
                }
                Ok(Job {
                    id: request.id,
                    caregiver: request.caregiver,
                    patient: request.patient,
                    region,
                    remaining: self.duration,
                })
            })
            .collect::<Result<_, _>>()?;
        let needs: HashMap<_, u128> = jobs
            .par_iter()
            .fold(HashMap::new, |mut map, job| {
                *map.entry(job.region).or_default() += u128::from(self.dose);
                map
            })
            .reduce(HashMap::new, |mut a, b| {
                for (region, amount) in b {
                    *a.entry(region).or_default() += amount;
                }
                a
            });
        let inventories_by_region: HashMap<_, _> = inventories
            .iter()
            .map(|inventory| (inventory.region, inventory))
            .collect();
        for (&region, &amount) in &needs {
            let stock = inventories_by_region
                .get(&region)
                .and_then(|inventory| {
                    inventory
                        .stocks
                        .iter()
                        .find(|stock| stock.good == self.medicine)
                })
                .ok_or("missing medicine stock")?;
            if amount > u128::from(stock.quantity) {
                return Err("insufficient medicine".into());
            }
        }
        inventories.par_iter_mut().for_each(|inventory| {
            if let Some(amount) = needs.get(&inventory.region) {
                inventory
                    .stocks
                    .iter_mut()
                    .find(|stock| stock.good == self.medicine)
                    .unwrap()
                    .quantity -= *amount as u64;
            }
        });
        self.jobs.extend(jobs);
        self.jobs.par_sort_unstable_by_key(|job| job.id);
        self.next_id = next;
        Ok(())
    }
}

/// Prepare time advancement using committed physical state.
pub fn prepare(care: &Care, people: &[PersonSnapshot], tick: u64) -> (Care, Vec<Job>) {
    let rows: Vec<_> = care
        .jobs
        .par_iter()
        .map(|old| {
            let local = |id| {
                person_index(people, id)
                    .is_some_and(|index| people[index].1 == Some(old.region) && people[index].2 > 0)
            };
            if !local(old.caregiver) || !local(old.patient) {
                return (
                    None,
                    None,
                    Some(Event {
                        tick,
                        id: old.id,
                        caregiver: old.caregiver,
                        patient: old.patient,
                        healed: 0,
                        interrupted: true,
                    }),
                );
            }
            let mut next = old.clone();
            next.remaining -= 1;
            if next.remaining == 0 {
                (None, Some(next), None)
            } else {
                (Some(next), None, None)
            }
        })
        .collect();
    let jobs = rows.par_iter().filter_map(|row| row.0.clone()).collect();
    let due = rows.par_iter().filter_map(|row| row.1.clone()).collect();
    let events = rows.par_iter().filter_map(|row| row.2.clone()).collect();
    (
        Care {
            medicine: care.medicine,
            dose: care.dose,
            duration: care.duration,
            healing: care.healing,
            next_id: care.next_id,
            jobs,
            events,
        },
        due,
    )
}
