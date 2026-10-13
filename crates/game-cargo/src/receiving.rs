//! Typed transaction requests for conserved inventory debits and credits.
use crate::{Inventory, consume_inventory_row, deposit_inventory_row};
use game_core::{Error, Good, InventoryId, PersonId, Quantity};
use game_ecs::StableId;
use game_world::{OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};
use std::collections::BTreeMap;

/// Consume a quantity from one explicitly selected cargo inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CargoConsume {
    pub inventory: InventoryId,
    pub quantity: Quantity,
}
impl Request for CargoConsume {
    type Response = ();
}

/// Deposit a matching good into one explicitly selected cargo inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CargoDeposit {
    pub inventory: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
}
impl Request for CargoDeposit {
    type Response = ();
}

/// A producing owner supplies an already-reserved fresh identity and exact row.
pub struct CreateInventory {
    pub row: Inventory,
}
impl Request for CreateInventory {
    type Response = InventoryId;
}
/// A receiving owner selects placement/access while Cargo conserves the source.
pub struct SplitInventory {
    pub source: InventoryId,
    pub receiving: Inventory,
    pub quantity: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SplitReceipt {
    pub source: InventoryId,
    pub destination: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
}
impl Request for SplitInventory {
    type Response = SplitReceipt;
}

/// Property authorizes the grant; Cargo preserves the actual stock row.
pub struct SetAccess {
    pub inventory: InventoryId,
    pub recipient: PersonId,
    pub allowed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccessReceipt {
    pub inventory: InventoryId,
    pub recipient: PersonId,
    pub allowed: bool,
}
impl Request for SetAccess {
    type Response = AccessReceipt;
}

/// Give actual carried material to the smallest compatible recipient compartment.
pub struct CargoGive {
    pub actor: PersonId,
    pub source: InventoryId,
    pub target: PersonId,
    pub quantity: Quantity,
}
impl Request for CargoGive {
    type Response = game_transport::systems::TransferReceipt;
}

/// Cargo's transaction receiver has no additional runtime state.
#[derive(Default)]
pub struct ReceivingSystems;
impl<E: 'static> OwnerSystem<E> for ReceivingSystems {
    type Runtime = ();
    type Input = ();
    type Output = ();

    fn execute(&self, _tx: &mut OwnerTxn<'_, '_, Self, E>, _input: ()) -> Result<(), Error> {
        Ok(())
    }
}

/// Apply a checked debit to the transaction's current inventory row.
pub fn receive_consume<E: 'static>(
    _owner: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: CargoConsume,
) -> Result<(), Error> {
    let id = StableId(request.inventory.0);
    let row = tx
        .read::<Inventory>(id)
        .cloned()
        .ok_or(Error::Unavailable)?;
    let after = consume_inventory_row(request.inventory, &row, request.quantity)?;
    tx.write(id, after.clone())?;
    tx.publish(CargoEvent::Consumed {
        actor: None,
        before: row,
        after,
    })
}

/// Apply a checked credit to the transaction's current inventory row.
pub fn receive_deposit<E: 'static>(
    _owner: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: CargoDeposit,
) -> Result<(), Error> {
    let id = StableId(request.inventory.0);
    let row = tx
        .read::<Inventory>(id)
        .cloned()
        .ok_or(Error::Unavailable)?;
    let after = deposit_inventory_row(request.inventory, &row, request.good, request.quantity)?;
    tx.write(id, after.clone())?;
    tx.publish(CargoEvent::Deposited { before: row, after })
}

