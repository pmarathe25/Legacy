//! Reusable tools are physical possessions, checked before work is admitted.
use game_parallel_lots::{Place, View};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub recipe: u32,
    pub good: u32,
    pub quantity: u64,
}

/// The host keeps this table sorted by recipe and good.
pub fn validate(
    rows: &[Requirement],
    recipes: &HashSet<u32>,
    goods: &HashSet<u32>,
) -> Result<(), String> {
    if rows
        .par_windows(2)
        .any(|p| (p[0].recipe, p[0].good) >= (p[1].recipe, p[1].good))
        || rows
            .par_iter()
            .any(|r| r.quantity == 0 || !recipes.contains(&r.recipe) || !goods.contains(&r.good))
    {
        return Err("invalid work tool requirement".into());
    }
    Ok(())
}

/// Each worker must actually carry the tools; they are checked but never consumed.
pub fn available(rows: &[Requirement], view: &View, worker: u64, recipe: u32) -> bool {
    let first = rows.partition_point(|r| r.recipe < recipe);
    let last = rows.partition_point(|r| r.recipe <= recipe);
    if first == last {
        return true;
    }
    view.container(Place::Person(worker)).is_some_and(|c| {
        rows[first..last]
            .iter()
            .all(|r| c.quantity(r.good) >= r.quantity)
    })
}

