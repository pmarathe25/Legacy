//! Fixed physical work sites and provisional, capacity-bounded job occupancy.
use game_parallel_lot_runtime::{Events, Phase, Work};
use game_parallel_production_model::Start;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub id: u64,
    pub region: u64,
    pub kind: String,
    pub capacity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub recipe: u32,
    pub kind: String,
    pub slots: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub job: u64,
    pub site: u64,
    pub slots: u64,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facilities {
    /// Next physical site identity; zero is initialized for older authored data.
    #[serde(default)]
    pub next_id: u64,
    pub sites: Vec<Site>,
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}
impl Clone for Facilities {
    fn clone(&self) -> Self {
        Self {
            next_id: self.next_id,
            sites: self.sites.par_iter().cloned().collect(),
            requirements: self.requirements.par_iter().cloned().collect(),
            bindings: self.bindings.par_iter().cloned().collect(),
        }
    }
}
impl Facilities {
    pub fn normalize(&mut self) -> Result<(), String> {
        self.sites.par_sort_unstable_by_key(|s| s.id);
        if self.next_id == 0 {
            self.next_id = self
                .sites
                .last()
                .map_or(Some(1), |s| s.id.checked_add(1))
                .ok_or("work site identity overflow")?;
        }
        self.requirements.par_sort_unstable_by_key(|r| r.recipe);
        self.bindings.par_sort_unstable_by_key(|b| b.job);
        Ok(())
    }
    pub fn validate(
        &self,
        work: &[Work],
        recipes: &HashSet<u32>,
        regions: &HashSet<u64>,
    ) -> Result<(), String> {
        if self.next_id == 0
            || self.sites.last().is_some_and(|s| s.id >= self.next_id)
            || self.sites.par_windows(2).any(|w| w[0].id >= w[1].id)
            || self.sites.par_iter().any(|s| {
                s.id == 0 || s.kind.is_empty() || s.capacity == 0 || !regions.contains(&s.region)
            })
            || self
                .requirements
                .par_windows(2)
                .any(|w| w[0].recipe >= w[1].recipe)
            || self
                .requirements
                .par_iter()
                .any(|r| r.kind.is_empty() || r.slots == 0 || !recipes.contains(&r.recipe))
            || self.bindings.par_windows(2).any(|w| w[0].job >= w[1].job)
        {
            return Err("invalid work sites".into());
        }
        if self.bindings.par_iter().any(|b| {
            let job = work
                .binary_search_by_key(&b.job, |w| w.job.id)
                .ok()
                .map(|i| &work[i]);
            let site = self
                .sites
                .binary_search_by_key(&b.site, |s| s.id)
                .ok()
                .map(|i| &self.sites[i]);
            match (job, site) {
                (Some(job), Some(site)) if job.phase == Phase::Working => self
                    .requirements
                    .binary_search_by_key(&job.job.recipe, |r| r.recipe)
                    .ok()
                    .is_none_or(|i| {
                        let r = &self.requirements[i];
                        job.job.region != site.region || r.kind != site.kind || r.slots != b.slots
                    }),
                _ => true,
            }
        }) || work
            .par_iter()
            .filter(|w| w.phase == Phase::Working)
            .any(|w| {
                self.requirements
                    .binary_search_by_key(&w.job.recipe, |r| r.recipe)
                    .is_ok()
                    != self
                        .bindings
                        .binary_search_by_key(&w.job.id, |b| b.job)
                        .is_ok()
            })
        {
            return Err("invalid work site binding".into());
        }
        let mut occupancy: Vec<_> = self
            .bindings
            .par_iter()
            .map(|b| (b.site, b.slots))
            .collect();
        occupancy.par_sort_unstable_by_key(|r| r.0);
        if occupancy.par_chunk_by(|a, b| a.0 == b.0).any(|group| {
            let total = group
                .par_iter()
                .map(|r| u128::from(r.1))
                .reduce(|| 0, |a, b| a + b);
            self.sites
                .binary_search_by_key(&group[0].0, |s| s.id)
                .ok()
                .is_none_or(|i| total > u128::from(self.sites[i].capacity))
        }) {
            return Err("work site over capacity".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Candidate<'a> {
    region: u64,
    kind: &'a str,
    job: u64,
    worker: u64,
    slots: u64,
    priority: u64,
}
struct SiteGroup<'a> {
    key: (u64, &'a str),
    sites: Vec<(u64, u64)>,
}
pub struct Plan {
    bindings: Vec<Binding>,
}
impl Plan {
    pub fn permits(&self, job: u64) -> bool {
        self.bindings.binary_search_by_key(&job, |b| b.job).is_ok()
    }
}
impl Facilities {
    /// Choose provisional slots before inputs move. Bind only physically started jobs.
    pub fn prepare(&self, starts: &[Start], seed: u64, tick: u64) -> Plan {
        if starts.is_empty() || self.requirements.is_empty() {
            return Plan {
                bindings: Vec::new(),
            };
        }
        let mut used: Vec<_> = self
            .bindings
            .par_iter()
            .map(|b| (b.site, b.slots))
            .collect();
        used.par_sort_unstable_by_key(|r| r.0);
        let occupancy: Vec<_> = used
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|g| (g[0].0, g.par_iter().map(|r| r.1).sum::<u64>()))
            .collect();
        let mut sites: Vec<_> = self
            .sites
            .par_iter()
            .map(|s| {
                let used = occupancy
                    .binary_search_by_key(&s.id, |r| r.0)
                    .ok()
                    .map_or(0, |i| occupancy[i].1);
                (s.region, s.kind.as_str(), s.id, s.capacity - used)
            })
            .filter(|s| s.3 > 0)
            .collect();
        sites.par_sort_unstable();
        let groups: Vec<_> = sites
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|g| SiteGroup {
                key: (g[0].0, g[0].1),
                sites: g.par_iter().map(|row| (row.2, row.3)).collect(),
            })
            .collect();
        let mut candidates: Vec<_> = starts
            .par_iter()
            .filter_map(|s| {
                let i = self
                    .requirements
                    .binary_search_by_key(&s.recipe, |r| r.recipe)
                    .ok()?;
                let r = &self.requirements[i];
                Some(Candidate {
                    region: s.region,
                    kind: r.kind.as_str(),
                    job: s.id,
                    worker: s.worker,
                    slots: r.slots,
                    priority: game_parallel_genetics::random(seed ^ s.worker, s.worker, tick),
                })
            })
            .collect();
        candidates.par_sort_unstable_by_key(|c| (c.region, c.kind, c.priority, c.worker));
        let bindings: Vec<_> = candidates
            .par_chunk_by(|a, b| (a.region, a.kind) == (b.region, b.kind))
            .flat_map_iter(|g| {
                let Some(group) = groups
                    .binary_search_by_key(&(g[0].region, g[0].kind), |r| r.key)
                    .ok()
                    .map(|i| &groups[i])
                else {
                    return Vec::new();
                };
                let min = g.par_iter().map(|c| c.slots).reduce(|| u64::MAX, u64::min);
                let uniform = g.par_iter().all(|c| c.slots == min);
                let usable: Vec<_> = group
                    .sites
                    .par_iter()
                    .filter_map(|&(id, free)| {
                        let capacity = if uniform {
                            free / min
                        } else if free >= min {
                            free
                        } else {
                            0
                        };
                        (capacity > 0).then_some((id, capacity))
                    })
                    .collect();
                let capacities: Vec<_> = usable.par_iter().map(|r| r.1).collect();
                let offsets = game_parallel_lot_allocation::prefixes(&capacities);
                let spans: Vec<_> = usable
                    .par_iter()
                    .zip(offsets.par_iter())
                    .map(|(&(site, capacity), &begin)| (begin, begin + u128::from(capacity), site))
                    .collect();
                let requests: Vec<_> = if uniform {
                    (0..g.len()).into_par_iter().map(|i| i as u128).collect()
                } else {
                    let amounts: Vec<_> = g.par_iter().map(|c| c.slots).collect();
                    game_parallel_lot_allocation::prefixes(&amounts)
                };
                g.par_iter()
                    .zip(requests.par_iter())
                    .filter_map(|(c, &offset)| {
                        let i = spans.partition_point(|s| s.1 <= offset);
                        let &(begin, end, site) = spans.get(i)?;
                        let amount = if uniform { 1 } else { u128::from(c.slots) };
                        (offset >= begin && offset + amount <= end).then_some(Binding {
                            job: c.job,
                            site,
                            slots: c.slots,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut bindings = bindings;
        bindings.par_sort_unstable_by_key(|b| b.job);
        Plan { bindings }
    }
    pub fn filter_starts(&self, starts: &[Start], plan: &Plan) -> Vec<Start> {
        starts
            .par_iter()
            .filter(|s| {
                self.requirements
                    .binary_search_by_key(&s.recipe, |r| r.recipe)
                    .is_err()
                    || plan.permits(s.id)
            })
            .copied()
            .collect()
    }
    /// Release completed or interrupted work and retain only actual physical starts.
    pub fn advance(&self, plan: &Plan, events: &Events) -> Self {
        let ended = |id| {
            events.finished.binary_search(&id).is_ok()
                || events.abandoned.binary_search(&id).is_ok()
                || events.interrupted.binary_search(&id).is_ok()
        };
        let mut bindings: Vec<_> = self
            .bindings
            .par_iter()
            .filter(|b| !ended(b.job))
            .cloned()
            .chain(
                plan.bindings
                    .par_iter()
                    .filter(|b| events.started.binary_search(&b.job).is_ok() && !ended(b.job))
                    .cloned(),
            )
            .collect();
        bindings.par_sort_unstable_by_key(|b| b.job);
        Self {
            next_id: self.next_id,
            sites: self.sites.par_iter().cloned().collect(),
            requirements: self.requirements.par_iter().cloned().collect(),
            bindings,
        }
    }
}
