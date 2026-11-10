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
mod host;
pub use host::{
    CoreAssistRecord, CoreAssistSession, CoreDeliveryResults, CoreDynamicsResults,
    CoreFoodProvision, CoreFoodSearch, CoreHealth, CoreHost, CoreHostSnapshot, CoreMarketResults,
    CorePracticeWorkEvidence, CorePurpose, CoreRepairResults, CoreRequestResults, CoreRun,
    CoreRunEvent, CoreSchedule, CoreSharedWork, CoreSites, CoreSkillCredit, CoreSkills, CoreSocial,
    CoreSocialResults, CoreToolRepairResults, CoreTreatmentResults, DecisionPhase,
};

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
    generation: u64,
    seed: u64,
    physical: Runtime,
    people: Vec<Person>,
    rules: Rules,
    travel: Option<Travel>,
    nutrition: Option<Nutrition>,
    pending_food: Vec<FoodEffect>,
}
/// Detached boundary state; an item owner may stage metadata from its physical
/// changes before the host commits either owner.
pub struct CoreSettlementDraft {
    generation: u64,
    physical: game_lot_runtime::IntervalSettlementDraft,
    people: game_economy_person::Draft,
    travel: Option<Travel>,
}
/// A meal's physical and physiological effects, unpublished until commit.
pub struct CoreMealDraft {
    generation: u64,
    physical: game_lot_runtime::IntervalConsumptionDraft,
    people: Vec<Person>,
    pending_food: Vec<FoodEffect>,
}
impl CoreMealDraft {
    pub fn transactions(&self) -> &[game_lots::TransactionOutcome] {
        self.physical.transactions()
    }
    pub fn item_updates(&self) -> &[(u64, Option<game_lots::Place>)] {
        self.physical.item_updates()
    }
}
impl CoreSettlementDraft {
    pub fn events(&self) -> &Events {
        self.physical.events()
    }
    pub fn produced_items(&self) -> &[game_lot_runtime::ProducedItem] {
        self.physical.produced_items()
    }
    pub fn next_produced_item(&self) -> Option<u64> {
        self.physical.next_produced_item()
    }
    pub fn item_updates(&self) -> &[(u64, Option<game_lots::Place>)] {
        self.physical.item_updates()
    }
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
            generation: 0,
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
        if !draft.item_updates().is_empty() {
            return Err("identified work requires a staged item owner".into());
        }
        self.physical.commit_early_work(draft).map(Some)
    }
    /// Stage early production so an item owner can prepare metadata before it
    /// is published. Perishable output still waits for boundary settlement.
    pub fn prepare_early_work(
        &self,
        identified: Option<game_lot_runtime::IdentifiedProduction<'_>>,
    ) -> Result<Option<game_lot_runtime::EarlyWorkDraft>, String> {
        self.physical
            .prepare_early_work_with_identified(self.seed, identified)
    }
    pub fn validate_early_work(
        &self,
        draft: &game_lot_runtime::EarlyWorkDraft,
    ) -> Result<(), String> {
        self.physical.validate_early_work(draft)
    }
    pub fn commit_early_work(
        &mut self,
        draft: game_lot_runtime::EarlyWorkDraft,
    ) -> Result<game_lot_runtime::EarlyWorkEvents, String> {
        self.physical.commit_early_work(draft)
    }
    /// Defer selected completed jobs to ordinary physical boundary settlement.
    /// Outputs requiring identified-item owners still need a broader host.
    pub fn defer_work(
        &mut self,
        tick: u64,
        offset_millis: u64,
        jobs: &[u64],
    ) -> Result<(), String> {
        if self.physical.tick() != tick
            || self.elapsed_millis() != Some(offset_millis)
            || offset_millis == 0
            || offset_millis >= self.physical.tick_millis()
        {
            return Err("completion deferral has a stale or invalid interval time".into());
        }
        self.physical.defer_interval_completions(jobs)
    }
    /// Publish completed living travelers without running other world effects.
    pub fn publish_arrivals(&mut self) -> Result<Option<CoreArrivals>, String> {
        let offset = self.elapsed_millis().ok_or("no physical interval")?;
        if offset == 0 || offset >= self.physical.tick_millis() {
            return Err("early arrival requires a partial interval".into());
        }
        let next_generation = self
            .generation
            .checked_add(1)
            .ok_or("core revision overflow")?;
        let Some(travel) = &mut self.travel else {
            return Ok(None);
        };
        let people = travel.publish_arrivals(&mut self.people);
        if !people.is_empty() {
            self.generation = next_generation;
        }
        Ok((!people.is_empty()).then_some(CoreArrivals {
            tick: self.physical.tick(),
            offset_millis: offset,
            people,
        }))
    }
    /// Start personally chosen journeys at the current offset. Work and other
    /// caller-supplied occupations exclude departure; closures resolve per route.
    pub fn depart(
        &mut self,
        tick: u64,
        offset_millis: u64,
        requests: &[travel::Departure],
        occupied: &[u64],
    ) -> Result<Vec<travel::DepartureOutcome>, String> {
        if self.physical.tick() != tick
            || self.elapsed_millis() != Some(offset_millis)
            || offset_millis >= self.physical.tick_millis()
        {
            return Err("departure has a stale or invalid interval time".into());
        }
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        if self.next_deadline(None)?.after_millis == 0 {
            return Err("publish pending interval events before departure".into());
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("core revision overflow")?;
        let mut busy = occupied.to_vec();
        busy.par_extend(
            self.physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let travel = self.travel.as_ref().ok_or("travel is disabled")?;
        let (travel, people, outcomes) = travel.attempt(&self.people, requests, &busy, tick)?;
        self.travel = Some(travel);
        self.people = people;
        self.generation = generation;
        Ok(outcomes)
    }
    /// Start explicitly chosen work at the current partial-interval offset.
    /// Due arrivals and work completions must be published first.
    pub fn start_work(
        &mut self,
        tick: u64,
        offset_millis: u64,
        requests: &[(game_production_model::Start, u64)],
    ) -> Result<Vec<(u64, bool)>, String> {
        if self.physical.tick() != tick
            || self.elapsed_millis() != Some(offset_millis)
            || offset_millis >= self.physical.tick_millis()
        {
            return Err("work start has a stale or invalid interval time".into());
        }
        if self.next_deadline(None)?.after_millis == 0 {
            return Err("publish pending interval events before starting work".into());
        }
        if requests.par_iter().any(|(start, _)| {
            self.people
                .binary_search_by_key(&start.worker, |person| person.id)
                .ok()
                .is_none_or(|index| {
                    let person = &self.people[index];
                    person.health == 0
                        || person.hunger > self.rules.max_work_hunger
                        || person.region != Some(start.region)
                })
        }) {
            return Err("work start requires an able local person".into());
        }
        self.physical.start_interval_work(requests, self.seed)
    }
    /// Let enabled personal work policies select and physically reserve work.
    /// The host excludes people occupied by other activities or blocked by
    /// owners absent from this limited core, such as sites, tools and claims.
    pub fn start_autonomous_work(
        &mut self,
        tick: u64,
        offset_millis: u64,
        owner: &mut game_autowork::Autowork,
        blocked: &[u64],
        rate: u64,
    ) -> Result<Vec<(u64, bool)>, String> {
        let Some(plan) = self.plan_autonomous_work(tick, offset_millis, owner, blocked, rate)?
        else {
            return Ok(Vec::new());
        };
        let requests: Vec<_> = plan
            .requests
            .par_iter()
            .map(|start| (*start, rate))
            .collect();
        let results = self.start_work(tick, offset_millis, &requests)?;
        owner.next_id = plan.next_id;
        Ok(results)
    }
    /// Prepare personal choices without admitting physical work yet.
    pub fn plan_autonomous_work(
        &self,
        tick: u64,
        offset_millis: u64,
        owner: &game_autowork::Autowork,
        blocked: &[u64],
        rate: u64,
    ) -> Result<Option<game_autowork::Plan>, String> {
        if self.physical.tick() != tick
            || self.elapsed_millis() != Some(offset_millis)
            || offset_millis >= self.physical.tick_millis()
            || rate == 0
            || owner.next_id == 0
            || blocked.par_windows(2).any(|rows| rows[0] >= rows[1])
        {
            return Err("invalid autonomous work request".into());
        }
        if self.next_deadline(None)?.after_millis == 0 {
            return Err("publish pending interval events before starting work".into());
        }
        let mut busy = blocked.to_vec();
        busy.par_extend(
            self.people
                .par_iter()
                .filter(|person| person.controlled || !person.willing_to_work)
                .map(|person| person.id),
        );
        busy.par_extend(
            self.physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let people: Vec<_> = self
            .people
            .par_iter()
            .map(|person| (person.id, person.region, person.health, person.hunger))
            .collect();
        let view = self.physical.view(tick)?;
        let production =
            game_lot_projection::production(&self.physical, &view, self.rules.max_work_hunger);
        let max_job = self.physical.work().last().map(|row| row.job.id);
        owner.plan(&people, &production, &busy, max_job)
    }
    /// Stop chosen jobs at this offset. Inputs stay in job custody until the
    /// boundary, where their return can succeed or remain blocked.
    pub fn abandon_work(
        &mut self,
        tick: u64,
        offset_millis: u64,
        requests: &[(u64, u64)],
    ) -> Result<(), String> {
        if self.physical.tick() != tick
            || self.elapsed_millis() != Some(offset_millis)
            || offset_millis == 0
            || offset_millis >= self.physical.tick_millis()
        {
            return Err("work abandonment has a stale or invalid interval time".into());
        }
        if self.next_deadline(None)?.after_millis == 0 {
            return Err("publish pending interval events before abandoning work".into());
        }
        self.physical.abandon_interval_work(requests)
    }
    /// Transfer goods between local people or between a person and local stock.
    /// The host decides who acts and whether their social choices permit it.
    pub fn transfer_goods(
        &mut self,
        tick: u64,
        offset_millis: u64,
        actions: &[game_lots::Transaction],
    ) -> Result<game_lot_runtime::PhysicalOutcomes, String> {
        if self.physical.tick() != tick
            || self.elapsed_millis() != Some(offset_millis)
            || offset_millis == 0
            || offset_millis >= self.physical.tick_millis()
        {
            return Err("transfer has a stale or invalid interval time".into());
        }
        if self.next_deadline(None)?.after_millis == 0 {
            return Err("publish pending interval events before transferring goods".into());
        }
        let region = |place| match place {
            game_lots::Place::Person(id) => self
                .people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .and_then(|index| self.people[index].region),
            game_lots::Place::Stock { region, .. } => Some(region),
            _ => None,
        };
        if actions.par_iter().any(|action| {
            action.movements.iter().any(|movement| {
                let Some(to) = movement.to else {
                    return true;
                };
                !(matches!(movement.from, game_lots::Place::Person(_))
                    || matches!(to, game_lots::Place::Person(_)))
                    || region(movement.from).is_none_or(|source| region(to) != Some(source))
            })
        }) {
            return Err("interval transfer requires local personal custody".into());
        }
        self.physical.transfer_interval(actions, self.seed)
    }
    /// Stage chosen meals and expose item removals for other owners.
    pub fn prepare_meals(
        &self,
        tick: u64,
        offset_millis: u64,
        meals: &[game_lot_diet::Meal],
    ) -> Result<Option<CoreMealDraft>, String> {
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("core.meal.validate");
        if self.physical.tick() != tick
            || self.elapsed_millis() != Some(offset_millis)
            || offset_millis >= self.physical.tick_millis()
        {
            return Err("meal has a stale or invalid interval time".into());
        }
        if self.next_deadline(None)?.after_millis == 0 {
            return Err("publish pending interval events before eating".into());
        }
        if meals.is_empty() {
            return Ok(None);
        }
        if meals.par_iter().any(|meal| {
            self.people
                .binary_search_by_key(&meal.person, |p| p.id)
                .ok()
                .is_none_or(|index| self.people[index].health == 0)
        }) {
            return Err("meal requires a living person".into());
        }
        let effect_tick = tick.checked_add(1).ok_or("food effect tick overflow")?;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("core.meal.plan");
        }
        let plan = game_lot_diet::plan(meals, self.physical.goods())?;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("core.meal.custody");
        }
        let draft = self
            .physical
            .prepare_interval_consumption(plan.actions(), self.seed)?;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("core.meal.effects");
        }
        let effects =
            plan.interval_effects(draft.transactions(), &draft.movements(), effect_tick)?;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("core.meal.people");
        }
        let mut people: Vec<_> = self.people.par_iter().cloned().collect();
        people.par_iter_mut().for_each(|person| {
            if let Ok(index) = effects
                .hunger_relief
                .binary_search_by_key(&person.id, |row| row.0)
            {
                let relief = effects.hunger_relief[index].1;
                if relief > person.hunger {
                    person.hunger_remainder = 0;
                }
                person.hunger = person.hunger.saturating_sub(relief);
            }
        });
        let mut pending_food = self.pending_food.clone();
        pending_food.extend(effects.pending);
        #[cfg(feature = "profile")]
        drop(phase);
        Ok(Some(CoreMealDraft {
            generation: self.generation,
            physical: draft,
            people,
            pending_food,
        }))
    }
    /// Stage meals independently chosen by hungry people from their own food.
    /// The host supplies sorted, unique people occupied by other activities.
    pub fn prepare_autonomous_meals(
        &self,
        blocked: &[u64],
    ) -> Result<Option<(Vec<game_lot_diet::Meal>, CoreMealDraft)>, String> {
        let offset = self.elapsed_millis().ok_or("no physical interval")?;
        let meals = game_economy_person::meal::choose_now(
            &self.people,
            &self.rules,
            self.physical.goods(),
            &self.physical,
            blocked,
        )?;
        let Some(draft) = self.prepare_meals(self.physical.tick(), offset, &meals)? else {
            return Ok(None);
        };
        Ok(Some((meals, draft)))
    }
    pub fn validate_meals(&self, draft: &CoreMealDraft) -> Result<(), String> {
        if self.generation != draft.generation {
            return Err("stale meal draft".into());
        }
        self.physical.validate_interval_consumption(&draft.physical)
    }
    /// Commit only after any item owner has staged its metadata changes.
    pub fn commit_meals(
        &mut self,
        draft: CoreMealDraft,
    ) -> Result<game_lot_runtime::PhysicalOutcomes, String> {
        #[cfg(feature = "profile")]
        let _phase = game_diagnostics::span("core.meal.commit");
        self.validate_meals(&draft)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("core revision overflow")?;
        let result = self.physical.commit_interval_consumption(draft.physical)?;
        self.people = draft.people;
        self.pending_food = draft.pending_food;
        self.generation = generation;
        Ok(result)
    }
    /// Eat ordinary fungible food; identified consumption needs a staged owner.
    pub fn eat(
        &mut self,
        tick: u64,
        offset_millis: u64,
        meals: &[game_lot_diet::Meal],
    ) -> Result<game_lot_runtime::PhysicalOutcomes, String> {
        let Some(draft) = self.prepare_meals(tick, offset_millis, meals)? else {
            return Ok(game_lot_runtime::PhysicalOutcomes {
                transactions: vec![],
                movements: vec![],
                spoiled: vec![],
            });
        };
        if !draft.item_updates().is_empty() {
            return Err("identified meals require a staged item owner".into());
        }
        self.commit_meals(draft)
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
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("core revision overflow")?;
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
        self.generation = generation;
        Ok(consumed)
    }
    /// Stage the full quantum without publishing physical or personal changes.
    /// An enclosing item owner retains its own rules, allocator and metadata.
    pub fn prepare_settlement(
        &mut self,
        identified: Option<game_lot_runtime::IdentifiedProduction<'_>>,
    ) -> Result<CoreSettlementDraft, String> {
        self.prepare_settlement_with_growth(&[], game_lot_runtime::Growth::default(), identified)
    }

    /// Stage boundary actions alongside the physical and personal settlement.
    pub fn prepare_settlement_with_growth(
        &mut self,
        actions: &[game_lots::Transaction],
        growth: game_lot_runtime::Growth<'_>,
        identified: Option<game_lot_runtime::IdentifiedProduction<'_>>,
    ) -> Result<CoreSettlementDraft, String> {
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
        let physical = self
            .physical
            .prepare_interval_settlement_with_growth(actions, growth, self.seed, identified)?;
        Ok(CoreSettlementDraft {
            generation: self.generation,
            physical,
            people: draft,
            travel,
        })
    }
    /// Validate the physical generation after other owners have staged changes.
    pub fn validate_settlement(&mut self, draft: &CoreSettlementDraft) -> Result<(), String> {
        if self.generation != draft.generation {
            return Err("stale core settlement draft".into());
        }
        self.physical.validate_interval_settlement(&draft.physical)
    }
    /// Publish after all other owners have validated their own staged state.
    pub fn commit_settlement(&mut self, draft: CoreSettlementDraft) -> Result<Events, String> {
        self.validate_settlement(&draft)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or("core revision overflow")?;
        let events = self.physical.commit_interval_settlement(draft.physical)?;
        self.people = draft.people.people;
        self.nutrition = draft.people.nutrition;
        self.pending_food = draft.people.pending;
        self.travel = draft.travel;
        self.generation = generation;
        Ok(events)
    }
    /// Settle without identified production; callers with an item owner must
    /// use the staged API with that owner's persistent configuration.
    pub fn settle(&mut self) -> Result<Events, String> {
        let draft = self.prepare_settlement(None)?;
        if draft.item_updates().iter().any(|row| row.1.is_none()) {
            return Err("identified consumption requires a staged item owner".into());
        }
        self.commit_settlement(draft)
    }
}
