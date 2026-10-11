//! Clinical state and pending health work share the host's component store.
use crate::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};

impl Clinical {
    /// Clinical structure is independent of the clock, rules and references.
    pub fn validate(&self) -> Result<(), Error> {
        if self.cold_remainder >= 1_000
            || self.doses.values().any(|dose| dose.remainder >= 1_000)
            || self.illnesses.iter().any(|(kind, illness)| {
                *kind != illness.kind
                    || illness.acquired > illness.onset
                    || illness.onset >= illness.recovers
            })
            || self
                .treatment
                .as_ref()
                .is_some_and(|course| course.healing > 11_000)
            || self.last_drink.is_some_and(|(source, _)| source.0 == 0)
        {
            return Err(Error::InvalidSave("invalid clinical component".into()));
        }
        Ok(())
    }
}

impl Job {
    pub fn validate(&self) -> Result<(), Error> {
        let valid_work = match self.work {
            Work::Drink { source, quantity } => source.0 != 0 && quantity.0 != 0,
            Work::Boil {
                source,
                quantity,
                tool,
            } => source.0 != 0 && quantity.0 != 0 && tool.0 != 0,
            Work::Shelter {
                timber_used,
                tool,
                capacity,
                insulation,
                started,
            } => {
                timber_used.0 != 0
                    && tool.0 != 0
                    && (1..=64).contains(&capacity)
                    && (1..=1_000).contains(&insulation)
                    && started < self.completes
            }
            Work::Treat {
                patient,
                skill_ms,
                elapsed,
            } => patient.0 != 0 && skill_ms <= u128::from(elapsed) * u128::from(u16::MAX),
        };
        if self.actor.0 == 0 || self.area.0 == 0 || self.completes.0 == 0 || !valid_work {
            return Err(Error::InvalidSave("invalid health job component".into()));
        }
        Ok(())
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Clinical>("health/clinical", Clinical::validate)?;
    schema.register::<Job>("health/job", Job::validate)
}

pub fn clinical(store: &Store, actor: PersonId) -> Option<&Clinical> {
    store.get(StableId(actor.0))
}
pub fn clinical_mut(store: &mut Store, actor: PersonId) -> Option<&mut Clinical> {
    store.get_mut(StableId(actor.0))
}
pub fn clinicals(store: &Store) -> impl Iterator<Item = (PersonId, &Clinical)> {
    store
        .iter::<Clinical>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_clinical(
    store: &mut Store,
    actor: PersonId,
    clinical: Clinical,
) -> Result<(), Error> {
    clinical.validate()?;
    store.insert_or_spawn(StableId(actor.0), clinical)
}
pub fn remove_clinical(store: &mut Store, actor: PersonId) -> Result<Option<Clinical>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
pub fn job(store: &Store, actor: PersonId) -> Option<&Job> {
    store.get(StableId(actor.0))
}
pub fn job_mut(store: &mut Store, actor: PersonId) -> Option<&mut Job> {
    store.get_mut(StableId(actor.0))
}
pub fn jobs(store: &Store) -> impl Iterator<Item = (PersonId, &Job)> {
    store.iter::<Job>().map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_job(store: &mut Store, job: Job) -> Result<(), Error> {
    job.validate()?;
    store.insert_or_spawn(StableId(job.actor.0), job)
}
pub fn remove_job(store: &mut Store, actor: PersonId) -> Result<Option<Job>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
