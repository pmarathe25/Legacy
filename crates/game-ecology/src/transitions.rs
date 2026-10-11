//! Permission and collection transitions own only ecological state.
use crate::*;
use game_ecs::{Batch, StableId, Store};

pub const ACCESS_REQUEST_RETRY: SimDuration = SimDuration(180_000);
#[derive(Clone, Copy, Debug)]
pub struct AccessFacts {
    pub actor: PersonId,
    pub area: AreaId,
    pub capable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessEffect {
    Requested {
        resource: ResourceId,
        actor: PersonId,
        custodian: PersonId,
    },
    Changed {
        resource: ResourceId,
        actor: PersonId,
        recipient: PersonId,
        allowed: bool,
    },
}
pub struct PreparedAccess {
    actor: PersonId,
    resource: Resource,
    changed_resource: Option<Resource>,
    requests: Option<(Option<Requests>, Requests)>,
    effect: AccessEffect,
}
fn stale(message: &str) -> Error {
    Error::InvalidSave(message.into())
}
impl PreparedAccess {
    pub fn effect(&self) -> &AccessEffect {
        &self.effect
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if !store.contains(StableId(self.actor.0))
            || runtime.resource(store, self.resource.id) != Some(&self.resource)
            || self
                .requests
                .as_ref()
                .is_some_and(|(before, _)| runtime.requests(store, self.actor) != before.as_ref())
        {
            return Err(stale("stale ecological access transition"));
        }
        Ok(())
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<AccessEffect, Error> {
        self.validate(runtime, store)?;
        let mut batch = Batch::new();
        if let Some(resource) = self.changed_resource {
            batch.insert(StableId(resource.id.0), resource);
        }
        if let Some((_, requests)) = self.requests {
            batch.insert(StableId(self.actor.0), requests);
        }
        store.apply_batch(batch)?;
        Ok(self.effect)
    }
}
impl Runtime {
    pub fn prepare_access(
        &self,
        store: &Store,
        now: SimTime,
        action: &EcologyAction,
        facts: AccessFacts,
        mut actual_area: impl FnMut(PersonId) -> Option<AreaId>,
    ) -> Result<PreparedAccess, Error> {
        if !facts.capable
            || facts.actor.0 == 0
            || facts.area.0 == 0
            || !store.contains(StableId(facts.actor.0))
        {
            return Err(Error::Unavailable);
        }
        match *action {
            EcologyAction::GrantAccess {
                resource,
                recipient,
                allowed,
            } => {
                let source = self.resource(store, resource).ok_or(Error::Unavailable)?;
                source.validate()?;
                if source.id != resource {
                    return Err(stale("inconsistent access resource entity"));
                }
                if source.area != facts.area
                    || source.custodian != Some(facts.actor)
                    || actual_area(recipient) != Some(facts.area)
                {
                    return Err(Error::Unavailable);
                }
                let mut after = source.clone();
                if allowed {
                    after.access.insert(recipient);
                } else {
                    after.access.remove(&recipient);
                }
                Ok(PreparedAccess {
                    actor: facts.actor,
                    resource: source.clone(),
                    changed_resource: Some(after),
                    requests: None,
                    effect: AccessEffect::Changed {
                        resource,
                        actor: facts.actor,
                        recipient,
                        allowed,
                    },
                })
            }
            EcologyAction::RequestAccess { resource } => {
                let before = self.requests(store, facts.actor);
                if before
                    .and_then(|row| row.next_request.get(&resource))
                    .is_some_and(|at| *at > now)
                {
                    return Err(Error::Unavailable);
                }
                let next = SimTime(
                    now.0
                        .checked_add(ACCESS_REQUEST_RETRY.0)
                        .ok_or(Error::Overflow)?,
                );
                let source = self.resource(store, resource).ok_or(Error::Unavailable)?;
                source.validate()?;
                if source.id != resource {
                    return Err(stale("inconsistent access resource entity"));
                }
                let custodian = source.custodian.ok_or(Error::Unavailable)?;
                if source.area != facts.area
                    || custodian == facts.actor
                    || actual_area(custodian) != Some(facts.area)
                {
                    return Err(Error::Unavailable);
                }
                let mut after = before.cloned().unwrap_or_default();
                after.next_request.insert(resource, next);
                after.validate()?;
                Ok(PreparedAccess {
                    actor: facts.actor,
                    resource: source.clone(),
                    changed_resource: None,
                    requests: Some((before.cloned(), after)),
                    effect: AccessEffect::Requested {
                        resource,
                        actor: facts.actor,
                        custodian,
                    },
                })
            }
            EcologyAction::Gather { .. } => Err(Error::Unavailable),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollectionOutcome {
    Collected(CollectionEffect),
    Interrupted { actor: PersonId },
}
pub struct PreparedCollectionFinish {
    actor: PersonId,
    job: Gathering,
    resource: Resource,
    outcome: CollectionOutcome,
}
impl PreparedCollectionFinish {
    /// Other owners admit cargo, water and event effects before any mutation.
    pub fn outcome(&self) -> &CollectionOutcome {
        &self.outcome
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if runtime.gathering(store, self.actor) != Some(&self.job)
            || runtime.resource(store, self.resource.id) != Some(&self.resource)
        {
            return Err(stale("stale ecological collection transition"));
        }
        Ok(())
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<CollectionOutcome, Error> {
        self.validate(runtime, store)?;
        let mut batch = Batch::new();
        if let CollectionOutcome::Collected(effect) = &self.outcome {
            let mut resource = self.resource;
            resource.remaining = effect.remaining;
            batch.insert(StableId(resource.id.0), resource);
        }
        batch.remove::<Gathering>(StableId(self.actor.0));
        store.apply_batch(batch)?;
        Ok(self.outcome)
    }
}
impl Runtime {
    /// Read the currently committed source after any environmental renewal.
    /// Neither the job nor stock changes until enclosing admission succeeds.
    pub fn prepare_collection_finish(
        &self,
        store: &Store,
        actor: PersonId,
        facts: CollectionFacts,
    ) -> Result<Option<PreparedCollectionFinish>, Error> {
        let Some(job) = self.gathering(store, actor) else {
            return Ok(None);
        };
        let source = self
            .resource(store, job.resource)
            .ok_or_else(|| stale("missing collection resource"))?;
        job.validate()?;
        source.validate()?;
        if source.id != job.resource {
            return Err(stale("inconsistent collection resource entity"));
        }
        let outcome = match prepare_collection(source, actor, job, facts) {
            Ok(effect) => CollectionOutcome::Collected(effect),
            Err(Error::Unavailable) => CollectionOutcome::Interrupted { actor },
            Err(error) => return Err(error),
        };
        Ok(Some(PreparedCollectionFinish {
            actor,
            job: job.clone(),
            resource: source.clone(),
            outcome,
        }))
    }
}
#[cfg(test)]
#[path = "transition_tests.rs"]
mod tests;
