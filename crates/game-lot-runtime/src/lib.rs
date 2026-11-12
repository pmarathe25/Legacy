//! Dated physical production; callers own character decisions and observation.
mod advance;
mod early_work;
mod identified_output;
mod interval_settlement;
mod receipts;
pub use advance::{Adoption, AdvanceDraft, Events, Suspension, WorkTransitions};
pub use early_work::{EarlyWorkDraft, EarlyWorkEvents, IntervalConsumptionDraft};
use game_lot_content::Catalog;
pub use game_lot_content::{Gathered, Growth, WritingPage};
pub use game_lot_work::{Phase, Snapshot, Work, WorkInterval};
use game_lots::{Outcome, Place, Spoilage, Store, Transaction, TransactionOutcome};
use game_production_model::{Good, JobPhase, Recipe, Start};
pub use identified_output::{IdentifiedProduction, ProducedItem};
pub use interval_settlement::IntervalSettlementDraft;
use rayon::prelude::*;
pub use receipts::{
    ExternalReceipt, ExternalReceiptBatch, ExternalReceiptHeader, ExternalReceiptRef,
};
use std::collections::HashSet;

pub struct PhysicalOutcomes {
    pub transactions: Vec<TransactionOutcome>,
    pub movements: Vec<Outcome>,
    pub spoiled: Vec<Spoilage>,
}
pub struct Runtime {
    store: Store,
    catalog: Catalog,
    goods: Vec<Good>,
    recipes: Vec<Recipe>,
    tick_millis: u64,
    work: Vec<Work>,
    work_interval: Option<WorkInterval>,
    epoch: u64,
}
impl Runtime {
    fn ensure_no_work_interval(&self) -> Result<(), String> {
        if self.work_interval.is_some() {
            Err("physical work interval is active".into())
        } else {
            Ok(())
        }
    }

