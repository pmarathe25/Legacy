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
            .filter(|claims| !claims.claims.is_empty())
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
        if $claims.is_some() {
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    #[serde(default)]
    pub disregard_others: bool,
    #[serde(default)]
    pub assert_outputs: bool,
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claims {
    #[serde(default)]
    pub claims: Vec<Claim>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
    #[serde(default)]
    pub events: Vec<UseEvent>,
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
        if keys.par_windows(2).any(|p| p[0] == p[1])
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
        Index {
            claims,
            preferences,
        }
    }
}
pub struct Index<'a> {
    claims: Vec<&'a Claim>,
    preferences: Vec<&'a Preference>,
}
impl Index<'_> {
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
        if self
            .preferences
            .binary_search_by_key(&actor, |p| p.person)
            .ok()
            .is_some_and(|i| self.preferences[i].disregard_others)
        {
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
