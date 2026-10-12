//! Assertions follow real fungible portions. Recognition, custody, permission,
//! and physical possession remain separate; observers choose their response.
use crate::*;

pub use game_property::{Attitude, Claim, Incident, IncidentKnowledge, Runtime};

impl Simulation {
    pub fn property_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "property-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let custodians = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .map(|stock| {
                (
                    stock.id,
                    match stock.location {
                        InventoryLocation::Carried(person) => person,
                        InventoryLocation::Area(_) => sim.host.state.ruler,
                    },
                )
            })
            .collect();
        let __custody: BTreeMap<InventoryId, PersonId> = custodians;
        for (id, owner) in __custody {
            game_property::declare_custody(&mut sim.host.storage, id, owner)?;
        }
        sim.host.state.agreements = Some(game_agreements::Runtime {
            last_event: sim.host.next_event() - 1,
            last_observation_event: sim.host.next_event() - 1,
        });
        let custodian = sim.host.state.people[3].id;
        let food = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Food
                    && stock.location == InventoryLocation::Area(sim.host.state.areas[0].id)
            })
            .unwrap()
            .id;
        game_cargo::inventory_mut(&mut sim.host.storage, food)
            .unwrap()
            .access = BTreeSet::from([custodian]);
        game_property::declare_custody(&mut sim.host.storage, food, custodian)
            .expect("declared custody");
        let attitudes: BTreeMap<PersonId, Attitude> = sim
            .host
            .state
            .people
            .iter()
            .enumerate()
            .map(|(index, person)| {
                (
                    person.id,
                    Attitude {
                        exclusive_claims: if index % 3 == 0 { 900 } else { 300 },
                        sharing: if index % 3 == 1 { 900 } else { 300 },
                        willingness_to_defend: if index % 3 == 0 { 900 } else { 100 },
                    },
                )
            })
            .collect();
        let portions: BTreeMap<InventoryId, BTreeMap<ClaimId, u64>> =
            game_cargo::inventories(&sim.host.storage)
                .map(|(_, row)| row)
                .map(|stock| {
                    (
                        stock.id,
                        if stock.quantity.0 > 0 {
                            BTreeMap::from([(ClaimId(0), stock.quantity.0)])
                        } else {
                            BTreeMap::new()
                        },
                    )
                })
                .collect();
        let mut batch = game_ecs::Batch::new();
        for (actor, row) in attitudes {
            batch.insert(game_ecs::StableId(actor.0), row);
        }
        for (inventory, parts) in portions {
            batch.insert(
                game_ecs::StableId(inventory.0),
                game_property::Provenance { portions: parts },
            );
        }
        sim.host.storage.apply_batch(batch)?;
        sim.host.state.property = Some(Runtime {
            last_event: sim.host.next_event() - 1,
        });
        sim.debug_property_action(
            custodian,
            PropertyAction::Claim {
                inventory: food,
                rights: BTreeSet::from([
                    ClaimRight::Consume,
                    ClaimRight::Transfer,
                    ClaimRight::GrantAccess,
                ]),
                evidence: Vec::new(),
            },
        )?;
        // Initial permission knowledge is authored alongside the fixture's
        // exclusive store, rather than retaining the earlier shared fixture.
        let actors: Vec<_> =
            game_characters::decisions::minds::<game_api::Command>(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
        for person in actors {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                person,
            )
            .unwrap();
            if let Some(fact) = mind.knowledge.get_mut(&food) {
                fact.accessible = person == custodian;
            }
        }
        Ok(sim)
    }

    fn property_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive {
            return None;
        }
        match *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))? {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn property_custodian(&self, inventory: InventoryId) -> Option<PersonId> {
        let stock = game_cargo::inventory(&self.host.storage, inventory)?;
        match stock.location {
            InventoryLocation::Carried(actor) => Some(actor),
            InventoryLocation::Area(_) => {
                game_property::inventory_custodian(&self.host.storage, inventory)
            }
        }
    }
    fn property_present(&self, area: AreaId) -> Vec<PersonId> {
        self.host
            .state
            .people
            .iter()
            .filter(|person| self.property_area(person.id) == Some(area))
            .map(|p| p.id)
            .collect()
    }
    fn property_able(&self, actor: PersonId) -> bool {
        self.property_area(actor).is_some()
            && self.independent(actor)
            && !self.combat_restricts(actor)
            && !self.care_index.contains_key(&actor)
            && game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .health
                > 0
    }
    fn property_actor_facts(&self, actor: PersonId) -> game_property::ActorFacts {
        game_property::ActorFacts {
            actor,
            now: self.host.clock().time,
            area: self.property_area(actor),
            capable: self.property_able(actor),
            exertion_ready: self
                .host
                .state
                .physical
                .as_ref()
                .and_then(|w| {
                    game_physiology::body(&self.host.storage, actor)
                        .map(|body| body.vitals.fatigue < w.rules.fatigue_limit)
                })
                .unwrap_or(false),
        }
    }
    fn property_stock_facts(
        &self,
        actor: PersonId,
        id: InventoryId,
    ) -> Result<game_property::StockFacts, Error> {
        let stock = game_cargo::inventory(&self.host.storage, id).ok_or(Error::Unavailable)?;
        Ok(game_property::StockFacts {
            id,
            area: self.inventory_area(stock),
            good: stock.good,
            quantity: stock.quantity,
            room: Quantity(stock.capacity.0.saturating_sub(stock.quantity.0)),
            carried_by: match stock.location {
                InventoryLocation::Carried(holder) => Some(holder),
                _ => None,
            },
            actor_allowed: stock.access.contains(&actor),
            custodian: self.property_custodian(id),
        })
    }
    fn property_contact_facts(&self, actor: PersonId) -> game_property::ContactFacts {
        game_property::ContactFacts {
            actor,
            area: self.property_area(actor),
            present: self.property_area(actor).is_some(),
            capable: self.property_able(actor),
            available: self.conversation_actor_available(actor),
            autonomous: actor != self.host.state.ruler,
        }
    }
    fn taking_possible(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        require_denial: bool,
    ) -> bool {
        self.property_stock_facts(actor, source)
            .ok()
            .zip(self.property_stock_facts(actor, destination).ok())
            .is_some_and(|(source, destination)| {
                game_property::taking_possible(
                    self.property_actor_facts(actor),
                    source,
                    destination,
                    quantity,
                    if self.transfer_capacity(destination.id, quantity.0) {
                        quantity
                    } else {
                        Quantity(0)
                    },
                    require_denial,
                )
            })
    }
    fn property_observer_facts(
        &self,
        observer: PersonId,
        incident: &Incident,
    ) -> game_property::ObserverFacts {
        let local = self.property_area(observer) == Some(incident.area);
        let custody = local && self.property_custodian(incident.source) == Some(observer);
        let allowed = custody
            && game_cargo::inventory(&self.host.storage, incident.source)
                .is_some_and(|stock| stock.access.contains(&incident.actor));
        game_property::ObserverFacts {
            actor: observer,
            personal_custody: custody && !allowed,
            personally_allowed: allowed,
            can_defend: observer != self.host.state.ruler && self.property_able(observer),
        }
    }
    pub fn debug_set_property_attitude(
        &mut self,
        actor: PersonId,
        attitude: Attitude,
    ) -> Result<(), Error> {
        if attitude.validate().is_err() || !self.person_index.contains_key(&actor) {
            return Err(Error::InvalidFixture("invalid property attitude".into()));
        }
        self.host
            .state
            .property
            .as_ref()
            .ok_or(Error::Unavailable)?;
        game_property::insert_attitude(&mut self.host.storage, actor, attitude)?;
        Ok(())
    }

    pub fn debug_property_action(
        &mut self,
        actor: PersonId,
        action: PropertyAction,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        use game_property::ActionFacts;
        let world = self
            .host
            .state
            .property
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let facts = match &action {
            PropertyAction::Claim { inventory, .. } => ActionFacts::Claim {
                stock: self.property_stock_facts(actor, *inventory)?,
                id: ClaimId(self.host.next_id()),
                witnesses: self
                    .property_area(actor)
                    .map(|area| self.property_present(area))
                    .unwrap_or_default(),
            },
            PropertyAction::Recognize { .. } | PropertyAction::Contest { .. } => {
                ActionFacts::Recognition {
                    witnesses: self
                        .property_area(actor)
                        .map(|area| self.property_present(area))
                        .unwrap_or_default(),
                }
            }
            PropertyAction::RequestAccess { inventory } => {
                let stock = self.property_stock_facts(actor, *inventory)?;
                let custodian = stock.custodian.ok_or(Error::Unavailable)?;
                ActionFacts::Request {
                    stock,
                    custodian: self.property_contact_facts(custodian),
                    trust: self
                        .host
                        .state
                        .decisions
                        .as_ref()
                        .and_then(|_| {
                            game_characters::decisions::mind::<game_api::Command>(
                                &self.host.storage,
                                custodian,
                            )
                        })
                        .and_then(|m| m.relationships.get(&actor))
                        .map_or(0, |r| r.trust),
                    reserve: Quantity(
                        self.host
                            .state
                            .physical
                            .as_ref()
                            .ok_or(Error::Unavailable)?
                            .rules
                            .meal_size
                            .0
                            .saturating_mul(2),
                    ),
                }
            }
            PropertyAction::Share {
                inventory,
                recipient,
                ..
            } => ActionFacts::Share {
                stock: self.property_stock_facts(actor, *inventory)?,
                recipient: self.property_contact_facts(*recipient),
            },
            PropertyAction::Take {
                source,
                destination,
                quantity,
            } => ActionFacts::Take {
                source: self.property_stock_facts(actor, *source)?,
                destination: self.property_stock_facts(actor, *destination)?,
                id: IncidentId(self.host.next_id()),
                carrying_room: if self.transfer_capacity(*destination, quantity.0) {
                    *quantity
                } else {
                    Quantity(0)
                },
                duration: self
                    .host
                    .state
                    .physical
                    .as_ref()
                    .ok_or(Error::Unavailable)?
                    .rules
                    .loading_time,
            },
            PropertyAction::ReportIncident { recipient, .. } => ActionFacts::Report {
                recipient: self.property_contact_facts(*recipient),
                duration: MINUTE,
            },
            PropertyAction::ResistTaking { thief } => ActionFacts::Resist {
                thief: self.property_contact_facts(*thief),
            },
        };
        let prefix = self.prepare_property_receipts()?;
        let change = world.prepare_action(
            &self.host.storage,
            self.property_actor_facts(actor),
            &action,
            facts,
            prefix,
        )?;
        self.apply_property_change(change)?;
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn apply_property_change(
        &mut self,
        change: game_property::PreparedProperty,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        use game_property::Effect;
        let mut next_id = self.host.next_id();
        let mut event_bound = 0usize;
        for effect in change.effects() {
            match effect {
                Effect::ClaimPublished { claim, .. } => {
                    if claim.0 != next_id {
                        return Err(Error::Unavailable);
                    }
                    next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
                }
                Effect::StartTake { incident, .. } => {
                    if incident.0 != next_id {
                        return Err(Error::Unavailable);
                    }
                    next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
                }
                Effect::Engage { defender, thief } => {
                    self.preflight_start_encounter(&[(*defender, 0), (*thief, 1)])?;
                }
                _ => {}
            }
            event_bound = event_bound
                .checked_add(match effect {
                    Effect::Interrupt { .. } => 128,
                    Effect::StartTake { .. } | Effect::Engage { .. } => {
                        self.host.state.people.len() * 8 + 8
                    }
                    Effect::ClearTaking { .. }
                    | Effect::Relationship { .. }
                    | Effect::StartReport { .. } => 0,
                    _ => 1,
                })
                .ok_or(Error::Overflow)?;
        }
        // An automatic local defender may need a map and encounter after the attempt.
        if change
            .effects()
            .iter()
            .any(|effect| matches!(effect, Effect::StartTake { .. }))
        {
            next_id.checked_add(3).ok_or(Error::Overflow)?;
        }
        self.ensure_event_capacity(event_bound)?;
        let mut batch = game_ecs::Batch::new();
        let transfers = change
            .effects()
            .iter()
            .filter_map(|effect| {
                if let Effect::Transfer {
                    actor,
                    source,
                    destination,
                    quantity,
                } = effect
                {
                    // The property owner already admitted this physical taking.
                    // Its authorization does not grant legal stock access.
                    Some(game_cargo::TransferRequest {
                        intent: TransferIntent {
                            actor: *actor,
                            source: *source,
                            destination: *destination,
                            quantity: *quantity,
                        },
                        facts: game_cargo::ClosingFacts {
                            alive: true,
                            health: 1,
                            source_contact: true,
                            source_access: true,
                            receiver_contact: true,
                            receiver_access: true,
                            receiver_accepted: false,
                            record_permission: true,
                        },
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if !transfers.is_empty() {
            let prepared =
                game_cargo::prepare_transfers(&self.host.storage, None, transfers, None, |_| true)?;
            if prepared.decisions().iter().any(|(_, granted)| !granted) {
                return Err(Error::Unavailable);
            }
            prepared.stage_into(&self.host.storage, &mut batch)?;
        }
        let finish = change.stage_into(
            self.host
                .state
                .property
                .as_ref()
                .ok_or(Error::Unavailable)?,
            &self.host.storage,
            &mut batch,
        )?;
        self.host.storage.apply_batch(batch)?;
        let effects = finish.finish(
            self.host
                .state
                .property
                .as_mut()
                .ok_or(Error::Unavailable)?,
        );
        self.host.allocate_ids(
            (next_id)
                .checked_sub(self.host.next_id())
                .ok_or(Error::Overflow)?,
        )?;
        for effect in effects {
            match effect {
                Effect::Interrupt { actor } => self.interrupt_actor_activity_except_property(actor),
                Effect::ClearTaking { actor } => {
                    let activity = self.host.storage.activity_mut(actor).unwrap();
                    if matches!(activity, Activity::Take { .. }) {
                        *activity = Activity::Idle;
                    }
                    for due in self.host.state.due.values_mut() {
                        due.retain(|d| !matches!(d,Due::Physical(id) if *id==actor));
                    }
                    self.host.state.due.retain(|_, due| !due.is_empty());
                }
                Effect::ClearReport {
                    sender,
                    completes,
                    interrupted,
                } => {
                    let activity = self.host.storage.activity_mut(sender).unwrap();
                    if matches!(*activity,Activity::ReportIncident {completes:at,..} if at==completes)
                    {
                        *activity = Activity::Idle;
                    }
                    if let Some(due) = self.host.state.due.get_mut(&completes) {
                        due.retain(|d| !matches!(d,Due::Physical(id) if *id==sender));
                    }
                    self.host.state.due.retain(|_, due| !due.is_empty());
                    if interrupted {
                        self.emit(EventKind::Interrupted { actor: sender });
                    }
                }
                Effect::ClaimPublished {
                    claim,
                    actor,
                    inventory,
                } => {
                    self.emit(EventKind::PropertyClaimed {
                        claim,
                        actor,
                        inventory,
                    });
                }
                Effect::Recognized {
                    claim,
                    actor,
                    support,
                } => {
                    self.emit(EventKind::ClaimRecognized {
                        claim,
                        actor,
                        support,
                    });
                }
                Effect::Requested {
                    actor,
                    custodian,
                    inventory,
                } => {
                    self.emit(EventKind::AccessRequested {
                        actor,
                        custodian,
                        inventory,
                    });
                }
                Effect::Refused {
                    actor,
                    custodian,
                    inventory,
                } => {
                    self.emit(EventKind::AccessRefused {
                        actor,
                        custodian,
                        inventory,
                    });
                }
                Effect::Permission {
                    actor,
                    inventory,
                    recipient,
                    allowed,
                } => {
                    let access = &mut game_cargo::inventory_mut(&mut self.host.storage, inventory)
                        .unwrap()
                        .access;
                    if allowed {
                        access.insert(recipient);
                    } else {
                        access.remove(&recipient);
                    }
                    if let Some(fact) = self
                        .host
                        .state
                        .decisions
                        .as_mut()
                        .and_then(|_| {
                            game_characters::decisions::mind_mut::<game_api::Command>(
                                &mut self.host.storage,
                                recipient,
                            )
                        })
                        .and_then(|m| m.knowledge.get_mut(&inventory))
                    {
                        fact.accessible = allowed;
                        fact.learned_at = owner_now;
                        fact.source = KnowledgeSource::Reported(actor);
                    }
                    self.emit(EventKind::PermissionShared {
                        actor,
                        inventory,
                        recipient,
                        allowed,
                    });
                }
                Effect::StartTake {
                    incident,
                    actor,
                    source,
                    destination,
                    quantity,
                    completes,
                } => {
                    *self.host.storage.activity_mut(actor).unwrap() = Activity::Take {
                        source,
                        destination,
                        quantity,
                        completes,
                    };
                    self.host
                        .state
                        .due
                        .entry(completes)
                        .or_default()
                        .push(Due::Physical(actor));
                    self.emit(EventKind::TakingAttempted {
                        incident,
                        actor,
                        source,
                        destination,
                        quantity,
                    });
                    self.witness_property_incident(incident)?;
                    self.review_property_resistance()?;
                }
                Effect::TakingInterrupted { incident, actor } => {
                    self.emit(EventKind::TakingInterrupted { incident, actor });
                    self.witness_property_incident(incident)?;
                }
                Effect::Transfer {
                    actor,
                    source,
                    destination,
                    quantity,
                } => {
                    self.emit(EventKind::Transferred {
                        actor,
                        source,
                        destination,
                        quantity,
                    });
                }
                Effect::Taken {
                    incident,
                    actor,
                    source,
                    destination,
                    quantity,
                } => {
                    self.emit(EventKind::Taken {
                        incident,
                        actor,
                        source,
                        destination,
                        quantity,
                    });
                    self.witness_property_incident(incident)?;
                }
                Effect::StartReport {
                    sender,
                    recipient,
                    incident,
                    completes,
                } => {
                    *self.host.storage.activity_mut(sender).unwrap() = Activity::ReportIncident {
                        recipient,
                        incident,
                        completes,
                    };
                    self.host
                        .state
                        .due
                        .entry(completes)
                        .or_default()
                        .push(Due::Physical(sender));
                }
                Effect::Reported {
                    sender,
                    recipient,
                    incident,
                } => {
                    self.emit(EventKind::IncidentReported {
                        sender,
                        recipient,
                        incident,
                    });
                }
                Effect::Engage { defender, thief } => {
                    self.debug_start_encounter(vec![(defender, 0), (thief, 1)])?;
                }
                Effect::Relationship {
                    observer,
                    target,
                    trust,
                } => {
                    if let Some(mind) = self.host.state.decisions.as_mut().and_then(|_| {
                        game_characters::decisions::mind_mut::<game_api::Command>(
                            &mut self.host.storage,
                            observer,
                        )
                    }) {
                        let relationship =
                            mind.relationships
                                .entry(target)
                                .or_insert(decisions::Relationship {
                                    trust: 0,
                                    affection: 0,
                                });
                        relationship.trust = (relationship.trust + trust).clamp(-1_000, 1_000);
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn observe_property(&mut self) -> Result<(), Error> {
        let pending = self
            .host
            .state
            .property
            .as_ref()
            .map(|_| {
                game_property::pending_rows(&self.host.storage)
                    .map(|(_, id)| id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in pending {
            self.witness_property_incident(id)?;
        }
        Ok(())
    }
    pub(crate) fn property_actor_talking(&self, actor: PersonId) -> bool {
        self.host
            .state
            .property
            .as_ref()
            .is_some_and(|w| w.actor_talking(&self.host.storage, actor))
    }
    fn witness_property_incident(&mut self, id: IncidentId) -> Result<(), Error> {
        let world = self
            .host
            .state
            .property
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let incident = game_property::incident(&self.host.storage, id).ok_or(Error::Unavailable)?;
        let observers = self
            .property_present(incident.area)
            .into_iter()
            .map(|actor| self.property_observer_facts(actor, incident))
            .collect::<Vec<_>>();
        let change =
            world.prepare_witness(&self.host.storage, id, self.host.clock().time, &observers)?;
        self.apply_property_change(change)
    }
    pub(crate) fn review_property_resistance(&mut self) -> Result<(), Error> {
        let pending = self
            .host
            .state
            .property
            .as_ref()
            .map(|_| {
                game_property::pending_rows(&self.host.storage)
                    .map(|(actor, _)| actor)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for thief in pending {
            let world = self.host.state.property.as_ref().unwrap();
            let incident = game_property::incident(
                &self.host.storage,
                *game_property::pending(&self.host.storage, thief).unwrap(),
            )
            .unwrap();
            let observers = self
                .property_present(incident.area)
                .into_iter()
                .map(|actor| self.property_observer_facts(actor, incident))
                .collect::<Vec<_>>();
            if let Some(game_property::Effect::Engage { defender, thief }) =
                world.resistance(&self.host.storage, thief, &observers)
                && self
                    .preflight_start_encounter(&[(defender, 0), (thief, 1)])
                    .is_ok()
            {
                self.debug_start_encounter(vec![(defender, 0), (thief, 1)])?;
            }
        }
        Ok(())
    }
    pub(crate) fn interrupt_property_action(&mut self, actor: PersonId) {
        if let Some(world) = &self.host.state.property {
            let change = world.prepare_cancel(&self.host.storage, actor, self.host.clock().time);
            let _ = self.apply_property_change(change);
        }
    }
    pub(crate) fn complete_property_action(&mut self, actor: PersonId) -> Result<(), Error> {
        let world = self
            .host
            .state
            .property
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let change = if let Some(id) = game_property::pending(&self.host.storage, actor) {
            let incident = game_property::incident(&self.host.storage, *id).unwrap();
            let from = self.property_stock_facts(actor, incident.source)?;
            let to = self.property_stock_facts(actor, incident.destination)?;
            let room = if self.transfer_capacity(incident.destination, incident.quantity.0) {
                incident.quantity
            } else {
                Quantity(0)
            };
            let prefix = self.prepare_property_receipts()?;
            world.prepare_finish_take(
                &self.host.storage,
                self.property_actor_facts(actor),
                from,
                to,
                room,
                prefix,
            )?
        } else if let Some(report) = game_property::report(&self.host.storage, actor) {
            let mut recipient = self.property_contact_facts(report.recipient);
            recipient.available = self
                .host
                .state
                .physical
                .as_ref()
                .and_then(|_| self.host.storage.activity(report.recipient))
                .is_some_and(|activity| matches!(activity, Activity::Idle | Activity::Rest));
            world.prepare_finish_report(
                &self.host.storage,
                self.property_actor_facts(actor),
                recipient,
                self.property_observer_facts(report.recipient, &report.snapshot.incident),
            )?
        } else {
            return Ok(());
        };
        self.apply_property_change(change)?;
        self.update_property_events()?;
        Ok(())
    }
    fn prepare_property_receipts(&self) -> Result<Option<game_property::PreparedProperty>, Error> {
        use game_property::{PropertyReceipt, ReceiptChange};
        let Some(world) = &self.host.state.property else {
            return Ok(None);
        };
        let through = self.host.next_event() - 1;
        if world.last_event == through
            && self.host.storage.count::<game_property::Provenance>()
                == self.host.storage.count::<game_cargo::Inventory>()
        {
            return Ok(None);
        }
        let mut receipts = Vec::new();
        let mut touched = BTreeSet::new();
        for event in self.host.events().iter().skip(world.last_event as usize) {
            let change = match event.event {
                EventKind::Transferred {
                    source,
                    destination,
                    quantity,
                    ..
                } => {
                    touched.insert(source);
                    touched.insert(destination);
                    Some(ReceiptChange::Transfer {
                        source,
                        destination,
                        quantity,
                    })
                }
                EventKind::Ate {
                    source, quantity, ..
                }
                | EventKind::Consumed {
                    source, quantity, ..
                }
                | EventKind::Cared {
                    source, quantity, ..
                } => {
                    touched.insert(source);
                    Some(ReceiptChange::Consume {
                        inventory: source,
                        quantity,
                    })
                }
                EventKind::Spoiled {
                    inventory,
                    quantity,
                } => {
                    touched.insert(inventory);
                    Some(ReceiptChange::Consume {
                        inventory,
                        quantity,
                    })
                }
                EventKind::Produced {
                    output, quantity, ..
                }
                | EventKind::ResourceCollected {
                    destination: output,
                    quantity,
                    ..
                } => {
                    touched.insert(output);
                    Some(ReceiptChange::Produce {
                        outputs: vec![(output, quantity)],
                    })
                }
                EventKind::CropHarvested {
                    field, food, seed, ..
                } => {
                    let land = self
                        .host
                        .state
                        .agriculture
                        .as_ref()
                        .and_then(|w| w.field(&self.host.storage, field))
                        .ok_or(Error::Unavailable)?;
                    touched.insert(land.food);
                    touched.insert(land.seed);
                    Some(ReceiptChange::Produce {
                        outputs: vec![(land.food, food), (land.seed, seed)],
                    })
                }
                EventKind::Born { child, .. } => Some(ReceiptChange::Born { person: child }),
                _ => None,
            };
            if let Some(change) = change {
                receipts.push(PropertyReceipt {
                    event: event.sequence,
                    change,
                });
            }
        }
        if self.host.storage.count::<game_property::Provenance>()
            != self.host.storage.count::<game_cargo::Inventory>()
        {
            touched.extend(
                game_cargo::inventories(&self.host.storage)
                    .map(|(id, _)| id)
                    .filter(|id| !game_property::portions(&self.host.storage, *id).is_some()),
            );
        }
        let balances = touched
            .into_iter()
            .map(|id| {
                game_cargo::inventory(&self.host.storage, id)
                    .map(|stock| (id, stock.quantity))
                    .ok_or(Error::Unavailable)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        world
            .prepare_receipts(&self.host.storage, &receipts, &balances, through)
            .map(Some)
    }
    pub(crate) fn update_property_events(&mut self) -> Result<(), Error> {
        if let Some(change) = self.prepare_property_receipts()? {
            self.apply_property_change(change)?;
        }
        Ok(())
    }
    pub(crate) fn available_property_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(runtime) = &self.host.state.property else {
            return Vec::new();
        };
        let actor_facts = self.property_actor_facts(actor);
        if !runtime.menu_admissible(actor_facts) {
            return Vec::new();
        }
        let area = actor_facts.area.unwrap();
        let quantity = self.host.state.physical.as_ref().unwrap().rules.meal_size;
        let cargo = self.host.state.people[self.person_index[&actor]].inventory;
        let local_stock = game_cargo::inventories(&self.host.storage)
            .filter(|(_, stock)| self.inventory_area(stock) == Some(area))
            .collect::<Vec<_>>();
        let stocks = local_stock
            .iter()
            .filter_map(|(id, _)| self.property_stock_facts(actor, *id).ok())
            .collect::<Vec<_>>();
        let contacts = self
            .property_present(area)
            .into_iter()
            .map(|person| self.property_contact_facts(person))
            .collect::<Vec<_>>();
        let recipient_access = local_stock
            .iter()
            .flat_map(|(id, row)| row.access.iter().map(move |person| (*id, *person)))
            .collect::<Vec<_>>();
        runtime
            .available_actions(
                &self.host.storage,
                game_property::MenuFacts {
                    actor: actor_facts,
                    quantity,
                    cargo: self.property_stock_facts(actor, cargo).ok(),
                    can_receive_quantity: self.transfer_capacity(cargo, quantity.0),
                    stocks: &stocks,
                    contacts: &contacts,
                    recipient_access: &recipient_access,
                },
            )
            .into_iter()
            .map(|action| Command::Property { action })
            .collect()
    }

    pub fn property_view(&self, actor: PersonId) -> PropertyView {
        let Some(_) = &self.host.state.property else {
            return PropertyView::default();
        };
        PropertyView {
            claims: game_property::known_claims(&self.host.storage, actor)
                .map(|known| known.values().take(64).cloned().collect())
                .unwrap_or_default(),
            incidents: game_property::known_incidents(&self.host.storage, actor)
                .map(|known| {
                    known
                        .values()
                        .rev()
                        .take(64)
                        .map(|known| IncidentView {
                            id: known.incident.id,
                            actor: known.incident.actor,
                            source: known.incident.source,
                            destination: known.incident.destination,
                            quantity: known.incident.quantity,
                            area: known.incident.area,
                            started_at: known.incident.started_at,
                            changed_at: known.incident.changed_at,
                            outcome: known.incident.outcome,
                            learned_at: known.learned_at,
                            source_of_knowledge: known.source.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub(crate) fn property_opportunities(&self, actor: PersonId) -> Vec<Command> {
        let Some(physical) = &self.host.state.physical else {
            return Vec::new();
        };
        self.property_acquisition_opportunities(actor, None, physical.rules.meal_size)
    }
    pub(crate) fn property_acquisition_opportunities(
        &self,
        actor: PersonId,
        good: Option<Good>,
        quantity: Quantity,
    ) -> Vec<Command> {
        let Some(world) = &self.host.state.property else {
            return Vec::new();
        };
        let actor_facts = self.property_actor_facts(actor);
        if !world.acquisition_eligible(&self.host.storage, actor_facts) {
            return Vec::new();
        }
        let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return Vec::new();
        };
        let mut destinations = BTreeMap::<Good, InventoryId>::new();
        for id in self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
        {
            let stock = game_cargo::inventory(&self.host.storage, *(id)).expect("inventory exists");
            destinations
                .entry(stock.good)
                .and_modify(|existing| *existing = (*existing).min(*id))
                .or_insert(*id);
        }
        let mut actions = Vec::new();
        for known in mind
            .knowledge
            .values()
            .filter(|known| good.is_none_or(|good| known.good == good))
        {
            let quantity = Quantity(quantity.0.min(known.quantity.0));
            let destination = destinations.get(&known.good).copied();
            let need = game_property::AcquisitionFacts {
                actor: actor_facts,
                good: known.good,
                destination,
                quantity,
            };
            if !world.acquisition_source_eligible(&self.host.storage, &need, known) {
                continue;
            }
            let opportunity = game_property::NeedOpportunity {
                known: known.clone(),
                local_custodian: self
                    .property_custodian(known.inventory)
                    .is_some_and(|c| c != actor && self.property_area(c) == actor_facts.area),
                can_take: quantity.0 > 0
                    && destination.is_some_and(|destination| {
                        self.taking_possible(actor, known.inventory, destination, quantity, true)
                    }),
            };
            actions.extend(
                world
                    .acquisition_opportunities(&self.host.storage, need, &[opportunity])
                    .into_iter()
                    .map(|action| Command::Property { action }),
            );
        }
        actions
    }
    pub(crate) fn review_property_sharing(&mut self) {
        let actors = self
            .host
            .state
            .people
            .iter()
            .filter(|p| p.id != self.host.state.ruler)
            .map(|p| p.id)
            .collect::<Vec<_>>();
        for actor in actors {
            let Some(world) = &self.host.state.property else {
                return;
            };
            let Some(area) = self.property_area(actor) else {
                continue;
            };
            let needy = self
                .host
                .state
                .decisions
                .as_ref()
                .and_then(|_| {
                    game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                })
                .is_some_and(|m| {
                    m.child_signals.values().any(|signal| {
                        signal.location == Location::AtArea(area) && signal.urgency >= 1_000
                    })
                });
            let actor_facts = self.property_actor_facts(actor);
            if !world.sharing_eligible(&self.host.storage, actor_facts, needy) {
                continue;
            }
            let recipients = self.property_present(area);
            let stores = game_cargo::inventories(&self.host.storage)
                .map(|(_, row)| row)
                .filter(|stock| {
                    self.inventory_area(stock) == Some(area)
                        && self.property_custodian(stock.id) == Some(actor)
                })
                .filter_map(|stock| {
                    self.property_stock_facts(actor, stock.id)
                        .ok()
                        .map(|facts| game_property::SharingStore {
                            stock: facts,
                            recipients_without_access: recipients
                                .iter()
                                .filter(|person| !stock.access.contains(person))
                                .copied()
                                .collect(),
                        })
                })
                .collect::<Vec<_>>();
            let minimum = self.host.state.physical.as_ref().unwrap().rules.meal_size;
            let actions = world.plan_sharing(
                &self.host.storage,
                actor_facts,
                needy,
                Good::Food,
                minimum,
                &stores,
            );
            for action in actions {
                let _ = self.debug_property_action(actor, action);
            }
        }
    }
}

fn property_action_reference(activity: &Activity) -> game_property::PhysicalActionReference {
    use game_property::PhysicalActionReference as Reference;
    match *activity {
        Activity::Idle | Activity::Rest => Reference::Idle,
        Activity::Take {
            source,
            destination,
            quantity,
            completes,
        } => Reference::Taking {
            source,
            destination,
            quantity,
            completes,
        },
        Activity::ReportIncident {
            recipient,
            incident,
            completes,
        } => Reference::Reporting {
            recipient,
            incident,
            completes,
        },
        _ => Reference::Other,
    }
}

struct PropertyReferenceBindings<'a, 'b> {
    state: &'a CanonicalState<'b>,
    people: BTreeSet<PersonId>,
    areas: BTreeSet<AreaId>,
}
impl game_property::PropertyReferences for PropertyReferenceBindings<'_, '_> {
    fn person(&self, id: PersonId) -> Option<game_property::PersonReference> {
        if !self.people.contains(&id) {
            return None;
        }
        let body = game_physiology::body(self.state.components, id)?;
        let area = match game_spatial::location(self.state.components, game_ecs::StableId(id.0))? {
            Location::AtArea(area) => Some(*area),
            _ => None,
        };
        Some(game_property::PersonReference {
            alive: body.vitals.alive,
            area,
            action: property_action_reference(self.state.components.activity(id)?),
        })
    }
    fn inventory(&self, id: InventoryId) -> Option<game_property::InventoryReference> {
        let stock = game_cargo::inventory(self.state.components, id)?;
        Some(game_property::InventoryReference {
            quantity: stock.quantity,
            good: stock.good,
            carrier: match stock.location {
                InventoryLocation::Carried(actor) => Some(actor),
                InventoryLocation::Area(_) => None,
            },
        })
    }
    fn area_exists(&self, id: AreaId) -> bool {
        self.areas.contains(&id)
    }
    fn physical_completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool {
        self.state.state.due.get(&at).is_some_and(|due| {
            due.iter()
                .any(|event| matches!(event, Due::Physical(id) if *id == actor))
        })
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let actions = state
        .components
        .activities()
        .map(|(actor, activity)| (actor, property_action_reference(activity)));
    let Some(world) = &state.state.property else {
        return game_property::validate_absent(state.components, actions);
    };
    if state.state.physical.is_none()
        || state.state.decisions.is_none()
        || state.state.agreements.is_none()
        || state.state.combat.is_none()
    {
        return Err(Error::InvalidSave("property providers are missing".into()));
    }
    world.validate_composition(
        state.components,
        game_property::ValidationFacts {
            now: state.time,
            next_id: state.next_id,
            next_event: state.next_event,
            inventory_count: state.components.count::<game_cargo::Inventory>(),
        },
        &PropertyReferenceBindings {
            state,
            people: state.state.people.iter().map(|person| person.id).collect(),
            areas: state.state.areas.iter().map(|area| area.id).collect(),
        },
        actions,
    )
}
