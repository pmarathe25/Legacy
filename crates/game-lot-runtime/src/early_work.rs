//! Transactional publication of already completed work within an open quantum.
use crate::{Phase, PhysicalOutcomes, Runtime, Work, WorkInterval};
use game_lots::{Place, Transaction};
use game_production_model::JobPhase;
use game_production_model::Start;
use rayon::prelude::*;

fn renumber_operations(transactions: &mut [Transaction]) -> Result<(), String> {
    let counts: Vec<u64> = transactions
        .par_iter()
        .map(|row| {
            row.movements
                .len()
                .checked_add(row.creations.len())
                .and_then(|count| u64::try_from(count).ok())
                .ok_or("operation identity overflow".to_string())
        })
        .collect::<Result<_, _>>()?;
    let offsets = game_lot_allocation::prefixes(&counts);
    if offsets
        .last()
        .zip(counts.last())
        .is_some_and(|(&offset, &count)| offset + u128::from(count) > u128::from(u64::MAX))
    {
        return Err("operation identity overflow".into());
    }
    transactions
        .par_iter_mut()
        .zip(offsets.par_iter())
        .for_each(|(row, &offset)| {
            let start = offset as u64;
            let movements = row.movements.len() as u64;
            row.movements
                .iter_mut()
                .enumerate()
                .for_each(|(index, movement)| movement.id = start + index as u64 + 1);
            row.creations
                .par_iter_mut()
                .enumerate()
                .for_each(|(index, creation)| creation.id = start + movements + index as u64 + 1);
        });
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EarlyWorkEvents {
    pub tick: u64,
    pub offset_millis: u64,
    pub finished: Vec<u64>,
    pub delivered: Vec<u64>,
    pub interrupted: Vec<u64>,
    pub rejected: Vec<u64>,
}
pub struct EarlyWorkDraft {
    epoch: u64,
    custody: game_lots::Draft,
    work: Vec<Work>,
    interval: WorkInterval,
    events: EarlyWorkEvents,
    produced_items: Vec<crate::ProducedItem>,
    next_produced_item: Option<u64>,
}
/// Unpublished interval custody changes; callers must validate their own
/// effects before committing them at the same interval offset.
pub struct IntervalConsumptionDraft {
    epoch: u64,
    tick: u64,
    offset_millis: u64,
    custody: game_lots::Draft,
}
pub struct StorageExposureDraft {
    custody: game_lots::Draft,
}
impl IntervalConsumptionDraft {
    pub fn transactions(&self) -> &[game_lots::TransactionOutcome] {
        self.custody.transactions()
    }
    pub fn movements(&self) -> Vec<game_lots::Outcome> {
        self.custody.outcomes()
    }
    pub fn item_updates(&self) -> &[(u64, Option<Place>)] {
        self.custody.item_updates()
    }
}
impl EarlyWorkDraft {
    pub fn events(&self) -> &EarlyWorkEvents {
        &self.events
    }
    pub fn produced_items(&self) -> &[crate::ProducedItem] {
        &self.produced_items
    }
    pub fn next_produced_item(&self) -> Option<u64> {
        self.next_produced_item
    }
    pub fn item_updates(&self) -> &[(u64, Option<Place>)] {
        self.custody.item_updates()
    }
}
impl Runtime {
    /// Leave completed jobs for ordinary boundary settlement. Their inputs
    /// remain reserved and their workers remain occupied until publication.
    pub fn defer_interval_completions(&mut self, jobs: &[u64]) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis == 0
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("completion deferral requires a valid partial interval".into());
        }
        let mut ordered = jobs.to_vec();
        ordered.par_sort_unstable();
        if ordered.par_windows(2).any(|rows| rows[0] == rows[1])
            || ordered.par_iter().any(|id| {
                interval
                    .rates
                    .binary_search_by_key(id, |row| row.0)
                    .is_err()
                    || self
                        .work
                        .binary_search_by_key(id, |row| row.job.id)
                        .ok()
                        .is_none_or(|index| {
                            let row = &self.work[index];
                            row.phase != Phase::Working || row.job.remaining != 0
                        })
            })
        {
            return Err("invalid interval completion deferral".into());
        }
        if ordered.is_empty() {
            return Ok(());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let rates: Vec<_> = interval
            .rates
            .par_iter()
            .copied()
            .filter(|(id, _)| ordered.binary_search(id).is_err())
            .collect();
        self.work_interval.as_mut().expect("checked interval").rates = rates;
        self.epoch = epoch;
        Ok(())
    }
    /// Stage personal consumption without committing custody or physiology.
    pub fn prepare_interval_consumption(
        &self,
        actions: &[Transaction],
        seed: u64,
    ) -> Result<IntervalConsumptionDraft, String> {
        self.prepare_interval_consumption_at(actions, seed, false)
    }

    /// Stage food consumed in personal custody or at its regional stock.
    /// The host validates physical access for each person before planning it.
    pub fn prepare_interval_food_consumption(
        &self,
        actions: &[Transaction],
        seed: u64,
    ) -> Result<IntervalConsumptionDraft, String> {
        self.prepare_interval_consumption_at(actions, seed, true)
    }

    fn prepare_interval_consumption_at(
        &self,
        actions: &[Transaction],
        seed: u64,
        at_stock: bool,
    ) -> Result<IntervalConsumptionDraft, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("consumption requires a valid partial interval".into());
        }
        game_lot_work::validate_actions(&self.catalog, actions)?;
        if actions.par_iter().any(|action| {
            !action.creations.is_empty()
                || action.movements.iter().any(|movement| {
                    !(matches!(movement.from, Place::Person(_))
                        || (at_stock && matches!(movement.from, Place::Stock { .. })))
                        || movement.to.is_some()
                })
        }) {
            return Err("interval consumption requires personal custody".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let custody = self.store.prepare_transactions_now(seed, actions)?;
        if !custody.spoiled().is_empty() {
            return Err("interval consumption cannot publish pending spoilage".into());
        }
        Ok(IntervalConsumptionDraft {
            epoch: self.epoch,
            tick: interval.tick,
            offset_millis: interval.elapsed_millis,
            custody,
        })
    }

    /// Validate a staged interval custody draft before another owner publishes metadata.
    pub fn validate_interval_consumption(
        &self,
        draft: &IntervalConsumptionDraft,
    ) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if draft.epoch != self.epoch
            || interval.tick != draft.tick
            || interval.elapsed_millis != draft.offset_millis
            || interval.custody_revision != self.store.revision()
        {
            return Err("stale interval consumption draft".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        self.store.validate_commit(&draft.custody)?;
        Ok(())
    }

    /// Add local storage and its climate as part of an already staged physical action.
    pub fn configure_interval_storage(
        &self,
        mut draft: IntervalConsumptionDraft,
        holders: Vec<game_lots::Container>,
        temperatures: &[(Place, Option<i16>)],
    ) -> Result<IntervalConsumptionDraft, String> {
        self.validate_interval_consumption(&draft)?;
        if holders.par_iter().any(|row| !matches!(row.place, Place::Inventory(_)))
            || temperatures.par_iter().any(|(place, _)| !matches!(place, Place::Inventory(_)))
        {
            return Err("local storage requires inventory custody".into());
        }
        self.store.add_empty_in(&mut draft.custody, holders)?;
        self.store.set_holder_temperatures_in(&mut draft.custody, temperatures)?;
        Ok(draft)
    }

    /// Retire emptied site custody in the same commit as the material transfer.
    pub fn retire_interval_site_custody(
        &self,
        draft: &mut IntervalConsumptionDraft,
        places: Vec<Place>,
    ) -> Result<(), String> {
        self.validate_interval_consumption(draft)?;
        if places
            .par_iter()
            .any(|place| !matches!(place, Place::Site(_)))
        {
            return Err("invalid site custody retirement".into());
        }
        self.store.retire_in(&mut draft.custody, places)
    }

    /// Commit a staged consumption or transfer if the generation and time have not changed.
    pub fn commit_interval_consumption(
        &mut self,
        draft: IntervalConsumptionDraft,
    ) -> Result<PhysicalOutcomes, String> {
        self.validate_interval_consumption(&draft)?;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let transactions = draft.custody.transactions().par_iter().copied().collect();
        let (movements, spoiled) = self.store.commit(draft.custody)?;
        self.work_interval
            .as_mut()
            .expect("checked interval")
            .custody_revision = self.store.revision();
        self.epoch = epoch;
        Ok(PhysicalOutcomes {
            transactions,
            movements,
            spoiled,
        })
    }
    /// Commit physical transfers at an interval offset after the host has
    /// checked actors and locations. Job custody remains with the work owner.
    pub fn transfer_interval(
        &mut self,
        actions: &[Transaction],
        seed: u64,
    ) -> Result<PhysicalOutcomes, String> {
        if self
            .work_interval
            .as_ref()
            .is_some_and(|interval| interval.elapsed_millis == 0)
        {
            return Err("transfer requires a valid partial interval".into());
        }
        if actions.is_empty() {
            let interval = self
                .work_interval
                .as_ref()
                .ok_or("no physical work interval")?;
            if interval.tick != self.tick()
                || interval.custody_revision != self.store.revision()
                || interval.elapsed_millis == 0
                || interval.elapsed_millis >= self.tick_millis
            {
                return Err("transfer requires a valid partial interval".into());
            }
            return Ok(PhysicalOutcomes {
                transactions: vec![],
                movements: vec![],
                spoiled: vec![],
            });
        }
        let draft = self.prepare_interval_transfer(actions, seed)?;
        self.commit_interval_consumption(draft)
    }

    /// Stage custody transfers, including at interval opening, for coordination
    /// with other owners. The host must validate actors and locations.
    pub fn prepare_interval_transfer(
        &self,
        actions: &[Transaction],
        seed: u64,
    ) -> Result<IntervalConsumptionDraft, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("transfer requires a valid partial interval".into());
        }
        game_lot_work::validate_actions(&self.catalog, actions)?;
        if actions.par_iter().any(|action| {
            !action.creations.is_empty()
                || action.movements.iter().any(|movement| {
                    matches!(movement.from, Place::JobInput(_) | Place::JobOutput(_))
                        || movement.to.is_none()
                })
        }) {
            return Err("interval transfers cannot consume goods or move job custody".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let flat: Option<Vec<_>> = actions
            .par_iter()
            .map(|action| {
                (action.movements.len() == 1).then(|| game_lots::SimpleTransfer {
                    action: action.id,
                    movement: action.movements[0],
                })
            })
            .collect();
        let fast = if let Some(flat) = flat {
            self.store.prepare_partitioned_flat(seed, &flat, false)?
        } else {
            None
        };
        let custody = match fast {
            Some(custody) => custody,
            None => self.store.prepare_transactions_now(seed, actions)?,
        };
        if !custody.spoiled().is_empty() {
            return Err("interval transfers cannot publish pending spoilage".into());
        }
        Ok(IntervalConsumptionDraft {
            epoch: self.epoch,
            tick: interval.tick,
            offset_millis: interval.elapsed_millis,
            custody,
        })
    }
    /// Stage construction, including conditional custody for retained site material.
    pub fn prepare_interval_construction(
        &self,
        actions: &[Transaction],
        containers: &[(u64, game_lots::Container)],
        seed: u64,
    ) -> Result<IntervalConsumptionDraft, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("construction requires a valid partial interval".into());
        }
        let mut sites: Vec<_> = containers
            .par_iter()
            .map(|(action, container)| (*action, container.place))
            .collect();
        if sites.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
            sites.par_sort_unstable_by_key(|row| row.0);
        }
        if sites.par_windows(2).any(|pair| pair[0].0 == pair[1].0)
            || sites
                .par_iter()
                .any(|(_, place)| !matches!(place, Place::Site(_)))
            || actions.par_iter().any(|action| {
                !action.creations.is_empty()
                    || action.movements.iter().any(|movement| {
                        !matches!(movement.from, Place::Person(_) | Place::Stock { .. })
                            || movement.to.is_some_and(|place| {
                                sites
                                    .binary_search_by_key(&action.id, |row| row.0)
                                    .ok()
                                    .is_none_or(|index| sites[index].1 != place)
                            })
                    })
            })
        {
            return Err("invalid construction custody".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let mut actions: Vec<_> = actions.par_iter().cloned().collect();
        if actions.par_windows(2).any(|rows| rows[0].id > rows[1].id) {
            actions.par_sort_unstable_by_key(|action| action.id);
        }
        renumber_operations(&mut actions)?;
        game_lot_work::validate_actions(&self.catalog, &actions)?;
        if sites.par_iter().any(|(id, place)| {
            actions
                .binary_search_by_key(id, |action| action.id)
                .ok()
                .is_none_or(|index| {
                    !actions[index]
                        .movements
                        .iter()
                        .any(|movement| movement.to == Some(*place))
                })
        }) {
            return Err("construction custody has no matching movement".into());
        }
        let flat: Option<Vec<_>> = if !actions.is_empty() && actions.len() == containers.len() {
            actions
                .par_iter()
                .map(|action| {
                    (action.movements.len() == 1 && action.movements[0].to.is_some()).then(|| {
                        game_lots::SimpleTransfer {
                            action: action.id,
                            movement: action.movements[0],
                        }
                    })
                })
                .collect()
        } else {
            None
        };
        let fast = if let Some(flat) = flat {
            let additions = containers
                .par_iter()
                .map(|(_, container)| container.clone())
                .collect();
            self.store
                .prepare_fan_out_flat_with_empty(seed, &flat, additions, false)?
        } else {
            None
        };
        let mut custody = match fast {
            Some(custody) => custody,
            None => {
                let additions = containers
                    .par_iter()
                    .map(|(_, container)| container.clone())
                    .collect();
                self.store
                    .prepare_with_empty(seed, &actions, additions, false)?
            }
        };
        let mut outcomes: Vec<_> = custody.transactions().par_iter().copied().collect();
        if outcomes.par_windows(2).any(|rows| rows[0].id > rows[1].id) {
            outcomes.par_sort_unstable_by_key(|row| row.id);
        }
        let retire: Vec<_> = sites
            .par_iter()
            .filter(|(id, _)| {
                outcomes
                    .binary_search_by_key(id, |row| row.id)
                    .ok()
                    .is_none_or(|index| !outcomes[index].accepted)
            })
            .map(|(_, place)| *place)
            .collect();
        self.store.retire_in(&mut custody, retire)?;
        if !custody.spoiled().is_empty() {
            return Err("interval construction cannot publish pending spoilage".into());
        }
        Ok(IntervalConsumptionDraft {
            epoch: self.epoch,
            tick: interval.tick,
            offset_millis: interval.elapsed_millis,
            custody,
        })
    }

    /// Stage personal yields from domain-validated withdrawals from resource nodes.
    pub fn prepare_interval_gathering(
        &self,
        outputs: &[crate::Gathered],
        seed: u64,
    ) -> Result<IntervalConsumptionDraft, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("invalid interval for gathering".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let actions = game_lot_content::prepare_gathered(
            &self.catalog,
            outputs,
            self.lot_instant(interval.tick),
        )?;
        let custody = self.store.prepare_transactions_now(seed, &actions)?;
        if !custody.spoiled().is_empty() {
            return Err("interval gathering cannot publish pending spoilage".into());
        }
        Ok(IntervalConsumptionDraft {
            epoch: self.epoch,
            tick: interval.tick,
            offset_millis: interval.elapsed_millis,
            custody,
        })
    }

    /// Stage seed withdrawals and harvest outputs at an open interval instant.
    pub fn prepare_interval_cultivation(
        &self,
        mut actions: Vec<Transaction>,
        seed: u64,
    ) -> Result<(IntervalConsumptionDraft, crate::Events), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("invalid interval for cultivation".into());
        }
        let instant = self.lot_instant(interval.tick);
        actions
            .par_iter()
            .try_for_each(|action| match &action.movements[..] {
                [] => game_lot_content::validate_growth(
                    &self.catalog,
                    std::slice::from_ref(action),
                    instant,
                ),
                [seed]
                    if (matches!(seed.from, Place::Stock { good, .. } if good == seed.good)
                        || (matches!(seed.from, Place::Person(_)) && seed.batch.is_some()))
                        && seed.to.is_none() =>
                {
                    game_lot_work::validate_actions(&self.catalog, std::slice::from_ref(action))
                }
                _ => Err("invalid cultivation custody".into()),
            })?;
        renumber_operations(&mut actions)?;
        let mut custody = self.store.prepare_transactions_now(seed, &actions)?;
        if !custody.spoiled().is_empty() {
            return Err("interval cultivation cannot publish pending spoilage".into());
        }
        let (movements, _) = custody.take_outcome_batch();
        let (headers, arenas) = movements.into_parts();
        let requests: Vec<_> = actions
            .par_iter()
            .filter_map(|action| action.movements.first().map(|row| (action.id, *row)))
            .collect();
        let receipts = headers
            .into_par_iter()
            .map(|movement| {
                let i = requests
                    .binary_search_by_key(&movement.id, |row| row.1.id)
                    .map_err(|_| "missing cultivation receipt request")?;
                let (action, request) = requests[i];
                Ok(crate::ExternalReceiptHeader {
                    action,
                    request,
                    movement,
                })
            })
            .collect::<Result<_, String>>()?;
        let mut physical_movements = crate::ExternalReceiptBatch::from_parts(receipts, arenas);
        physical_movements.sort_by_action();
        let events = crate::Events {
            tick: interval.tick,
            started: Vec::new(),
            start_denied: Vec::new(),
            abandoned: Vec::new(),
            suspended: Vec::new(),
            adopted: Vec::new(),
            finished: Vec::new(),
            delivered: Vec::new(),
            output_lost: Vec::new(),
            interrupted: Vec::new(),
            returned: Vec::new(),
            empty_returns: Vec::new(),
            spoiled: Vec::new(),
            physical_actions: custody.transactions().to_vec(),
            physical_movements,
        };
        Ok((
            IntervalConsumptionDraft {
                epoch: self.epoch,
                tick: interval.tick,
                offset_millis: interval.elapsed_millis,
                custody,
            },
            events,
        ))
    }

    /// Stage identified written objects and consume the writer's chosen materials atomically.
    pub fn prepare_interval_writing(
        &self,
        pages: &[crate::WritingPage],
        seed: u64,
    ) -> Result<IntervalConsumptionDraft, String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("invalid interval for writing".into());
        }
        self.epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let mut actions = game_lot_content::prepare_writing(
            &self.catalog,
            pages,
            self.lot_instant(interval.tick),
        )?;
        renumber_operations(&mut actions)?;
        let custody = self.store.prepare_transactions_now(seed, &actions)?;
        if !custody.spoiled().is_empty() {
            return Err("interval writing cannot publish pending spoilage".into());
        }
        Ok(IntervalConsumptionDraft {
            epoch: self.epoch,
            tick: interval.tick,
            offset_millis: interval.elapsed_millis,
            custody,
        })
    }

    /// Stop chosen jobs at this offset. Their surviving inputs remain in job
    /// custody until the normal boundary return, even if that return is blocked.
    pub fn abandon_interval_work(&mut self, requests: &[(u64, u64)]) -> Result<(), String> {
        self.abandon_interval_work_with_exposure(requests, None)
    }

    pub fn prepare_storage_exposure(
        &self,
        holders: &[(Place, Option<i16>)],
    ) -> Result<Option<StorageExposureDraft>, String> {
        if holders.is_empty() {
            return Ok(None);
        }
        let custody = self.store.prepare_holder_temperatures(holders)?;
        Ok(Some(StorageExposureDraft { custody }))
    }

    /// Stop work and change the storage affected by the same physical event.
    pub fn abandon_interval_work_with_exposure(
        &mut self,
        requests: &[(u64, u64)],
        exposure: Option<StorageExposureDraft>,
    ) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("work abandonment requires a valid partial interval".into());
        }
        let mut ordered = requests.to_vec();
        ordered.par_sort_unstable_by_key(|row| row.0);
        if ordered.par_windows(2).any(|rows| rows[0].0 == rows[1].0)
            || ordered.par_iter().any(|&(job, worker)| {
                self.work
                    .binary_search_by_key(&job, |row| row.job.id)
                    .ok()
                    .is_none_or(|index| {
                        let row = &self.work[index];
                        row.phase != Phase::Working
                            || row.job.worker != worker
                            || row.job.remaining == 0
                    })
            })
        {
            return Err("invalid interval work abandonment".into());
        }
        if ordered.is_empty() && exposure.is_none() {
            return Ok(());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let rates = (!ordered.is_empty()).then(|| {
            interval.rates.par_iter().copied()
                .filter(|(job, _)| ordered.binary_search_by_key(job, |row| row.0).is_err())
                .collect()
        });
        if let Some(exposure) = exposure {
            self.store.commit(exposure.custody)?;
        }
        let interval = self.work_interval.as_mut().expect("checked interval");
        if let Some(rates) = rates {
            self.work.par_iter_mut().for_each(|row| {
                if ordered.binary_search_by_key(&row.job.id, |request| request.0).is_ok() {
                    row.phase = Phase::Returning;
                }
            });
            interval.rates = rates;
        }
        interval.custody_revision = self.store.revision();
        self.epoch = epoch;
        Ok(())
    }
    /// Preserve an unfinished job and its inputs for a personally chosen successor.
    pub fn suspend_interval_work(&mut self, requests: &[crate::Suspension]) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("invalid interval for work suspension".into());
        }
        let mut ordered = requests.to_vec();
        ordered.par_sort_unstable_by_key(|row| row.job);
        if ordered
            .par_windows(2)
            .any(|rows| rows[0].job == rows[1].job)
            || ordered.par_iter().any(|request| {
                request.until <= self.tick().saturating_add(1)
                    || self
                        .work
                        .binary_search_by_key(&request.job, |row| row.job.id)
                        .ok()
                        .is_none_or(|i| {
                            let work = &self.work[i];
                            work.phase != Phase::Working
                                || work.job.worker != request.worker
                                || work.job.remaining == 0
                        })
            })
        {
            return Err("invalid interval work suspension".into());
        }
        if ordered.is_empty() {
            return Ok(());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let rates: Vec<_> = interval
            .rates
            .par_iter()
            .copied()
            .filter(|(job, _)| ordered.binary_search_by_key(job, |row| row.job).is_err())
            .collect();
        self.work.par_iter_mut().for_each(|work| {
            if let Ok(i) = ordered.binary_search_by_key(&work.job.id, |row| row.job) {
                work.phase = Phase::Suspended {
                    until: ordered[i].until,
                };
            }
        });
        self.work_interval.as_mut().expect("checked interval").rates = rates;
        self.epoch = epoch;
        Ok(())
    }
    /// Admit independent successors without recreating or transferring reserved inputs.
    pub fn adopt_interval_work(
        &mut self,
        requests: &[(crate::Adoption, u64)],
    ) -> Result<(), String> {
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
        {
            return Err("invalid interval for work adoption".into());
        }
        let mut ordered = requests.to_vec();
        ordered.par_sort_unstable_by_key(|row| row.0.job);
        let mut workers: Vec<_> = ordered.par_iter().map(|row| row.0.worker).collect();
        workers.par_sort_unstable();
        let mut busy: Vec<_> = self
            .work
            .par_iter()
            .filter(|row| row.phase == Phase::Working)
            .map(|row| row.job.worker)
            .collect();
        busy.par_sort_unstable();
        if ordered.par_windows(2).any(|rows| rows[0].0.job == rows[1].0.job)
            || workers.par_windows(2).any(|rows| rows[0] == rows[1])
            || ordered.par_iter().any(|(request,rate)| *rate == 0 || request.worker == 0 || request.worker == request.previous_worker
                || busy.binary_search(&request.worker).is_ok()
                || self.work.binary_search_by_key(&request.job, |row| row.job.id).ok().is_none_or(|i| {
                    let work = &self.work[i]; work.job.remaining == 0 || work.job.worker != request.previous_worker || !matches!(work.phase, Phase::Suspended { until } if until > self.tick())
                })) { return Err("invalid interval work adoption".into()); }
        if ordered.is_empty() {
            return Ok(());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let mut rates = interval.rates.clone();
        rates.par_extend(ordered.par_iter().map(|(row, rate)| (row.job, *rate)));
        rates.par_sort_unstable_by_key(|row| row.0);
        if rates.par_windows(2).any(|rows| rows[0].0 == rows[1].0) {
            return Err("duplicate interval work rate".into());
        }
        self.work.par_iter_mut().for_each(|work| {
            if let Ok(i) = ordered.binary_search_by_key(&work.job.id, |row| row.0.job) {
                work.job.worker = ordered[i].0.worker;
                work.phase = Phase::Working;
            }
        });
        self.work_interval.as_mut().expect("checked interval").rates = rates;
        self.epoch = epoch;
        Ok(())
    }
    /// Reserve new physical work inside a partial interval after the host has
    /// admitted its actors. Rejected reservations leave no custody changes.
    pub fn start_interval_work(
        &mut self,
        requests: &[(Start, u64)],
        seed: u64,
    ) -> Result<Vec<(u64, bool)>, String> {
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("physical.interval_start.validate");
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis >= self.tick_millis
            || requests.par_iter().any(|(_, rate)| *rate == 0)
        {
            return Err("work start requires a valid partial interval and positive rates".into());
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("physical work revision overflow")?;
        let starts: Vec<_> = requests.par_iter().map(|(start, _)| *start).collect();
        game_lot_work::validate_starts(&self.work, &starts)?;
        let admitted = self.starts_with_storage(&starts);
        let physical_starts = admitted.as_deref().unwrap_or(&starts);
        if physical_starts.is_empty() {
            let mut results = Vec::new();
            Self::include_unavailable_starts(&mut results, &starts);
            return Ok(results);
        }
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.interval_start.plan");
        }
        let flat = game_lot_work::start_flat_with_capacity(
            physical_starts,
            &self.recipes,
            self.store.footprints(),
        )?;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.interval_start.custody");
        }
        let fast = if let Some(plan) = flat {
            self.store
                .prepare_fan_out_flat_with_empty(seed, &plan.transfers, plan.containers, false)?
                .map(|custody| (plan.jobs, custody))
        } else {
            None
        };
        let (jobs, mut custody) = if let Some(fast) = fast {
            fast
        } else {
            let plan = game_lot_work::start_with_capacity(
                physical_starts,
                &self.recipes,
                self.store.footprints(),
            )?;
            let custody =
                self.store
                    .prepare_with_empty(seed, &plan.transactions, plan.containers, false)?;
            (plan.jobs, custody)
        };
        let (_, spoiled) = custody.take_outcome_batch();
        if !spoiled.is_empty() {
            return Err("interval work cannot publish pending spoilage".into());
        }
        let mut results: Vec<_> = custody
            .transactions()
            .iter()
            .map(|row| (row.id, row.accepted))
            .collect();
        if admitted.is_some() {
            Self::include_unavailable_starts(&mut results, &starts);
        }
        if results.iter().all(|row| !row.1) {
            return Ok(results);
        }
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.interval_start.bookkeeping");
        }
        let accepted = |id| {
            results
                .binary_search_by_key(&id, |row| row.0)
                .is_ok_and(|index| results[index].1)
        };
        let retire: Vec<_> = jobs
            .par_iter()
            .filter(|job| !accepted(job.id))
            .flat_map_iter(|job| [Place::JobInput(job.id), Place::JobOutput(job.id)])
            .collect();
        self.store.retire_in(&mut custody, retire)?;
        self.store.validate_commit(&custody)?;
        let mut work: Vec<_> = self.work.par_iter().cloned().collect();
        work.par_extend(
            jobs.into_par_iter()
                .filter(|job| accepted(job.id))
                .map(|job| Work {
                    job,
                    phase: Phase::Working,
                    progress_remainder: 0,
                }),
        );
        work.par_sort_unstable_by_key(|row| row.job.id);
        let mut rates = interval.rates.clone();
        rates.par_extend(
            requests
                .par_iter()
                .filter(|(start, _)| accepted(start.id))
                .map(|(start, rate)| (start.id, *rate)),
        );
        rates.par_sort_unstable_by_key(|row| row.0);
        let mut next_interval = interval.clone();
        next_interval.rates = rates;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.interval_start.commit");
        }
        self.store.commit(custody)?;
        next_interval.custody_revision = self.store.revision();
        self.work = work;
        self.work_interval = Some(next_interval);
        self.epoch = epoch;
        #[cfg(feature = "profile")]
        drop(phase);
        Ok(results)
    }
    /// Stage only already completed, admitted work without advancing the tick.
    /// Perishables use the actual completion instant in elapsed custody.
    /// Callers must exclude jobs whose outputs require separately owned item
    /// identities: this physical-only operation creates fungible lots.
    pub fn prepare_early_work(&self, seed: u64) -> Result<Option<EarlyWorkDraft>, String> {
        self.prepare_early_work_with_identified(seed, None)
    }
    /// Stage early output, optionally allocating identified
    /// goods for a separately staged item owner.
    pub fn prepare_early_work_with_identified(
        &self,
        seed: u64,
        identified: Option<crate::IdentifiedProduction<'_>>,
    ) -> Result<Option<EarlyWorkDraft>, String> {
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("physical.early.select");
        let interval = self
            .work_interval
            .as_ref()
            .ok_or("no physical work interval")?;
        if interval.tick != self.tick()
            || interval.custody_revision != self.store.revision()
            || interval.elapsed_millis == 0
            || interval.elapsed_millis > self.tick_millis
            || (interval.elapsed_millis == self.tick_millis && self.elapsed_expiry_time().is_none())
        {
            return Err("early work requires a valid elapsed interval".into());
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
        let mut finished = game_lot_work::finish_with_capacity(
            &rows,
            &self.catalog,
            self.lot_instant(self.tick()),
            self.store.footprints(),
        )?;
        if self.elapsed_expiry_time().is_none()
            && finished.transactions.par_iter().any(|transaction| {
                transaction
                    .creations
                    .iter()
                    .any(|creation| creation.lot.expires_at.is_some())
            })
        {
            return Err("early perishable production is unsupported".into());
        }
        let identified_plan = identified
            .map(|config| {
                crate::identified_output::prepare(
                    &self.store,
                    &mut finished.transactions,
                    &[],
                    config,
                )
            })
            .transpose()?;
        if identified_plan
            .as_ref()
            .is_some_and(|plan| !plan.items.is_empty())
        {
            renumber_operations(&mut finished.transactions)?;
        }
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.early.custody");
        }
        let (mut custody, delivered) = self.stage_finished_custody(finished.transactions, seed)?;
        let produced_items = identified_plan.as_ref().map_or_else(Vec::new, |plan| {
            plan.items
                .par_iter()
                .filter(|item| {
                    custody
                        .item_updates()
                        .binary_search_by_key(&item.item, |row| row.0)
                        .ok()
                        .is_some_and(|index| custody.item_updates()[index].1.is_some())
                })
                .copied()
                .collect()
        });
        let next_produced_item = identified_plan.map(|plan| plan.next_item);
        // The elapsed owner publishes expiry before work at the same instant.
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
            accepted
                .par_iter()
                .map(|&id| Place::JobInput(id))
                .chain(delivered.par_iter().map(|&id| Place::JobOutput(id)))
                .collect(),
        )?;
        self.store.validate_commit(&custody)?;
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("physical.early.bookkeeping");
        }
        let mut work: Vec<_> = self
            .work
            .par_iter()
            .filter(|row| delivered.binary_search(&row.job.id).is_err())
            .cloned()
            .collect();
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
            produced_items,
            next_produced_item,
            events: EarlyWorkEvents {
                tick: self.tick(),
                offset_millis: interval.elapsed_millis,
                finished: accepted,
                delivered,
                interrupted,
                rejected,
            },
        }))
    }
    fn stage_finished_custody(
        &self,
        mut transactions: Vec<game_lots::Transaction>,
        seed: u64,
    ) -> Result<(game_lots::Draft, Vec<u64>), String> {
        if self.elapsed_expiry_time().is_none() || transactions.is_empty() {
            return Ok((
                self.store.prepare_transactions_now(seed, &transactions)?,
                Vec::new(),
            ));
        }
        let mut direct: Vec<_> = transactions
            .par_iter_mut()
            .filter_map(|transaction| {
                let work = self
                    .work
                    .binary_search_by_key(&transaction.id, |row| row.job.id)
                    .ok()?;
                let region = self.work[work].job.region;
                let stock = |good| Place::Stock { region, good };
                if transaction
                    .creations
                    .iter()
                    .any(|output| self.store.container(stock(output.lot.good)).is_none())
                {
                    return None;
                }
                for output in &mut transaction.creations {
                    output.to = stock(output.lot.good);
                }
                Some(transaction.id)
            })
            .collect();
        direct.par_sort_unstable();
        let mut custody = self.store.prepare_transactions_now(seed, &transactions)?;
        let denied: Vec<_> = custody
            .transactions()
            .par_iter()
            .filter_map(|result| {
                (!result.accepted && direct.binary_search(&result.id).is_ok()).then_some(result.id)
            })
            .collect();
        if !denied.is_empty() {
            transactions
                .par_iter_mut()
                .filter(|row| denied.binary_search(&row.id).is_ok())
                .for_each(|transaction| {
                    for output in &mut transaction.creations {
                        output.to = Place::JobOutput(transaction.id);
                    }
                });
            direct.retain(|id| denied.binary_search(id).is_err());
            custody = self.store.prepare_transactions_now(seed, &transactions)?;
            direct.retain(|id| {
                custody
                    .transactions()
                    .binary_search_by_key(id, |result| result.id)
                    .ok()
                    .is_some_and(|index| custody.transactions()[index].accepted)
            });
        }
        Ok((custody, direct))
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
