//! Progress, physical completion, loss and returns at one shared boundary.
use super::{ExternalReceiptBatch, ExternalReceiptHeader, Growth, Phase, Runtime, Work};
use game_lots::{Place, PlaceIndex, SimpleTransfer, Spoilage, Transaction, TransactionOutcome};
use game_production_model::{JobPhase, Start};
use rayon::prelude::*;

/// An outgoing lead leaves physical inputs with the job for a bounded period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Suspension {
    pub job: u64,
    pub worker: u64,
    pub until: u64,
}
/// A new lead independently takes over one committed suspended job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adoption {
    pub job: u64,
    pub previous_worker: u64,
    pub worker: u64,
}
#[derive(Default)]
pub struct WorkTransitions<'a> {
    pub suspend: &'a [Suspension],
    pub adopt: &'a [Adoption],
}

#[derive(Debug, PartialEq, Eq)]
pub struct Events {
    pub tick: u64,
    pub started: Vec<u64>,
    pub start_denied: Vec<u64>,
    pub abandoned: Vec<u64>,
    pub suspended: Vec<u64>,
    pub adopted: Vec<Adoption>,
    pub finished: Vec<u64>,
    pub delivered: Vec<u64>,
    pub output_lost: Vec<u64>,
    pub interrupted: Vec<u64>,
    pub returned: Vec<u64>,
    pub empty_returns: Vec<u64>,
    pub spoiled: Vec<Spoilage>,
    pub physical_actions: Vec<TransactionOutcome>,
    pub physical_movements: ExternalReceiptBatch,
}
pub struct AdvanceDraft {
    epoch: u64,
    custody: game_lots::Draft,
    work: Vec<Work>,
    events: Events,
    produced_items: Vec<crate::ProducedItem>,
    next_produced_item: Option<u64>,
}
impl AdvanceDraft {
    pub fn events(&self) -> &Events {
        &self.events
    }
    /// Work in this detached generation, ordered by job identity.
    pub fn work(&self) -> &[Work] {
        &self.work
    }
    /// Staged identified-item custody changes, independent of receipt retention.
    pub fn item_updates(&self) -> &[(u64, Option<Place>)] {
        self.custody.item_updates()
    }
    pub fn produced_items(&self) -> &[crate::ProducedItem] {
        &self.produced_items
    }
    pub fn next_produced_item(&self) -> Option<u64> {
        self.next_produced_item
    }
}
impl Runtime {
    /// Preserve eligible unfinished work when its worker becomes incapacitated.
    /// Eligibility is decided by the site owner; completed work is never reopened.
    pub fn suspend_workers_in(
        &self,
        draft: &mut AdvanceDraft,
        workers: &[u64],
        eligible_jobs: &[u64],
        until: u64,
    ) -> Result<(), String> {
        if workers.is_empty() || eligible_jobs.is_empty() {
            return Ok(());
        }
        self.validate_advance(draft)?;
        if until <= draft.events.tick
            || workers.par_windows(2).any(|pair| pair[0] >= pair[1])
            || eligible_jobs.par_windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err("invalid incapacitated work suspension".into());
        }
        let suspended: Vec<_> = draft
            .work
            .par_iter_mut()
            .filter_map(|row| {
                if row.phase == Phase::Working
                    && row.job.remaining > 0
                    && workers.binary_search(&row.job.worker).is_ok()
                    && eligible_jobs.binary_search(&row.job.id).is_ok()
                {
                    row.phase = Phase::Suspended { until };
                    Some(row.job.id)
                } else {
                    None
                }
            })
            .collect();
        draft.events.suspended.par_extend(suspended);
        draft.events.suspended.par_sort_unstable();
        draft.events.suspended.dedup();
        Ok(())
    }

    /// Interrupt the specified jobs when their physical sites are destroyed.
    pub fn interrupt_jobs_in(&self, draft: &mut AdvanceDraft, jobs: &[u64]) -> Result<(), String> {
        if jobs.is_empty() {
            return Ok(());
        }
        self.validate_advance(draft)?;
        if jobs.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("job ids must be sorted and unique".into());
        }
        let interrupted: Vec<_> = draft
            .work
            .par_iter_mut()
            .filter_map(|row| {
                if matches!(row.phase, Phase::Working | Phase::Suspended { .. })
                    && jobs.binary_search(&row.job.id).is_ok()
                {
                    row.phase = Phase::Returning;
                    Some(row.job.id)
                } else {
                    None
                }
            })
            .collect();
        draft.events.interrupted.par_extend(interrupted);
        draft.events.interrupted.par_sort_unstable();
        draft.events.interrupted.dedup();
        Ok(())
    }
    /// Interrupt active work in a detached generation after its worker dies.
    /// Worker IDs must be strictly increasing.
    /// Reserved material remains in its physical holder until the usual return succeeds.
    pub fn interrupt_workers_in(
        &self,
        draft: &mut AdvanceDraft,
        workers: &[u64],
    ) -> Result<(), String> {
        if workers.is_empty() {
            return Ok(());
        }
        if workers.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("worker ids must be sorted and unique".into());
        }
        self.validate_advance(draft)?;
        let interrupted: Vec<_> = draft
            .work
            .par_iter_mut()
            .filter_map(|row| {
                if row.phase == Phase::Working && workers.binary_search(&row.job.worker).is_ok() {
                    row.phase = Phase::Returning;
                    Some(row.job.id)
                } else {
                    None
                }
            })
            .collect();
        draft.events.interrupted.par_extend(interrupted);
        draft.events.interrupted.par_sort_unstable();
        draft.events.interrupted.dedup();
        Ok(())
    }
    /// Register newborn holders in the detached physical generation.
    pub fn add_people_in(
        &self,
        draft: &mut AdvanceDraft,
        people: &[(u64, u64)],
    ) -> Result<(), String> {
        self.validate_advance(draft)?;
        let containers = people
            .par_iter()
            .map(|&(id, capacity)| game_lots::Container::new(Place::Person(id), capacity, vec![]))
            .collect::<Result<Vec<_>, _>>()?;
        self.store.add_empty_in(&mut draft.custody, containers)
    }
    /// Retire empty non-job custody within the same detached generation.
    pub fn retire_empty_in(
        &self,
        draft: &mut AdvanceDraft,
        places: Vec<Place>,
    ) -> Result<(), String> {
        self.validate_advance(draft)?;
        if places
            .par_iter()
            .any(|place| !matches!(place, Place::Site(_)))
        {
            return Err("invalid site custody retirement".into());
        }
        self.store.retire_in(&mut draft.custody, places)
    }
    /// Work rates are voluntary actor decisions made by the enclosing simulation.
    pub fn advance(&mut self, rates: &[(u64, u64)], seed: u64) -> Result<Events, String> {
        self.advance_with_actions(rates, &[], seed)
    }
    /// Stage physical actions alongside work against the same boundary snapshot.
    pub fn advance_with_actions(
        &mut self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        seed: u64,
    ) -> Result<Events, String> {
        let draft = self.prepare_advance(rates, external, seed)?;
        self.commit_advance(draft)
    }
    /// Attempt a flat batch when no physical work is active. Contested or
    /// unsupported movements return `None` for the general transaction path.
    pub fn prepare_flat_transfers(
        &self,
        transfers: &[SimpleTransfer],
    ) -> Result<Option<AdvanceDraft>, String> {
        self.ensure_no_work_interval()?;
        if !self.work.is_empty() {
            return Ok(None);
        }
        if transfers.par_iter().any(|row| {
            !self.catalog.contains(row.movement.good)
                || row
                    .movement
                    .to
                    .is_some_and(|place| matches!(place, Place::JobInput(_) | Place::JobOutput(_)))
        }) {
            return Err("invalid external physical action".into());
        }
        let Some(mut custody) = self.store.prepare_flat_transfers(transfers, true)? else {
            return Ok(None);
        };
        let tick = self.store.tick().checked_add(1).ok_or("tick overflow")?;
        let physical_actions = custody.transactions().par_iter().copied().collect();
        let (outcomes, spoiled) = custody.take_outcome_batch();
        let (headers, arenas) = outcomes.into_parts();
        let aligned = headers.len() == transfers.len()
            && headers
                .par_iter()
                .zip(transfers.par_iter())
                .all(|(header, transfer)| header.id == transfer.movement.id);
        let headers: Vec<ExternalReceiptHeader> = if aligned {
            headers
                .into_par_iter()
                .zip(transfers.par_iter())
                .map(|(movement, transfer)| ExternalReceiptHeader {
                    action: transfer.action,
                    request: transfer.movement,
                    movement,
                })
                .collect()
        } else {
            let mut keys: Vec<_> = transfers
                .par_iter()
                .map(|row| (row.movement.id, row.action, row.movement))
                .collect();
            if keys.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
                keys.par_sort_unstable_by_key(|row| row.0);
            }
            headers
                .into_par_iter()
                .chunks(2048)
                .flat_map_iter(|chunk| {
                    let mut cursor = keys.partition_point(|row| row.0 < chunk[0].id);
                    let keys = &keys;
                    chunk.into_iter().filter_map(move |movement| {
                        while cursor < keys.len() && keys[cursor].0 < movement.id {
                            cursor += 1;
                        }
                        let &(_, action, request) =
                            keys.get(cursor).filter(|row| row.0 == movement.id)?;
                        cursor += 1;
                        Some(ExternalReceiptHeader {
                            action,
                            request,
                            movement,
                        })
                    })
                })
                .collect()
        };
        let mut physical_movements = ExternalReceiptBatch::from_parts(headers, arenas);
        physical_movements.sort_by_action();
        Ok(Some(AdvanceDraft {
            epoch: self.epoch,
            custody,
            work: Vec::new(),
            produced_items: Vec::new(),
            next_produced_item: None,
            events: Events {
                tick,
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
                spoiled,
                physical_actions,
                physical_movements,
            },
        }))
    }
    /// Resolve flat transfers with a shared endpoint by preserving the same
    /// composed action identities used by the general arbiter.
    pub fn prepare_partitioned_flat_transfers(
        &self,
        transfers: &[SimpleTransfer],
        seed: u64,
    ) -> Result<Option<AdvanceDraft>, String> {
        self.ensure_no_work_interval()?;
        if !self.work.is_empty() {
            return Ok(None);
        }
        if transfers.len() < 256 {
            return self.prepare_flat_transfers(transfers);
        }
        if transfers.par_iter().any(|row| {
            !self.catalog.contains(row.movement.good)
                || row
                    .movement
                    .to
                    .is_some_and(|place| matches!(place, Place::JobInput(_) | Place::JobOutput(_)))
        }) {
            return Err("invalid external physical action".into());
        }
        let mut ordered = transfers.to_vec();
        if ordered
            .par_windows(2)
            .any(|pair| pair[0].action > pair[1].action)
        {
            ordered.par_sort_unstable_by_key(|row| row.action);
        }
        if ordered.first().is_some_and(|row| row.action == 0)
            || ordered
                .par_windows(2)
                .any(|pair| pair[0].action == pair[1].action)
            || u64::try_from(ordered.len()).is_err()
        {
            return Err("invalid or duplicate physical action key".into());
        }
        let normalized: Vec<_> = ordered
            .par_iter()
            .enumerate()
            .map(|(index, row)| {
                let id = index as u64 + 1;
                let mut movement = row.movement;
                movement.id = id;
                SimpleTransfer {
                    action: id,
                    movement,
                }
            })
            .collect();
        let Some(mut custody) = self
            .store
            .prepare_partitioned_flat(seed, &normalized, true)?
        else {
            return Ok(None);
        };
        let tick = self.store.tick().checked_add(1).ok_or("tick overflow")?;
        let physical_actions = custody
            .transactions()
            .par_iter()
            .map(|row| TransactionOutcome {
                id: ordered[(row.id - 1) as usize].action,
                accepted: row.accepted,
            })
            .collect();
        let (outcomes, spoiled) = custody.take_outcome_batch();
        let (headers, arenas) = outcomes.into_parts();
        let headers = headers
            .into_par_iter()
            .map(|mut movement| {
                let original = ordered[(movement.id - 1) as usize];
                movement.id = original.movement.id;
                ExternalReceiptHeader {
                    action: original.action,
                    request: original.movement,
                    movement,
                }
            })
            .collect();
        let physical_movements = ExternalReceiptBatch::from_parts(headers, arenas);
        Ok(Some(AdvanceDraft {
            epoch: self.epoch,
            custody,
            work: Vec::new(),
            produced_items: Vec::new(),
            next_produced_item: None,
            events: Events {
                tick,
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
                spoiled,
                physical_actions,
                physical_movements,
            },
        }))
    }
    /// Prepare all owners against the same generation without publishing.
    pub fn prepare_advance(
        &self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        seed: u64,
    ) -> Result<AdvanceDraft, String> {
        self.prepare_with_starts(rates, external, &[], seed)
    }
    pub fn prepare_with_starts(
        &self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        starts: &[Start],
        seed: u64,
    ) -> Result<AdvanceDraft, String> {
        self.prepare_with_choices(rates, external, starts, &[], seed)
    }
    pub fn prepare_with_choices(
        &self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        starts: &[Start],
        abandon: &[(u64, u64)],
        seed: u64,
    ) -> Result<AdvanceDraft, String> {
        self.prepare_with_growth(rates, external, starts, abandon, Growth::default(), seed)
    }
    pub fn prepare_with_growth(
        &self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        starts: &[Start],
        abandon: &[(u64, u64)],
        growth: Growth<'_>,
        seed: u64,
    ) -> Result<AdvanceDraft, String> {
        self.prepare_with_growth_and_custody(rates, external, starts, abandon, (growth, &[]), seed)
    }
    /// Stage action-owned empty site custody in the same generation as its initiating action.
    pub fn prepare_with_growth_and_custody(
        &self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        starts: &[Start],
        abandon: &[(u64, u64)],
        (growth, custody): (Growth<'_>, &[(u64, game_lots::Container)]),
        seed: u64,
    ) -> Result<AdvanceDraft, String> {
        self.prepare_with_identified_production(
            rates,
            external,
            starts,
            abandon,
            (growth, custody),
            seed,
            None,
        )
    }
    /// Stage optional identified production in the same transaction as job completion.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_identified_production(
        &self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        starts: &[Start],
        abandon: &[(u64, u64)],
        (growth, custody): (Growth<'_>, &[(u64, game_lots::Container)]),
        seed: u64,
        identified: Option<crate::IdentifiedProduction<'_>>,
    ) -> Result<AdvanceDraft, String> {
        self.prepare_with_work_transitions(
            rates,
            external,
            starts,
            abandon,
            (growth, custody),
            seed,
            identified,
            WorkTransitions::default(),
        )
    }
    /// Apply checked job suspension and adoption in the detached generation.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_work_transitions(
        &self,
        rates: &[(u64, u64)],
        external: &[Transaction],
        starts: &[Start],
        abandon: &[(u64, u64)],
        (growth, custody): (Growth<'_>, &[(u64, game_lots::Container)]),
        seed: u64,
        identified: Option<crate::IdentifiedProduction<'_>>,
        transitions: WorkTransitions<'_>,
    ) -> Result<AdvanceDraft, String> {
        self.ensure_no_work_interval()?;
        #[cfg(feature = "profile")]
        let mut profile_phase = game_diagnostics::span("physical.validate_and_plan");
        let mut owners: Vec<_> = custody
            .par_iter()
            .map(|(action, container)| (*action, container.place))
            .collect();
        owners.par_sort_unstable();
        let mut places: Vec<_> = owners.par_iter().map(|row| row.1).collect();
        places.par_sort_unstable();
        let mut external_by_id: Vec<_> = if custody.is_empty() {
            Vec::new()
        } else {
            external.par_iter().collect()
        };
        if !custody.is_empty() {
            external_by_id.par_sort_unstable_by_key(|action| action.id);
        }
        if owners.par_windows(2).any(|pair| pair[0].0 == pair[1].0)
            || places.par_windows(2).any(|pair| pair[0] == pair[1])
            || custody.par_iter().any(|(id, container)| {
                !matches!(container.place, Place::Site(_))
                    || container.used != 0
                    || external_by_id
                        .binary_search_by_key(id, |action| action.id)
                        .ok()
                        .map(|i| external_by_id[i])
                        .is_none_or(|action| {
                            !action
                                .movements
                                .iter()
                                .any(|movement| movement.to == Some(container.place))
                        })
            })
        {
            return Err("invalid staged site custody".into());
        }
        game_lot_work::validate_actions(&self.catalog, external)?;
        let next_tick = self.store.tick().checked_add(1).ok_or("tick overflow")?;
        game_lot_content::validate_growth(&self.catalog, growth.harvest, next_tick)?;
        let mut animal_yield =
            game_lot_content::prepare_animal_yield(&self.catalog, growth.animal_yield, next_tick)?;
        if !growth.animal_products.is_empty() {
            let animals: std::collections::HashSet<_> =
                growth.animal_yield.par_iter().map(|y| y.animal).collect();
            let hunters: std::collections::HashSet<_> =
                growth.animal_yield.par_iter().map(|y| y.hunter).collect();
            let actions: std::collections::HashSet<_> =
                growth.animal_yield.par_iter().map(|y| y.action).collect();
            if growth.animal_products.par_iter().any(|p| {
                animals.contains(&p.animal)
                    || hunters.contains(&p.collector)
                    || actions.contains(&p.action)
            }) {
                return Err("animal product conflicts with hunting yield".into());
            }
            animal_yield.extend(game_lot_content::prepare_animal_products(
                &self.catalog,
                growth.animal_products,
                next_tick,
            )?);
        }
        animal_yield.extend(game_lot_content::prepare_writing(
            &self.catalog,
            growth.writing,
            next_tick,
        )?);
        animal_yield.extend(game_lot_content::prepare_gathered(
            &self.catalog,
            growth.gathered,
            next_tick,
        )?);
        let direct_external = external;
        let combined = game_lot_content::combine_growth(external, growth.harvest, &animal_yield);
        let external = combined.as_deref().unwrap_or(external);
        game_lot_work::validate_starts(&self.work, starts)?;
        let start_plan = game_lot_work::start(starts, &self.recipes)?;
        let mut rates = rates.to_vec();
        rates.par_sort_unstable_by_key(|row| row.0);
        if rates.par_windows(2).any(|rows| rows[0].0 == rows[1].0)
            || rates.par_iter().any(|row| {
                row.1 == 0
                    || self
                        .work
                        .binary_search_by_key(&row.0, |work| work.job.id)
                        .map_or(true, |index| self.work[index].phase != Phase::Working)
            })
        {
            return Err("invalid work rate".into());
        }
        let mut abandoned = abandon.to_vec();
        abandoned.par_sort_unstable_by_key(|row| row.0);
        if abandoned.par_windows(2).any(|rows| rows[0].0 == rows[1].0)
            || abandoned.par_iter().any(|&(job, worker)| {
                rates.binary_search_by_key(&job, |row| row.0).is_ok()
                    || self
                        .work
                        .binary_search_by_key(&job, |row| row.job.id)
                        .map_or(true, |index| {
                            self.work[index].phase != Phase::Working
                                || self.work[index].job.worker != worker
                        })
            })
        {
            return Err("invalid work abandonment".into());
        }
        let mut suspensions = transitions.suspend.to_vec();
        suspensions.par_sort_unstable_by_key(|row| row.job);
        let mut adoptions = transitions.adopt.to_vec();
        adoptions.par_sort_unstable_by_key(|row| row.job);
        let mut adopting_workers: Vec<_> = adoptions.par_iter().map(|row| row.worker).collect();
        adopting_workers.par_sort_unstable();
        let mut occupied_workers: Vec<_> = if adoptions.is_empty() {
            Vec::new()
        } else {
            self.work
                .par_iter()
                .filter(|work| work.phase == Phase::Working)
                .map(|work| work.job.worker)
                .chain(starts.par_iter().map(|start| start.worker))
                .collect()
        };
        if occupied_workers
            .par_windows(2)
            .any(|pair| pair[0] > pair[1])
        {
            occupied_workers.par_sort_unstable();
        }
        if suspensions.par_windows(2).any(|pair| pair[0].job == pair[1].job)
            || adoptions.par_windows(2).any(|pair| pair[0].job == pair[1].job)
            || adopting_workers.par_windows(2).any(|pair| pair[0] == pair[1])
            || suspensions.par_iter().any(|row| row.until <= next_tick
                || abandoned.binary_search(&(row.job, row.worker)).is_err())
            || adoptions.par_iter().any(|row| {
                row.worker == 0 || row.worker == row.previous_worker
                    || occupied_workers.binary_search(&row.worker).is_ok()
                    || self.work.binary_search_by_key(&row.job, |work| work.job.id).ok().is_none_or(|i| {
                        let work = &self.work[i];
                        work.job.worker != row.previous_worker || !matches!(work.phase, Phase::Suspended { until } if until > next_tick)
                    })
            })
        {
            return Err("invalid suspended work transition".into());
        }
        let suspended_ids: Vec<_> = suspensions.par_iter().map(|row| row.job).collect();
        let abandoned_ids: Vec<_> = abandoned
            .par_iter()
            .filter(|row| suspended_ids.binary_search(&row.0).is_err())
            .map(|row| row.0)
            .collect();
        let expired_ids: Vec<_> = self
            .work
            .par_iter()
            .filter(|work| matches!(work.phase, Phase::Suspended { until } if until <= next_tick))
            .map(|work| work.job.id)
            .collect();
        let mut next: Vec<_> = self.work.par_iter().cloned().collect();
        next.par_iter_mut()
            .filter(|row| abandoned_ids.binary_search(&row.job.id).is_ok())
            .for_each(|row| row.phase = Phase::Returning);
        if !suspensions.is_empty() || !adoptions.is_empty() || !expired_ids.is_empty() {
            next.par_iter_mut().for_each(|row| {
                if let Ok(i) =
                    suspensions.binary_search_by_key(&row.job.id, |transition| transition.job)
                {
                    row.phase = Phase::Suspended {
                        until: suspensions[i].until,
                    };
                } else if let Ok(i) =
                    adoptions.binary_search_by_key(&row.job.id, |transition| transition.job)
                {
                    row.job.worker = adoptions[i].worker;
                    row.phase = Phase::Working;
                } else if expired_ids.binary_search(&row.job.id).is_ok() {
                    row.phase = Phase::Returning;
                }
            });
        }
        next.par_iter_mut()
            .filter(|row| row.phase == Phase::Working)
            .for_each(|row| {
                if let Ok(index) = rates.binary_search_by_key(&row.job.id, |rate| rate.0) {
                    if row.progress_remainder == 0 {
                        row.job.remaining = row.job.remaining.saturating_sub(rates[index].1);
                    } else {
                        row.advance_elapsed(rates[index].1, self.tick_millis, self.tick_millis)
                            .expect("validated fractional work progress");
                    }
                }
            });
        let custody_rows = self.store.containers();
        let work_places =
            (!self.work.is_empty()).then(|| PlaceIndex::new(custody_rows, |c| c.place));
        let work_custody = |place| {
            work_places
                .as_ref()
                .and_then(|index| index.locate(place))
                .map(|i| &custody_rows[i])
                .expect("validated work custody")
        };
        let finishing: Vec<_> = next
            .par_iter()
            .filter(|row| row.phase == Phase::Working && row.job.remaining == 0)
            .map(|row| (&row.job, work_custody(Place::JobInput(row.job.id))))
            .collect();
        let mut finished = game_lot_work::finish(&finishing, &self.catalog, next_tick)?;
        let identified_plan = identified
            .map(|config| {
                crate::identified_output::prepare(
                    &self.store,
                    &mut finished.transactions,
                    external,
                    config,
                )
            })
            .transpose()?;
        let mut interrupted = finished.interrupted;
        interrupted.par_extend(expired_ids);
        interrupted.par_sort_unstable();
        next.par_iter_mut()
            .filter(|row| interrupted.binary_search(&row.job.id).is_ok())
            .for_each(|row| row.phase = Phase::Returning);
        let returning: Vec<_> = next
            .par_iter()
            .filter(|row| row.phase == Phase::Returning)
            .map(|row| (&row.job, work_custody(Place::JobInput(row.job.id))))
            .collect();
        let returned = game_lot_work::cancel(&returning, next_tick)?;
        let waiting: Vec<_> = next
            .par_iter()
            .filter(|row| row.phase == Phase::Waiting)
            .map(|row| (&row.job, work_custody(Place::JobOutput(row.job.id))))
            .collect();
        let delivered = game_lot_work::deliver(&waiting, next_tick)?;
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("physical.compose");
        }
        let actions = finished
            .transactions
            .into_par_iter()
            .map(|row| (1, row))
            .chain(delivered.returns.into_par_iter().map(|row| (2, row)))
            .chain(returned.returns.into_par_iter().map(|row| (3, row)))
            .chain(external.par_iter().cloned().map(|row| (4, row)))
            .chain(start_plan.transactions.into_par_iter().map(|row| (5, row)))
            .collect();
        let mut composed = game_lots::compose(actions)?;
        let compact = composed.compact_ids(4, growth.compact_consumption)?;
        let mut containers = start_plan.containers;
        containers.par_extend(custody.par_iter().map(|(_, container)| container.clone()));
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("physical.store_prepare");
        }
        let mut draft = self.store.prepare_with_empty_compact(
            seed,
            &composed.transactions,
            containers,
            &compact,
            true,
        )?;
        let produced_items = identified_plan.as_ref().map_or_else(Vec::new, |plan| {
            plan.items
                .par_iter()
                .filter(|item| {
                    draft
                        .item_updates()
                        .binary_search_by_key(&item.item, |row| row.0)
                        .ok()
                        .is_some_and(|i| draft.item_updates()[i].1.is_some())
                })
                .copied()
                .collect()
        });
        let next_produced_item = identified_plan.map(|plan| plan.next_item);
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("physical.finalize");
        }
        #[cfg(feature = "profile")]
        let mut finalize_phase = game_diagnostics::span("physical.finalize.keys");
        // A sorted run of single-movement external actions receives contiguous
        // movement IDs during composition. It can be mapped directly without a
        // separate per-movement index; mixed actions use the general merge.
        let external_begin = composed.keys.partition_point(|key| key.0 < 4);
        let external_end = composed.keys.partition_point(|key| key.0 <= 4);
        debug_assert_eq!(external_end - external_begin, external.len());
        let direct_start = (combined.is_none()
            && compact.is_empty()
            && !external.is_empty()
            && external.par_windows(2).all(|p| p[0].id < p[1].id)
            && external
                .par_iter()
                .all(|action| action.movements.len() == 1 && action.creations.is_empty()))
        .then(|| composed.transactions[external_begin].movements[0].id);
        let movement_keys: Vec<_> = if direct_start.is_some() {
            Vec::new()
        } else {
            let mut external_index: Vec<_> = external.par_iter().collect();
            if external_index.par_windows(2).any(|p| p[0].id > p[1].id) {
                external_index.par_sort_unstable_by_key(|row| row.id);
            }
            composed.transactions[external_begin..external_end]
                .par_iter()
                .zip(composed.keys[external_begin..external_end].par_iter())
                .zip(external_index.par_iter())
                .filter(|((action, _), _)| compact.binary_search(&action.id).is_err())
                .flat_map_iter(|((action, key), original)| {
                    debug_assert_eq!(key.1, original.id);
                    action
                        .movements
                        .iter()
                        .zip(original.movements.iter())
                        .map(move |(assigned, source)| (assigned.id, key.1, *source))
                })
                .collect()
        };
        #[cfg(feature = "profile")]
        {
            drop(finalize_phase);
            finalize_phase = game_diagnostics::span("physical.finalize.results");
        }
        game_lot_work::release_actions(&mut composed.transactions, combined);
        let physical_actions: Vec<_> = draft
            .transactions()
            .par_iter()
            .filter_map(|outcome| {
                let key = composed.keys[(outcome.id - 1) as usize];
                (key.0 == 4).then_some(TransactionOutcome {
                    id: key.1,
                    accepted: outcome.accepted,
                })
            })
            .collect();
        let accepted: Vec<_> = draft
            .transactions()
            .par_iter()
            .filter(|row| row.accepted && composed.keys[(row.id - 1) as usize].0 != 4)
            .map(|outcome| composed.keys[(outcome.id - 1) as usize])
            .collect();
        let mut started_ids: Vec<_> = accepted
            .par_iter()
            .filter_map(|&(owner, id)| (owner == 5).then_some(id))
            .collect();
        started_ids.par_sort_unstable();
        let mut denied_ids: Vec<_> = start_plan
            .jobs
            .par_iter()
            .filter_map(|job| {
                started_ids
                    .binary_search(&job.id)
                    .is_err()
                    .then_some(job.id)
            })
            .collect();
        denied_ids.par_sort_unstable();
        let mut finished_ids: Vec<_> = accepted
            .par_iter()
            .filter_map(|&(owner, id)| (owner == 1).then_some(id))
            .collect();
        let mut delivered_ids: Vec<_> = accepted
            .par_iter()
            .filter_map(|&(owner, id)| (owner == 2).then_some(id))
            .collect();
        let mut returned_ids: Vec<_> = accepted
            .par_iter()
            .filter_map(|&(owner, id)| (owner == 3).then_some(id))
            .collect();
        let mut output_lost = delivered.empty;
        let mut empty_returns = returned.empty;
        finished_ids.par_sort_unstable();
        delivered_ids.par_sort_unstable();
        returned_ids.par_sort_unstable();
        output_lost.par_sort_unstable();
        empty_returns.par_sort_unstable();
        let mut retire: Vec<_> = finished_ids
            .par_iter()
            .map(|&id| Place::JobInput(id))
            .chain(
                denied_ids
                    .par_iter()
                    .flat_map_iter(|&id| [Place::JobInput(id), Place::JobOutput(id)]),
            )
            .chain(
                delivered_ids
                    .par_iter()
                    .chain(output_lost.par_iter())
                    .map(|&id| Place::JobOutput(id)),
            )
            .chain(
                returned_ids
                    .par_iter()
                    .chain(empty_returns.par_iter())
                    .flat_map_iter(|&id| [Place::JobInput(id), Place::JobOutput(id)]),
            )
            .collect();
        if !custody.is_empty() {
            let accepted_external: std::collections::HashSet<_> = physical_actions
                .par_iter()
                .filter(|row| row.accepted)
                .map(|row| row.id)
                .collect();
            retire.par_extend(custody.par_iter().filter_map(|(action, container)| {
                (!accepted_external.contains(action)).then_some(container.place)
            }));
        }
        #[cfg(feature = "profile")]
        {
            drop(finalize_phase);
            finalize_phase = game_diagnostics::span("physical.finalize.bookkeeping");
        }
        self.store.retire_in(&mut draft, retire)?;
        next.par_iter_mut()
            .filter(|row| finished_ids.binary_search(&row.job.id).is_ok())
            .for_each(|row| {
                row.phase = Phase::Waiting;
                row.job.phase = JobPhase::Waiting;
                row.job.reserved.clear();
            });
        next = next
            .into_par_iter()
            .filter(|row| {
                delivered_ids.binary_search(&row.job.id).is_err()
                    && output_lost.binary_search(&row.job.id).is_err()
                    && returned_ids.binary_search(&row.job.id).is_err()
                    && empty_returns.binary_search(&row.job.id).is_err()
            })
            .collect();
        next.par_extend(
            start_plan
                .jobs
                .into_par_iter()
                .filter(|job| started_ids.binary_search(&job.id).is_ok())
                .map(|job| Work {
                    job,
                    phase: Phase::Working,
                    progress_remainder: 0,
                }),
        );
        next.par_sort_unstable_by_key(|row| row.job.id);
        self.store.validate_commit(&draft)?;
        #[cfg(feature = "profile")]
        {
            drop(finalize_phase);
            finalize_phase = game_diagnostics::span("physical.finalize.receipts");
        }
        let (outcomes, spoiled) = draft.take_outcome_batch();
        let (headers, arenas) = outcomes.into_parts();
        let headers = if let Some(first) = direct_start {
            headers
                .into_par_iter()
                .filter_map(|mut movement| {
                    let index = movement
                        .id
                        .checked_sub(first)
                        .and_then(|n| usize::try_from(n).ok())?;
                    let action = direct_external.get(index)?;
                    let request = action.movements[0];
                    movement.id = request.id;
                    Some(ExternalReceiptHeader {
                        action: action.id,
                        request,
                        movement,
                    })
                })
                .collect()
        } else {
            headers
                .into_par_iter()
                .chunks(2048)
                .flat_map_iter(|chunk| {
                    let mut cursor = movement_keys.partition_point(|row| row.0 < chunk[0].id);
                    let keys = &movement_keys;
                    chunk.into_iter().filter_map(move |mut movement| {
                        while cursor < keys.len() && keys[cursor].0 < movement.id {
                            cursor += 1;
                        }
                        let &(_, action, request) =
                            keys.get(cursor).filter(|row| row.0 == movement.id)?;
                        cursor += 1;
                        movement.id = request.id;
                        Some(ExternalReceiptHeader {
                            action,
                            request,
                            movement,
                        })
                    })
                })
                .collect()
        };
        let physical_movements = ExternalReceiptBatch::from_parts(headers, arenas);
        #[cfg(feature = "profile")]
        drop(finalize_phase);
        let events = Events {
            tick: next_tick,
            started: started_ids,
            start_denied: denied_ids,
            abandoned: abandoned_ids,
            suspended: suspended_ids,
            adopted: adoptions,
            finished: finished_ids,
            delivered: delivered_ids,
            output_lost,
            interrupted,
            returned: returned_ids,
            empty_returns,
            spoiled,
            physical_actions,
            physical_movements,
        };
        #[cfg(feature = "profile")]
        drop(profile_phase);
        Ok(AdvanceDraft {
            epoch: self.epoch,
            custody: draft,
            work: next,
            events,
            produced_items,
            next_produced_item,
        })
    }
    /// Check before publishing other owners in the same generation.
    pub fn validate_advance(&self, draft: &AdvanceDraft) -> Result<(), String> {
        self.ensure_no_work_interval()?;
        if self.epoch != draft.epoch {
            return Err("stale physical work draft".into());
        }
        self.store.validate_commit(&draft.custody)
    }
    pub fn commit_advance(&mut self, draft: AdvanceDraft) -> Result<Events, String> {
        self.validate_advance(&draft)?;
        self.store.commit(draft.custody)?;
        self.work = draft.work;
        Ok(draft.events)
    }
}
