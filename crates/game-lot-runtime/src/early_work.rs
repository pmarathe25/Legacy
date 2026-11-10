//! Transactional publication of already completed work within an open quantum.
use crate::{Phase, Runtime, Work, WorkInterval};
use game_lots::Place;
use game_production_model::JobPhase;
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EarlyWorkEvents {
    pub tick: u64,
    pub offset_millis: u64,
    pub finished: Vec<u64>,
    pub interrupted: Vec<u64>,
    pub rejected: Vec<u64>,
}
pub struct EarlyWorkDraft {
    epoch: u64,
    custody: game_lots::Draft,
    work: Vec<Work>,
    interval: WorkInterval,
    events: EarlyWorkEvents,
}
impl EarlyWorkDraft {
    pub fn events(&self) -> &EarlyWorkEvents {
        &self.events
    }
}
impl Runtime {
    /// Stage only already completed, admitted work without advancing the tick.
    /// Early perishable output is unsupported until lots have finer timestamps.
    /// Callers must exclude jobs whose outputs require separately owned item
    /// identities: this physical-only operation creates fungible lots.
    pub fn prepare_early_work(&self, seed: u64) -> Result<Option<EarlyWorkDraft>, String> {
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("physical.early.select");
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis == 0
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("early work requires a valid partial interval".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let rows: Vec<_> = self
            .work
            .par_iter()
            .filter(|row| {
                row.phase == Phase::Working
                    && row.job.remaining == 0
                    && interval
                        .rates
                        .binary_search_by_key(&row.job.id, |rate| rate.0)
                        .is_ok()
            })
            .map(|row| {
                self.store
                    .container(Place::JobInput(row.job.id))
                    .map(|input| (&row.job, input))
                    .ok_or("missing work input")
            })
            .collect::<Result<_, _>>()?;
        if rows.is_empty() {
            return Ok(None);
        }
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.early.plan");
        }
        let finished = game_lot_work::finish(&rows, &self.catalog, self.tick())?;
        if finished.transactions.par_iter().any(|transaction| {
            transaction
                .creations
                .iter()
                .any(|creation| creation.lot.expires_at.is_some())
        }) {
            return Err("early perishable production is unsupported".into());
        }
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.early.custody");
        }
        let mut custody = self
            .store
            .prepare_transactions_now(seed, &finished.transactions)?;
        // Early work exposes acceptance, not cohort receipts, and must not
        // silently publish unrelated expiry before the scheduled boundary.
        let (_, spoiled) = custody.take_outcome_batch();
        if !spoiled.is_empty() {
            return Err("early work cannot publish pending spoilage".into());
        }
        let mut accepted: Vec<_> = custody
            .transactions()
            .par_iter()
            .filter_map(|row| row.accepted.then_some(row.id))
            .collect();
        let mut rejected: Vec<_> = custody
            .transactions()
            .par_iter()
            .filter_map(|row| (!row.accepted).then_some(row.id))
            .collect();
        let mut interrupted = finished.interrupted;
        accepted.par_sort_unstable();
        rejected.par_sort_unstable();
        interrupted.par_sort_unstable();
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.early.retire");
        }
        self.store.retire_in(
            &mut custody,
            accepted.par_iter().map(|&id| Place::JobInput(id)).collect(),
        )?;
        self.store.validate_commit(&custody)?;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.early.bookkeeping");
        }
        let mut work: Vec<_> = self.work.par_iter().cloned().collect();
        work.par_iter_mut().for_each(|row| {
            if accepted.binary_search(&row.job.id).is_ok() {
                row.phase = Phase::Waiting;
                row.job.phase = JobPhase::Waiting;
                row.job.reserved.clear();
            } else if interrupted.binary_search(&row.job.id).is_ok() {
                row.phase = Phase::Returning;
            }
        });
        let next_interval = WorkInterval {
            tick: interval.tick,
            custody_revision: interval.custody_revision,
            elapsed_millis: interval.elapsed_millis,
            rates: interval
                .rates
                .par_iter()
                .copied()
                .filter(|(id, _)| {
                    accepted.binary_search(id).is_err()
                        && interrupted.binary_search(id).is_err()
                        && rejected.binary_search(id).is_err()
                })
                .collect(),
        };
        #[cfg(feature = "profile")]
        drop(phase);
        Ok(Some(EarlyWorkDraft {
            epoch: self.epoch,
            custody,
            work,
            interval: next_interval,
            events: EarlyWorkEvents {
                tick: self.tick(),
                offset_millis: interval.elapsed_millis,
                finished: accepted,
                interrupted,
                rejected,
            },
        }))
    }
    pub fn validate_early_work(&self, draft: &EarlyWorkDraft) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if self.epoch != draft.epoch
            || interval.tick != draft.events.tick
            || interval.elapsed_millis != draft.events.offset_millis
        {
            return Err("stale early work draft".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        self.store.validate_commit(&draft.custody)
    }
    /// Commit one staged batch exactly once while keeping the interval open.
    pub fn commit_early_work(
        &mut self,
        mut draft: EarlyWorkDraft,
    ) -> Result<EarlyWorkEvents, String> {
        #[cfg(feature = "profile")]
        let _phase = game_diagnostics::span("physical.early.commit");
        self.validate_early_work(&draft)?;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        self.store.commit(draft.custody)?;
        draft.interval.custody_revision = self.store.revision();
        self.work = draft.work;
        self.work_interval = Some(draft.interval);
        self.epoch = epoch;
        Ok(draft.events)
    }
}
