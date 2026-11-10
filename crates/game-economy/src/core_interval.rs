//! Fractional progress for an explicitly limited physical, travel, and needs core.
//!
//! This coordinator does not run economic decisions or other world owners.
//! Its caller supplies already admitted work rates. Nonperishable work can be
//! published early, as can completed travel; periodic effects settle at the boundary.
use game_economy_person::{
    Person, Rules,
    travel::{self, Travel},
};
use game_lot_diet::Effects;
use game_lot_runtime::{Events, Runtime, Snapshot};
use game_nutrition::Nutrition;
use game_production_model::FoodEffect;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSnapshot {
    pub seed: u64,
    pub physical: Snapshot,
    pub people: Vec<Person>,
    pub rules: Rules,
    #[serde(default)]
    pub travel: Option<Travel>,
    #[serde(default)]
    pub nutrition: Option<Nutrition>,
    #[serde(default)]
    pub pending_food: Vec<FoodEffect>,
}

pub struct CoreInterval {
    seed: u64,
    physical: Runtime,
    people: Vec<Person>,
    rules: Rules,
    travel: Option<Travel>,
    nutrition: Option<Nutrition>,
    pending_food: Vec<FoodEffect>,
}
/// Observed deadlines relative to the current interval position. Several
/// causes can share the earliest time; observation does not publish them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreDeadline {
    pub after_millis: u64,
    pub quantum: bool,
    pub work: bool,
    pub arrival: bool,
    pub hunger: bool,
}
/// Travelers published at a partial-interval offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreArrivals {
    pub tick: u64,
    pub offset_millis: u64,
    pub people: Vec<u64>,
}
fn earliest(left: Option<u128>, right: Option<u128>) -> Option<u128> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}
impl CoreInterval {
    /// Start a quantum after the caller has selected work and reserved inputs.
    pub fn begin(snapshot: CoreSnapshot, rates: &[(u64, u64)]) -> Result<Self, String> {
        if snapshot.physical.work_interval.is_some() {
            return Err("physical interval already active".into());
        }
        let mut core = Self::compile(snapshot)?;
        core.validate_rate_workers(rates)?;
        core.physical.begin_work_interval(rates)?;
        Ok(core)
    }
    /// Restore an in-flight interval, preserving frozen work rates and elapsed time.
    pub fn resume(snapshot: CoreSnapshot) -> Result<Self, String> {
        if snapshot.physical.work_interval.is_none() {
            return Err("missing physical interval".into());
        }
        let core = Self::compile(snapshot)?;
        core.validate_rate_workers(
            &core
                .physical
                .work_interval()
                .expect("checked interval")
                .rates,
        )?;
        Ok(core)
    }
    fn validate_rate_workers(&self, rates: &[(u64, u64)]) -> Result<(), String> {
        if rates.par_iter().any(|&(job, _)| {
            self.physical
                .work()
                .binary_search_by_key(&job, |work| work.job.id)
                .ok()
                .and_then(|i| {
                    self.people
                        .binary_search_by_key(&self.physical.work()[i].job.worker, |person| {
                            person.id
                        })
                        .ok()
                })
                .is_none_or(|i| self.people[i].health == 0)
        }) {
            return Err("interval work rate has no living worker".into());
        }
        Ok(())
    }
    fn compile(snapshot: CoreSnapshot) -> Result<Self, String> {
        let CoreSnapshot {
            seed,
            physical,
            people,
            rules,
            travel,
            nutrition,
            pending_food,
        } = snapshot;
        let physical = Runtime::compile(physical)?;
        let quantum = physical.tick_millis();
        if people.par_windows(2).any(|p| p[0].id >= p[1].id)
            || rules.max_health == 0
            || rules.carrying_capacity == 0
            || rules.starvation_threshold == 0
            || rules.meal_threshold == 0
            || people.par_iter().any(|p| {
                p.id == 0
                    || p.health > rules.max_health
                    || p.hunger_remainder >= quantum
                    || (p.hunger == u32::MAX && p.hunger_remainder != 0)
                    || p.immortal.as_ref().is_some_and(|immortal| {
                        immortal.recovery_per_tick == 0 || p.hunger != 0 || p.hunger_remainder != 0
                    })
            })
        {
            return Err("invalid interval people".into());
        }
        let ids: HashSet<_> = people.par_iter().map(|p| p.id).collect();
        if physical.work().par_iter().any(|work| {
            work.phase == game_lot_runtime::Phase::Working
                && people
                    .binary_search_by_key(&work.job.worker, |p| p.id)
                    .ok()
                    .is_none_or(|i| people[i].region != Some(work.job.region))
        }) {
            return Err("interval work has no local worker".into());
        }
        let view = physical.view(physical.tick())?;
        if let Some(travel) = &travel {
            if travel
                .journeys
                .par_windows(2)
                .any(|pair| pair[0].person >= pair[1].person)
            {
                return Err("interval journeys must be ordered".into());
            }
            travel::validate_world(
                travel,
                &people,
                &view,
                physical.work(),
                physical.tick(),
                quantum,
            )?;
        } else {
            let regions: HashSet<_> = view
                .containers
                .par_iter()
                .filter_map(|container| match container.place {
                    game_lots::Place::Stock { region, .. } => Some(region),
                    _ => None,
                })
                .collect();
            if people
                .par_iter()
                .any(|p| p.region.is_none_or(|region| !regions.contains(&region)))
            {
                return Err("interval person has no valid region".into());
            }
        }
        if let Some(nutrition) = &nutrition {
            nutrition.validate(&ids).map_err(str::to_owned)?;
        }
        game_lot_diet::validate_pending(&pending_food, &ids, physical.tick())?;
        Ok(Self {
            seed,
            physical,
            people,
            rules,
            travel,
            nutrition,
            pending_food,
        })
    }
    pub fn snapshot(&self) -> CoreSnapshot {
        CoreSnapshot {
            seed: self.seed,
            physical: self.physical.snapshot(),
            people: self.people.par_iter().cloned().collect(),
            rules: self.rules.clone(),
            travel: self.travel.clone(),
            nutrition: self.nutrition.clone(),
            pending_food: self.pending_food.par_iter().copied().collect(),
        }
    }
    pub fn elapsed_millis(&self) -> Option<u64> {
        self.physical
            .work_interval()
            .map(|interval| interval.elapsed_millis)
    }
    /// Inspect the earliest work, travel, or optional hunger threshold. Early
    /// Work and travel can be published early under the restrictions below.
    pub fn next_deadline(&self, hunger_threshold: Option<u32>) -> Result<CoreDeadline, String> {
        let interval = self
            .physical
            .work_interval()
            .ok_or("no physical interval")?;
        let quantum = self.physical.tick_millis();
        let remaining = quantum
            .checked_sub(interval.elapsed_millis)
            .ok_or("invalid interval elapsed time")?;
        let work = interval
            .rates
            .par_iter()
            .map(|&(id, rate)| {
                let index = self
                    .physical
                    .work()
                    .binary_search_by_key(&id, |row| row.job.id)
                    .map_err(|_| "missing interval work")?;
                self.physical.work()[index].completion_after(rate, quantum)
            })
            .try_reduce(|| None, |left, right| Ok(earliest(left, right)))
            .map_err(str::to_owned)?;
        let arrival = self
            .travel
            .as_ref()
            .map_or(Ok(None), |travel| {
                travel
                    .journeys
                    .par_iter()
                    .filter(|journey| {
                        self.people
                            .binary_search_by_key(&journey.person, |person| person.id)
                            .ok()
                            .is_some_and(|i| self.people[i].health > 0)
                    })
                    .map(|journey| journey.arrival_after(quantum).map(Some))
                    .try_reduce(|| None, |left, right| Ok(earliest(left, right)))
            })
            .map_err(str::to_owned)?;
        let hunger = hunger_threshold
            .map(|threshold| {
                game_economy_person::next_hunger_threshold(
                    &self.people,
                    threshold,
                    self.rules.hunger_per_tick,
                    quantum,
                )
            })
            .transpose()
            .map_err(str::to_owned)?
            .flatten();
        let after = [Some(u128::from(remaining)), work, arrival, hunger]
            .into_iter()
            .flatten()
            .min()
            .expect("quantum deadline");
        Ok(CoreDeadline {
            after_millis: after as u64,
            quantum: after == u128::from(remaining),
            work: work == Some(after),
            arrival: arrival == Some(after),
            hunger: hunger == Some(after),
        })
    }
    /// Progress no farther than the next observed deadline. A zero result
    /// means a deadline is already pending and needs explicit handling.
    pub fn progress_to_deadline(
        &mut self,
        requested_millis: u64,
        hunger_threshold: Option<u32>,
    ) -> Result<u64, String> {
        let deadline = self.next_deadline(hunger_threshold)?;
        self.progress(requested_millis.min(deadline.after_millis))
    }
    /// Publish admitted nonperishable work that has reached its deadline.
    /// No other world decisions or periodic effects run at this offset.
    /// The caller must exclude work whose outputs require identified items.
    pub fn publish_early_work(
        &mut self,
    ) -> Result<Option<game_lot_runtime::EarlyWorkEvents>, String> {
        let Some(draft) = self.physical.prepare_early_work(self.seed)? else {
            return Ok(None);
        };
        self.physical.commit_early_work(draft).map(Some)
    }
    /// Publish completed living travelers without running other world effects.
    pub fn publish_arrivals(&mut self) -> Result<Option<CoreArrivals>, String> {
        let offset = self.elapsed_millis().ok_or("no physical interval")?;
        if offset == 0 || offset >= self.physical.tick_millis() {
            return Err("early arrival requires a partial interval".into());
        }
        let Some(travel) = &mut self.travel else {
            return Ok(None);
        };
        let people = travel.publish_arrivals(&mut self.people);
        Ok((!people.is_empty()).then_some(CoreArrivals {
            tick: self.physical.tick(),
            offset_millis: offset,
            people,
        }))
    }
    /// Progress without repeating decisions, publishing arrivals, or completing work.
    /// Failed validation leaves all owners unchanged.
    pub fn progress(&mut self, requested_millis: u64) -> Result<u64, String> {
        let elapsed = self.elapsed_millis().ok_or("no physical interval")?;
        let quantum = self.physical.tick_millis();
        let consumed = requested_millis.min(
            quantum
                .checked_sub(elapsed)
                .ok_or("invalid interval elapsed time")?,
        );
        if consumed == 0 {
            return Ok(0);
        }
        let mut people: Vec<_> = self.people.par_iter().cloned().collect();
        let mut travel = self.travel.clone();
        if let Some(travel) = &mut travel {
            travel
                .progress_elapsed(&people, consumed, quantum)
                .map_err(str::to_owned)?;
        }
        game_economy_person::progress_hunger(
            &mut people,
            self.rules.hunger_per_tick,
            consumed,
            quantum,
        )
        .map_err(str::to_owned)?;
        let advanced = self.physical.progress_work_interval(consumed)?;
        debug_assert_eq!(advanced, consumed);
        self.people = people;
        self.travel = travel;
        Ok(consumed)
    }
    /// Settle the full quantum once; no decisions or meals are generated here.
    pub fn settle(&mut self) -> Result<Events, String> {
        let quantum = self.physical.tick_millis();
        if self.elapsed_millis() != Some(quantum) {
            return Err("interval is not complete".into());
        }
        let tick = self.physical.tick().checked_add(1).ok_or("tick overflow")?;
        let mut draft = game_economy_person::settle_elapsed(
            &self.people,
            &self.rules,
            self.nutrition.as_ref(),
            &self.pending_food,
            Effects {
                hunger_relief: vec![],
                pending: vec![],
            },
            tick,
        );
        let travel = self
            .travel
            .as_ref()
            .map(|travel| travel.settle_elapsed(&mut draft.people));
        let events = self.physical.settle_work_interval(self.seed)?;
        self.people = draft.people;
        self.nutrition = draft.nutrition;
        self.pending_food = draft.pending;
        self.travel = travel;
        Ok(events)
    }
}
