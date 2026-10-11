//! Personal guardian delivery requests use the shared planner and real stock.
//! Lineage owns consent/provenance; physical owners execute every handoff.
use crate::*;
use game_logistics::{Destination, KnownDestination, Step};

impl Simulation {
    fn guardian_supply_capable(&self, actor: PersonId) -> bool {
        self.host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
            .is_some_and(|body| {
                body.vitals.alive
                    && body.vitals.health
                        > self
                            .host
                            .state
                            .combat
                            .as_ref()
                            .map_or(0, |w| w.rules.incapacitation)
                    && self.independent(actor)
                    && !self.combat_restricts(actor)
                    && !self.in_combat(actor)
                    && matches!(
                        self.resolved_location(actor),
                        Location::AtArea(_) | Location::StoppedOnRoad { .. }
                    )
            })
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
        let source_carried = self
            .host
            .state
            .inventories
            .get(&source)
            .is_some_and(|i| i.location == InventoryLocation::Carried(guardian));
        let known = mind.knowledge.get(&source).cloned();
        let known_route = source_carried
            || known
                .as_ref()
                .is_some_and(|k| match self.resolved_location(guardian) {
                    Location::AtArea(area) => {
                        decisions::known_path(&mind.routes, area, k.area).is_ok()
                    }
                    Location::StoppedOnRoad { from, to, .. } => [from, to]
                        .iter()
                        .any(|area| decisions::known_path(&mind.routes, *area, k.area).is_ok()),
                    _ => false,
                });
        let requester_available = game_physiology::body(&self.host.storage, requester)
            .expect("body exists")
            .vitals
            .alive
            && game_physiology::body(&self.host.storage, requester)
                .expect("body exists")
                .vitals
                .health
                > 0
            && !self.in_combat(requester)
            && matches!(
                *self.host.storage.activity(requester).expect("body exists"),
                Activity::Idle | Activity::Rest
            );
        let facts = game_lineage::SupplyFacts {
            now: owner_now,
            requester_independent: self.independent(requester),
            requester_available,
            fresh_contact: self.transport_same_place(guardian, requester),
            guardian_capable: self.guardian_supply_capable(guardian),
            guardian_free: self.conversation_actor_available(guardian)
                && !self.health_actor_busy(guardian),
            known_source: known,
            source_route_known: known_route,
            immediate_care: mind.child_signals.get(&requester).is_some_and(|s| {
                s.observed_at == owner_now && (s.urgency > 0 || s.hydration_urgency > 0)
            }),
            requester_area: match self.resolved_location(requester) {
                Location::AtArea(area) => Some(area),
                _ => None,
            },
            source_carried,
            room: self.delivery_carrying_room(guardian),
            carrying_capacity: self.body_rules(guardian).carrying_capacity,
            cursor: self.host.next_event() - 1,
        };
        let answer = lineage.judge_supply(guardian, requester, good, source, quantity, &facts)?;
        self.ensure_event_capacity(8)?;
        let delivery = match answer {
            game_lineage::SupplyAnswer::Refuse(reason) => {
                self.refuse_guardian_supply(guardian, requester, reason);
                return Ok(());
            }
            game_lineage::SupplyAnswer::Accept(delivery) => delivery,
        };
        owner_now
            .0
            .checked_add(self.body_rules(guardian).loading_time.0)
            .ok_or(Error::Overflow)?;
        self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        let step = self.requested_delivery_step(guardian, &delivery);
        if !matches!(
            step,
            Step::Carry { .. }
                | Step::Acquire(_)
                | Step::Travel(_)
                | Step::Transfer { .. }
                | Step::Arrived { .. }
        ) {
            self.refuse_guardian_supply(
                guardian,
                requester,
                "I cannot begin a safe delivery from my current observations and carrying space",
            );
            return Ok(());
        }
        let previous = self
            .host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap()
            .requested_delivery
            .replace(delivery);
        if let Err(error) = self.execute_requested_delivery_step(guardian, step) {
            self.host
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&guardian)
                .unwrap()
                .requested_delivery = previous;
            if error == Error::Overflow {
                return Err(error);
            }
            self.refuse_guardian_supply(
                guardian,
                requester,
                "The actual stock, permission or carrying space prevents this delivery",
            );
            return Ok(());
        }
        let g = self
            .host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap();
        g.assignment = Some(game_lineage::Assignment {
            request: GuardianRequest::Supply {
                good,
                source,
                quantity,
            },
            accepted_at: owner_now,
        });
        g.explanation =
            "I accepted the named goods and will use my own access and actual carrying".into();
        self.emit(EventKind::GuardianAccepted {
            guardian,
            requester,
        });
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn refuse_guardian_supply(&mut self, guardian: PersonId, requester: PersonId, reason: &str) {
        self.host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap()
            .explanation = reason.into();
        self.emit(EventKind::GuardianRefused {
            guardian,
            requester,
            reason: reason.into(),
        });
        self.host
            .bump_revision(1)
            .expect("the guardian request preflighted its revision");
    }
    fn requested_delivery_destination(
        &self,
        guardian: PersonId,
        requester: PersonId,
        _accepted_at: SimTime,
    ) -> Option<KnownDestination> {
        let g = self.host.state.lineage.as_ref()?.guardians.get(&guardian)?;
        let (location, observed_at) = if self.transport_same_place(guardian, requester) {
            (
                self.delivery_observed_location(requester),
                self.host.clock().time,
            )
        } else if g.target == Some(requester) {
            let sensed = g.sensed_location.as_ref()?;
            let position = crate::transport::location_position(sensed, g.sensed_at).ok()?;
            let location = match position {
                game_transport::Position::Area(area) => Location::AtArea(area),
                game_transport::Position::Road(p) => Location::StoppedOnRoad {
                    from: p.from,
                    to: p.to,
                    progress: p.progress,
                    duration: p.duration,
                },
            };
            (location, g.sensed_at)
        } else {
            return None;
        };
        Some(KnownDestination {
            destination: Destination::Contact(requester),
            location,
            observed_at,
            accessible: true,
            room: None,
            direct_transfer: false,
        })
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
                self.host.state.lineage.as_ref().unwrap().guardians[&guardian]
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
        let owner_now = self.host.clock().time;
        self.reconcile_guardian_deliveries()?;
        let Some(request) = self
            .host
            .state
            .lineage
            .as_ref()
            .and_then(|w| w.guardians.get(&guardian))
            .and_then(|g| g.requested_delivery.as_ref())
            .cloned()
        else {
            return Ok(false);
        };
        let Destination::Contact(requester) = request.delivery.destination else {
            return Ok(false);
        };
        if self.host.state.lineage.as_ref().unwrap().current() != Some(requester) {
            self.cancel_guardian_delivery(guardian);
            return Ok(false);
        }
        let sensed = self.resolved_location(requester);
        let g = self
            .host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap();
        g.target = Some(requester);
        g.sensed_location = Some(sensed);
        g.sensed_at = owner_now;
        if request.delivery.remaining.0 == 0 {
            return Ok(false);
        }
        let step = self.requested_delivery_step(guardian, &request);
        if matches!(step, Step::Continue) {
            let available = game_physiology::body(&self.host.storage, guardian)
                .expect("body exists")
                .vitals
                .alive
                && self.independent(guardian)
                && !self.combat_restricts(guardian)
                && !self.in_combat(guardian);
            if available {
                self.note_requested_delivery(guardian);
            }
            return Ok(available);
        }
        if matches!(step, Step::Complete | Step::Blocked(_)) {
            return Ok(false);
        }
        match self.execute_requested_delivery_step(guardian, step) {
            Ok(()) => {
                self.note_requested_delivery(guardian);
                Ok(true)
            }
            Err(Error::Overflow) => Err(Error::Overflow),
            Err(_) => Ok(false),
        }
    }
    fn note_requested_delivery(&mut self, guardian: PersonId) {
        let g = self
            .host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap();
        g.goal = game_lineage::GuardianGoal::Mentoring;
        g.explanation =
            "Carrying out an accepted supply delivery using known stores and actual cargo".into();
    }
    pub(crate) fn cancel_guardian_delivery(&mut self, guardian: PersonId) {
        if let Some(g) = self
            .host
            .state
            .lineage
            .as_mut()
            .and_then(|w| w.guardians.get_mut(&guardian))
        {
            g.requested_delivery = None;
        }
    }
    pub(crate) fn guardian_accepts_delivery(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> bool {
        self.host
            .state
            .inventories
            .get(&destination)
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
        self.host
            .state
            .lineage
            .as_ref()
            .and_then(|w| w.guardians.get(&actor))
            .and_then(|g| g.requested_delivery.as_ref())
            .is_some_and(|request| {
                let Destination::Contact(requester) = request.delivery.destination else {
                    return false;
                };
                quantity.0 > 0
                    && quantity <= request.delivery.remaining
                    && request.cargo.get(&source).is_some_and(|q| *q >= quantity.0)
                    && self.host.state.inventories.get(&source).is_some_and(|i| {
                        i.good == request.delivery.requirement.good
                            && i.location == InventoryLocation::Carried(actor)
                    })
                    && destination.good == request.delivery.requirement.good
                    && destination.location == InventoryLocation::Carried(requester)
                    && self.host.state.physical.as_ref().is_some_and(|_| {
                        game_physiology::body(&self.host.storage, requester)
                            .expect("body exists")
                            .vitals
                            .alive
                    })
                    && self.transport_same_place(actor, requester)
            })
    }
    /// Only actual conserved event receipts move source tags or satisfy a task.
    /// Global stock owners remain authoritative for quantities and custody.
    pub(crate) fn reconcile_guardian_deliveries(&mut self) -> Result<(), Error> {
        let Some(lineage) = &self.host.state.lineage else {
            return Ok(());
        };
        let actors = lineage
            .guardians
            .iter()
            .filter_map(|(p, g)| g.requested_delivery.as_ref().map(|r| (*p, r.last_event)))
            .collect::<Vec<_>>();
        for (guardian, cursor) in actors {
            let events = self
                .host
                .events()
                .iter()
                .skip(cursor as usize)
                .map(|e| (EventId(e.sequence), e.event.clone()))
                .collect::<Vec<_>>();
            for (sequence, event) in events {
                use game_lineage::SupplyReceipt;
                let receipt =
                    match event {
                        EventKind::Transferred {
                            actor,
                            source,
                            destination,
                            quantity,
                        } => self.host.state.inventories.get(&source).map(|s| {
                            SupplyReceipt::Transfer {
                                actor,
                                source,
                                destination,
                                quantity,
                                good: s.good,
                                destination_holder: self
                                    .host
                                    .state
                                    .inventories
                                    .get(&destination)
                                    .and_then(|d| match d.location {
                                        InventoryLocation::Carried(p) => Some(p),
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
                    };
                if let Some(receipt) = receipt {
                    self.host
                        .state
                        .lineage
                        .as_mut()
                        .unwrap()
                        .guardians
                        .get_mut(&guardian)
                        .unwrap()
                        .requested_delivery
                        .as_mut()
                        .unwrap()
                        .receive(guardian, sequence, &receipt)?;
                }
            }
            self.host
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&guardian)
                .unwrap()
                .requested_delivery
                .as_mut()
                .unwrap()
                .last_event = self.host.next_event() - 1;
        }
        Ok(())
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
        let guardian = sim.host.state.people[1].id;
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
        let source = sim
            .host
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Food && stock.location == InventoryLocation::Area(AreaId(1))
            })
            .unwrap()
            .id;
        let destination = sim.host.state.people[sim.person_index[&requester]].inventory;
        assert!(!sim.has_stock_access(requester, &sim.host.state.inventories[&source]));
        let opening = sim.host.state.inventories[&destination].quantity.0;
        sim.request_guardian_supply(guardian, requester, Good::Food, source, Quantity(1_000))
            .unwrap();
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::GuardianAccepted {guardian:id,requester:person} if id==guardian&&person==requester)));
        assert_eq!(sim.host.state.inventories[&destination].quantity.0, opening);
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.host.state.inventories[&destination].quantity.0, opening);
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.host.state.inventories[&destination].quantity.0,
            opening + 1_000
        );
        assert_eq!(
            1_000
                - sim.host.state.lineage.as_ref().unwrap().guardians[&guardian]
                    .requested_delivery
                    .as_ref()
                    .unwrap()
                    .delivery
                    .remaining
                    .0,
            1_000
        );
        assert!(!sim.has_stock_access(requester, &sim.host.state.inventories[&source]));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn unknown_or_unsafe_food_requests_are_explicitly_refused_without_stock_changes() {
        let mut sim = fixture();
        let requester = sim.host.state.ruler;
        let guardian = sim.host.state.people[1].id;
        let opening = sim.host.state.inventories.clone();
        sim.request_guardian_supply(
            guardian,
            requester,
            Good::Food,
            InventoryId(999_999),
            Quantity(1_000),
        )
        .unwrap();
        assert_eq!(sim.host.state.inventories, opening);
        assert!(sim.host.events().iter().any(
            |event| matches!(event.event,EventKind::GuardianRefused {guardian:id,..} if id==guardian)
        ));
        assert!(
            sim.host.state.lineage.as_ref().unwrap().guardians[&guardian]
                .assignment
                .is_none()
        );
    }
    #[test]
    fn a_requested_good_creates_a_real_receiving_compartment_and_credits_only_handoff() {
        let mut sim = fixture();
        let requester = sim.host.state.ruler;
        let guardian = sim.host.state.people[1].id;
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
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, guardian)
            .unwrap()
            .knowledge
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
                .any(|id| sim.host.state.inventories[id].good == Good::Water)
        );
        sim.request_guardian_supply(guardian, requester, Good::Water, source, Quantity(1000))
            .unwrap();
        assert_eq!(sim.host.state.inventories[&source].quantity, Quantity(1000));
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.host.state.lineage.as_ref().unwrap().guardians[&guardian]
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
        assert_eq!(sim.host.state, restored.host.state);
        let receiver = sim.inventory_index[&InventoryLocation::Carried(requester)]
            .iter()
            .find(|id| sim.host.state.inventories[id].good == Good::Water)
            .unwrap();
        assert_eq!(
            sim.host.state.inventories[receiver].quantity,
            Quantity(1000)
        );
        assert_eq!(sim.host.state.inventories[&source].quantity, Quantity(0));
        assert_eq!(
            sim.host.state.lineage.as_ref().unwrap().guardians[&guardian]
                .requested_delivery
                .as_ref()
                .unwrap()
                .delivery
                .remaining,
            Quantity(0)
        );
        assert!(!sim.has_stock_access(requester, &sim.host.state.inventories[&source]));
    }
}