/// Register cargo's inventory write ownership and typed receiving contracts.
pub fn register_cargo_receiving<S: 'static, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, ()>,
) -> Result<(), Error> {
    registry.register(ReceivingSystems, slot)?;
    registry.owns::<ReceivingSystems, Inventory>()?;
    registry.register_request::<ReceivingSystems, CargoConsume>(receive_consume::<E>)?;
    registry.register_request::<ReceivingSystems, CargoDeposit>(receive_deposit::<E>)?;
    registry.register_request::<ReceivingSystems, game_physiology::systems::ConsumeStock>(
        consume_stock::<E>,
    )?;
    registry.register_request::<ReceivingSystems, game_transport::systems::AdmitCargoLoad>(
        admit_load::<E>,
    )?;
    registry.register_request::<ReceivingSystems, game_transport::systems::TransferCargo>(
        transfer::<E>,
    )?;
    registry.register_request::<ReceivingSystems, CargoTransferGroup>(transfer_group::<E>)?;
    registry.register_request::<ReceivingSystems, game_transport::systems::TransferCargoGroup>(
        transport_transfer_group::<E>,
    )?;
    registry.register_request::<ReceivingSystems, CargoGive>(give::<E>)?;
    registry.register_request::<ReceivingSystems, SetAccess>(set_access::<E>)?;
    registry.register_request::<ReceivingSystems, CreateInventory>(create_inventory::<E>)?;
    registry.register_request::<ReceivingSystems, SplitInventory>(split_inventory::<E>)?;
    Ok(())
}

