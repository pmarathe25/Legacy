//! Fractional progress for an explicitly limited physical, travel, and needs core.
//!
//! This coordinator does not run economic decisions or other world owners.
//! Its caller supplies already admitted work rates; settlement occurs only at
//! the existing quantum boundary, even if an individual counter finishes early.
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