    pub fn compile(mut state: Snapshot) -> Result<Self, String> {
        let catalog = Catalog::from_tick_millis(&state.goods, state.tick_millis)?;
        state.work.par_sort_unstable_by_key(|row| row.job.id);
        game_lot_work::validate(&state, &catalog)?;
        let store = Store::compile(state.custody)?;
        let mut workers: Vec<_> = state
            .work
            .par_iter()
            .filter(|row| row.phase == Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        workers.par_sort_unstable();
        let recipes: HashSet<_> = state.recipes.iter().map(|row| row.id).collect();
        if recipes.len() != state.recipes.len()
            || state
                .work
                .par_windows(2)
                .any(|pair| pair[0].job.id == pair[1].job.id)
            || workers.par_windows(2).any(|pair| pair[0] == pair[1])
            || state.work.par_iter().any(|row| {
                let job = &row.job;
                job.id == 0
                    || job.worker == 0
                    || job.region == 0
                    || !recipes.contains(&job.recipe)
                    || match row.phase {
                        Phase::Working | Phase::Returning | Phase::Suspended { .. } => {
                            job.phase != JobPhase::Working
                                || store.container(Place::JobInput(job.id)).is_none()
                                || store.container(Place::JobOutput(job.id)).is_none()
                        }
                        Phase::Waiting => {
                            job.phase != JobPhase::Waiting
                                || store.container(Place::JobOutput(job.id)).is_none()
                        }
                    }
            })
        {
            return Err("invalid physical production state".into());
        }
        Ok(Self {
            store,
            catalog,
            goods: state.goods,
            recipes: state.recipes,
            tick_millis: state.tick_millis,
            work: state.work,
            work_interval: state.work_interval,
            epoch: 0,
        })
    }
    pub fn tick(&self) -> u64 {
        self.store.tick()
    }
    pub fn next_batch(&self) -> Option<u64> {
        self.store.next_batch()
    }
    /// Enable durable batch allocation at an idle physical boundary.
    pub fn enable_batch_allocator(&mut self, minimum: u64) -> Result<u64, String> {
        self.ensure_no_work_interval()?;
        let epoch = self.epoch.checked_add(1).ok_or("physical epoch overflow")?;
        let next = self.store.enable_batch_allocator(minimum)?;
        self.epoch = epoch;
        Ok(next)
    }
    /// Plan disjoint owner ranges without changing physical state.
    pub fn plan_batches(
        &self,
        counts: &[game_lots::BatchCount],
    ) -> Result<game_lots::BatchAllocation, String> {
        self.store.plan_batches(counts)
    }
    /// Current custody of a unique physical item.
    pub fn item_place(&self, id: u64) -> Option<Place> {
        self.store.item_place(id)
    }
    pub fn highest_item(&self) -> Option<u64> {
        self.store.highest_item()
    }
    pub fn next_unused_item(&self, start: u64) -> Option<u64> {
        self.store.next_unused_item(start)
    }
    pub fn next_unused_items(&self, start: u64, count: usize) -> Option<Vec<u64>> {
        self.store.next_unused_items(start, count)
    }
    pub fn tick_millis(&self) -> u64 {
        self.tick_millis
    }
    pub fn goods(&self) -> &[Good] {
        &self.goods
    }
    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            custody: self.store.export(),
            goods: self.goods.clone(),
            recipes: self.recipes.clone(),
            tick_millis: self.tick_millis,
            work: self.work.clone(),
            work_interval: self.work_interval.clone(),
        }
    }
    /// Register real people as independent physical holders.
    pub fn add_people(&mut self, people: &[(u64, u64)]) -> Result<(), String> {
        self.ensure_no_work_interval()?;
        let containers = people
            .par_iter()
            .map(|&(person, capacity)| {
                game_lots::Container::new(Place::Person(person), capacity, Vec::new())
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.store.add_empty(containers)
    }
    pub fn view(&self, boundary: u64) -> Result<game_lots::View, String> {
        self.store.view(boundary)
    }
    pub fn stock_view(&self, boundary: u64) -> Result<game_lots::View, String> {
        self.store.stock_view(boundary)
    }
    pub fn custody(&self, place: Place) -> Option<&game_lots::Container> {
        self.store.container(place)
    }
    /// Possessions remain in custody until physically removed.
    pub fn retire_person(&mut self, person: u64) -> Result<(), String> {
        self.ensure_no_work_interval()?;
        self.store.remove_empty(vec![Place::Person(person)])
    }
    pub fn work(&self) -> &[Work] {
        &self.work
    }
    pub fn work_interval(&self) -> Option<&WorkInterval> {
        self.work_interval.as_ref()
    }

    /// Freeze one boundary's work rates. Custody and other work mutations are
    /// blocked until this interval is closed or cancelled.
    pub fn begin_work_interval(&mut self, rates: &[(u64, u64)]) -> Result<(), String> {
        self.ensure_no_work_interval()?;
        let rates = self.checked_work_rates(rates)?;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        self.work_interval = Some(WorkInterval {
            tick: self.tick(),
            custody_revision: self.store.revision(),
            elapsed_millis: 0,
            rates,
        });
        self.epoch = epoch;
        Ok(())
    }

    /// Replace active rates at the current offset without granting past progress.
    pub fn replace_interval_rates(&mut self, rates: &[(u64, u64)]) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("stale physical work interval".into());
        }
        let next = self.checked_work_rates(rates)?;
        if interval.rates == next {
            return Ok(());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        self.work_interval.as_mut().expect("checked interval").rates = next;
        self.epoch = epoch;
        Ok(())
    }

    /// Consume at most the remaining interval and retain progress for saving.
    pub fn progress_work_interval(&mut self, requested_millis: u64) -> Result<u64, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis > self.tick_millis
        {
            return Err("stale physical work interval".into());
        }
        let consumed = requested_millis.min(self.tick_millis - interval.elapsed_millis);
        if consumed == 0 {
            return Ok(0);
        }
        let mut interval = self.work_interval.take().expect("checked interval");
        let result = self.apply_work_progress(&interval.rates, consumed);
        if result.is_ok() {
            interval.elapsed_millis += consumed;
        }
        self.work_interval = Some(interval);
        result.map(|()| consumed)
    }

    /// Release the interval lock after the full quantum has elapsed. The
    /// caller must then settle completion with no additional work rates.
    pub fn close_work_interval(&mut self) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis != self.tick_millis
        {
            return Err("physical work interval is not complete".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        self.work_interval = None;
        self.epoch = epoch;
        Ok(())
    }

    /// Settle a completed physical-only interval without applying work twice.
    /// If settlement fails, the completed interval remains available to retry.
    pub fn settle_work_interval(&mut self, seed: u64) -> Result<Events, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis != self.tick_millis
        {
            return Err("physical work interval is not complete".into());
        }
        let interval = self.work_interval.take().expect("checked interval");
        match self.advance(&[], seed) {
            Ok(events) => Ok(events),
            Err(error) => {
                self.work_interval = Some(interval);
                Err(error)
            }
        }
    }

    /// Release the lock after interruption, retaining completed work. The
    /// caller must account for the returned elapsed time before rescheduling.
    pub fn cancel_work_interval(&mut self) -> Result<u64, String> {
        let elapsed = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?
            .elapsed_millis;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        self.work_interval = None;
        self.epoch = epoch;
        Ok(elapsed)
    }
    /// Advance reserved work counters without publishing completion or advancing
    /// the boundary. The caller owns interval admission, frozen rates and time.
    /// Invalid batches leave both counters and the draft generation unchanged.
    pub fn progress_work(
        &mut self,
        rates: &[(u64, u64)],
        elapsed_millis: u64,
    ) -> Result<(), String> {
        self.ensure_no_work_interval()?;
        let rates = self.checked_work_rates(rates)?;
        self.apply_work_progress(&rates, elapsed_millis)
    }

    fn checked_work_rates(&self, rates: &[(u64, u64)]) -> Result<Vec<(u64, u64)>, String> {
        let mut rates = rates.to_vec();
        if rates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            rates.par_sort_unstable_by_key(|row| row.0);
        }
        if rates.par_windows(2).any(|pair| pair[0].0 == pair[1].0)
            || rates.par_iter().any(|&(job, rate)| {
                rate == 0
                    || self
                        .work
                        .binary_search_by_key(&job, |row| row.job.id)
                        .map_or(true, |index| self.work[index].phase != Phase::Working)
            })
        {
            return Err("invalid work rate".into());
        }
        Ok(rates)
    }

    // Rates are checked on direct entry or when an interval is opened or loaded.
    fn apply_work_progress(
        &mut self,
        rates: &[(u64, u64)],
        elapsed_millis: u64,
    ) -> Result<(), String> {
        if rates.is_empty() || elapsed_millis == 0 {
            return Ok(());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let quantum = self.tick_millis;
        self.work.par_iter_mut().for_each(|row| {
            if let Ok(index) = rates.binary_search_by_key(&row.job.id, |rate| rate.0) {
                row.advance_elapsed(rates[index].1, elapsed_millis, quantum)
                    .expect("validated work progress");
            }
        });
        self.epoch = epoch;
        Ok(())
    }
    /// Reserve physical materials at the current time, after the caller's actor checks.
    pub fn start(&mut self, requests: &[Start], seed: u64) -> Result<Vec<(u64, bool)>, String> {
        self.ensure_no_work_interval()?;
        game_lot_work::validate_starts(&self.work, requests)?;
        let plan = game_lot_work::start(requests, &self.recipes)?;
        let mut draft =
            self.store
                .prepare_with_empty(seed, &plan.transactions, plan.containers, false)?;
        let result: Vec<_> = draft
            .transactions()
            .iter()
            .map(|row| (row.id, row.accepted))
            .collect();
        let retire: Vec<_> = plan
            .jobs
            .par_iter()
            .filter(|job| {
                !result
                    .binary_search_by_key(&job.id, |row| row.0)
                    .is_ok_and(|index| result[index].1)
            })
            .flat_map_iter(|job| [Place::JobInput(job.id), Place::JobOutput(job.id)])
            .collect();
        let accepted: Vec<_> = plan
            .jobs
            .into_par_iter()
            .filter(|job| {
                result
                    .binary_search_by_key(&job.id, |row| row.0)
                    .is_ok_and(|index| result[index].1)
            })
            .map(|job| Work {
                job,
                phase: Phase::Working,
                progress_remainder: 0,
            })
            .collect();
        self.store.retire_in(&mut draft, retire)?;
        self.store.validate_commit(&draft)?;
        self.store.commit(draft)?;
        self.work.extend(accepted);
        self.work.par_sort_unstable_by_key(|row| row.job.id);
        Ok(result)
    }
    /// Apply physical exchanges, gifts or consumption now. Social decisions remain
    /// with their characters; this owner checks custody and capacity only.
    pub fn actions_now(
        &mut self,
        actions: &[Transaction],
        seed: u64,
    ) -> Result<PhysicalOutcomes, String> {
        self.ensure_no_work_interval()?;
        game_lot_work::validate_actions(&self.catalog, actions)?;
        let draft = self.store.prepare_transactions_now(seed, actions)?;
        let decisions = draft.transactions().to_vec();
        self.store.validate_commit(&draft)?;
        let (outcomes, spoiled) = self.store.commit(draft)?;
        Ok(PhysicalOutcomes {
            transactions: decisions,
            movements: outcomes,
            spoiled,
        })
    }
    /// A worker may stop without requiring a successful materials return.
    pub fn abandon(&mut self, job: u64, worker: u64) -> Result<(), String> {
        self.ensure_no_work_interval()?;
        let index = self
            .work
            .binary_search_by_key(&job, |row| row.job.id)
            .map_err(|_| "unknown job")?;
        let row = &mut self.work[index];
        if row.phase != Phase::Working || row.job.worker != worker {
            return Err("worker is not doing this job".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        row.phase = Phase::Returning;
        self.epoch = epoch;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
