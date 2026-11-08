//! Individual contributions to an existing, physically reserved job.
use crate::{sites::Facilities, tools};
use game_economy_claims::Index as Claims;
use game_economy_combat::injury::Injuries;
use game_economy_person::Person;
use game_families::Families;
use game_lot_runtime::{Phase, Work};
use game_lots::View;
use game_opinions::Opinions;
use game_skills::Skills;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Helpers fit within this many additional places in a job's reserved footprint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrewRule {
    pub recipe: u32,
    pub max_helpers: u64,
}

/// A person's own choice to assist a particular lead's work, reconsidered each boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub lead: u64,
    pub recipe: u32,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub fear_at_least: Option<u64>,
    #[serde(default)]
    pub obligation_at_least: Option<u64>,
    #[serde(default)]
    pub disregard_claims: bool,
}

/// A helper spent this boundary offering effort; progress is capped by remaining work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub lead: u64,
    pub job: u64,
    pub recipe: u32,
    pub offered: u64,
    pub progress: u64,
    pub finished: bool,
    pub interrupted: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedWork {
    pub rules: Vec<CrewRule>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
    #[serde(default)]
    pub events: Vec<Event>,
    /// Optional personal choices to take responsibility for abandoned work.
    #[serde(default)]
    pub succession: Option<crate::succession::Succession>,
}
impl Clone for SharedWork {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.par_iter().cloned().collect(),
            preferences: self.preferences.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
            succession: self.succession.clone(),
        }
    }
}
impl SharedWork {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        recipes: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), String> {
        let mut rules: Vec<_> = self.rules.par_iter().map(|r| r.recipe).collect();
        rules.par_sort_unstable();
        let mut actors: Vec<_> = self.preferences.par_iter().map(|p| p.person).collect();
        actors.par_sort_unstable();
        if rules.par_windows(2).any(|p| p[0] == p[1])
            || actors.par_windows(2).any(|p| p[0] == p[1])
            || self
                .rules
                .par_iter()
                .any(|r| r.max_helpers == 0 || !recipes.contains(&r.recipe))
            || self.preferences.par_iter().any(|p| {
                p.person == p.lead
                    || !people.contains(&p.person)
                    || !people.contains(&p.lead)
                    || rules.binary_search(&p.recipe).is_err()
            })
            || self.events.par_iter().any(|e| {
                e.tick > tick
                    || !people.contains(&e.person)
                    || !people.contains(&e.lead)
                    || !recipes.contains(&e.recipe)
                    || e.progress > e.offered
            })
        {
            return Err("invalid shared work state".into());
        }
        if let Some(succession) = &self.succession {
            succession.validate(people)?;
        }
        Ok(())
    }
}

pub struct Context<'a> {
    pub owner: &'a SharedWork,
    pub people: &'a [Person],
    pub work: &'a [Work],
    pub rates: &'a [(u64, u64)],
    pub busy: &'a [u64],
    pub resting: &'a [u64],
    pub manual: Option<&'a Preference>,
    pub sites: &'a Facilities,
    pub unavailable_sites: &'a [u64],
    pub site_classes: &'a [(u64, Option<u64>)],
    pub claims: Option<&'a Claims<'a>>,
    pub skills: Option<&'a Skills>,
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

#[derive(Clone)]
struct Candidate {
    job: u64,
    person: u64,
    lead: u64,
    recipe: u32,
    offered: u64,
    remaining: u64,
    base: u64,
    limit: u64,
    priority: u64,
}

