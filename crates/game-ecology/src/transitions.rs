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
        let mut batch = Batch::new();
        let (effect, guard) = self.stage_into(runtime, store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(effect)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(AccessEffect, game_ecs::ReadGuard), Error> {
        let (mut effects, guard) = self.stage_with_response(None, runtime, store, batch)?;
        Ok((effects.remove(0), guard))
    }
    /// A receiving character may grant the request immediately. Both owners'
    /// accepted access transitions share one ecological publication, so refused
    /// grant admission cannot leave behind an already published cooldown.
    pub fn stage_with_response(
        self,
        response: Option<Self>,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(Vec<AccessEffect>, game_ecs::ReadGuard), Error> {
        self.validate(runtime, store)?;
        if let Some(response) = &response {
            response.validate(runtime, store)?;
            if self.resource != response.resource
                || !matches!((&self.effect, &response.effect),
                (AccessEffect::Requested { resource, actor, custodian },
                 AccessEffect::Changed { resource: target, actor: grantor, recipient, allowed: true })
                if resource == target && custodian == grantor && actor == recipient)
            {
                return Err(Error::Unavailable);
            }
        }
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        let mut effects = vec![self.append_access(store, batch, &mut reads, &mut admitted)?];
        if let Some(response) = response {
            effects.push(response.append_access(store, batch, &mut reads, &mut admitted)?);
        }
        Ok((effects, reads.bind(store, batch, &admitted)?))
    }
    fn append_access(
        self,
        store: &Store,
        batch: &mut Batch,
        reads: &mut game_ecs::ReadSet,
        admitted: &mut game_ecs::ReadFootprint,
    ) -> Result<AccessEffect, Error> {
        reads.entity(store, StableId(self.actor.0));
        reads.expect_row(StableId(self.resource.id.0), Some(self.resource.clone()));
        if let Some(resource) = self.changed_resource {
            if batch.touches::<Resource>(StableId(resource.id.0)) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Resource>(StableId(resource.id.0));
            batch.insert(StableId(resource.id.0), resource);
        }
        if let Some((before, requests)) = self.requests {
            if batch.touches::<Requests>(StableId(self.actor.0)) {
                return Err(Error::Unavailable);
            }
            reads.expect_row(StableId(self.actor.0), before);
            admitted.protect::<Requests>(StableId(self.actor.0));
            batch.insert(StableId(self.actor.0), requests);
        }
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
pub struct CollectionStorageBounds {
    pub destination: InventoryId,
    pub good: Good,
    pub water: bool,
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
    /// Validate first, then combine with other owners' admitted effects. Only
    /// ecological rows enter this batch; cargo and health supply their own rows.
    fn append_writes(self, batch: &mut Batch) -> CollectionOutcome {
        if let CollectionOutcome::Collected(effect) = &self.outcome {
            let mut resource = self.resource;
            resource.remaining = effect.remaining;
            batch.insert(StableId(resource.id.0), resource);
        }
        batch.remove::<Gathering>(StableId(self.actor.0));
        self.outcome
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<CollectionOutcome, Error> {
        let mut batch = Batch::new();
        let (outcome, guard) = self.stage_into(runtime, store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(outcome)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(CollectionOutcome, game_ecs::ReadGuard), Error> {
        self.validate(runtime, store)?;
        let actor = StableId(self.actor.0);
        let source = StableId(self.resource.id.0);
        if batch.touches::<Gathering>(actor) {
            return Err(Error::Unavailable);
        }
        let mut reads = game_ecs::ReadSet::new();
        reads.expect_row(actor, Some(self.job.clone()));
        reads.expect_row(source, Some(self.resource.clone()));
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Gathering>(actor);
        if matches!(self.outcome, CollectionOutcome::Collected(_)) {
            if batch.touches::<Resource>(source) {
                return Err(Error::Unavailable);
            }
            admitted.protect::<Resource>(source);
        }
        let outcome = self.append_writes(batch);
        Ok((outcome, reads.bind(store, batch, &admitted)?))
    }
}
impl Runtime {
    /// Admit the minimum remaining-source shape for the built-in Resource
    /// validator. Renewal preserves identity/kind/capacity and validates its
    /// outputs; collection only decreases remaining inside [0, capacity].
    pub fn append_collection_schema_bounds(
        &self,
        store: &Store,
        actor: PersonId,
        source: &Resource,
        batch: &mut Batch,
    ) -> Result<CollectionStorageBounds, Error> {
        let job = self.gathering(store, actor).ok_or(Error::Unavailable)?;
        job.validate()?;
        source.validate()?;
        if source.id != job.resource {
            return Err(stale("inconsistent future collection source"));
        }
        let mut after = source.clone();
        after.remaining = Quantity(0);
        batch.insert(StableId(after.id.0), after);
        batch.remove::<Gathering>(StableId(actor.0));
        Ok(CollectionStorageBounds {
            destination: job.destination,
            good: source.kind.good(),
            water: source.kind == ResourceKind::Spring,
        })
    }
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
