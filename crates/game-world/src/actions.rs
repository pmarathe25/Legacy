//! Canonical actor action leases and owner supplied cancellation routes.
//!
//! The lease contains only coordination metadata. Each registered owner keeps
//! its job payload, due work and cancellation rules in its own domain.
use crate::{OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};
use game_core::{Error, SimTime};
use game_ecs::{Schema, StableId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The single canonical lease for an actor. An absent row means no action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSlot {
    pub actor: StableId,
    pub owner: String,
    pub started: SimTime,
    pub completes: Option<SimTime>,
}
impl ActionSlot {
    /// Validate the lease's intrinsic metadata independently of a live clock.
    pub fn validate(&self) -> Result<(), Error> {
        if self.actor.0 == 0
            || self.owner.is_empty()
            || self.completes.is_some_and(|at| at < self.started)
        {
            return Err(Error::InvalidConfiguration("invalid action lease".into()));
        }
        Ok(())
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<ActionSlot>("actions/slot", ActionSlot::validate)
}

/// Replace the actor's lease after cancelling its outgoing owner, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquireAction {
    pub actor: StableId,
    pub owner: String,
    pub completes: Option<SimTime>,
}
impl Request for AcquireAction {
    type Response = ();
}

/// Release only a lease currently held by the supplied owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAction {
    pub actor: StableId,
    pub owner: String,
}
impl Request for ReleaseAction {
    type Response = bool;
}

/// Cancel a lease unless it belongs to the owner initiating this operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterruptActor {
    pub actor: StableId,
    pub preserve: String,
}
impl Request for InterruptActor {
    type Response = bool;
}

/// Cancel any current lease. Defer this request when its owner is still active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CancelAction {
    pub actor: StableId,
}
impl Request for CancelAction {
    type Response = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadAction {
    pub actor: StableId,
}
impl Request for ReadAction {
    type Response = Option<ActionSlot>;
}

/// A route invokes the outgoing owner's typed cancellation request. Receiving
/// owners remove their own job and due work without reentering this coordinator.
pub type Cancellation<E> = fn(&mut OwnerTxn<'_, '_, Actions<E>, E>, StableId) -> Result<(), Error>;

/// Immutable owner registrations; mutable action state lives only in ECS rows.
pub struct Actions<E> {
    cancellations: BTreeMap<String, Cancellation<E>>,
}
impl<E: 'static> Default for Actions<E> {
    fn default() -> Self {
        Self::new()
    }
}
impl<E: 'static> Actions<E> {
    pub fn new() -> Self {
        Self {
            cancellations: BTreeMap::new(),
        }
    }

    /// Add structural wiring before the coordinator enters the registry.
    pub fn with_route(
        mut self,
        owner: impl Into<String>,
        cancel: Cancellation<E>,
    ) -> Result<Self, Error> {
        let owner = owner.into();
        if owner.is_empty() || self.cancellations.contains_key(&owner) {
            return Err(Error::InvalidConfiguration(
                "empty or duplicate action owner".into(),
            ));
        }
        self.cancellations.insert(owner, cancel);
        Ok(self)
    }

    /// Register canonical write ownership and typed mutable/readonly contracts.
    pub fn register<S: 'static>(
        self,
        registry: &mut Registry<S, E>,
        slot: RuntimeSlot<S, ()>,
    ) -> Result<(), Error> {
        registry.register(self, slot)?;
        registry.owns::<Self, ActionSlot>()?;
        registry.register_request::<Self, AcquireAction>(Self::acquire)?;
        registry.register_request::<Self, ReleaseAction>(Self::release)?;
        registry.register_request::<Self, InterruptActor>(Self::interrupt)?;
        registry.register_request::<Self, CancelAction>(Self::cancel_action)?;
        registry.register_read_request::<Self, ReadAction>(Self::read)
    }

    pub fn acquire(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: AcquireAction,
    ) -> Result<(), Error> {
        // Every admitted owner must be cancellable by a later lease holder.
        if !self.cancellations.contains_key(&request.owner) {
            return Err(Error::InvalidConfiguration(
                "unregistered action owner".into(),
            ));
        }
        if request.completes.is_some_and(|at| at < tx.now()) {
            return Err(Error::SequenceConflict);
        }
        if let Some(previous) = tx.read::<ActionSlot>(request.actor).cloned() {
            if previous.owner != request.owner {
                self.cancel(tx, &previous)?;
            }
        }
        tx.write(
            request.actor,
            ActionSlot {
                actor: request.actor,
                owner: request.owner,
                started: tx.now(),
                completes: request.completes,
            },
        )
    }

    pub fn release(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ReleaseAction,
    ) -> Result<bool, Error> {
        if tx
            .read::<ActionSlot>(request.actor)
            .is_some_and(|slot| slot.owner == request.owner)
        {
            tx.remove::<ActionSlot>(request.actor)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn interrupt(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: InterruptActor,
    ) -> Result<bool, Error> {
        let Some(previous) = tx.read::<ActionSlot>(request.actor).cloned() else {
            return Ok(false);
        };
        if previous.owner == request.preserve {
            return Ok(false);
        }
        self.cancel(tx, &previous)?;
        tx.remove::<ActionSlot>(request.actor)?;
        Ok(true)
    }

    pub fn read(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadAction,
    ) -> Result<Option<ActionSlot>, Error> {
        Ok(tx.read::<ActionSlot>(request.actor).cloned())
    }

    pub fn cancel_action(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: CancelAction,
    ) -> Result<(), Error> {
        self.interrupt(
            tx,
            InterruptActor {
                actor: request.actor,
                preserve: String::new(),
            },
        )?;
        Ok(())
    }

    fn cancel(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        previous: &ActionSlot,
    ) -> Result<(), Error> {
        let cancel = self.cancellations.get(&previous.owner).ok_or_else(|| {
            Error::InvalidConfiguration("unregistered outgoing action owner".into())
        })?;
        cancel(tx, previous.actor)
    }
}
impl<E: 'static> OwnerSystem<E> for Actions<E> {
    type Runtime = ();
    type Input = ();
    type Output = ();

    fn execute(&self, _tx: &mut OwnerTxn<'_, '_, Self, E>, _input: ()) -> Result<(), Error> {
        Ok(())
    }
}
