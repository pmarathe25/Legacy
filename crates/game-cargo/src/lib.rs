//! Admission of conserved stock loading and carried compartments.
//! Contacts, permissions, reservations and body/transport room are factual ports
//! supplied by their owners. This crate owns their admission conjunction; it
//! neither locates distant goods nor stores a second stock or activity ledger.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod storage;
pub use storage::{
    insert_inventory, inventories, inventory, inventory_mut, register_components, remove_inventory,
};
#[cfg(test)]
mod storage_tests;

mod planning_projection;
pub use planning_projection::{
    Reservation, ReservationTotals, ToolAllocations, ToolSlots, ToolUse,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InventoryLocation {
    Area(AreaId),
    Carried(PersonId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub id: InventoryId,
    pub location: InventoryLocation,
    pub quantity: Quantity,
    pub capacity: Quantity,
    pub access: BTreeSet<PersonId>,
    pub last_granted: Option<PersonId>,
    pub good: Good,
    pub spoils_at: Option<SimTime>,
}
impl Inventory {
    pub fn room(&self) -> Result<Quantity, Error> {
        self.capacity
            .0
            .checked_sub(self.quantity.0)
            .map(Quantity)
            .ok_or(Error::Unavailable)
    }
}

/// A stock/capacity ceiling only. Rights, accepted duties, record units and
/// transport constraints still require native admission at the chosen amount.
pub fn load_upper_bound(
    source: &Inventory,
    destination: &Inventory,
    supply: SourceFacts,
    receiver: ReceiverFacts,
) -> Result<Quantity, Error> {
    source.room()?;
    let mut bound = source
        .quantity
        .0
        .min(supply.unreserved.0)
        .min(destination.room()?.0);
    if source.location != destination.location {
        bound = bound.min(receiver.body_room.0);
    }
    Ok(Quantity(bound))
}

/// Pickup can allocate a matching compartment; actual compartment selection
/// and all admission constraints remain in prepare_carry.
pub fn pickup_upper_bound(
    source: &Inventory,
    supply: SourceFacts,
    body_room: Quantity,
    compartment_capacity: Quantity,
) -> Result<Quantity, Error> {
    source.room()?;
    Ok(Quantity(
        source
            .quantity
            .0
            .min(supply.unreserved.0)
            .min(body_room.0)
            .min(compartment_capacity.0),
    ))
}
#[derive(Clone, Copy, Debug)]
pub struct ActorFacts {
    pub actor: PersonId,
    pub alive: bool,
    pub health: u64,
    pub independent: bool,
    pub confined: bool,
    pub in_combat: bool,
    pub receiving_care: bool,
    pub stationary: bool,
    pub fatigue: u64,
    pub fatigue_limit: u64,
}
impl ActorFacts {
    fn capable(&self) -> bool {
        self.actor.0 > 0
            && self.alive
            && self.health > 0
            && self.independent
            && !self.confined
            && !self.in_combat
            && !self.receiving_care
            && self.stationary
            && self.fatigue < self.fatigue_limit
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SourceFacts {
    pub contact: bool,
    pub accessible: bool,
    pub unreserved: Quantity,
    pub record_permission: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct ReceiverFacts {
    pub contact: bool,
    pub accessible: bool,
    /// An explicit accepted owner obligation can authorize this actual receipt.
    pub accepted: bool,
    pub body_room: Quantity,
    pub transport_allows: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadJob {
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub completes: SimTime,
}
impl LoadJob {
    /// Structural saved-job validation, without reinterpreting present rights
    /// as the rights at admission. Closing execution must recheck actual stock.
    pub fn validate(
        &self,
        now: SimTime,
        source: &Inventory,
        destination: &Inventory,
    ) -> Result<(), Error> {
        if self.actor.0 == 0
            || self.source != source.id
            || self.destination != destination.id
            || self.source == self.destination
            || self.quantity.0 == 0
            || self.completes <= now
            || source.good != destination.good
            || (source.location != InventoryLocation::Carried(self.actor)
                && destination.location != InventoryLocation::Carried(self.actor))
            || source.room().is_err()
            || destination.room().is_err()
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}
/// Pure admission. No reservation, allocation, transfer or interruption occurs.
#[allow(clippy::too_many_arguments)]
pub fn prepare_load(
    actor: ActorFacts,
    source: &Inventory,
    destination: &Inventory,
    quantity: Quantity,
    supply: SourceFacts,
    receiver: ReceiverFacts,
    now: SimTime,
    duration: SimDuration,
) -> Result<LoadJob, Error> {
    if !actor.capable()
        || source.id.0 == 0
        || destination.id.0 == 0
        || source.id == destination.id
        || quantity.0 == 0
        || source.good != destination.good
        || (source.location != InventoryLocation::Carried(actor.actor)
            && destination.location != InventoryLocation::Carried(actor.actor))
        || !supply.contact
        || !supply.accessible
        || !supply.record_permission
        || supply.unreserved < quantity
        || source.quantity < quantity
        || !receiver.contact
        || !(receiver.accessible || receiver.accepted)
        || destination.room()? < quantity
        || source.room().is_err()
        || (source.location != destination.location
            && (receiver.body_room < quantity || !receiver.transport_allows))
        || duration.0 == 0
    {
        return Err(Error::Unavailable);
    }
    let completes = SimTime(now.0.checked_add(duration.0).ok_or(Error::Overflow)?);
    Ok(LoadJob {
        actor: actor.actor,
        source: source.id,
        destination: destination.id,
        quantity,
        completes,
    })
}
#[derive(Clone, Copy)]
pub struct Compartment<'a> {
    pub stock: &'a Inventory,
    pub receiver: ReceiverFacts,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Existing(InventoryId),
    Proposed(Inventory),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCarry {
    pub selection: Selection,
    pub job: LoadJob,
}
#[derive(Clone, Copy)]
pub struct NewCompartment {
    pub id: InventoryId,
    pub holder: PersonId,
    pub alive: bool,
    pub capacity: Quantity,
    pub receiver: ReceiverFacts,
}
/// Reuses the first eligible compartment in stable ID order. A hypothetical
/// empty slot is returned only after its complete native-load admission passes.
#[allow(clippy::too_many_arguments)]
fn prepare_compartment(
    actor: ActorFacts,
    source: &Inventory,
    quantity: Quantity,
    supply: SourceFacts,
    candidates: &[Compartment<'_>],
    proposed: NewCompartment,
    now: SimTime,
    duration: SimDuration,
) -> Result<PreparedCarry, Error> {
    if !proposed.alive || proposed.holder.0 == 0 {
        return Err(Error::Unavailable);
    }
    let mut ordered: Vec<_> = candidates.iter().collect();
    ordered.sort_by_key(|candidate| candidate.stock.id);
    for candidate in ordered {
        if candidate.stock.location != InventoryLocation::Carried(proposed.holder) {
            continue;
        }
        match prepare_load(
            actor,
            source,
            candidate.stock,
            quantity,
            supply,
            candidate.receiver,
            now,
            duration,
        ) {
            Ok(job) => {
                return Ok(PreparedCarry {
                    selection: Selection::Existing(candidate.stock.id),
                    job,
                });
            }
            Err(Error::Unavailable) => {}
            Err(error) => return Err(error),
        }
    }
    if proposed.id.0 == 0 || candidates.iter().any(|c| c.stock.id == proposed.id) {
        return Err(Error::Unavailable);
    }
    let slot = Inventory {
        id: proposed.id,
        location: InventoryLocation::Carried(proposed.holder),
        quantity: Quantity(0),
        capacity: proposed.capacity,
        access: BTreeSet::from([proposed.holder]),
        last_granted: None,
        good: source.good,
        spoils_at: None,
    };
    let job = prepare_load(
        actor,
        source,
        &slot,
        quantity,
        supply,
        proposed.receiver,
        now,
        duration,
    )?;
    Ok(PreparedCarry {
        selection: Selection::Proposed(slot),
        job,
    })
}
#[cfg(test)]
mod tests;

/// Pickup always brings another stock into the actor's own carried space.
#[allow(clippy::too_many_arguments)]
pub fn prepare_carry(
    actor: ActorFacts,
    source: &Inventory,
    quantity: Quantity,
    supply: SourceFacts,
    candidates: &[Compartment<'_>],
    proposed: NewCompartment,
    now: SimTime,
    duration: SimDuration,
) -> Result<PreparedCarry, Error> {
    if proposed.holder != actor.actor || source.location == InventoryLocation::Carried(actor.actor)
    {
        return Err(Error::Unavailable);
    }
    prepare_compartment(
        actor, source, quantity, supply, candidates, proposed, now, duration,
    )
}
/// A contact handoff starts with the actor's actually held goods and targets a
/// different living holder; accepted rights authorize only this physical receipt.
#[allow(clippy::too_many_arguments)]
pub fn prepare_handoff(
    actor: ActorFacts,
    source: &Inventory,
    quantity: Quantity,
    supply: SourceFacts,
    candidates: &[Compartment<'_>],
    proposed: NewCompartment,
    now: SimTime,
    duration: SimDuration,
) -> Result<PreparedCarry, Error> {
    if proposed.holder == actor.actor || source.location != InventoryLocation::Carried(actor.actor)
    {
        return Err(Error::Unavailable);
    }
    prepare_compartment(
        actor, source, quantity, supply, candidates, proposed, now, duration,
    )
}

mod arbitration;
mod transfers;
pub use arbitration::{
    Balance, Request, arbitrate, arbitrate_with_capacity, arbitrate_with_constraints,
};
pub use transfers::{
    ClosingFacts, PreparedTransfers, TransferIntent, TransferRequest, ValidatedTransfers,
    prepare_transfers,
};