/// A person can personally fetch a missing tool for work they choose to undertake.
pub struct AcquisitionContext<'a> {
    pub requirements: &'a [Requirement],
    pub auto: Option<&'a game_parallel_autowork::Autowork>,
    pub requests: Option<&'a game_parallel_work_requests::Draft>,
    pub work: &'a [game_parallel_lot_runtime::Work],
    pub rates: &'a [(u64, u64)],
    pub people: &'a [game_parallel_economy_person::Person],
    pub view: &'a View,
    pub claims: Option<&'a game_parallel_economy_claims::Index<'a>>,
    pub busy: &'a [u64],
    pub unwilling: &'a [u64],
    pub max_hunger: u32,
}
pub struct Acquisition {
    pub actions: Vec<game_parallel_lots::Transaction>,
    pub actors: Vec<u64>,
    pub uses: Vec<game_parallel_economy_claims::Use>,
}
impl AcquisitionContext<'_> {
    pub fn prepare(&self, base: u64) -> Result<Acquisition, String> {
        let policies = self.auto.map_or(&[][..], |a| a.policies.as_slice());
        let mut policy_index: Vec<_> = if self.rates.is_empty() {
            Vec::new()
        } else {
            policies.par_iter().collect()
        };
        if policy_index
            .par_windows(2)
            .any(|p| p[0].person > p[1].person)
        {
            policy_index.par_sort_unstable_by_key(|p| p.person);
        }
        let mut request_preferences: Vec<_> = if self.rates.is_empty() {
            Vec::new()
        } else {
            self.auto
                .and_then(|a| a.requests.as_ref())
                .map_or(&[][..], |r| r.preferences.as_slice())
                .par_iter()
                .collect()
        };
        if request_preferences
            .par_windows(2)
            .any(|p| p[0].person > p[1].person)
        {
            request_preferences.par_sort_unstable_by_key(|p| p.person);
        }
        let mut request_jobs: Vec<_> = if self.rates.is_empty() {
            Vec::new()
        } else {
            self.auto
                .and_then(|a| a.requests.as_ref())
                .map_or(&[][..], |r| r.work_links.as_slice())
                .par_iter()
                .map(|link| link.job)
                .collect()
        };
        if request_jobs.par_windows(2).any(|r| r[0] > r[1]) {
            request_jobs.par_sort_unstable();
        }
        let mut requested: Vec<_> = self
            .requests
            .map_or(&[][..], |r| r.decisions.as_slice())
            .par_iter()
            .collect();
        requested.par_sort_unstable_by_key(|r| r.worker);
        let mut rates: Vec<_> = self.rates.par_iter().map(|r| r.0).collect();
        if rates.par_windows(2).any(|r| r[0] > r[1]) {
            rates.par_sort_unstable();
        }
        let active: Vec<_> = self
            .work
            .par_iter()
            .filter(|w| rates.binary_search(&w.job.id).is_ok())
            .map(|w| {
                let disregards = if request_jobs.binary_search(&w.job.id).is_ok() {
                    request_preferences
                        .binary_search_by_key(&w.job.worker, |p| p.person)
                        .ok()
                        .is_some_and(|i| request_preferences[i].disregard_claims)
                } else {
                    policy_index
                        .binary_search_by_key(&w.job.worker, |p| p.person)
                        .ok()
                        .is_some_and(|i| policy_index[i].disregard_claims)
                };
                (w.job.worker, w.job.recipe, disregards, 2u8)
            })
            .collect();
        let requests = requested
            .par_iter()
            .filter(|r| r.help)
            .map(|r| (r.worker, r.recipe, r.disregard_claims, 1u8));
        let idle = policies
            .par_iter()
            .filter(|p| {
                p.enabled
                    && requested
                        .binary_search_by_key(&p.person, |r| r.worker)
                        .is_err()
            })
            .map(|p| (p.person, p.recipe, p.disregard_claims, 0u8));
        let choices: Vec<_> = active
            .into_par_iter()
            .chain(requests)
            .chain(idle)
            .filter_map(|(id, recipe, disregard, mode)| {
                if (mode != 2 && self.busy.binary_search(&id).is_ok())
                    || (mode == 0 && self.unwilling.binary_search(&id).is_ok())
                {
                    return None;
                }
                let person = self
                    .people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let region = person.region?;
                if person.controlled
                    || person.health == 0
                    || (mode == 0 && !person.willing_to_work)
                    || person.hunger > self.max_hunger
                {
                    return None;
                }
                let holder = self.view.container(Place::Person(person.id))?;
                let first = self.requirements.partition_point(|r| r.recipe < recipe);
                let last = self.requirements.partition_point(|r| r.recipe <= recipe);
                self.requirements[first..last]
                    .iter()
                    .find_map(|requirement| {
                        let amount = requirement
                            .quantity
                            .saturating_sub(holder.quantity(requirement.good));
                        if amount == 0 || amount > holder.capacity.saturating_sub(holder.used) {
                            return None;
                        }
                        if !disregard
                            && self.claims.is_some_and(|claims| {
                                !claims.allows(person.id, region, requirement.good)
                            })
                        {
                            return None;
                        }
                        let stock = self.view.container(Place::Stock {
                            region,
                            good: requirement.good,
                        })?;
                        (stock.quantity(requirement.good) >= amount).then_some((
                            person.id,
                            region,
                            requirement.good,
                            amount,
                        ))
                    })
            })
            .collect();
        base.checked_add(u64::try_from(choices.len()).map_err(|_| "tool pickup count overflow")?)
            .ok_or("tool pickup identity overflow")?;
        let actions: Vec<_> = choices
            .par_iter()
            .enumerate()
            .map(|(i, &(person, region, good, quantity))| {
                let id = base + i as u64;
                game_parallel_lots::Transaction {
                    id,
                    movements: vec![game_parallel_lots::Movement {
                        id,
                        from: Place::Stock { region, good },
                        to: Some(Place::Person(person)),
                        good,
                        quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let actors = choices.par_iter().map(|row| row.0).collect();
        let uses = if self.claims.is_some() {
            choices
                .par_iter()
                .enumerate()
                .map(
                    |(i, &(person, region, good, quantity))| game_parallel_economy_claims::Use {
                        action: base + i as u64,
                        person,
                        region,
                        good,
                        quantity,
                        job: false,
                    },
                )
                .collect()
        } else {
            Vec::new()
        };
        Ok(Acquisition {
            actions,
            actors,
            uses,
        })
    }
}
