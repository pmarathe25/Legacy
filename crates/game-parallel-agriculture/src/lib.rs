//! Region-owned cultivation: workers plant, tend and harvest physical food.
use game_parallel_people::Region;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Agriculture {
    pub rules: FarmRules,
    pub farms: Vec<Farm>,
    /// When present, only these people have volunteered their farm labor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volunteers: Option<Vec<u64>>,
    /// People who explicitly declined future automatic volunteering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refusals: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recruitment: Option<Recruitment>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Recruitment {
    pub interval: u64,
    pub one_in: u64,
    /// Zero disables spontaneous departure; otherwise this is a one-in-N choice per review.
    #[serde(default)]
    pub leave_one_in: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FarmRules {
    pub seed_food: u32,
    pub growing_work: u32,
    pub harvest_food: u32,
    pub min_health: u32,
    pub max_hunger: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Farm {
    pub region: u64,
    pub fields: Vec<Field>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: u64,
    pub phase: CropPhase,
    pub work: u32,
}
/// Which field operations are possible in the current season.
#[derive(Clone, Copy)]
pub struct FarmWindow {
    pub planting: bool,
    pub tending: bool,
    pub harvesting: bool,
}
impl Default for FarmWindow {
    fn default() -> Self {
        Self {
            planting: true,
            tending: true,
            harvesting: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CropPhase {
    Fallow,
    Growing,
    Ripe,
}

impl Agriculture {
    /// Initialize generated people's voluntary participation from their own seeded choices.
    pub fn generate_volunteers(&mut self, people: &[u64], willing: impl Fn(u64) -> bool + Sync) {
        self.refusals.clear();
        if let Some(volunteers) = &mut self.volunteers {
            *volunteers = people
                .par_iter()
                .copied()
                .filter(|&person| willing(person))
                .collect();
        }
    }
    pub fn volunteers_index(&self) -> Option<HashSet<u64>> {
        self.volunteers
            .as_ref()
            .map(|rows| rows.par_iter().copied().collect())
    }
    pub fn validate_volunteers(&self, people: &HashSet<u64>) -> Result<(), String> {
        let refusals: HashSet<_> = self.refusals.par_iter().copied().collect();
        if refusals.len() != self.refusals.len()
            || refusals.par_iter().any(|id| !people.contains(id))
        {
            return Err("invalid farm refusal".into());
        }
        if let Some(rows) = &self.volunteers {
            let ids: HashSet<_> = rows.par_iter().copied().collect();
            if ids.len() != rows.len()
                || ids
                    .par_iter()
                    .any(|id| !people.contains(id) || refusals.contains(id))
            {
                return Err("invalid farm volunteer".into());
            }
        }
        Ok(())
    }
    pub fn validate(&self, regions: &[Region]) -> Result<(), String> {
        if self.recruitment.as_ref().is_some_and(|rules| {
            rules.interval == 0 || rules.one_in == 0 || self.volunteers.is_none()
        }) {
            return Err("invalid farm recruitment rules".into());
        }
        if self.volunteers.is_none() && !self.refusals.is_empty() {
            return Err("farm refusals require volunteers".into());
        }
        if self.rules.seed_food == 0 || self.rules.growing_work == 0 || self.rules.harvest_food == 0
        {
            return Err("invalid farming rules".into());
        }
        let region_ids: HashSet<_> = regions.iter().map(|r| r.id).collect();
        let mut farms = HashSet::new();
        for farm in &self.farms {
            if !region_ids.contains(&farm.region) || !farms.insert(farm.region) {
                return Err("invalid farm region".into());
            }
        }
        if self.farms.par_iter().any(|farm| {
            farm.fields.par_iter().any(|field| match field.phase {
                CropPhase::Growing => field.work >= self.rules.growing_work,
                _ => field.work != 0,
            })
        }) {
            return Err("invalid field".into());
        }
        let mut ids: Vec<u64> = self
            .farms
            .par_iter()
            .flat_map(|farm| farm.fields.par_iter().map(|field| field.id))
            .collect();
        if ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            ids.par_sort_unstable();
        }
        if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("invalid field".into());
        }
        Ok(())
    }
}

/// Work, seeds and storage are reserved against the start-of-phase snapshot.
/// Field ID determines priority; crops harvested this tick cannot provide seed
/// or storage space to another field until the next tick.
pub fn advance(regions: &mut [Region], agriculture: &mut Agriculture, capacity: u32) {
    advance_excluding(regions, agriculture, capacity, &[]);
}

/// Advance using workers not assigned to other work.
pub fn advance_excluding(
    regions: &mut [Region],
    agriculture: &mut Agriculture,
    capacity: u32,
    busy: &[u64],
) {
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let volunteers = agriculture.volunteers_index();
    let snapshots: HashMap<_, _> = regions
        .par_iter()
        .map(|r| {
            let workers = r
                .people
                .par_iter()
                .filter(|p| {
                    volunteers.as_ref().is_none_or(|rows| rows.contains(&p.id))
                        && busy.binary_search(&p.id).is_err()
                        && p.health >= agriculture.rules.min_health
                        && p.health > 0
                        && p.hunger <= agriculture.rules.max_hunger
                })
                .count();
            (r.id, (r.food, workers))
        })
        .collect();
    apply_snapshots(
        regions,
        agriculture,
        capacity,
        &snapshots,
        FarmWindow::default(),
    );
}
/// Advance from a validated, shared person snapshot and regional food state.
pub fn advance_indexed(
    regions: &mut [Region],
    agriculture: &mut Agriculture,
    capacity: u32,
    people: &[(u64, Option<u64>, u32, u32)],
    busy: &[u64],
) {
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let volunteers = agriculture.volunteers_index();
    let workers: HashMap<u64, usize> = people
        .par_iter()
        .filter_map(|&(id, region, health, hunger)| {
            (volunteers.as_ref().is_none_or(|rows| rows.contains(&id))
                && health > 0
                && health >= agriculture.rules.min_health
                && hunger <= agriculture.rules.max_hunger
                && busy.binary_search(&id).is_err())
            .then_some(region)
            .flatten()
        })
        .fold(HashMap::new, |mut counts, region| {
            *counts.entry(region).or_default() += 1;
            counts
        })
        .reduce(HashMap::new, |mut a, b| {
            for (region, count) in b {
                *a.entry(region).or_default() += count;
            }
            a
        });
    let snapshots: HashMap<_, _> = regions
        .par_iter()
        .map(|region| {
            (
                region.id,
                (region.food, workers.get(&region.id).copied().unwrap_or(0)),
            )
        })
        .collect();
    apply_snapshots(
        regions,
        agriculture,
        capacity,
        &snapshots,
        FarmWindow::default(),
    );
}
/// Advance fields from pre-counted eligible workers per region.
/// Counts are supplied by the person-block owner from one committed generation.
pub fn advance_counts(
    regions: &mut [Region],
    agriculture: &mut Agriculture,
    capacity: u32,
    workers: &[usize],
) {
    advance_counts_window(
        regions,
        agriculture,
        capacity,
        workers,
        FarmWindow::default(),
    );
}
/// Advance using a seasonal operation window and committed worker counts.
pub fn advance_counts_window(
    regions: &mut [Region],
    agriculture: &mut Agriculture,
    capacity: u32,
    workers: &[usize],
    window: FarmWindow,
) {
    assert_eq!(regions.len(), workers.len());
    let snapshots: HashMap<_, _> = regions
        .par_iter()
        .zip(workers.par_iter())
        .map(|(region, &count)| (region.id, (region.food, count)))
        .collect();
    apply_snapshots(regions, agriculture, capacity, &snapshots, window);
}
fn apply_snapshots(
    regions: &mut [Region],
    agriculture: &mut Agriculture,
    capacity: u32,
    snapshots: &HashMap<u64, (u32, usize)>,
    window: FarmWindow,
) {
    let updates: HashMap<_, _> = agriculture
        .farms
        .par_iter_mut()
        .map(|farm| {
            let (food, workers) = snapshots[&farm.region];
            (
                farm.region,
                advance_farm(farm, &agriculture.rules, food, capacity, workers, window),
            )
        })
        .collect();
    regions.par_iter_mut().for_each(|region| {
        if let Some(&food) = updates.get(&region.id) {
            region.food = food;
        }
    });
}

const CHUNK: usize = 2048;

#[derive(Clone, Copy, Default)]
struct Counts {
    plant: usize,
    harvest: usize,
    work: usize,
}

fn advance_farm(
    farm: &mut Farm,
    rules: &FarmRules,
    food: u32,
    capacity: u32,
    workers: usize,
    window: FarmWindow,
) -> u32 {
    if farm
        .fields
        .par_windows(2)
        .any(|pair| pair[0].id > pair[1].id)
    {
        farm.fields.par_sort_unstable_by_key(|field| field.id);
    }
    let seed_limit = (food / rules.seed_food) as usize;
    let space_limit = (capacity.saturating_sub(food) / rules.harvest_food) as usize;
    // Each chunk reports resource demand. The prefix carries only fixed-size
    // counters; field state is disjoint and updated without locks.
    let counts: Vec<Counts> = farm
        .fields
        .par_chunks(CHUNK)
        .map(|chunk| {
            let mut c = Counts::default();
            for field in chunk {
                match field.phase {
                    CropPhase::Fallow => c.plant += 1,
                    CropPhase::Ripe => c.harvest += 1,
                    CropPhase::Growing => {}
                }
            }
            c
        })
        .collect();
    let mut prefixes = Vec::with_capacity(counts.len());
    let mut prefix = Counts::default();
    for c in counts {
        prefixes.push(prefix);
        prefix.plant += c.plant;
        prefix.harvest += c.harvest;
    }
    let grant_counts: Vec<usize> = farm
        .fields
        .par_chunks(CHUNK)
        .zip(prefixes.par_iter())
        .map(|(chunk, start)| {
            let mut c = *start;
            for field in chunk {
                let grant = match field.phase {
                    CropPhase::Fallow => {
                        let ok = window.planting && c.plant < seed_limit;
                        c.plant += 1;
                        ok
                    }
                    CropPhase::Ripe => {
                        let ok = window.harvesting && c.harvest < space_limit;
                        c.harvest += 1;
                        ok
                    }
                    CropPhase::Growing => window.tending,
                };
                c.work += usize::from(grant);
            }
            c.work
        })
        .collect();
    let mut work_prefix = 0;
    for (prefix, count) in prefixes.iter_mut().zip(grant_counts) {
        prefix.work = work_prefix;
        work_prefix += count;
    }
    let used: Counts = farm
        .fields
        .par_chunks_mut(CHUNK)
        .zip(prefixes.par_iter())
        .map(|(chunk, start)| {
            let mut c = *start;
            let mut used = Counts::default();
            for field in chunk {
                let grant = match field.phase {
                    CropPhase::Fallow => {
                        let ok = window.planting && c.plant < seed_limit;
                        c.plant += 1;
                        ok
                    }
                    CropPhase::Ripe => {
                        let ok = window.harvesting && c.harvest < space_limit;
                        c.harvest += 1;
                        ok
                    }
                    CropPhase::Growing => window.tending,
                };
                if !grant {
                    continue;
                }
                let assigned = c.work < workers;
                c.work += 1;
                if !assigned {
                    continue;
                }
                match field.phase {
                    CropPhase::Fallow => {
                        field.phase = CropPhase::Growing;
                        used.plant += 1;
                    }
                    CropPhase::Growing => {
                        field.work += 1;
                        if field.work == rules.growing_work {
                            field.work = 0;
                            field.phase = CropPhase::Ripe;
                        }
                    }
                    CropPhase::Ripe => {
                        field.phase = CropPhase::Fallow;
                        used.harvest += 1;
                    }
                }
            }
            used
        })
        .reduce(Counts::default, |a, b| Counts {
            plant: a.plant + b.plant,
            harvest: a.harvest + b.harvest,
            work: 0,
        });
    food - (used.plant as u32) * rules.seed_food + (used.harvest as u32) * rules.harvest_food
}