/// Native material observations contain exact Cargo rows and typed transfer receipts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CargoEvent {
    Created {
        row: Inventory,
    },
    Split {
        receipt: SplitReceipt,
        before: Inventory,
        source_after: Inventory,
        receiving: Inventory,
    },
    AccessChanged {
        receipt: AccessReceipt,
        before: Inventory,
        after: Inventory,
    },
    Consumed {
        actor: Option<PersonId>,
        before: Inventory,
        after: Inventory,
    },
    Deposited {
        before: Inventory,
        after: Inventory,
    },
    Transferred {
        receipt: game_transport::systems::TransferReceipt,
        before: [Inventory; 2],
        after: [Inventory; 2],
    },
}
pub fn create_inventory<E: 'static>(
    _: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: CreateInventory,
) -> Result<InventoryId, Error> {
    request.row.validate()?;
    let id = request.row.id;
    if tx.entity_exists(StableId(id.0)) {
        return Err(Error::Unavailable);
    }
    tx.write(StableId(id.0), request.row.clone())?;
    tx.publish(CargoEvent::Created { row: request.row })?;
    Ok(id)
}
pub fn split_inventory<E: 'static>(
    _: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: SplitInventory,
) -> Result<SplitReceipt, Error> {
    let id = request.receiving.id;
    if tx.entity_exists(StableId(id.0)) {
        return Err(Error::Unavailable);
    }
    let before = tx
        .read::<Inventory>(StableId(request.source.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let (source_after, receiving) =
        crate::split_inventory_rows(request.source, &before, request.receiving, request.quantity)?;
    tx.write(StableId(before.id.0), source_after.clone())?;
    tx.write(StableId(id.0), receiving.clone())?;
    let receipt = SplitReceipt {
        source: before.id,
        destination: id,
        good: before.good,
        quantity: request.quantity,
    };
    tx.publish(CargoEvent::Split {
        receipt: receipt.clone(),
        before,
        source_after,
        receiving,
    })?;
    Ok(receipt)
}

pub fn set_access<E: 'static>(
    _: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: SetAccess,
) -> Result<AccessReceipt, Error> {
    if request.recipient.0 == 0 {
        return Err(Error::Unavailable);
    }
    let before = tx
        .read::<Inventory>(StableId(request.inventory.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    if before.id != request.inventory {
        return Err(Error::Unavailable);
    }
    let mut after = before.clone();
    if request.allowed {
        after.access.insert(request.recipient);
    } else {
        after.access.remove(&request.recipient);
    }
    tx.write(StableId(before.id.0), after.clone())?;
    let receipt = AccessReceipt {
        inventory: request.inventory,
        recipient: request.recipient,
        allowed: request.allowed,
    };
    tx.publish(CargoEvent::AccessChanged {
        receipt: receipt.clone(),
        before,
        after,
    })?;
    Ok(receipt)
}

pub fn consume_stock<E: 'static>(
    _: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: game_physiology::systems::ConsumeStock,
) -> Result<(), Error> {
    let before = tx
        .read::<Inventory>(StableId(request.source.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let facts = tx
        .request(game_physiology::systems::ReadStock {
            actor: request.actor,
            source: request.source,
        })?
        .ok_or(Error::Unavailable)?;
    if request.actor.0 == 0
        || !crate::portion_available(
            &before,
            request.quantity,
            crate::ConsumptionFacts {
                good: request.good,
                accessible: facts.accessible,
                contact: facts.contact,
                unreserved: facts.unreserved,
            },
        )
    {
        return Err(Error::Unavailable);
    }
    let mut after = consume_inventory_row(before.id, &before, request.quantity)?;
    after.last_granted = Some(request.actor);
    tx.write(StableId(before.id.0), after.clone())?;
    tx.publish(CargoEvent::Consumed {
        actor: Some(request.actor),
        before,
        after,
    })
}
fn material_rows<E: 'static>(
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    actor: PersonId,
    source: InventoryId,
    destination: InventoryId,
    quantity: Quantity,
) -> Result<
    (
        Inventory,
        Inventory,
        crate::SourceFacts,
        crate::ReceiverFacts,
    ),
    Error,
> {
    let from = tx
        .read::<Inventory>(StableId(source.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let to = tx
        .read::<Inventory>(StableId(destination.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let source_facts = tx
        .request(game_physiology::systems::ReadStock { actor, source })?
        .ok_or(Error::Unavailable)?;
    let receiver_facts = tx
        .request(game_physiology::systems::ReadStock {
            actor,
            source: destination,
        })?
        .ok_or(Error::Unavailable)?;
    let room = match to.location {
        crate::InventoryLocation::Area(_) => Quantity(u64::MAX),
        crate::InventoryLocation::Carried(holder) => {
            let capacity = tx
                .request(game_physiology::services::ReadBody(holder))?
                .map_or(Quantity(u64::MAX), |body| body.rules.carrying_capacity);
            let occupied = tx.request(game_transport::systems::ReadCargoMass(holder))?;
            Quantity(capacity.0.saturating_sub(occupied.0))
        }
    };
    let transport_allows = match to.location {
        crate::InventoryLocation::Carried(holder) if from.location != to.location => {
            tx.request(game_transport::systems::CheckCargoCapacity {
                holder,
                additional: quantity,
            })?
        }
        _ => true,
    };
    let supply = crate::SourceFacts {
        contact: source_facts.contact,
        accessible: source_facts.accessible,
        unreserved: source_facts.unreserved,
        record_permission: tx.request(crate::facts::ReadRecordPermission {
            actor,
            source,
            good: from.good,
            quantity,
        })?,
    };
    let receiver = crate::ReceiverFacts {
        contact: receiver_facts.contact,
        accessible: receiver_facts.accessible,
        accepted: tx.request(crate::facts::ReadReceiptAcceptance {
            actor,
            source,
            destination,
            quantity,
        })?,
        body_room: room,
        transport_allows,
    };
    Ok((from, to, supply, receiver))
}
pub fn admit_load<E: 'static>(
    _: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: game_transport::systems::AdmitCargoLoad,
) -> Result<game_core::SimTime, Error> {
    let (from, to, supply, receiver) = material_rows(
        tx,
        request.actor,
        request.source,
        request.destination,
        request.quantity,
    )?;
    let body = tx
        .request(game_physiology::services::ReadBody(request.actor))?
        .ok_or(Error::Unavailable)?;
    let independent = tx
        .request(game_physiology::systems::ReadAgency(request.actor))?
        .independent;
    let conflict = tx.request(game_physiology::systems::ReadConflict {
        actor: request.actor,
        kind: game_physiology::PersonalActionKind::Load,
    })?;
    let care = tx.request(game_physiology::services::ReadCareReceived(request.actor))?;
    let stationary = tx
        .request(game_transport::services::ReadPresence(request.actor))?
        .is_some_and(|presence| matches!(presence.location, game_core::Location::AtArea(_)));
    crate::prepare_load(
        crate::ActorFacts {
            actor: request.actor,
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent,
            confined: !conflict.allowed,
            in_combat: false,
            receiving_care: care,
            stationary,
            fatigue: body.vitals.fatigue,
            fatigue_limit: body.rules.fatigue_limit,
        },
        &from,
        &to,
        request.quantity,
        supply,
        receiver,
        tx.now(),
        body.rules.loading_time,
    )
    .map(|job| job.completes)
}
pub fn give<E: 'static>(
    owner: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: CargoGive,
) -> Result<game_transport::systems::TransferReceipt, Error> {
    let source = tx
        .read::<Inventory>(StableId(request.source.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    if request.actor == request.target
        || request.quantity.0 == 0
        || source.location != crate::InventoryLocation::Carried(request.actor)
    {
        return Err(Error::Unavailable);
    }
    let destination = tx
        .iter::<Inventory>()
        .into_iter()
        .filter(|(_, stock)| {
            stock.location == crate::InventoryLocation::Carried(request.target)
                && stock.good == source.good
                && stock.room().is_ok_and(|room| room >= request.quantity)
        })
        .map(|(_, stock)| stock.id)
        .min()
        .ok_or(Error::Unavailable)?;
    transfer(
        owner,
        tx,
        game_transport::systems::TransferCargo {
            actor: request.actor,
            source: request.source,
            destination,
            quantity: request.quantity,
        },
    )
}

/// One simultaneous arbitration group freezes its opening stock and carrying room.
/// Separate requests in the same journal observe prior producers' provisional output.
pub struct CargoTransferGroup {
    pub transfers: Vec<game_transport::systems::TransferCargo>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TransferDecision {
    pub intent: crate::TransferIntent,
    pub receipt: Option<game_transport::systems::TransferReceipt>,
}
impl Request for CargoTransferGroup {
    type Response = Vec<TransferDecision>;
}
pub fn transfer<E: 'static>(
    owner: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    request: game_transport::systems::TransferCargo,
) -> Result<game_transport::systems::TransferReceipt, Error> {
    transfer_group(
        owner,
        tx,
        CargoTransferGroup {
            transfers: vec![request],
        },
    )?
    .into_iter()
    .next()
    .and_then(|decision| decision.receipt)
    .ok_or(Error::Unavailable)
}
/// Transport retains its own task order; Cargo arbitrates the physical group.
pub fn transport_transfer_group<E: 'static>(
    owner: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    group: game_transport::systems::TransferCargoGroup,
) -> Result<
    Vec<(
        game_transport::systems::TransferCargo,
        Option<game_transport::systems::TransferReceipt>,
    )>,
    Error,
> {
    let decisions = transfer_group(
        owner,
        tx,
        CargoTransferGroup {
            transfers: group.transfers.clone(),
        },
    )?;
    let mut receipts = BTreeMap::<
        crate::TransferIntent,
        std::collections::VecDeque<Option<game_transport::systems::TransferReceipt>>,
    >::new();
    for decision in decisions {
        receipts
            .entry(decision.intent)
            .or_default()
            .push_back(decision.receipt);
    }
    group
        .transfers
        .into_iter()
        .map(|request| {
            let intent = crate::TransferIntent {
                actor: request.actor,
                source: request.source,
                destination: request.destination,
                quantity: request.quantity,
            };
            let receipt = receipts
                .get_mut(&intent)
                .and_then(|receipts| receipts.pop_front())
                .ok_or(Error::Unavailable)?;
            Ok((request, receipt))
        })
        .collect()
}

pub fn transfer_group<E: 'static>(
    _: &ReceivingSystems,
    tx: &mut OwnerTxn<'_, '_, ReceivingSystems, E>,
    group: CargoTransferGroup,
) -> Result<Vec<TransferDecision>, Error> {
    let mut stocks = BTreeMap::new();
    let mut requests = Vec::new();
    let mut room = BTreeMap::new();
    for request in group.transfers {
        let from = tx.read::<Inventory>(StableId(request.source.0)).cloned();
        let to = tx
            .read::<Inventory>(StableId(request.destination.0))
            .cloned();
        if from.is_none() || to.is_none() {
            if let Some(stock) = from {
                stocks.insert(stock.id, stock);
            }
            if let Some(stock) = to {
                stocks.insert(stock.id, stock);
            }
            requests.push(crate::TransferRequest {
                intent: crate::TransferIntent {
                    actor: request.actor,
                    source: request.source,
                    destination: request.destination,
                    quantity: request.quantity,
                },
                facts: crate::ClosingFacts {
                    alive: false,
                    health: 0,
                    source_contact: false,
                    source_access: false,
                    receiver_contact: false,
                    receiver_access: false,
                    receiver_accepted: false,
                    record_permission: false,
                },
            });
            continue;
        }
        let (from, to, supply, receiver) = material_rows(
            tx,
            request.actor,
            request.source,
            request.destination,
            request.quantity,
        )?;
        let body = tx.request(game_physiology::services::ReadBody(request.actor))?;
        let (alive, health) = body.map_or((true, u64::MAX), |body| {
            (body.vitals.alive, body.vitals.health)
        });
        if let crate::InventoryLocation::Carried(holder) = to.location {
            room.entry(holder).or_insert(receiver.body_room.0);
        }
        requests.push(crate::TransferRequest {
            intent: crate::TransferIntent {
                actor: request.actor,
                source: request.source,
                destination: request.destination,
                quantity: request.quantity,
            },
            facts: crate::ClosingFacts {
                alive,
                health,
                source_contact: supply.contact,
                source_access: supply.accessible && supply.unreserved >= request.quantity,
                receiver_contact: receiver.contact,
                receiver_access: receiver.accessible,
                receiver_accepted: receiver.accepted,
                record_permission: supply.record_permission,
            },
        });
        stocks.insert(from.id, from);
        stocks.insert(to.id, to);
    }
    let rows = stocks.iter().map(|(id, row)| (*id, row)).collect();
    let mut reserved = BTreeMap::<PersonId, u64>::new();
    let mut capacity_error = None;
    let prepared = crate::prepare_transfers_rows(&rows, None, requests, Some(&room), |intent| {
        let crate::InventoryLocation::Carried(holder) = stocks[&intent.destination].location else {
            return true;
        };
        let Some(amount) = reserved
            .get(&holder)
            .copied()
            .unwrap_or(0)
            .checked_add(intent.quantity.0)
        else {
            capacity_error = Some(Error::Overflow);
            return false;
        };
        match tx.request(game_transport::systems::CheckCargoCapacity {
            holder,
            additional: Quantity(amount),
        }) {
            Ok(true) => {
                reserved.insert(holder, amount);
                true
            }
            Ok(false) => false,
            Err(error) => {
                capacity_error = Some(error);
                false
            }
        }
    })?;
    if let Some(error) = capacity_error {
        return Err(error);
    }
    let mut decisions = Vec::new();
    for (intent, rows) in prepared.into_row_effects() {
        let receipt = if let Some((before, after)) = rows {
            tx.write(StableId(after[0].id.0), after[0].clone())?;
            tx.write(StableId(after[1].id.0), after[1].clone())?;
            let receipt = game_transport::systems::TransferReceipt {
                actor: intent.actor,
                source: intent.source,
                destination: intent.destination,
                good: before[0].good,
                quantity: intent.quantity,
            };
            tx.publish(CargoEvent::Transferred {
                receipt: receipt.clone(),
                before,
                after,
            })?;
            Some(receipt)
        } else {
            None
        };
        decisions.push(TransferDecision { intent, receipt });
    }
    Ok(decisions)
}

#[cfg(test)]
mod tests;
