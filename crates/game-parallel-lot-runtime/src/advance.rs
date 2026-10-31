//! Progress, physical completion, loss and returns at one shared boundary.
use super::{Growth, Phase, Runtime, Work};
use game_parallel_lots::{Movement, Outcome, Place, Spoilage, Transaction, TransactionOutcome};
use game_parallel_production_model::{JobPhase, Start};
use rayon::prelude::*;

#[derive(Debug, PartialEq, Eq)]
pub struct ExternalReceipt {
    pub action: u64,
    pub request: Movement,
    pub movement: Outcome,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Events {
    pub tick: u64,
    pub started: Vec<u64>,
    pub start_denied: Vec<u64>,
    pub abandoned: Vec<u64>,
    pub finished: Vec<u64>,
    pub delivered: Vec<u64>,
    pub output_lost: Vec<u64>,
    pub interrupted: Vec<u64>,
    pub returned: Vec<u64>,
    pub empty_returns: Vec<u64>,
    pub spoiled: Vec<Spoilage>,
    pub physical_actions: Vec<TransactionOutcome>,
    pub physical_movements: Vec<ExternalReceipt>,
}
pub struct AdvanceDraft {
    epoch: u64,
    custody: game_parallel_lots::Draft,
    work: Vec<Work>,
    events: Events,
}
impl AdvanceDraft {
    pub fn events(&self) -> &Events {
        &self.events
    }
}
impl Runtime {
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
            .map(|&(id, capacity)| {
                game_parallel_lots::Container::new(Place::Person(id), capacity, vec![])
            })
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
        (growth, custody): (Growth<'_>, &[(u64, game_parallel_lots::Container)]),
        seed: u64,
    ) -> Result<AdvanceDraft, String> {
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
        game_parallel_lot_work::validate_actions(&self.catalog, external)?;
        let next_tick = self.store.tick().checked_add(1).ok_or("tick overflow")?;
        game_parallel_lot_content::validate_growth(&self.catalog, growth.harvest, next_tick)?;
        let combined = game_parallel_lot_content::combine_growth(external, growth.harvest);
        let external = combined.as_deref().unwrap_or(external);
        game_parallel_lot_work::validate_starts(&self.work, starts)?;
        let start_plan = game_parallel_lot_work::start(starts, &self.recipes)?;
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
        let abandoned_ids: Vec<_> = abandoned.par_iter().map(|row| row.0).collect();
        let mut next: Vec<_> = self.work.par_iter().cloned().collect();
        next.par_iter_mut()
            .filter(|row| abandoned_ids.binary_search(&row.job.id).is_ok())
            .for_each(|row| row.phase = Phase::Returning);
        next.par_iter_mut()
            .filter(|row| row.phase == Phase::Working)
            .for_each(|row| {
                if let Ok(index) = rates.binary_search_by_key(&row.job.id, |rate| rate.0) {
                    row.job.remaining = row.job.remaining.saturating_sub(rates[index].1);
                }
            });
        let finishing: Vec<_> = next
            .par_iter()
            .filter(|row| row.phase == Phase::Working && row.job.remaining == 0)
            .map(|row| {
                (
                    &row.job,
                    self.store
                        .container(Place::JobInput(row.job.id))
                        .expect("validated input custody"),
                )
            })
            .collect();
        let finished = game_parallel_lot_work::finish(&finishing, &self.catalog, next_tick)?;
        let mut interrupted = finished.interrupted;
        interrupted.par_sort_unstable();
        next.par_iter_mut()
            .filter(|row| interrupted.binary_search(&row.job.id).is_ok())
            .for_each(|row| row.phase = Phase::Returning);
        let returning: Vec<_> = next
            .par_iter()
            .filter(|row| row.phase == Phase::Returning)
            .map(|row| {
                (
                    &row.job,
                    self.store
                        .container(Place::JobInput(row.job.id))
                        .expect("validated input custody"),
                )
            })
            .collect();
        let returned = game_parallel_lot_work::cancel(&returning, next_tick)?;
        let waiting: Vec<_> = next
            .par_iter()
            .filter(|row| row.phase == Phase::Waiting)
            .map(|row| {
                (
                    &row.job,
                    self.store
                        .container(Place::JobOutput(row.job.id))
                        .expect("validated output custody"),
                )
            })
            .collect();
        let delivered = game_parallel_lot_work::deliver(&waiting, next_tick)?;
        let actions = finished
            .transactions
            .into_par_iter()
            .map(|row| (1, row))
            .chain(delivered.returns.into_par_iter().map(|row| (2, row)))
            .chain(returned.returns.into_par_iter().map(|row| (3, row)))
            .chain(external.par_iter().cloned().map(|row| (4, row)))
            .chain(start_plan.transactions.into_par_iter().map(|row| (5, row)))
            .collect();
        let mut composed = game_parallel_lots::compose(actions)?;
        let compact = composed.compact_ids(4, growth.compact_consumption)?;
        let mut containers = start_plan.containers;
        containers.par_extend(custody.par_iter().map(|(_, container)| container.clone()));
        let mut draft = self.store.prepare_with_empty_compact(
            seed,
            &composed.transactions,
            containers,
            &compact,
            true,
        )?;
        let mut external_index: Vec<_> = external.par_iter().collect();
        if external_index.par_windows(2).any(|p| p[0].id > p[1].id) {
            external_index.par_sort_unstable_by_key(|row| row.id);
        }
        let mut movement_keys: Vec<_> = composed
            .transactions
            .par_iter()
            .zip(composed.keys.par_iter())
            .filter(|(action, key)| key.0 == 4 && compact.binary_search(&action.id).is_err())
            .flat_map_iter(|(action, key)| {
                let original = external_index[external_index
                    .binary_search_by_key(&key.1, |row| row.id)
                    .expect("composed external action")];
                action
                    .movements
                    .iter()
                    .zip(original.movements.iter())
                    .map(move |(assigned, source)| (assigned.id, key.1, *source))
            })
            .collect();
        movement_keys.par_sort_unstable_by_key(|row| row.0);
        game_parallel_lot_work::release_actions(&mut composed.transactions, combined);
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
            .filter(|row| row.accepted)
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
                }),
        );
        next.par_sort_unstable_by_key(|row| row.job.id);
        self.store.validate_commit(&draft)?;
        let physical_movements = draft
            .outcomes()
            .par_iter()
            .filter_map(|movement| {
                let index = movement_keys
                    .binary_search_by_key(&movement.id, |row| row.0)
                    .ok()?;
                let (_, action, request) = movement_keys[index];
                let mut movement = movement.clone();
                movement.id = request.id;
                Some(ExternalReceipt {
                    action,
                    request,
                    movement,
                })
            })
            .collect();
        let spoiled = draft.spoiled().to_vec();
        let events = Events {
            tick: next_tick,
            started: started_ids,
            start_denied: denied_ids,
            abandoned: abandoned_ids,
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
        Ok(AdvanceDraft {
            epoch: self.epoch,
            custody: draft,
            work: next,
            events,
        })
    }
    /// Check before publishing other owners in the same generation.
    pub fn validate_advance(&self, draft: &AdvanceDraft) -> Result<(), String> {
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
