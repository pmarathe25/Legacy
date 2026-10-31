//! Competing individual assertions and personal responses to physical resource use.
use game_parallel_economy_person::{Person, provision};
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Place, View};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

#[macro_export]
macro_rules! claim_methods {
    () => {
        /// Assert a personal claim to a local physical site; this grants no authority.
        pub fn claim_site(&mut self, person: u64, site: u64) -> Result<(), String> {
            let sites = self.work_sites.as_ref().ok_or("work sites are disabled")?;
            let target = sites
                .sites
                .binary_search_by_key(&site, |s| s.id)
                .ok()
                .map(|i| &sites.sites[i])
                .ok_or("unknown work site")?;
            if self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| {
                    self.people[i].health == 0 || self.people[i].region != Some(target.region)
                })
            {
                return Err("person and site must be local and alive".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            if claims
                .site_claims
                .iter()
                .any(|c| c.person == person && c.site == site)
            {
                return Err("duplicate personal site claim".into());
            }
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            claims.site_claims.push($crate::SiteClaim { person, site });
            self.epoch = epoch;
            Ok(())
        }
        /// Withdraw only this person's site assertion, even if the site has been destroyed.
        pub fn withdraw_site_claim(&mut self, person: u64, site: u64) -> Result<(), String> {
            if self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| self.people[i].health == 0)
            {
                return Err("person must be alive".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            let index = claims
                .site_claims
                .iter()
                .position(|c| c.person == person && c.site == site)
                .ok_or("unknown personal site claim")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            claims.site_claims.remove(index);
            self.epoch = epoch;
            Ok(())
        }
        /// Make a personal assertion; other people may make conflicting assertions.
        pub fn claim_stock(&mut self, person: u64, region: u64, good: u32) -> Result<(), String> {
            if !self
                .people
                .iter()
                .any(|p| p.id == person && p.health > 0 && p.region == Some(region))
                || self
                    .physical
                    .custody(game_parallel_lots::Place::Stock { region, good })
                    .is_none()
            {
                return Err("person and stock must be local".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            if claims
                .claims
                .iter()
                .any(|c| (c.person, c.region, c.good) == (person, region, good))
            {
                return Err("duplicate personal claim".into());
            }
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            claims.claims.push($crate::Claim {
                person,
                region,
                good,
            });
            self.epoch = epoch;
            Ok(())
        }
        /// Withdraw only this person's assertion, without affecting other claims.
        pub fn withdraw_claim(
            &mut self,
            person: u64,
            region: u64,
            good: u32,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("person must be alive".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            let index = claims
                .claims
                .iter()
                .position(|c| (c.person, c.region, c.good) == (person, region, good))
                .ok_or("unknown personal claim")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            claims.claims.remove(index);
            self.epoch = epoch;
            Ok(())
        }
        /// Change only this person's choice about other people's assertions.
        pub fn set_claim_choice(
            &mut self,
            person: u64,
            disregard_others: bool,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("person must be alive".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            if let Some(policy) = claims.preferences.iter_mut().find(|p| p.person == person) {
                policy.disregard_others = disregard_others;
            } else {
                claims.preferences.push($crate::Preference {
                    person,
                    disregard_others,
                    assert_outputs: false,
                    assert_built_sites: false,
                    desperate_hunger: None,
                });
            }
            self.epoch = epoch;
            Ok(())
        }
        /// Choose whether to assert a claim after this person's actual output arrives.
        pub fn set_output_claim_choice(
            &mut self,
            person: u64,
            enabled: bool,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("person must be alive".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            if let Some(policy) = claims.preferences.iter_mut().find(|p| p.person == person) {
                policy.assert_outputs = enabled;
            } else {
                claims.preferences.push($crate::Preference {
                    person,
                    disregard_others: false,
                    assert_outputs: enabled,
                    assert_built_sites: false,
                    desperate_hunger: None,
                });
            }
            self.epoch = epoch;
            Ok(())
        }
        /// Choose whether to assert a personal claim when this person builds a site.
        pub fn set_built_site_claim_choice(
            &mut self,
            person: u64,
            enabled: bool,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("person must be alive".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            if let Some(policy) = claims.preferences.iter_mut().find(|p| p.person == person) {
                policy.assert_built_sites = enabled;
            } else {
                claims.preferences.push($crate::Preference {
                    person,
                    disregard_others: false,
                    assert_outputs: false,
                    assert_built_sites: enabled,
                    desperate_hunger: None,
                });
            }
            self.epoch = epoch;
            Ok(())
        }
        /// Set this person's threshold for considering claimed food when hungry.
        pub fn set_desperate_food_choice(
            &mut self,
            person: u64,
            hunger: Option<u32>,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("person must be alive".into());
            }
            let claims = self
                .dynamics
                .as_mut()
                .and_then(|d| d.claims.as_mut())
                .ok_or("claims are disabled")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            if let Some(policy) = claims.preferences.iter_mut().find(|p| p.person == person) {
                policy.desperate_hunger = hunger;
            } else {
                claims.preferences.push($crate::Preference {
                    person,
                    disregard_others: false,
                    assert_outputs: false,
                    assert_built_sites: false,
                    desperate_hunger: hunger,
                });
            }
            self.epoch = epoch;
            Ok(())
        }
    };
}
#[macro_export]
macro_rules! prepare_pickup {
    ($world:expr, $view:expr, $busy:expr, $base:expr, $index:expr) => {{
        let plan = game_parallel_economy_person::provision::prepare(
            &$world.people,
            &$world.rules,
            $world.physical.goods(),
            &$view,
            &$busy,
            $base,
            |person, region, good| {
                $index
                    .as_ref()
                    .map_or(Some(0), |i| i.food_access(person, region, good))
            },
        )?;
        let uses = if $index.is_some() {
            $crate::pickups(&plan)
        } else {
            Vec::new()
        };
        (plan, uses)
    }};
}
#[macro_export]
macro_rules! prepare_manual_transfer {
    ($world:expr, $choice:expr, $view:expr, $base:expr, $index:expr, $uses:expr) => {{
        let stock_choice = match $choice {
            Some(game_parallel_economy_interaction::PhysicalChoice::Stock(choice)) => Some(choice),
            _ => None,
        };
        let mut transfer = game_parallel_economy_interaction::transfer(
            stock_choice,
            &$world.people,
            &$view,
            $base,
        )?;
        if transfer.is_none() {
            let recovery = match $choice {
                Some(game_parallel_economy_interaction::PhysicalChoice::Recover {
                    person,
                    deceased,
                    good,
                    quantity,
                }) => Some((person, deceased, good, quantity)),
                _ => None,
            };
            transfer = game_parallel_economy_interaction::recover(
                recovery,
                &$world.people,
                &$view,
                $base,
            )?;
        }
        if let Some(action) = &transfer {
            $base = $base.checked_add(1).ok_or("action count overflow")?;
            let movement = action.movements[0];
            if $index.is_some() {
                if let game_parallel_lots::Place::Stock { region, good } = movement.from {
                    $uses.push($crate::Use {
                        action: action.id,
                        person: stock_choice.expect("manual transfer choice").person,
                        region,
                        good,
                        quantity: movement.quantity,
                        job: false,
                    });
                }
            }
        }
        transfer
    }};
}
#[macro_export]
macro_rules! prepare_index {
    ($world:expr) => {
        $world
            .dynamics
            .as_ref()
            .and_then(|d| d.claims.as_ref())
            .filter(|claims| !claims.claims.is_empty() || !claims.site_claims.is_empty())
            .map(|claims| claims.index())
    };
}
#[macro_export]
macro_rules! publish_claims {
    ($dynamics:expr, $uses:expr, $events:expr, $people:expr, $opinions:expr) => {
        if let Some(claims) = $dynamics.as_mut().and_then(|d| d.claims.as_mut()) {
            $crate::publish(claims, &$uses, $events, &$people, &mut $opinions);
        }
    };
}
#[macro_export]
macro_rules! work_actions {
    ($work:expr, $claims:expr, $world:expr, $uses:expr, $busy:expr) => {{
        let starts = $work
            .as_ref()
            .and_then(|plan| plan.work.as_ref())
            .map_or(&[][..], |plan| plan.requests.as_slice());
        if $claims
            .as_ref()
            .is_some_and(|index| index.has_stock_claims())
        {
            $uses.extend($crate::work(starts, $world.physical.recipes())?);
        }
        if let Some(plan) = &$work {
            game_parallel_economy_work::reserve(&mut $busy, plan.occupied.iter().copied());
        }
        starts
    }};
}
#[macro_export]
macro_rules! include_care_farming {
    ($uses:expr, $care:expr, $farming:expr) => {
        if let Some(plan) = &$care {
            $uses.extend_from_slice(&plan.uses);
        }
        if let Some(plan) = &$farming {
            $uses.extend_from_slice(&plan.uses);
        }
    };
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub person: u64,
    pub region: u64,
    pub good: u32,
}
/// One person's assertion about a physical site, without authority to enforce it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteClaim {
    pub person: u64,
    pub site: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    #[serde(default)]
    pub disregard_others: bool,
    #[serde(default)]
    pub assert_outputs: bool,
    /// This person's own preference about asserting newly built physical sites.
    #[serde(default)]
    pub assert_built_sites: bool,
    /// At this hunger, this individual may take claimed food after considering open food.
    #[serde(default)]
    pub desperate_hunger: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseEvent {
    pub tick: u64,
    pub person: u64,
    pub claimant: u64,
    pub region: u64,
    pub good: u32,
    pub quantity: u64,
    pub witnessed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteUseEvent {
    pub tick: u64,
    pub person: u64,
    pub claimant: u64,
    pub site: u64,
    pub destroyed: bool,
}
#[derive(Clone, Copy)]
pub struct SiteUse {
    pub person: u64,
    pub site: u64,
    pub destroyed: bool,
}
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claims {
    /// Newborns may inherit preferences, never assertions or relationships.
    #[serde(default)]
    pub inherit_births: bool,
    #[serde(default)]
    pub claims: Vec<Claim>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
    #[serde(default)]
    pub events: Vec<UseEvent>,
    #[serde(default)]
    pub site_claims: Vec<SiteClaim>,
    #[serde(default)]
    pub site_events: Vec<SiteUseEvent>,
}
impl Clone for Claims {
    fn clone(&self) -> Self {
        Self {
            inherit_births: self.inherit_births,
            claims: self.claims.par_iter().cloned().collect(),
            preferences: self.preferences.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
            site_claims: self.site_claims.par_iter().cloned().collect(),
            site_events: self.site_events.par_iter().cloned().collect(),
        }
    }
}

/// Enroll actual newborns without copying any person's claims or feelings.
pub fn enroll(
    owner: Option<&mut Claims>,
    family: Option<&game_parallel_families::Families>,
    tick: u64,
) {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return;
    };
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|member| member.born == Some(tick))
        .collect();
    if newborns.is_empty() {
        return;
    }
    let mut preferences: Vec<_> = owner.preferences.par_iter().collect();
    preferences.par_sort_unstable_by_key(|preference| preference.person);
    let inherited: Vec<_> = newborns
        .into_par_iter()
        .filter_map(|member| {
            if preferences
                .binary_search_by_key(&member.id, |p| p.person)
                .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                preferences
                    .binary_search_by_key(id, |p| p.person)
                    .ok()
                    .map(|i| preferences[i])
            })?;
            Some(Preference {
                person: member.id,
                disregard_others: parent.disregard_others,
                assert_outputs: parent.assert_outputs,
                assert_built_sites: parent.assert_built_sites,
                desperate_hunger: parent.desperate_hunger,
            })
        })
        .collect();
    owner.preferences.par_extend(inherited);
}

impl Claims {
    pub fn validate(&self, people: &HashSet<u64>, view: &View, tick: u64) -> Result<(), String> {
        let mut keys: Vec<_> = self
            .claims
            .par_iter()
            .map(|c| (c.region, c.good, c.person))
            .collect();
        keys.par_sort_unstable();
        let mut policies: Vec<_> = self.preferences.par_iter().map(|p| p.person).collect();
        policies.par_sort_unstable();
        let mut site_keys: Vec<_> = self
            .site_claims
            .par_iter()
            .map(|c| (c.site, c.person))
            .collect();
        site_keys.par_sort_unstable();
        if site_keys.par_windows(2).any(|p| p[0] == p[1])
            || self
                .site_claims
                .par_iter()
                .any(|c| c.site == 0 || !people.contains(&c.person))
            || self.site_events.par_iter().any(|e| {
                e.tick > tick
                    || e.site == 0
                    || e.person == e.claimant
                    || !people.contains(&e.person)
                    || !people.contains(&e.claimant)
            })
            || keys.par_windows(2).any(|p| p[0] == p[1])
            || policies.par_windows(2).any(|p| p[0] == p[1])
            || self.claims.par_iter().any(|c| {
                !people.contains(&c.person)
                    || view
                        .container(Place::Stock {
                            region: c.region,
                            good: c.good,
                        })
                        .is_none()
            })
            || policies.par_iter().any(|person| !people.contains(person))
            || self.events.par_iter().any(|e| {
                e.tick > tick
                    || e.person == e.claimant
                    || !people.contains(&e.person)
                    || !people.contains(&e.claimant)
                    || e.quantity == 0
                    || view
                        .container(Place::Stock {
                            region: e.region,
                            good: e.good,
                        })
                        .is_none()
            })
        {
            return Err("invalid personal resource claims".into());
        }
        Ok(())
    }
    pub fn index(&self) -> Index<'_> {
        let mut claims: Vec<_> = self.claims.par_iter().collect();
        claims.par_sort_unstable_by_key(|c| (c.region, c.good, c.person));
        let mut preferences: Vec<_> = self.preferences.par_iter().collect();
        preferences.par_sort_unstable_by_key(|p| p.person);
        let mut site_claims: Vec<_> = self.site_claims.par_iter().collect();
        site_claims.par_sort_unstable_by_key(|c| (c.site, c.person));
        Index {
            claims,
            preferences,
            site_claims,
        }
    }
}
pub struct Index<'a> {
    claims: Vec<&'a Claim>,
    preferences: Vec<&'a Preference>,
    site_claims: Vec<&'a SiteClaim>,
}
impl Index<'_> {
    pub fn has_stock_claims(&self) -> bool {
        !self.claims.is_empty()
    }
    pub fn disregards(&self, actor: u64) -> bool {
        self.preferences
            .binary_search_by_key(&actor, |p| p.person)
            .ok()
            .is_some_and(|i| self.preferences[i].disregard_others)
    }
    /// Sorted claimed sites; None identifies competing assertions by several people.
    pub fn site_classes(&self) -> Vec<(u64, Option<u64>)> {
        self.site_claims
            .par_chunk_by(|a, b| a.site == b.site)
            .map(|group| (group[0].site, (group.len() == 1).then_some(group[0].person)))
            .collect()
    }
    pub fn allows_site(&self, actor: u64, site: u64) -> bool {
        if self.disregards(actor) {
            return true;
        }
        let start = self.site_claims.partition_point(|claim| claim.site < site);
        let end = self.site_claims.partition_point(|claim| claim.site <= site);
        self.site_claims[start..end]
            .iter()
            .all(|claim| claim.person == actor)
    }
    pub fn food_access(&self, person: &Person, region: u64, good: u32) -> Option<u8> {
        if self.allows(person.id, region, good) {
            return Some(0);
        }
        self.preferences
            .binary_search_by_key(&person.id, |p| p.person)
            .ok()
            .and_then(|i| self.preferences[i].desperate_hunger)
            .filter(|&threshold| person.hunger >= threshold)
            .map(|_| 1)
    }
    pub fn allows(&self, actor: u64, region: u64, good: u32) -> bool {
        if self.disregards(actor) {
            return true;
        }
        let a = self
            .claims
            .partition_point(|c| (c.region, c.good) < (region, good));
        let b = self
            .claims
            .partition_point(|c| (c.region, c.good) <= (region, good));
        self.claims[a..b].iter().all(|c| c.person == actor)
    }
    pub fn allows_inputs(
        &self,
        actor: u64,
        region: u64,
        recipe: &game_parallel_production_model::Recipe,
    ) -> bool {
        recipe
            .inputs
            .iter()
            .all(|input| self.allows(actor, region, input.good))
    }
}
/// Filter only a person's own planned work; this never prevents physical use.
pub fn filter_work(
    auto: &game_parallel_autowork::Autowork,
    people: &[game_parallel_production_model::PersonSnapshot],
    production: &game_parallel_production::Production,
    claims: Option<&Index<'_>>,
) -> Option<game_parallel_autowork::Autowork> {
    let claims = claims?;
    if !claims.has_stock_claims() {
        return None;
    }
    let mut recipes: Vec<_> = production.recipes.par_iter().collect();
    recipes.par_sort_unstable_by_key(|r| r.id);
    let policies = auto
        .policies
        .par_iter()
        .filter(|policy| {
            if policy.disregard_claims {
                return true;
            }
            let region = people
                .binary_search_by_key(&policy.person, |p| p.0)
                .ok()
                .and_then(|i| people[i].1);
            let recipe = recipes
                .binary_search_by_key(&policy.recipe, |r| r.id)
                .ok()
                .map(|i| recipes[i]);
            region
                .zip(recipe)
                .is_some_and(|(region, recipe)| claims.allows_inputs(policy.person, region, recipe))
        })
        .cloned()
        .collect();
    Some(game_parallel_autowork::Autowork {
        next_id: auto.next_id,
        policies,
        requests: None,
    })
}
/// Resolve local personal choices with bounded retries and disjoint workers.
pub fn match_pairs<T: Copy + Send + Sync, W: Copy + Send + Sync>(
    fields: &[T],
    workers: &[W],
    allowed: impl Fn(T, W) -> bool + Sync,
) -> Vec<(T, W)> {
    let mut fields = fields.to_vec();
    let mut workers = workers.to_vec();
    let mut result = Vec::new();
    for round in 0..4 {
        if fields.is_empty() || workers.is_empty() {
            break;
        }
        let count = fields.len().min(workers.len());
        let accepted: Vec<_> = (0..count)
            .into_par_iter()
            .filter_map(|i| {
                let j = (i + round) % workers.len();
                allowed(fields[i], workers[j]).then_some((i, j))
            })
            .collect();
        if accepted.is_empty() {
            continue;
        }
        let field_done: Vec<_> = (0..fields.len())
            .into_par_iter()
            .map(|_| AtomicBool::new(false))
            .collect();
        let worker_done: Vec<_> = (0..workers.len())
            .into_par_iter()
            .map(|_| AtomicBool::new(false))
            .collect();
        accepted.par_iter().for_each(|&(i, j)| {
            field_done[i].store(true, Ordering::Relaxed);
            worker_done[j].store(true, Ordering::Relaxed);
        });
        result.par_extend(accepted.par_iter().map(|&(i, j)| (fields[i], workers[j])));
        fields = fields
            .into_par_iter()
            .enumerate()
            .filter_map(|(i, field)| (!field_done[i].load(Ordering::Relaxed)).then_some(field))
            .collect();
        workers = workers
            .into_par_iter()
            .enumerate()
            .filter_map(|(i, worker)| (!worker_done[i].load(Ordering::Relaxed)).then_some(worker))
            .collect();
    }
    result
}
#[derive(Clone, Copy)]
pub struct Use {
    pub action: u64,
    pub person: u64,
    pub region: u64,
    pub good: u32,
    pub quantity: u64,
    pub job: bool,
}
/// Record only actual uses witnessed locally by a claimant, bounded per use.
pub fn publish_site_uses(
    claims: &mut Claims,
    uses: &[SiteUse],
    sites: &[(u64, u64)],
    people: &[Person],
    tick: u64,
) {
    if claims.site_claims.is_empty() || uses.is_empty() {
        claims.site_events.clear();
        return;
    }
    let mut observers: Vec<_> = claims
        .site_claims
        .par_iter()
        .filter(|claim| {
            let Some(region) = sites
                .binary_search_by_key(&claim.site, |s| s.0)
                .ok()
                .map(|i| sites[i].1)
            else {
                return false;
            };
            people
                .binary_search_by_key(&claim.person, |p| p.id)
                .ok()
                .is_some_and(|i| people[i].health > 0 && people[i].region == Some(region))
        })
        .collect();
    observers.par_sort_unstable_by_key(|claim| (claim.site, claim.person));
    let observers = &observers;
    let mut events: Vec<_> = uses
        .par_iter()
        .flat_map_iter(|action| {
            let a = observers.partition_point(|claim| claim.site < action.site);
            let b = observers.partition_point(|claim| claim.site <= action.site);
            let count = b - a;
            let first = if count == 0 {
                0
            } else {
                (tick.wrapping_add(action.person) % count as u64) as usize
            };
            (0..count.min(5))
                .map(move |offset| observers[a + (first + offset) % count])
                .filter(move |claim| claim.person != action.person)
                .take(4)
                .map(move |claim| SiteUseEvent {
                    tick,
                    person: action.person,
                    claimant: claim.person,
                    site: action.site,
                    destroyed: action.destroyed,
                })
        })
        .collect();
    events.par_sort_unstable_by_key(|event| {
        (event.claimant, event.person, event.site, event.destroyed)
    });
    claims.site_events = events;
}
/// People who chose to claim their own work may assert newly completed sites.
pub fn publish_built_site_claims(claims: &mut Claims, built: &[(u64, u64)], people: &[Person]) {
    if built.is_empty() || !claims.preferences.par_iter().any(|p| p.assert_built_sites) {
        return;
    }
    let interested: HashSet<_> = claims
        .preferences
        .par_iter()
        .filter(|p| p.assert_built_sites)
        .map(|p| p.person)
        .collect();
    let existing: HashSet<_> = claims
        .site_claims
        .par_iter()
        .map(|c| (c.person, c.site))
        .collect();
    let mut choices: Vec<_> = built
        .par_iter()
        .copied()
        .filter(|&(person, site)| {
            interested.contains(&person)
                && !existing.contains(&(person, site))
                && people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0)
        })
        .collect();
    choices.par_sort_unstable();
    let claims_to_add: Vec<_> = choices
        .par_chunk_by(|a, b| a == b)
        .map(|group| SiteClaim {
            person: group[0].0,
            site: group[0].1,
        })
        .collect();
    claims.site_claims.par_extend(claims_to_add);
}
pub fn pickups(plan: &provision::Plan) -> Vec<Use> {
    plan.actions
        .par_iter()
        .zip(plan.actors.par_iter())
        .filter_map(|(action, &person)| {
            let movement = action.movements.first()?;
            let Place::Stock { region, good } = movement.from else {
                return None;
            };
            Some(Use {
                action: action.id,
                person,
                region,
                good,
                quantity: movement.quantity,
                job: false,
            })
        })
        .collect()
}
pub fn work(
    starts: &[game_parallel_production_model::Start],
    recipes: &[game_parallel_production_model::Recipe],
) -> Result<Vec<Use>, String> {
    let mut recipes: Vec<_> = recipes.par_iter().collect();
    recipes.par_sort_unstable_by_key(|r| r.id);
    let groups: Vec<_> = starts
        .par_iter()
        .map(|start| {
            let recipe = recipes
                .binary_search_by_key(&start.recipe, |r| r.id)
                .ok()
                .map(|i| recipes[i])
                .ok_or("unknown recipe")?;
            recipe
                .inputs
                .iter()
                .map(|input| {
                    Ok(Use {
                        action: start.id,
                        person: start.worker,
                        region: start.region,
                        good: input.good,
                        quantity: input
                            .quantity
                            .checked_mul(u64::from(start.units))
                            .ok_or("work quantity overflow")?,
                        job: true,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .collect::<Result<_, _>>()?;
    Ok(groups.into_par_iter().flatten_iter().collect())
}
/// Record only successful physical use, and update only claimants who observed it.
pub fn publish(
    claims: &mut Claims,
    uses: &[Use],
    events: &Events,
    people: &[Person],
    opinions: &mut Option<Opinions>,
) {
    if claims.claims.is_empty() {
        claims.events.clear();
        return;
    }
    // A bounded sample of locally present claimants observes each use. This
    // prevents a crowded stockpile from producing a quadratic event graph.
    let mut observers: Vec<_> = claims
        .claims
        .par_iter()
        .filter(|c| {
            people
                .binary_search_by_key(&c.person, |p| p.id)
                .ok()
                .is_some_and(|i| people[i].health > 0 && people[i].region == Some(c.region))
        })
        .collect();
    observers.par_sort_unstable_by_key(|c| (c.region, c.good, c.person));
    let observers = &observers;
    let mut uses = uses.to_vec();
    uses.par_sort_unstable_by_key(|u| (u.region, u.good, u.person));
    let mut rows: Vec<_> = uses
        .par_iter()
        .filter(|u| {
            if u.job {
                events.started.binary_search(&u.action).is_ok()
            } else {
                events
                    .physical_actions
                    .binary_search_by_key(&u.action, |e| e.id)
                    .ok()
                    .is_some_and(|i| events.physical_actions[i].accepted)
            }
        })
        .flat_map_iter(|u| {
            let a = observers.partition_point(|c| (c.region, c.good) < (u.region, u.good));
            let b = observers.partition_point(|c| (c.region, c.good) <= (u.region, u.good));
            let count = b - a;
            let first = if count == 0 {
                0
            } else {
                (u.action % count as u64) as usize
            };
            (0..count.min(5))
                .map(move |offset| observers[a + (first + offset) % count])
                .filter(move |c| c.person != u.person)
                .take(4)
                .map(move |c| UseEvent {
                    tick: events.tick,
                    person: u.person,
                    claimant: c.person,
                    region: u.region,
                    good: u.good,
                    quantity: u.quantity,
                    witnessed: true,
                })
        })
        .collect();
    rows.par_sort_unstable_by_key(|e| (e.claimant, e.person, e.region, e.good));
    if let Some(opinions) = opinions {
        let witnessed: Vec<_> = rows
            .par_iter()
            .filter(|r| r.witnessed)
            .map(|r| (r.claimant, r.person))
            .collect();
        opinions.witness_thefts(&witnessed, events.tick);
    }
    claims.events = rows;
}
