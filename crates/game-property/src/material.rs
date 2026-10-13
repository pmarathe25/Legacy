//! Physical conservation and claim provenance share a prepared publication.
use crate::*;
use game_cargo::{ClosingFacts, PreparedTransfers, TransferIntent, TransferRequest};
use game_core::*;
use game_ecs::{Batch, ReadGuard, Store};
use std::collections::BTreeMap;

pub struct PreparedMaterialTransfer {
    cargo: PreparedTransfers,
    property: PreparedProperty,
}
impl Runtime {
    /// A caller supplies the actual closing authorization facts and the receipt's
    /// reserved event identity. Cargo decides conservation; Property relocates
    /// conserved claim portions. Neither owner publishes during preparation.
    pub fn prepare_material_transfer(
        &self,
        store: &Store,
        request: TransferRequest,
        event: u64,
    ) -> Result<PreparedMaterialTransfer, Error> {
        let cargo = game_cargo::prepare_transfers(store, None, vec![request], None, |_| true)?;
        if cargo.decisions().iter().any(|(_, granted)| !granted) {
            return Err(Error::Unavailable);
        }
        let (intent, _) = &cargo.decisions()[0];
        let property = self.prepare_material_receipt(store, intent, event)?;
        Ok(PreparedMaterialTransfer { cargo, property })
    }
    /// React to an already Cargo-admitted transfer before its stock batch is
    /// published. Event identity prevents the later event scan applying it twice.
    pub fn prepare_material_receipt(
        &self,
        store: &Store,
        intent: &TransferIntent,
        event: u64,
    ) -> Result<PreparedProperty, Error> {
        self.prepare_material_receipt_with_prefix(store, intent, event, None)
    }
    pub fn prepare_material_receipt_with_prefix(
        &self,
        store: &Store,
        intent: &TransferIntent,
        event: u64,
        prefix: Option<PreparedProperty>,
    ) -> Result<PreparedProperty, Error> {
        let source = game_cargo::inventory(store, intent.source).ok_or(Error::Unavailable)?;
        let destination =
            game_cargo::inventory(store, intent.destination).ok_or(Error::Unavailable)?;
        if intent.quantity.0 == 0
            || intent.source == intent.destination
            || source.good != destination.good
        {
            return Err(Error::Unavailable);
        }
        let source_after = source
            .quantity
            .0
            .checked_sub(intent.quantity.0)
            .ok_or(Error::Unavailable)?;
        let destination_after = destination
            .quantity
            .0
            .checked_add(intent.quantity.0)
            .ok_or(Error::Overflow)?;
        if destination_after > destination.capacity.0 {
            return Err(Error::Unavailable);
        }
        let mut change = prefix.unwrap_or_else(PreparedProperty::new);
        change.validate(self, store)?;
        let through = change
            .cursor
            .map_or(self.last_event, |(_, through)| through);
        if event <= through {
            return Err(Error::Unavailable);
        }
        for stock in [source, destination] {
            if change.current_portions(store, stock.id).is_none() {
                change.set_portions(
                    store,
                    stock.id,
                    if stock.quantity.0 == 0 {
                        BTreeMap::new()
                    } else {
                        BTreeMap::from([(ClaimId(0), stock.quantity.0)])
                    },
                );
            }
            let total = change
                .current_portions(store, stock.id)
                .ok_or(Error::Unavailable)?
                .values()
                .try_fold(0u64, |sum, q| sum.checked_add(*q).ok_or(Error::Overflow))?;
            if total != stock.quantity.0 {
                return Err(Error::Unavailable);
            }
        }
        let (left, moved) = split_portions(
            change
                .current_portions(store, source.id)
                .ok_or(Error::Unavailable)?,
            intent.quantity.0,
        )?;
        let mut receiving = change
            .current_portions(store, destination.id)
            .cloned()
            .ok_or(Error::Unavailable)?;
        for (marker, quantity) in moved {
            let total = receiving.entry(marker).or_default();
            *total = total.checked_add(quantity).ok_or(Error::Overflow)?;
        }
        if left
            .values()
            .try_fold(0u64, |sum, q| sum.checked_add(*q).ok_or(Error::Overflow))?
            != source_after
            || receiving
                .values()
                .try_fold(0u64, |sum, q| sum.checked_add(*q).ok_or(Error::Overflow))?
                != destination_after
        {
            return Err(Error::Unavailable);
        }
        change.set_portions(store, source.id, left);
        change.set_portions(store, destination.id, receiving);
        change.cursor = Some((self.last_event, event));
        Ok(change)
    }
}
impl PreparedMaterialTransfer {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<(PropertyFinalization, ReadGuard), Error> {
        let (_, guard) = self.cargo.stage_guarded_into(store, batch)?;
        let finish = self.property.stage_into(runtime, store, batch)?;
        Ok((finish, guard))
    }
    pub fn commit(self, runtime: &mut Runtime, store: &mut Store) -> Result<Vec<Effect>, Error> {
        let mut batch = Batch::new();
        let (finish, guard) = self.stage_into(runtime, store, &mut batch)?;
        guard.validate(store, &batch)?;
        finish.validate_runtime(runtime)?;
        finish.validate_batch(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
    }
}
impl PreparedProperty {
    /// Property's admitted unauthorized taking and permission effects execute
    /// through Cargo's public physical owner API. Authorization here belongs to
    /// the prepared Property decision, never to legal Inventory access.
    pub fn stage_material_into(
        mut self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
        transfer_event: u64,
    ) -> Result<(PropertyFinalization, Vec<ReadGuard>), Error> {
        let intents: Vec<_> = self
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                Effect::Transfer {
                    actor,
                    source,
                    destination,
                    quantity,
                } => Some(TransferIntent {
                    actor: *actor,
                    source: *source,
                    destination: *destination,
                    quantity: *quantity,
                }),
                _ => None,
            })
            .collect();
        // A Property action produces at most one physical taking receipt.
        if intents.len() > 1 {
            return Err(Error::Unavailable);
        }
        let mut guards = Vec::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        for intent in intents {
            let request = TransferRequest {
                intent: intent.clone(),
                facts: ClosingFacts {
                    alive: true,
                    health: 1,
                    source_contact: true,
                    source_access: true,
                    receiver_contact: true,
                    receiver_access: true,
                    receiver_accepted: false,
                    record_permission: true,
                },
            };
            let cargo = game_cargo::prepare_transfers(store, None, vec![request], None, |_| true)?;
            if cargo.decisions().iter().any(|(_, granted)| !granted) {
                return Err(Error::Unavailable);
            }
            let from = self
                .current_portions(store, intent.source)
                .ok_or(Error::Unavailable)?;
            let (left, moved) = split_portions(from, intent.quantity.0)?;
            let mut target = self
                .current_portions(store, intent.destination)
                .cloned()
                .ok_or(Error::Unavailable)?;
            for (marker, amount) in moved {
                let total = target.entry(marker).or_default();
                *total = total.checked_add(amount).ok_or(Error::Overflow)?;
            }
            self.set_portions(store, intent.source, left);
            self.set_portions(store, intent.destination, target);
            if transfer_event <= runtime.last_event {
                return Err(Error::Unavailable);
            }
            self.cursor = Some((runtime.last_event, transfer_event));
            for id in cargo.closing_stocks().keys() {
                admitted.protect::<game_cargo::Inventory>(game_ecs::StableId(id.0));
            }
            let (_, guard) = cargo.stage_guarded_into(store, batch)?;
            guards.push(guard);
        }
        let permissions: Vec<_> = self
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                Effect::Permission {
                    inventory,
                    recipient,
                    allowed,
                    ..
                } => Some((*inventory, *recipient, *allowed)),
                _ => None,
            })
            .collect();
        if !permissions.is_empty() {
            for (id, _, _) in &permissions {
                admitted.protect::<game_cargo::Inventory>(game_ecs::StableId(id.0));
            }
            guards.push(
                game_cargo::prepare_access_changes(store, &permissions)?
                    .stage_into(store, batch)?,
            );
        }
        let finish = self.stage_with_material(runtime, store, batch, admitted)?;
        Ok((finish, guards))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_cargo::{Inventory, InventoryLocation};
    use std::collections::BTreeSet;

    fn fixture() -> (Runtime, game_health::Runtime, Store) {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        game_health::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        for (id, amount, location) in [
            (10, 1000, InventoryLocation::Area(AreaId(1))),
            (11, 0, InventoryLocation::Carried(PersonId(1))),
        ] {
            game_cargo::insert_inventory(
                &mut store,
                Inventory {
                    id: InventoryId(id),
                    location,
                    quantity: Quantity(amount),
                    capacity: Quantity(2000),
                    access: BTreeSet::from([PersonId(1)]),
                    last_granted: None,
                    good: Good::Water,
                    spoils_at: None,
                },
            )
            .unwrap();
            insert_provenance(
                &mut store,
                InventoryId(id),
                Provenance {
                    portions: if amount == 0 {
                        BTreeMap::new()
                    } else {
                        BTreeMap::from([(ClaimId(7), amount)])
                    },
                },
            )
            .unwrap();
        }
        game_health::insert_water_lot(
            &mut store,
            InventoryId(10),
            game_health::WaterLot::new(1000, 5000),
        )
        .unwrap();
        (Runtime::default(), game_health::Runtime::default(), store)
    }
    fn request(allowed: bool) -> TransferRequest {
        TransferRequest {
            intent: TransferIntent {
                actor: PersonId(1),
                source: InventoryId(10),
                destination: InventoryId(11),
                quantity: Quantity(500),
            },
            facts: ClosingFacts {
                alive: true,
                health: 100,
                source_contact: true,
                source_access: allowed,
                receiver_contact: true,
                receiver_access: true,
                receiver_accepted: false,
                record_permission: true,
            },
        }
    }
    fn spoil_fixture(
        reject_empty_water: bool,
        pending_output: bool,
    ) -> (Runtime, game_health::Runtime, Store) {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        if reject_empty_water {
            schema
                .register::<game_health::WaterLot>("health/water", |lot| {
                    lot.validate_fields()?;
                    if lot.quantity() == 0 {
                        return Err(Error::Unavailable);
                    }
                    Ok(())
                })
                .unwrap();
        } else {
            game_health::register_components(&mut schema).unwrap();
        }
        let mut store = Store::with_schema(schema);
        game_cargo::insert_inventory(
            &mut store,
            Inventory {
                id: InventoryId(10),
                location: InventoryLocation::Area(AreaId(1)),
                good: Good::Water,
                quantity: Quantity(1000),
                capacity: Quantity(2000),
                access: BTreeSet::from([PersonId(1)]),
                last_granted: Some(PersonId(2)),
                spoils_at: Some(SimTime(20)),
            },
        )
        .unwrap();
        insert_provenance(
            &mut store,
            InventoryId(10),
            Provenance {
                portions: BTreeMap::from([(ClaimId(7), if pending_output { 900 } else { 1000 })]),
            },
        )
        .unwrap();
        game_health::insert_water_lot(
            &mut store,
            InventoryId(10),
            game_health::WaterLot::new(1000, 5000),
        )
        .unwrap();
        (Runtime::default(), game_health::Runtime::default(), store)
    }
    fn pending_output(property: &Runtime, store: &Store) -> PreparedProperty {
        property
            .prepare_receipts(
                store,
                &[PropertyReceipt {
                    event: 1,
                    change: ReceiptChange::Produce {
                        outputs: vec![(InventoryId(10), Quantity(100))],
                    },
                }],
                &BTreeMap::from([(InventoryId(10), Quantity(1000))]),
                1,
            )
            .unwrap()
    }
    #[test]
    fn spoilage_loss_and_water_reaction_publish_together_and_keep_pending_provenance_prefix() {
        for pending in [false, true] {
            for reject in [true, false] {
                let (mut property, health, mut store) = spoil_fixture(reject, pending);
                let before = store.snapshot().unwrap();
                let stock = game_cargo::prepare_spoilage(&store, InventoryId(10)).unwrap();
                let expected = stock.closing().clone();
                let prefix = pending.then(|| pending_output(&property, &store));
                let event = if pending { 2 } else { 1 };
                let provenance = property
                    .prepare_loss_receipt(
                        &store,
                        stock.inventory(),
                        stock.quantity(),
                        event,
                        prefix,
                    )
                    .unwrap();
                let water = health
                    .prepare_water_reaction(
                        &store,
                        SimTime(20),
                        game_health::WaterReaction::Spoil {
                            inventory: stock.inventory(),
                        },
                    )
                    .unwrap();
                let mut batch = Batch::new();
                let cargo = stock.stage_into(&store, &mut batch).unwrap();
                let finish = provenance
                    .stage_into(&property, &store, &mut batch)
                    .unwrap();
                let water_finish = water.stage_into(&health, &store, &mut batch).unwrap();
                cargo.validate(&store, &batch).unwrap();
                finish.validate_runtime(&property).unwrap();
                finish.validate_batch(&store, &batch).unwrap();
                water_finish
                    .validate_batch(&health, &store, &batch)
                    .unwrap();
                assert_eq!(store.snapshot().unwrap(), before);
                if reject {
                    assert_eq!(store.apply_batch(batch), Err(Error::Unavailable));
                    assert_eq!(store.snapshot().unwrap(), before);
                    assert_eq!(property.last_event, 0);
                } else {
                    store.apply_batch(batch).unwrap();
                    finish.finish(&mut property);
                    water_finish.finish();
                    assert_eq!(
                        game_cargo::inventory(&store, InventoryId(10)),
                        Some(&expected)
                    );
                    assert!(portions(&store, InventoryId(10)).unwrap().is_empty());
                    assert_eq!(
                        game_health::water_lot(&store, InventoryId(10)).unwrap(),
                        &game_health::WaterLot::new(0, 0)
                    );
                    assert_eq!(property.last_event, event);
                    assert!(
                        property
                            .prepare_loss_receipt(&store, InventoryId(10), Quantity(0), event, None)
                            .is_err()
                    );
                }
            }
        }
    }
    #[test]
    fn loss_receipt_rejects_unreconciled_or_stale_prefix_and_overspending() {
        let (mut property, _, mut store) = spoil_fixture(false, true);
        let before = store.snapshot().unwrap();
        assert!(
            property
                .prepare_loss_receipt(&store, InventoryId(10), Quantity(1000), 2, None)
                .is_err()
        );
        assert!(
            property
                .prepare_loss_receipt(
                    &store,
                    InventoryId(10),
                    Quantity(1001),
                    2,
                    Some(pending_output(&property, &store))
                )
                .is_err()
        );
        assert!(
            property
                .prepare_loss_receipt(
                    &store,
                    InventoryId(10),
                    Quantity(1000),
                    1,
                    Some(pending_output(&property, &store))
                )
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        let loss = property
            .prepare_loss_receipt(
                &store,
                InventoryId(10),
                Quantity(1000),
                2,
                Some(pending_output(&property, &store)),
            )
            .unwrap();
        provenance_mut(&mut store, InventoryId(10))
            .unwrap()
            .portions
            .insert(ClaimId(7), 899);
        let changed = store.snapshot().unwrap();
        assert!(loss.commit(&mut property, &mut store).is_err());
        assert_eq!(store.snapshot().unwrap(), changed);
        assert_eq!(property.last_event, 0);
    }
    #[test]
    fn actual_carried_custody_precedes_declared_area_control() {
        let (_, _, mut store) = fixture();
        declare_custody(&mut store, InventoryId(10), PersonId(2)).unwrap();
        declare_custody(&mut store, InventoryId(11), PersonId(2)).unwrap();
        assert_eq!(
            physical_custodian(&store, InventoryId(10)),
            Some(PersonId(2))
        );
        assert_eq!(
            physical_custodian(&store, InventoryId(11)),
            Some(PersonId(1))
        );
        assert_eq!(physical_custodian(&store, InventoryId(999)), None);
    }
    #[test]
    fn custody_permission_rejects_a_changed_native_stock_and_guards_its_admitted_access_write() {
        let (mut property, _, mut store) = fixture();
        game_cargo::inventory_mut(&mut store, InventoryId(10))
            .unwrap()
            .access
            .clear();
        declare_custody(&mut store, InventoryId(10), PersonId(2)).unwrap();
        insert_attitude(
            &mut store,
            PersonId(2),
            Attitude {
                exclusive_claims: 900,
                sharing: 900,
                willingness_to_defend: 900,
            },
        )
        .unwrap();
        let prepare = |property: &Runtime, store: &Store| {
            property
                .prepare_action(
                    store,
                    ActorFacts {
                        actor: PersonId(1),
                        now: SimTime(0),
                        area: Some(AreaId(1)),
                        capable: true,
                        exertion_ready: true,
                    },
                    &PropertyAction::RequestAccess {
                        inventory: InventoryId(10),
                    },
                    ActionFacts::Request {
                        stock: StockFacts {
                            id: InventoryId(10),
                            area: Some(AreaId(1)),
                            good: Good::Water,
                            quantity: Quantity(1000),
                            room: Quantity(1000),
                            carried_by: None,
                            actor_allowed: false,
                            custodian: Some(PersonId(2)),
                        },
                        custodian: ContactFacts {
                            actor: PersonId(2),
                            area: Some(AreaId(1)),
                            present: true,
                            capable: true,
                            available: true,
                            autonomous: true,
                        },
                        trust: 0,
                        reserve: Quantity(500),
                    },
                    None,
                )
                .unwrap()
        };
        let pending = prepare(&property, &store);
        game_cargo::inventory_mut(&mut store, InventoryId(10))
            .unwrap()
            .quantity = Quantity(400);
        let changed = store.snapshot().unwrap();
        assert!(pending.commit(&mut property, &mut store).is_err());
        assert_eq!(store.snapshot().unwrap(), changed);
        game_cargo::inventory_mut(&mut store, InventoryId(10))
            .unwrap()
            .quantity = Quantity(1000);
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let (finish, guards) = prepare(&property, &store)
            .stage_material_into(&property, &store, &mut batch, 1)
            .unwrap();
        for guard in &guards {
            guard.validate(&store, &batch).unwrap();
        }
        finish.validate_batch(&store, &batch).unwrap();
        batch.remove::<Inventory>(game_ecs::StableId(10));
        assert_eq!(
            finish.validate_batch(&store, &batch),
            Err(Error::Unavailable)
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn material_receipt_keeps_pending_output_prefix_until_joint_transfer_publication() {
        let (mut property, _, mut store) = fixture();
        provenance_mut(&mut store, InventoryId(10))
            .unwrap()
            .portions
            .insert(ClaimId(7), 900);
        let prefix = pending_output(&property, &store);
        let request = request(true);
        let before = store.snapshot().unwrap();
        assert!(
            property
                .prepare_material_receipt(&store, &request.intent, 2)
                .is_err()
        );
        let receipt = property
            .prepare_material_receipt_with_prefix(&store, &request.intent, 2, Some(prefix))
            .unwrap();
        let cargo =
            game_cargo::prepare_transfers(&store, None, vec![request], None, |_| true).unwrap();
        let mut batch = Batch::new();
        let (_, guard) = cargo.stage_guarded_into(&store, &mut batch).unwrap();
        let finish = receipt.stage_into(&property, &store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        finish.validate_runtime(&property).unwrap();
        finish.validate_batch(&store, &batch).unwrap();
        assert_eq!(store.snapshot().unwrap(), before);
        store.apply_batch(batch).unwrap();
        finish.finish(&mut property);
        assert_eq!(
            portions(&store, InventoryId(10)).unwrap(),
            &BTreeMap::from([(ClaimId(7), 500)])
        );
        assert_eq!(
            portions(&store, InventoryId(11)).unwrap(),
            &BTreeMap::from([(ClaimId(0), 100), (ClaimId(7), 400)])
        );
        assert_eq!(property.last_event, 2);
    }
    #[test]
    fn material_and_receiving_health_publish_together_without_assembly() {
        let (mut property, health, mut store) = fixture();
        let before = store.snapshot().unwrap();
        assert!(
            property
                .prepare_material_transfer(&store, request(false), 1)
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        let material = property
            .prepare_material_transfer(&store, request(true), 1)
            .unwrap();
        let water = health
            .prepare_water_reaction(
                &store,
                SimTime(1),
                game_health::WaterReaction::Transfer {
                    actor: PersonId(1),
                    source: InventoryId(10),
                    destination: InventoryId(11),
                    quantity: Quantity(500),
                    source_before: Quantity(1000),
                    destination_before: Quantity(0),
                },
            )
            .unwrap();
        let mut batch = Batch::new();
        let (finish, cargo_guard) = material.stage_into(&property, &store, &mut batch).unwrap();
        let water_finish = water.stage_into(&health, &store, &mut batch).unwrap();
        cargo_guard.validate(&store, &batch).unwrap();
        finish.validate_batch(&store, &batch).unwrap();
        water_finish
            .validate_batch(&health, &store, &batch)
            .unwrap();
        assert_eq!(store.snapshot().unwrap(), before);
        store.apply_batch(batch).unwrap();
        finish.finish(&mut property);
        water_finish.finish();
        assert_eq!(
            game_cargo::inventory(&store, InventoryId(10))
                .unwrap()
                .quantity,
            Quantity(500)
        );
        assert_eq!(portions(&store, InventoryId(11)).unwrap()[&ClaimId(7)], 500);
        assert_eq!(
            game_health::water_lot(&store, InventoryId(11))
                .unwrap()
                .load(),
            2500
        );
        assert_eq!(property.last_event, 1);
        assert!(
            property
                .prepare_material_receipt(&store, &request(true).intent, 1)
                .is_err()
        );
    }
    #[test]
    fn unauthorized_taking_uses_cargo_without_granting_access_and_moves_claims_once() {
        let (mut property, _, mut store) = fixture();
        game_cargo::inventory_mut(&mut store, InventoryId(10))
            .unwrap()
            .access
            .clear();
        let actor = ActorFacts {
            actor: PersonId(1),
            now: SimTime(0),
            area: Some(AreaId(1)),
            capable: true,
            exertion_ready: true,
        };
        let source = StockFacts {
            id: InventoryId(10),
            area: Some(AreaId(1)),
            good: Good::Water,
            quantity: Quantity(1000),
            room: Quantity(1000),
            carried_by: None,
            actor_allowed: false,
            custodian: Some(PersonId(2)),
        };
        let destination = StockFacts {
            id: InventoryId(11),
            quantity: Quantity(0),
            room: Quantity(2000),
            carried_by: Some(PersonId(1)),
            actor_allowed: true,
            custodian: Some(PersonId(1)),
            ..source
        };
        property
            .prepare_action(
                &store,
                actor,
                &PropertyAction::Take {
                    source: source.id,
                    destination: destination.id,
                    quantity: Quantity(500),
                },
                ActionFacts::Take {
                    source,
                    destination,
                    id: IncidentId(30),
                    carrying_room: Quantity(1000),
                    duration: SimDuration(100),
                },
                None,
            )
            .unwrap()
            .commit(&mut property, &mut store)
            .unwrap();
        let change = property
            .prepare_finish_take(
                &store,
                ActorFacts {
                    now: SimTime(100),
                    ..actor
                },
                source,
                destination,
                Quantity(1000),
                None,
            )
            .unwrap();
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let (finish, guards) = change
            .stage_material_into(&property, &store, &mut batch, 1)
            .unwrap();
        for guard in &guards {
            guard.validate(&store, &batch).unwrap();
        }
        finish.validate_batch(&store, &batch).unwrap();
        assert_eq!(store.snapshot().unwrap(), before);
        store.apply_batch(batch).unwrap();
        let effects = finish.finish(&mut property);
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::Transfer { .. }))
                .count(),
            1
        );
        assert_eq!(
            game_cargo::inventory(&store, source.id).unwrap().quantity,
            Quantity(500)
        );
        assert!(
            !game_cargo::inventory(&store, source.id)
                .unwrap()
                .access
                .contains(&actor.actor)
        );
        assert_eq!(portions(&store, destination.id).unwrap()[&ClaimId(7)], 500);
        assert_eq!(
            incident(&store, IncidentId(30)).unwrap().outcome,
            IncidentOutcome::Taken
        );
        assert_eq!(property.last_event, 1);
    }
    #[test]
    fn late_foreign_stock_and_provenance_edits_reject_and_preserve_state() {
        for physical in [true, false] {
            let (property, _, store) = fixture();
            let before = store.snapshot().unwrap();
            let mut batch = Batch::new();
            let (finish, cargo) = property
                .prepare_material_transfer(&store, request(true), 1)
                .unwrap()
                .stage_into(&property, &store, &mut batch)
                .unwrap();
            if physical {
                batch.remove::<Inventory>(game_ecs::StableId(10));
                assert!(cargo.validate(&store, &batch).is_err());
            } else {
                batch.remove::<Provenance>(game_ecs::StableId(11));
                assert!(finish.validate_batch(&store, &batch).is_err());
            }
            assert_eq!(store.snapshot().unwrap(), before);
            assert_eq!(property.last_event, 0);
        }
    }
    #[test]
    fn stale_opening_stock_invalidates_composed_material_plan() {
        let (mut property, _, mut store) = fixture();
        let material = property
            .prepare_material_transfer(&store, request(true), 1)
            .unwrap();
        game_cargo::inventory_mut(&mut store, InventoryId(10))
            .unwrap()
            .quantity = Quantity(999);
        let before = store.snapshot().unwrap();
        assert!(material.commit(&mut property, &mut store).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(property.last_event, 0);
    }
}
