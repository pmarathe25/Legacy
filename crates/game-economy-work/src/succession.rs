//! Individual decisions to take over physical work left by another person.
use crate::{sites::Facilities, tools};
use game_economy_claims::Index as Claims;
use game_economy_combat::injury::Injuries;
use game_economy_person::Person;
use game_families::Families;
use game_lot_runtime::{Adoption, Phase, Work};
use game_lots::View;
use game_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A personal choice, reconsidered while the identified job is vacant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub job: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub fear_at_least: Option<u64>,
    #[serde(default)]
    pub obligation_at_least: Option<u64>,
    #[serde(default)]
    pub disregard_claims: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Succession {
    /// Boundaries for which a departing lead's reserved inputs remain at the site.
    pub retention: u64,
    #[serde(default)]
    pub preferences: Vec<Preference>,
    /// The original relationship relevant to a currently adopted job.
    #[serde(default)]
    pub engagements: Vec<Engagement>,
}

impl Clone for Succession {
    fn clone(&self) -> Self {
        Self {
            retention: self.retention,
            preferences: self.preferences.par_iter().cloned().collect(),
            engagements: self.engagements.par_iter().cloned().collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engagement {
    pub job: u64,
    pub person: u64,
    pub former_worker: u64,
}

impl Succession {
    pub fn validate(&self, people: &HashSet<u64>) -> Result<(), String> {
        let mut actors: Vec<_> = self.preferences.par_iter().map(|p| p.person).collect();
        actors.par_sort_unstable();
        if self.retention == 0
            || actors.par_windows(2).any(|p| p[0] == p[1])
            || self
                .preferences
                .par_iter()
                .any(|p| p.job == 0 || !people.contains(&p.person))
            || self.engagements.par_iter().any(|e| {
                e.job == 0
                    || e.person == e.former_worker
                    || !people.contains(&e.person)
                    || !people.contains(&e.former_worker)
            })
            || self
                .engagements
                .par_windows(2)
                .any(|p| p[0].job >= p[1].job)
        {
            return Err("invalid work succession state".into());
        }
        Ok(())
    }

    pub fn validate_links(&self, work: &[Work]) -> Result<(), String> {
        if self.engagements.par_iter().any(|e| {
            work.binary_search_by_key(&e.job, |w| w.job.id)
                .ok()
                .is_none_or(|i| work[i].phase != Phase::Working || work[i].job.worker != e.person)
        }) {
            return Err("invalid work succession engagement".into());
        }
        Ok(())
    }

    /// Reconsider each successor's own preference against their original relationship.
    pub fn continuing(&self, opinions: Option<&Opinions>) -> Vec<u64> {
        if self.engagements.is_empty() {
            return Vec::new();
        }
        let mut preferences: Vec<_> = self.preferences.par_iter().collect();
        if preferences
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            preferences.par_sort_unstable_by_key(|p| p.person);
        }
        let feelings = opinions
            .filter(|_| {
                self.preferences.par_iter().any(|p| {
                    !p.willing && (p.fear_at_least.is_some() || p.obligation_at_least.is_some())
                })
            })
            .map(Opinions::index);
        let mut people: Vec<_> = self
            .engagements
            .par_iter()
            .filter_map(|e| {
                let p = preferences
                    .binary_search_by_key(&e.person, |p| p.person)
                    .ok()
                    .map(|i| preferences[i])?;
                if p.job != e.job {
                    return None;
                }
                let (fear, obligation) = feelings
                    .as_ref()
                    .map_or((0, 0), |i| i.feelings(e.person, e.former_worker));
                (p.willing
                    || p.fear_at_least.is_some_and(|v| fear >= v)
                    || p.obligation_at_least.is_some_and(|v| obligation >= v))
                .then_some(e.person)
            })
            .collect();
        people.par_sort_unstable();
        people
    }

    pub fn publish(&mut self, events: &game_lot_runtime::Events, work: &[Work]) {
        self.engagements = std::mem::take(&mut self.engagements)
            .into_par_iter()
            .filter(|e| {
                work.binary_search_by_key(&e.job, |w| w.job.id)
                    .ok()
                    .is_some_and(|i| {
                        work[i].phase == Phase::Working && work[i].job.worker == e.person
                    })
            })
            .collect();
        self.engagements
            .par_extend(events.adopted.par_iter().filter_map(|a| {
                work.binary_search_by_key(&a.job, |w| w.job.id)
                    .ok()
                    .filter(|&i| work[i].phase == Phase::Working && work[i].job.worker == a.worker)
                    .map(|_| Engagement {
                        job: a.job,
                        person: a.worker,
                        former_worker: a.previous_worker,
                    })
            }));
        self.engagements.par_sort_unstable_by_key(|e| e.job);
    }
}

pub struct Context<'a> {
    pub owner: &'a Succession,
    pub people: &'a [Person],
    pub work: &'a [Work],
    pub busy: &'a [u64],
    pub resting: &'a [u64],
    pub sites: &'a Facilities,
    pub unavailable_sites: &'a [u64],
    pub site_classes: &'a [(u64, Option<u64>)],
    pub claims: Option<&'a Claims<'a>>,
    pub injuries: Option<&'a Injuries>,
    pub family: Option<&'a Families>,
    pub tools: &'a [tools::Requirement],
    pub tool_wear: Option<&'a crate::tool_wear::ToolWear>,
    pub physical: &'a game_lot_runtime::Runtime,
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub max_hunger: u32,
    pub seed: u64,
    pub tick: u64,
}

fn priority(seed: u64, tick: u64, job: u64, person: u64) -> u64 {
    let mut x = seed ^ tick.rotate_left(17) ^ job.rotate_left(31) ^ person;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

impl Context<'_> {
    pub fn prepare(&self) -> Vec<Adoption> {
        if self.owner.preferences.is_empty()
            || !self
                .work
                .par_iter()
                .any(|row| matches!(row.phase, Phase::Suspended { .. }))
        {
            return Vec::new();
        }
        let underage = self.family.map_or_else(Vec::new, crate::underage);
        let feelings = self
            .opinions
            .filter(|_| {
                self.owner.preferences.par_iter().any(|p| {
                    !p.willing && (p.fear_at_least.is_some() || p.obligation_at_least.is_some())
                })
            })
            .map(Opinions::index);
        // Reduce locally first, so a highly contested vacancy does not need a
        // global sort or a serial scan through all of its candidates.
        let reduced: Vec<Vec<(u64, u64, u64, u64)>> = self
            .owner
            .preferences
            .par_chunks(1024)
            .map(|chunk| {
                let mut best = HashMap::new();
                for p in chunk {
                    let candidate = (|| {
                        if self.busy.binary_search(&p.person).is_ok()
                            || self.resting.binary_search(&p.person).is_ok()
                            || underage.binary_search(&p.person).is_ok()
                        {
                            return None;
                        }
                        let person = &self.people
                            [self.people.binary_search_by_key(&p.person, |p| p.id).ok()?];
                        if person.controlled
                            || person.health == 0
                            || person.hunger > self.max_hunger
                        {
                            return None;
                        }
                        let row = &self.work
                            [self.work.binary_search_by_key(&p.job, |w| w.job.id).ok()?];
                        if !matches!(row.phase, Phase::Suspended { until } if until > self.tick)
                            || row.job.worker == p.person
                            || person.region != Some(row.job.region)
                        {
                            return None;
                        }
                        let binding = &self.sites.bindings[self
                            .sites
                            .bindings
                            .binary_search_by_key(&p.job, |b| b.job)
                            .ok()?];
                        if self.unavailable_sites.binary_search(&binding.site).is_ok() {
                            return None;
                        }
                        let class = self
                            .site_classes
                            .binary_search_by_key(&binding.site, |r| r.0)
                            .ok()
                            .map(|i| self.site_classes[i].1);
                        if class.is_some_and(|owner| owner != Some(person.id))
                            && !p.disregard_claims
                            && self
                                .claims
                                .is_none_or(|claims| !claims.disregards(person.id))
                        {
                            return None;
                        }
                        if self.tool_wear.map_or_else(
                            || !tools::available(self.tools, self.view, person.id, row.job.recipe),
                            |wear| {
                                wear.select(
                                    self.tools,
                                    self.physical,
                                    self.view,
                                    person.id,
                                    row.job.recipe,
                                )
                                .is_none()
                            },
                        ) {
                            return None;
                        }
                        if self.injuries.is_some_and(|injuries| {
                            injuries.rules.work_percent == 0
                                && injuries
                                    .cases
                                    .binary_search_by_key(&person.id, |c| c.person)
                                    .ok()
                                    .is_some_and(|i| {
                                        injuries.cases[i].heals_at > self.tick.saturating_sub(1)
                                    })
                        }) {
                            return None;
                        }
                        let (fear, obligation) = feelings
                            .as_ref()
                            .map_or((0, 0), |i| i.feelings(person.id, row.job.worker));
                        if !p.willing
                            && !p.fear_at_least.is_some_and(|v| fear >= v)
                            && !p.obligation_at_least.is_some_and(|v| obligation >= v)
                        {
                            return None;
                        }
                        Some((
                            p.job,
                            priority(self.seed, self.tick, p.job, p.person),
                            p.person,
                            row.job.worker,
                        ))
                    })();
                    if let Some(candidate) = candidate {
                        best.entry(candidate.0)
                            .and_modify(|old: &mut (u64, u64, u64, u64)| {
                                if candidate < *old {
                                    *old = candidate;
                                }
                            })
                            .or_insert(candidate);
                    }
                }
                best.into_values().collect()
            })
            .collect();
        let mut candidates: Vec<_> = reduced.into_par_iter().flatten().collect();
        candidates.par_sort_unstable();
        candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| Adoption {
                job: group[0].0,
                previous_worker: group[0].3,
                worker: group[0].2,
            })
            .collect()
    }
}
