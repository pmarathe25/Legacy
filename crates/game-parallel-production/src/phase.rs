//! Parallel, conservative capacity arbitration for completed output.
use crate::*;
use game_parallel_production_output::{Progress, arbitrate};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Decisions prepared from an immutable snapshot and published after all fallible owners succeed.
pub struct Draft {
    people: Vec<PersonSnapshot>,
    progress: Vec<Progress>,
    denied: Option<Vec<AtomicBool>>,
    updates: HashMap<(u64, u32), u64>,
    events: Vec<Event>,
    any_delivered: bool,
}
impl Draft {
    pub fn people(&self) -> &[PersonSnapshot] {
        &self.people
    }
    pub fn discard(self) -> Vec<PersonSnapshot> {
        self.people
    }
    pub fn commit(self, production: &mut Production) -> Vec<PersonSnapshot> {
        production.inventories.par_iter_mut().for_each(|inventory| {
            for stock in &mut inventory.stocks {
                stock.quantity += self
                    .updates
                    .get(&(inventory.region, stock.good))
                    .copied()
                    .unwrap_or(0);
            }
        });
        if !self.any_delivered {
            production
                .jobs
                .par_iter_mut()
                .zip(self.progress.par_iter())
                .for_each(|(job, progress)| {
                    job.remaining = progress.remaining;
                    if progress.finished {
                        job.phase = JobPhase::Waiting;
                        job.reserved.clear();
                    }
                });
            if production
                .jobs
                .par_windows(2)
                .any(|pair| pair[0].id > pair[1].id)
            {
                production.jobs.par_sort_unstable_by_key(|job| job.id);
            }
            production.events = self.events;
            return self.people;
        }
        production.jobs = std::mem::take(&mut production.jobs)
            .into_par_iter()
            .enumerate()
            .filter_map(|(index, mut job)| {
                let progress = &self.progress[index];
                if progress.waiting
                    && self
                        .denied
                        .as_ref()
                        .is_none_or(|denied| !denied[index].load(Ordering::Relaxed))
                {
                    return None;
                }
                job.remaining = progress.remaining;
                if progress.finished {
                    job.phase = JobPhase::Waiting;
                    job.reserved.clear();
                }
                Some(job)
            })
            .collect();
        if production
            .jobs
            .par_windows(2)
            .any(|pair| pair[0].id > pair[1].id)
        {
            production.jobs.par_sort_unstable_by_key(|job| job.id);
        }
        production.events = self.events;
        self.people
    }
}
impl Production {
    /// Prepare from a pre-tick snapshot. Completed goods wait physically for space.
    pub fn prepare(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        now: u64,
        next_tick: u64,
    ) -> Result<Draft, String> {
        let workers = self.validate_people(regions, travelers, now)?;
        if now.checked_add(1) != Some(next_tick) {
            return Err("invalid production tick".into());
        }
        Ok(self.prepare_validated(workers, next_tick))
    }
    /// Prepare after validating the owner and sorted person snapshot at this boundary.
    pub fn prepare_validated(&self, workers: Vec<PersonSnapshot>, next_tick: u64) -> Draft {
        let capacity: HashMap<_, _> = self
            .inventories
            .par_iter()
            .flat_map(|i| {
                i.stocks
                    .par_iter()
                    .map(move |s| ((i.region, s.good), s.capacity - s.quantity))
            })
            .collect();
        let progress: Vec<_> = self
            .jobs
            .par_iter()
            .map(|job| {
                let active = job.phase == JobPhase::Working
                    && person_index(&workers, job.worker).is_some_and(|index| {
                        let (_, region, health, hunger) = workers[index];
                        region == Some(job.region) && health > 0 && hunger <= self.max_work_hunger
                    });
                let remaining = job.remaining - u64::from(active);
                let finished = active && remaining == 0;
                Progress {
                    remaining,
                    finished,
                    waiting: job.phase == JobPhase::Waiting || finished,
                }
            })
            .collect();
        let (denied, updates) = arbitrate(&self.jobs, &progress, &capacity);
        let any_delivered = progress.par_iter().enumerate().any(|(index, progress)| {
            progress.waiting
                && denied
                    .as_ref()
                    .is_none_or(|denied| !denied[index].load(Ordering::Relaxed))
        });
        let mut events: Vec<_> = self
            .jobs
            .par_iter()
            .enumerate()
            .flat_map_iter(|(index, job)| {
                let p = &progress[index];
                [
                    p.finished.then_some(Event {
                        tick: next_tick,
                        job: job.id,
                        kind: EventKind::Finished,
                    }),
                    (p.waiting
                        && denied
                            .as_ref()
                            .is_none_or(|denied| !denied[index].load(Ordering::Relaxed)))
                    .then_some(Event {
                        tick: next_tick,
                        job: job.id,
                        kind: EventKind::Delivered,
                    }),
                ]
                .into_iter()
                .flatten()
            })
            .collect();
        let event_key = |event: &Event| {
            (
                event.job,
                match event.kind {
                    EventKind::Finished => 0,
                    EventKind::Delivered => 1,
                },
            )
        };
        if events
            .par_windows(2)
            .any(|pair| event_key(&pair[0]) > event_key(&pair[1]))
        {
            events.par_sort_unstable_by_key(event_key);
        }
        Draft {
            people: workers,
            progress,
            denied,
            updates,
            events,
            any_delivered,
        }
    }
}
