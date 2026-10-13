//! Sparse interval calculations and guarded publication of owned components.
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct JobInput<'a> {
    pub job: &'a Job,
    pub can_progress: bool,
    pub medical_level: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedInterval {
    rules: Rules,
    clinical: BTreeMap<PersonId, (Option<Clinical>, Option<Clinical>)>,
    jobs: BTreeMap<PersonId, (Job, Option<Job>)>,
}

impl Runtime {
    pub fn prepare_interval(
        &self,
        store: &Store,
        now: SimTime,
        next: SimTime,
        clinical_inputs: &[(PersonId, ClinicalInput<'_>)],
        job_inputs: &[(PersonId, JobInput<'_>)],
    ) -> Result<PreparedInterval, Error> {
        validate_rules(&self.rules)?;
        let elapsed = next.0.checked_sub(now.0).ok_or(Error::Overflow)?;
        let mut prepared = PreparedInterval {
            rules: self.rules.clone(),
            clinical: BTreeMap::new(),
            jobs: BTreeMap::new(),
        };
        for (actor, input) in clinical_inputs {
            if actor.0 == 0 || prepared.clinical.contains_key(actor) {
                return Err(Error::InvalidConfiguration(
                    "invalid clinical interval keys".into(),
                ));
            }
            let before = clinical(store, *actor).cloned();
            let opening = before.clone().unwrap_or_default();
            if input.rules != &self.rules
                || input.now != now
                || input.next != next
                || input.state != &opening
            {
                return Err(Error::Unavailable);
            }
            opening.validate()?;
            let after = advance_clinical(*input)?;
            after.validate()?;
            let changed = (after != opening).then_some(after);
            prepared.clinical.insert(*actor, (before, changed));
        }
        for (actor, input) in job_inputs {
            if actor.0 == 0 || prepared.jobs.contains_key(actor) {
                return Err(Error::InvalidConfiguration(
                    "invalid health work interval keys".into(),
                ));
            }
            if input.job.actor != *actor || job(store, *actor) != Some(input.job) {
                return Err(Error::Unavailable);
            }
            input.job.validate()?;
            let after = advance_job(input.job, elapsed, input.can_progress, input.medical_level)?;
            after.validate()?;
            let changed = (after != *input.job).then_some(after);
            prepared.jobs.insert(*actor, (input.job.clone(), changed));
        }
        Ok(prepared)
    }
}

impl PreparedInterval {
    fn validate_clinical(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if runtime.rules != self.rules
            || self
                .clinical
                .iter()
                .any(|(actor, (before, _))| clinical(store, *actor) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }

    /// The host runs this guard before publishing clock, body, or other phase
    /// writes. Closing publication later permits only intentional job removal.
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        self.validate_clinical(runtime, store)?;
        if self
            .jobs
            .iter()
            .any(|(actor, (before, _))| job(store, *actor) != Some(before))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }

    pub fn changed_jobs(&self) -> impl Iterator<Item = (PersonId, &Job)> {
        self.jobs
            .iter()
            .filter_map(|(actor, (_, after))| after.as_ref().map(|job| (*actor, job)))
    }

    /// Select closing job retention and deadline effects, then stage Health
    /// rows. The receiving activity port supplies factual work identities; its
    /// adapter retains the concrete component witnesses in the same batch.
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        mut keep_job: impl FnMut(PersonId) -> bool,
        mut activity_work: impl FnMut(PersonId) -> Option<HealthWorkKind>,
        batch: &mut Batch,
    ) -> Result<IntervalFinalization, Error> {
        self.validate_clinical(runtime, store)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        // Deadline effects cover the closing Job column, including unrelated
        // jobs created since opening. Retain exact rows as well as membership.
        reads.membership::<Job>(store, |_, _| true);
        let mut deadlines = BTreeMap::new();
        for (actor, current) in jobs(store) {
            reads.expect_row::<Job>(StableId(actor.0), Some(current.clone()));
            deadlines.insert(actor, current.completes);
        }
        for (actor, (before, after)) in &self.jobs {
            let keep = keep_job(*actor);
            match job(store, *actor) {
                Some(current) if current == before => {
                    if keep {
                        if let Some(after) = after {
                            batch.insert(StableId(actor.0), after.clone());
                            admitted.protect::<Job>(StableId(actor.0));
                            deadlines.insert(*actor, after.completes);
                        }
                    } else {
                        batch.remove::<Job>(StableId(actor.0));
                        admitted.protect::<Job>(StableId(actor.0));
                        deadlines.remove(actor);
                    }
                }
                None if !keep => {
                    reads.expect_row::<Job>(StableId(actor.0), None);
                    deadlines.remove(actor);
                }
                _ => return Err(Error::Unavailable),
            }
        }
        for (actor, (before, after)) in self.clinical {
            reads.expect_row::<Clinical>(StableId(actor.0), before);
            if let Some(after) = after {
                batch.insert(StableId(actor.0), after);
                admitted.protect::<Clinical>(StableId(actor.0));
            }
        }
        let deadlines = deadlines
            .into_iter()
            .filter_map(|(actor, completes)| {
                activity_work(actor).map(|kind| WorkDeadlineEffect {
                    actor,
                    kind,
                    completes,
                })
            })
            .collect();
        let guard = reads.bind(store, batch, &admitted)?;
        Ok(IntervalFinalization {
            rules: self.rules,
            deadlines,
            guard,
        })
    }

    /// Recheck closing cancellation policy and publish only changed Health rows.
    pub fn commit(
        self,
        runtime: &Runtime,
        store: &mut Store,
        keep_job: impl Fn(PersonId) -> bool,
    ) -> Result<(), Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, keep_job, |_| None, &mut batch)?;
        finish.validate_batch(runtime, store, &batch)?;
        store.apply_batch(batch)
    }
}

/// Projected deadline for an existing Health work activity. The kind comes
/// from the receiving feature's factual activity witness and is preserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkDeadlineEffect {
    pub actor: PersonId,
    pub kind: HealthWorkKind,
    pub completes: SimTime,
}
pub struct IntervalFinalization {
    rules: Rules,
    deadlines: Vec<WorkDeadlineEffect>,
    guard: game_ecs::ReadGuard,
}
impl IntervalFinalization {
    pub fn deadlines(&self) -> &[WorkDeadlineEffect] {
        &self.deadlines
    }
    pub fn validate_batch(
        &self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        if runtime.rules != self.rules {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
}

/// Physical and expressed-trait facts for an actor, independent of clinical storage.
#[derive(Clone, Copy, Debug)]
pub struct ClinicalActorFacts {
    pub actor: PersonId,
    pub alive: bool,
    pub thirst_enabled: bool,
    pub resting: bool,
    pub exposure: ExposureInput,
    pub physiology: ClinicalPhysiology,
}
impl Runtime {
    fn clinical_actor_input<'a>(
        &'a self,
        state: &'a Clinical,
        now: SimTime,
        next: SimTime,
        facts: ClinicalActorFacts,
    ) -> ClinicalInput<'a> {
        ClinicalInput {
            rules: &self.rules,
            state,
            now,
            next,
            alive: facts.alive,
            thirst_enabled: facts.thirst_enabled,
            resting: facts.resting,
            exposure: facts.exposure,
            physiology: facts.physiology,
        }
    }
    pub fn prepare_actor_interval(
        &self,
        store: &Store,
        now: SimTime,
        next: SimTime,
        actors: &[ClinicalActorFacts],
        mut progress: impl FnMut(&Job) -> (bool, u16),
    ) -> Result<PreparedInterval, Error> {
        let empty = Clinical::default();
        let clinical = actors
            .iter()
            .filter(|facts| facts.alive)
            .map(|facts| {
                (
                    facts.actor,
                    self.clinical_actor_input(
                        clinical(store, facts.actor).unwrap_or(&empty),
                        now,
                        next,
                        *facts,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let jobs = jobs(store)
            .map(|(actor, job)| {
                let (can_progress, medical_level) = progress(job);
                (
                    actor,
                    JobInput {
                        job,
                        can_progress,
                        medical_level,
                    },
                )
            })
            .collect::<Vec<_>>();
        self.prepare_interval(store, now, next, &clinical, &jobs)
    }
    pub fn actor_boundary(
        &self,
        store: &Store,
        now: SimTime,
        limit: SimTime,
        actors: &[ClinicalActorFacts],
        mut progressing: impl FnMut(&Job) -> bool,
    ) -> Result<SimTime, Error> {
        let mut boundary = limit;
        for (_, job) in jobs(store) {
            if progressing(job) && job.completes > now {
                boundary = boundary.min(job.completes);
            }
        }
        let empty = Clinical::default();
        for facts in actors {
            boundary = boundary.min(clinical_boundary(
                self.clinical_actor_input(
                    clinical(store, facts.actor).unwrap_or(&empty),
                    now,
                    limit,
                    *facts,
                ),
                limit,
            )?);
        }
        Ok(boundary)
    }
    pub fn actor_body_delta(
        &self,
        store: &Store,
        now: SimTime,
        next: SimTime,
        mut facts: ClinicalActorFacts,
    ) -> Result<HealthDelta, Error> {
        facts.exposure.infectious_contacts = [0; 2];
        let empty = Clinical::default();
        clinical_contribution(self.clinical_actor_input(
            clinical(store, facts.actor).unwrap_or(&empty),
            now,
            next,
            facts,
        ))
    }
    pub fn work_deadlines(&self, store: &Store) -> Vec<(PersonId, SimTime)> {
        jobs(store)
            .map(|(actor, job)| (actor, job.completes))
            .collect()
    }
    pub fn due_jobs(&self, store: &Store, now: SimTime) -> Vec<Job> {
        jobs(store)
            .filter(|(_, job)| job.completes <= now)
            .map(|(_, job)| job.clone())
            .collect()
    }
}