fn priority(seed: u64, tick: u64, job: u64, person: u64) -> u64 {
    let mut x = seed ^ tick.rotate_left(17) ^ job.rotate_left(31) ^ person;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

impl Context<'_> {
    pub fn prepare(&self) -> Vec<Event> {
        if (self.owner.preferences.is_empty() && self.manual.is_none()) || self.rates.is_empty() {
            return Vec::new();
        }
        let mut rates = self.rates.to_vec();
        rates.par_sort_unstable_by_key(|r| r.0);
        let mut active: Vec<_> = self
            .work
            .par_iter()
            .filter(|work| {
                work.phase == Phase::Working
                    && rates.binary_search_by_key(&work.job.id, |r| r.0).is_ok()
            })
            .collect();
        active.par_sort_unstable_by_key(|work| work.job.worker);
        let mut rules: Vec<_> = self.owner.rules.par_iter().collect();
        rules.par_sort_unstable_by_key(|r| r.recipe);
        let underage = self.family.map_or_else(Vec::new, crate::underage);
        let skills = self.skills.map(Skills::index);
        let needs_feelings =
            self.owner.preferences.par_iter().any(|p| {
                !p.willing && (p.fear_at_least.is_some() || p.obligation_at_least.is_some())
            });
        let feelings = self
            .opinions
            .filter(|_| needs_feelings)
            .map(Opinions::index);
        let mut candidates: Vec<_> = self
            .owner
            .preferences
            .par_iter()
            .filter(|preference| {
                self.manual
                    .is_none_or(|manual| manual.person != preference.person)
            })
            .chain(self.manual.into_par_iter())
            .filter_map(|preference| {
                let manual = self
                    .manual
                    .is_some_and(|choice| choice.person == preference.person);
                if (!manual && self.busy.binary_search(&preference.person).is_ok())
                    || self.resting.binary_search(&preference.person).is_ok()
                    || underage.binary_search(&preference.person).is_ok()
                {
                    return None;
                }
                let person = self
                    .people
                    .binary_search_by_key(&preference.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                if (person.controlled && !manual)
                    || person.health == 0
                    || person.hunger > self.max_hunger
                {
                    return None;
                }
                let job = active
                    .binary_search_by_key(&preference.lead, |work| work.job.worker)
                    .ok()
                    .map(|i| active[i])?;
                if person.region != Some(job.job.region)
                    || job.job.recipe != preference.recipe
                    || job.job.remaining == 0
                {
                    return None;
                }
                let binding = self
                    .sites
                    .bindings
                    .binary_search_by_key(&job.job.id, |binding| binding.job)
                    .ok()
                    .map(|i| &self.sites.bindings[i])?;
                if self.unavailable_sites.binary_search(&binding.site).is_ok() {
                    return None;
                }
                let class = self
                    .site_classes
                    .binary_search_by_key(&binding.site, |row| row.0)
                    .ok()
                    .map(|i| self.site_classes[i].1);
                if class.is_some_and(|owner| owner != Some(person.id))
                    && !preference.disregard_claims
                    && self
                        .claims
                        .is_none_or(|claims| !claims.disregards(person.id))
                {
                    return None;
                }
                if self.tool_wear.map_or_else(
                    || !tools::available(self.tools, self.view, person.id, preference.recipe),
                    |wear| {
                        wear.select(
                            self.tools,
                            self.physical,
                            self.view,
                            person.id,
                            preference.recipe,
                        )
                        .is_none()
                    },
                ) {
                    return None;
                }
                let (fear, obligation) = feelings
                    .as_ref()
                    .map_or((0, 0), |index| index.feelings(person.id, preference.lead));
                if !preference.willing
                    && !preference.fear_at_least.is_some_and(|v| fear >= v)
                    && !preference
                        .obligation_at_least
                        .is_some_and(|v| obligation >= v)
                {
                    return None;
                }
                let limit = rules
                    .binary_search_by_key(&preference.recipe, |r| r.recipe)
                    .ok()
                    .map(|i| rules[i].max_helpers)?;
                let mut offered = skills
                    .as_ref()
                    .map_or(1, |index| index.rate(person.id, preference.recipe));
                if let Some(injuries) = self.injuries
                    && injuries
                        .cases
                        .binary_search_by_key(&person.id, |c| c.person)
                        .ok()
                        .is_some_and(|i| injuries.cases[i].heals_at > self.tick.saturating_sub(1))
                {
                    offered = ((u128::from(offered) * u128::from(injuries.rules.work_percent))
                        / 100) as u64;
                }
                if offered == 0 {
                    return None;
                }
                let base = rates[rates.binary_search_by_key(&job.job.id, |r| r.0).ok()?].1;
                Some(Candidate {
                    job: job.job.id,
                    person: person.id,
                    lead: preference.lead,
                    recipe: preference.recipe,
                    offered,
                    remaining: job.job.remaining,
                    base,
                    limit,
                    priority: priority(self.seed, self.tick, job.job.id, person.id),
                })
            })
            .collect();
        // For a heavily contested small crew, retain only each chunk's best
        // candidates before the deterministic global arbitration.
        if let Some(first) = candidates.first() {
            let limit = usize::try_from(first.limit).unwrap_or(usize::MAX);
            if limit <= 256
                && limit.saturating_mul(8) < candidates.len()
                && candidates
                    .par_iter()
                    .all(|candidate| candidate.job == first.job)
            {
                candidates = candidates
                    .par_chunks(1024)
                    .map(|chunk| {
                        let mut local = chunk.to_vec();
                        if local.len() > limit {
                            local.select_nth_unstable_by_key(limit, |candidate| {
                                (candidate.priority, candidate.person)
                            });
                            local.truncate(limit);
                        }
                        local
                    })
                    .collect::<Vec<_>>()
                    .into_par_iter()
                    .flatten()
                    .collect();
            }
        }
        candidates.par_sort_unstable_by_key(|c| (c.job, c.priority, c.person));
        if candidates
            .par_windows(2)
            .all(|pair| pair[0].job != pair[1].job)
        {
            return candidates
                .into_par_iter()
                .map(|candidate| Event {
                    tick: self.tick,
                    person: candidate.person,
                    lead: candidate.lead,
                    job: candidate.job,
                    recipe: candidate.recipe,
                    offered: candidate.offered,
                    progress: candidate
                        .offered
                        .min(candidate.remaining.saturating_sub(candidate.base)),
                    finished: false,
                    interrupted: false,
                })
                .collect();
        }
        let mut events: Vec<_> = candidates
            .par_chunk_by(|a, b| a.job == b.job)
            .flat_map(|group| {
                let admitted = &group[..group
                    .len()
                    .min(usize::try_from(group[0].limit).unwrap_or(usize::MAX))];
                let budget = u128::from(group[0].remaining.saturating_sub(group[0].base));
                let totals: Vec<u128> = admitted
                    .par_chunks(1024)
                    .map(|chunk| chunk.iter().map(|c| u128::from(c.offered)).sum())
                    .collect();
                let mut sum = 0u128;
                let offsets: Vec<_> = totals
                    .into_iter()
                    .map(|amount| {
                        let before = sum;
                        sum = sum.saturating_add(amount);
                        before
                    })
                    .collect();
                admitted
                    .par_chunks(1024)
                    .zip(offsets.into_par_iter())
                    .flat_map_iter(move |(chunk, offset)| {
                        let mut used = offset;
                        chunk.iter().map(move |candidate| {
                            let progress = u128::from(candidate.offered)
                                .min(budget.saturating_sub(used))
                                as u64;
                            used = used.saturating_add(u128::from(candidate.offered));
                            Event {
                                tick: self.tick,
                                person: candidate.person,
                                lead: candidate.lead,
                                job: candidate.job,
                                recipe: candidate.recipe,
                                offered: candidate.offered,
                                progress,
                                finished: false,
                                interrupted: false,
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        events.par_sort_unstable_by_key(|event| (event.job, event.person));
        events
    }
}

/// Add only effective helper progress to the lead's existing rate.
pub fn apply_rates(events: &[Event], rates: &mut [(u64, u64)]) {
    if events.is_empty() {
        return;
    }
    if events.par_windows(2).all(|pair| pair[0].job != pair[1].job) {
        rates.par_iter_mut().for_each(|row| {
            if let Ok(i) = events.binary_search_by_key(&row.0, |event| event.job) {
                row.1 = row.1.saturating_add(events[i].progress);
            }
        });
        return;
    }
    let sums: Vec<_> = events
        .par_chunk_by(|a, b| a.job == b.job)
        .map(|group| {
            (
                group[0].job,
                group
                    .par_iter()
                    .map(|event| u128::from(event.progress))
                    .sum::<u128>()
                    .min(u128::from(u64::MAX)) as u64,
            )
        })
        .collect();
    rates.par_iter_mut().for_each(|row| {
        if let Ok(i) = sums.binary_search_by_key(&row.0, |sum| sum.0) {
            row.1 = row.1.saturating_add(sums[i].1);
        }
    });
}
