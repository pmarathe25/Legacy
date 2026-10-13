//! Personal guardian delivery requests use the shared planner and real stock.
//! Lineage owns consent/provenance; physical owners execute every handoff.
use crate::*;
use game_logistics::{Destination, KnownDestination, Step};

impl Simulation {
    fn guardian_supply_capable(&self, actor: PersonId) -> bool {
        let Some(body) = game_physiology::body(&self.host.storage, actor) else {
            return false;
        };
        game_lineage::supply_capable(
            game_lineage::DeliveryCapability {
                alive: body.vitals.alive,
                health: body.vitals.health,
                incapacitation: self
                    .host
                    .state
                    .combat
                    .as_ref()
                    .map_or(0, |owner| owner.rules.incapacitation),
                independent: self.independent(actor),
                restricted: self.combat_restricts(actor),
                in_combat: self.in_combat(actor),
            },
            &self.resolved_location(actor),
        )
    }
    pub(crate) fn request_guardian_supply(
        &mut self,
        guardian: PersonId,
        requester: PersonId,
        good: Good,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        self.reconcile_guardian_deliveries()?;
        let lineage = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        if !self.person_index.contains_key(&requester) || !self.person_index.contains_key(&guardian)
        {
            return Err(Error::Unavailable);
        }
        let mind =
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, guardian)
                .ok_or(Error::Unavailable)?;
        let source_carried = game_cargo::inventory(&self.host.storage, source)
            .is_some_and(|i| i.location == InventoryLocation::Carried(guardian));
        let known = game_characters::decisions::stocks(&self.host.storage, guardian)
            .ok_or(Error::Unavailable)?
            .known
            .get(&source)
            .cloned();
        let known_route = game_lineage::supply_route_known(
            source_carried,
            &self.resolved_location(guardian),
            known.as_ref(),
            |from, to| decisions::known_path(&mind.routes, from, to).is_ok(),
        );
        let requester_available =
            game_lineage::requester_available(&self.lineage_participant(requester));
        let signal =
            mind.child_signals
                .get(&requester)
                .map(|signal| game_lineage::NeedObservation {
                    location: signal.location.clone(),
                    observed_at: signal.observed_at,
                    urgency: signal.urgency,
                    hydration_urgency: signal.hydration_urgency,
                    preference: signal.preference,
                });
        let facts = game_lineage::SupplyFacts {
            now: owner_now,
            requester_independent: self.independent(requester),
            requester_available,
            fresh_contact: self.transport_same_place(guardian, requester),
            guardian_capable: self.guardian_supply_capable(guardian),
            conversation_available: self.conversation_actor_available(guardian),
            health_busy: self.health_actor_busy(guardian),
            known_source: known,
            source_route_known: known_route,
            immediate_care: game_lineage::fresh_care(signal.as_ref(), owner_now),
            requester_area: match self.resolved_location(requester) {
                Location::AtArea(area) => Some(area),
                _ => None,
            },
            source_carried,
            room: self.delivery_carrying_room(guardian),
            carrying_capacity: self.body_rules(guardian).carrying_capacity,
            cursor: self.host.next_event() - 1,
        };
        let prepared = lineage.prepare_supply_pending(
            &self.host.storage,
            guardian,
            requester,
            good,
            source,
            quantity,
            &facts,
        )?;
        self.ensure_event_capacity(8)?;
        let step = match prepared.output() {
            game_lineage::SupplyAttempt::Refused(reason) => {
                let reason = *reason;
                self.commit_lineage(prepared)?;
                self.emit(EventKind::GuardianRefused {
                    guardian,
                    requester,
                    reason: reason.into(),
                });
                self.host.bump_revision(1)?;
                return Ok(());
            }
            game_lineage::SupplyAttempt::Accepted(attempt) => {
                self.requested_delivery_step(guardian, attempt.delivery())
            }
        };
        owner_now
            .0
            .checked_add(self.body_rules(guardian).loading_time.0)
            .ok_or(Error::Overflow)?;
        self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        if let Some(refusal) = self
            .host
            .state
            .lineage
            .as_ref()
            .unwrap()
            .prepare_supply_start_failure(&self.host.storage, guardian, &step)?
        {
            let game_lineage::RequestDecision::Refused(reason) = self.commit_lineage(refusal)?
            else {
                unreachable!()
            };
            self.emit(EventKind::GuardianRefused {
                guardian,
                requester,
                reason: reason.into(),
            });
            self.host.bump_revision(1)?;
            return Ok(());
        }
        let game_lineage::SupplyAttempt::Accepted(attempt) = self.commit_lineage(prepared)? else {
            unreachable!()
        };
        if let Err(error) = self.execute_requested_delivery_step(guardian, step) {
            if error == Error::Overflow {
                let change = attempt.prepare_rollback(
                    self.host.state.lineage.as_ref().unwrap(),
                    &self.host.storage,
                )?;
                self.commit_lineage(change)?;
                return Err(error);
            }
            let change = attempt.prepare_finish(
                self.host.state.lineage.as_ref().unwrap(),
                &self.host.storage,
                false,
            )?;
            let game_lineage::RequestDecision::Refused(reason) = self.commit_lineage(change)?
            else {
                unreachable!()
            };
            self.emit(EventKind::GuardianRefused {
                guardian,
                requester,
                reason: reason.into(),
            });
            self.host.bump_revision(1)?;
            return Ok(());
        }
        let change = attempt.prepare_finish(
            self.host.state.lineage.as_ref().unwrap(),
            &self.host.storage,
            true,
        )?;
        self.commit_lineage(change)?;
        self.emit(EventKind::GuardianAccepted {
            guardian,
            requester,
        });
        self.host.bump_revision(1)?;
        Ok(())
    }

    fn requested_delivery_destination(
        &self,
        guardian: PersonId,
        requester: PersonId,
        _accepted_at: SimTime,
    ) -> Option<KnownDestination> {
        self.host.state.lineage.as_ref()?.delivery_destination(
            &self.host.storage,
            guardian,
            requester,
            self.transport_same_place(guardian, requester)
                .then(|| self.delivery_observed_location(requester)),
            self.host.clock().time,
        )
    }
    fn requested_delivery_step(
        &self,
        guardian: PersonId,
        request: &game_lineage::RequestedDelivery,
    ) -> Step<Command> {
        let Destination::Contact(requester) = request.delivery.destination else {
            return Step::Blocked(game_logistics::BlockReason::UnknownDestination);
        };
        let Some(destination) =
            self.requested_delivery_destination(guardian, requester, request.accepted_at)
        else {
            return Step::Blocked(game_logistics::BlockReason::UnknownDestination);
        };
        let tags = request
            .cargo
            .iter()
            .map(|(id, q)| (*id, Quantity(*q)))
            .collect();
        self.plan_delivery_with_source_cargo(
            guardian,
            &request.delivery,
            &destination,
            Quantity(1),
            &tags,
        )
    }
    fn execute_requested_delivery_step(
        &mut self,
        guardian: PersonId,
        step: Step<Command>,
    ) -> Result<(), Error> {
        if let Step::Arrived { source, quantity } = step {
            let Destination::Contact(requester) =
                game_lineage::guardian(&self.host.storage, guardian)
                    .unwrap()
                    .requested_delivery
                    .as_ref()
                    .ok_or(Error::Unavailable)?
                    .delivery
                    .destination
            else {
                return Err(Error::Unavailable);
            };
            return self.start_contact_delivery(guardian, source, requester, quantity);
        }
        let command = self
            .delivery_command(guardian, step)
            .ok_or(Error::Unavailable)?;
        match command {
            Command::Move { destination } => self.start_movement(guardian, destination),
            Command::Carry { source, quantity } => self.carry_good(guardian, source, quantity),
            command => self.debug_actor_command(guardian, command),
        }
    }
    /// Returns true only for an accepted task currently occupying the guardian,
    /// or an actual newly admitted physical action. No abstract progress credit.
    pub(crate) fn drive_guardian_delivery(&mut self, guardian: PersonId) -> Result<bool, Error> {
        self.reconcile_guardian_deliveries()?;
        let world = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        let sensed = world
            .current(&self.host.storage)
            .map(|target| self.resolved_location(target));
        let prepared = world.prepare_delivery_tracking(
            &self.host.storage,
            guardian,
            sensed,
            self.host.clock().time,
        )?;
        let Some(request) = self.commit_lineage(prepared)? else {
            return Ok(false);
        };
        let step = self.requested_delivery_step(guardian, &request);
        let body = game_physiology::body(&self.host.storage, guardian).expect("guardian body");
        let facts = game_lineage::DeliveryCapability {
            alive: body.vitals.alive,
            health: body.vitals.health,
            incapacitation: 0,
            independent: self.independent(guardian),
            restricted: self.combat_restricts(guardian),
            in_combat: self.in_combat(guardian),
        };
        match game_lineage::delivery_step_decision(&step, facts) {
            game_lineage::DeliveryStepDecision::Continue(available) => {
                if available {
                    self.note_requested_delivery(guardian);
                }
                Ok(available)
            }
            game_lineage::DeliveryStepDecision::Stop => Ok(false),
            game_lineage::DeliveryStepDecision::Execute => {
                match self.execute_requested_delivery_step(guardian, step) {
                    Ok(()) => {
                        self.note_requested_delivery(guardian);
                        Ok(true)
                    }
                    Err(Error::Overflow) => Err(Error::Overflow),
                    Err(_) => Ok(false),
                }
            }
        }
    }
    fn note_requested_delivery(&mut self, guardian: PersonId) {
        let prepared = self
            .host
            .state
            .lineage
            .as_ref()
            .unwrap()
            .prepare_delivery_working(&self.host.storage, guardian)
            .expect("accepted delivery owner");
        self.commit_lineage(prepared)
            .expect("delivery working admission");
    }

    pub(crate) fn guardian_accepts_delivery(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> bool {
        game_cargo::inventory(&self.host.storage, destination)
            .is_some_and(|stock| self.guardian_accepts_cargo(actor, source, stock, quantity))
    }
    /// Evaluate the same accepted receipt against a hypothetical empty slot,
    /// before allocation. This does not grant access to any other stock.
    pub(crate) fn guardian_accepts_cargo(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: &Inventory,
        quantity: Quantity,
    ) -> bool {
        let Some(world) = self.host.state.lineage.as_ref() else {
            return false;
        };
        fn facts(stock: &Inventory) -> game_lineage::StockReference {
            game_lineage::StockReference {
                good: stock.good,
                quantity: stock.quantity,
                holder: match stock.location {
                    InventoryLocation::Carried(actor) => Some(actor),
                    _ => None,
                },
            }
        }
        let destination = facts(destination);
        world.accepts_cargo(
            &self.host.storage,
            actor,
            source,
            game_cargo::inventory(&self.host.storage, source).map(facts),
            destination,
            quantity,
            destination
                .holder
                .and_then(|id| game_physiology::body(&self.host.storage, id))
                .is_some_and(|body| body.vitals.alive),
            destination
                .holder
                .is_some_and(|id| self.transport_same_place(actor, id)),
        )
    }
    /// Only actual conserved event receipts move source tags or satisfy a task.
    /// Global stock owners remain authoritative for quantities and custody.
    pub(crate) fn reconcile_guardian_deliveries(&mut self) -> Result<(), Error> {
        let Some(world) = self.host.state.lineage.as_ref() else {
            return Ok(());
        };
        let Some(cursor) = world.delivery_receipt_cursor(&self.host.storage) else {
            return Ok(());
        };
        let receipts = self
            .host
            .events_since(cursor)
            .iter()
            .filter_map(|event| {
                use game_lineage::SupplyReceipt;
                let receipt = match event.event {
                    EventKind::Transferred {
                        actor,
                        source,
                        destination,
                        quantity,
                    } => game_cargo::inventory(&self.host.storage, source).map(|stock| {
                        SupplyReceipt::Transfer {
                            actor,
                            source,
                            destination,
                            quantity,
                            good: stock.good,
                            destination_holder: game_cargo::inventory(
                                &self.host.storage,
                                destination,
                            )
                            .and_then(|stock| match stock.location {
                                InventoryLocation::Carried(id) => Some(id),
                                _ => None,
                            }),
                        }
                    }),
                    EventKind::Ate {
                        source, quantity, ..
                    }
                    | EventKind::Consumed {
                        source, quantity, ..
                    }
                    | EventKind::Cared {
                        source, quantity, ..
                    } => Some(SupplyReceipt::Lost {
                        inventory: source,
                        quantity,
                    }),
                    EventKind::Spoiled {
                        inventory,
                        quantity,
                    } => Some(SupplyReceipt::Lost {
                        inventory,
                        quantity,
                    }),
                    _ => None,
                }?;
                Some((EventId(event.sequence), receipt))
            })
            .collect::<Vec<_>>();
        let prepared = world.prepare_delivery_receipts(
            &self.host.storage,
            &receipts,
            self.host.next_event() - 1,
        )?;
        self.commit_lineage(prepared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        Simulation::force_fixture(91, Execution::default()).unwrap()
    }
    #[test]
    fn guardian_supply_uses_actual_cargo_without_granting_new_incarnation_estate_access() {
        let mut sim = Simulation::force_fixture(91, Execution::default()).unwrap();
        let predecessor = sim.host.state.ruler;
        let guardian = sim.person_at(1).id;
        let maximum = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.debug_injure(predecessor, maximum).unwrap();
        let born = sim.advance(SimDuration(900_000)).unwrap();
        assert!(matches!(born.reason, StopReason::SuccessorBorn { .. }));
        let adulthood = sim.host.state.development.as_ref().unwrap().rules.adulthood;
        sim.advance(adulthood).unwrap();
        let requester = sim.host.state.ruler;
        assert_ne!(requester, predecessor);
        assert_eq!(
            game_lifecycle::life(&sim.host.storage, requester)
                .unwrap()
                .stage,
            LifeStage::Adult
        );
        sim.debug_actor_command(guardian, Command::Cancel).unwrap();
        let source = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Food && stock.location == InventoryLocation::Area(AreaId(1))
            })
            .unwrap()
            .id;
        let destination = sim.person_at(sim.person_index[&requester]).inventory;
        assert!(!sim.has_stock_access(
            requester,
            game_cargo::inventory(&sim.host.storage, source).expect("inventory exists")
        ));
        let opening = game_cargo::inventory(&sim.host.storage, destination)
            .expect("inventory exists")
            .quantity
            .0;
        sim.request_guardian_supply(guardian, requester, Good::Food, source, Quantity(1_000))
            .unwrap();
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::GuardianAccepted {guardian:id,requester:person} if id==guardian&&person==requester)));
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity
                .0,
            opening
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity
                .0,
            opening
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity
                .0,
            opening + 1_000
        );
        assert_eq!(
            1_000
                - game_lineage::guardian(&sim.host.storage, guardian)
                    .unwrap()
                    .requested_delivery
                    .as_ref()
                    .unwrap()
                    .delivery
                    .remaining
                    .0,
            1_000
        );
        assert!(!sim.has_stock_access(
            requester,
            game_cargo::inventory(&sim.host.storage, source).expect("inventory exists")
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn unknown_or_unsafe_food_requests_are_explicitly_refused_without_stock_changes() {
        let mut sim = fixture();
        let requester = sim.host.state.ruler;
        let guardian = sim.person_at(1).id;
        let opening: Vec<_> = game_cargo::inventories(&sim.host.storage)
            .map(|(id, row)| (id, row.clone()))
            .collect();
        sim.request_guardian_supply(
            guardian,
            requester,
            Good::Food,
            InventoryId(999_999),
            Quantity(1_000),
        )
        .unwrap();
        assert_eq!(
            game_cargo::inventories(&sim.host.storage)
                .map(|(id, row)| (id, row.clone()))
                .collect::<Vec<_>>(),
            opening
        );
        assert!(sim.host.events().iter().any(
            |event| matches!(event.event,EventKind::GuardianRefused {guardian:id,..} if id==guardian)
        ));
        assert!(
            game_lineage::guardian(&sim.host.storage, guardian)
                .unwrap()
                .assignment
                .is_none()
        );
    }
    #[test]
    fn a_requested_good_creates_a_real_receiving_compartment_and_credits_only_handoff() {
        let mut sim = fixture();
        let requester = sim.host.state.ruler;
        let guardian = sim.person_at(1).id;
        let area = match sim.resolved_location(requester) {
            Location::AtArea(area) => area,
            _ => panic!("fixture ruler not stationary"),
        };
        if sim.resolved_location(guardian) != Location::AtArea(area) {
            sim.debug_move_actor(guardian, area).unwrap();
        }
        sim.debug_actor_command(guardian, Command::Cancel).unwrap();
        let source = sim
            .add_inventory(
                Good::Water,
                InventoryLocation::Area(area),
                Quantity(1000),
                Quantity(10000),
                BTreeSet::from([guardian]),
            )
            .unwrap();
        let observed_at = sim.time();
        game_characters::decisions::stocks_mut(&mut sim.host.storage, guardian)
            .unwrap()
            .known
            .insert(
                source,
                StockKnowledge {
                    inventory: source,
                    area,
                    good: Good::Water,
                    quantity: Quantity(1000),
                    accessible: true,
                    observed_at,
                    learned_at: observed_at,
                    source: KnowledgeSource::Observed(guardian),
                },
            );
        assert!(
            !sim.inventory_index
                .get(&InventoryLocation::Carried(requester))
                .into_iter()
                .flatten()
                .any(|id| game_cargo::inventory(&sim.host.storage, *(id))
                    .expect("inventory exists")
                    .good
                    == Good::Water)
        );
        sim.request_guardian_supply(guardian, requester, Good::Water, source, Quantity(1000))
            .unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(1000)
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_lineage::guardian(&sim.host.storage, guardian)
                .unwrap()
                .requested_delivery
                .as_ref()
                .unwrap()
                .delivery
                .remaining,
            Quantity(1000)
        );
        let saved = sim.save().unwrap();
        let mut restored = Simulation::load(
            &saved,
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        restored.advance(SimDuration(17000)).unwrap();
        restored.advance(SimDuration(43000)).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        let receiver = sim.inventory_index[&InventoryLocation::Carried(requester)]
            .iter()
            .find(|id| {
                game_cargo::inventory(&sim.host.storage, **id)
                    .expect("inventory exists")
                    .good
                    == Good::Water
            })
            .unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, *(receiver))
                .expect("inventory exists")
                .quantity,
            Quantity(1000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_lineage::guardian(&sim.host.storage, guardian)
                .unwrap()
                .requested_delivery
                .as_ref()
                .unwrap()
                .delivery
                .remaining,
            Quantity(0)
        );
        assert!(!sim.has_stock_access(
            requester,
            game_cargo::inventory(&sim.host.storage, source).expect("inventory exists")
        ));
    }
}
