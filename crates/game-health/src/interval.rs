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

    /// Recheck opening clinical rows and the allowed closing job policy, then
    /// publish only changed rows. A canceled job is never recreated.
    pub fn commit(
        self,
        runtime: &Runtime,
        store: &mut Store,
        keep_job: impl Fn(PersonId) -> bool,
    ) -> Result<(), Error> {
        self.validate_clinical(runtime, store)?;
        let mut batch = Batch::new();
        for (actor, (before, after)) in &self.jobs {
            let keep = keep_job(*actor);
            match job(store, *actor) {
                Some(current) if current == before => {
                    if keep {
                        if let Some(after) = after {
                            batch.insert(StableId(actor.0), after.clone());
                        }
                    } else {
                        batch.remove::<Job>(StableId(actor.0));
                    }
                }
                None if !keep => {}
                _ => return Err(Error::Unavailable),
            }
        }
        for (actor, (_, after)) in self.clinical {
            if let Some(after) = after {
                batch.insert(StableId(actor.0), after);
            }
        }
        store.apply_batch(batch)
    }
}
