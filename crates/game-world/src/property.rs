//! Assertions follow real fungible portions. Recognition, custody, permission,
//! and physical possession remain separate; observers choose their response.
use crate::*;

pub use game_property::{Attitude, Claim, Incident, IncidentKnowledge, World};

impl Simulation {
    pub fn property_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "property-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let custodians = sim
            .state
            .inventories
            .values()
            .map(|stock| {
                (
                    stock.id,
                    match stock.location {
                        InventoryLocation::Carried(person) => person,
                        InventoryLocation::Area(_) => sim.state.ruler,
                    },
                )
            })
            .collect();
        sim.state.agreements = Some(agreements::World {
            ledger: game_agreements::State {
                knowledge: BTreeMap::new(),
                last_observation_event: sim.state.next_event - 1,
                agreements: BTreeMap::new(),
                obligations: BTreeMap::new(),
                amendments: BTreeMap::new(),
                last_event: sim.state.next_event - 1,
                next_search: BTreeMap::new(),
                pending_work: BTreeMap::new(),
            },
            custodians,
        });
        let custodian = sim.state.people[3].id;
        let food = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Food
                    && stock.location == InventoryLocation::Area(sim.state.areas[0].id)
            })
            .unwrap()
            .id;
        sim.state.inventories.get_mut(&food).unwrap().access = BTreeSet::from([custodian]);
        sim.state
            .agreements
            .as_mut()
            .unwrap()
            .custodians
            .insert(food, custodian);
        let attitudes = sim
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
        let portions = sim
            .state
            .inventories
            .values()
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
        sim.state.property = Some(World {
            claims: BTreeMap::new(),
            incidents: BTreeMap::new(),
            known_claims: BTreeMap::new(),
            knowledge: BTreeMap::new(),
            attitudes,
            portions,
            pending: BTreeMap::new(),
            reports: BTreeMap::new(),
            last_event: sim.state.next_event - 1,
            next_search: BTreeMap::new(),
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
        for (person, mind) in &mut sim.state.decisions.as_mut().unwrap().minds {
            if let Some(fact) = mind.knowledge.get_mut(&food) {
                fact.accessible = *person == custodian;
            }
        }
        Ok(sim)
    }

    fn property_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = self.state.components.body(actor)?;
        if !body.vitals.alive {
            return None;
        }
        match self
            .state
            .people
            .get(*self.person_index.get(&actor)?)?
            .location
        {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn property_custodian(&self, inventory: InventoryId) -> Option<PersonId> {
        let stock = self.state.inventories.get(&inventory)?;
        match stock.location {
            InventoryLocation::Carried(actor) => Some(actor),
            InventoryLocation::Area(_) => self
                .state
                .agreements
                .as_ref()?
                .custodians
                .get(&inventory)
                .copied(),
        }
    }
    fn property_present(&self, area: AreaId) -> Vec<PersonId> {
        self.state
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
            && self
                .state
                .components
                .body(actor)
                .expect("body exists")
                .vitals
                .health
                > 0
    }
    fn property_actor_facts(&self, actor: PersonId) -> game_property::ActorFacts {
        game_property::ActorFacts {
            actor,
            now: self.state.time,
            area: self.property_area(actor),
            capable: self.property_able(actor),
            exertion_ready: self
                .state
                .physical
                .as_ref()
                .and_then(|w| {
                    self.state
                        .components
                        .body(actor)
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
        let stock = self.state.inventories.get(&id).ok_or(Error::Unavailable)?;
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
            autonomous: actor != self.state.ruler,
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
            && self
                .state
                .inventories
                .get(&incident.source)
                .is_some_and(|stock| stock.access.contains(&incident.actor));
        game_property::ObserverFacts {
            actor: observer,
            personal_custody: custody && !allowed,
            personally_allowed: allowed,
            can_defend: observer != self.state.ruler && self.property_able(observer),
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
        self.state
            .property
            .as_mut()
            .ok_or(Error::Unavailable)?
            .attitudes
            .insert(actor, attitude);
        Ok(())
    }

    pub fn debug_property_action(
        &mut self,
        actor: PersonId,
        action: PropertyAction,
    ) -> Result<(), Error> {
        use game_property::ActionFacts;
        let world = self.state.property.as_ref().ok_or(Error::Unavailable)?;
        let facts = match &action {
            PropertyAction::Claim { inventory, .. } => ActionFacts::Claim {
                stock: self.property_stock_facts(actor, *inventory)?,
                id: ClaimId(self.state.next_id),
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
                        .state
                        .decisions
                        .as_ref()
                        .and_then(|w| w.minds.get(&custodian))
                        .and_then(|m| m.relationships.get(&actor))
                        .map_or(0, |r| r.trust),
                    reserve: Quantity(
                        self.state
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
                id: IncidentId(self.state.next_id),
                carrying_room: if self.transfer_capacity(*destination, quantity.0) {
                    *quantity
                } else {
                    Quantity(0)
                },
                duration: self
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
        let change =
            world.prepare_action(self.property_actor_facts(actor), &action, facts, prefix)?;
        self.apply_property_change(change)?;
        self.revision += 1;
        Ok(())
    }
    fn apply_property_change(
        &mut self,
        change: game_property::PreparedProperty,
    ) -> Result<(), Error> {
        use game_property::Effect;
        let mut next_id = self.state.next_id;
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
                Effect::Transfer {
                    source,
                    destination,
                    quantity,
                    ..
                } => {
                    let from = self
                        .state
                        .inventories
                        .get(source)
                        .ok_or(Error::Unavailable)?;
                    let to = self
                        .state
                        .inventories
                        .get(destination)
                        .ok_or(Error::Unavailable)?;
                    if from.quantity < *quantity
                        || to.capacity.0.saturating_sub(to.quantity.0) < quantity.0
                    {
                        return Err(Error::Unavailable);
                    }
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
                        self.state.people.len() * 8 + 8
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
        let effects = change.commit(self.state.property.as_mut().ok_or(Error::Unavailable)?)?;
        self.state.next_id = next_id;
        for effect in effects {
            match effect {
                Effect::Interrupt { actor } => self.interrupt_actor_activity_except_property(actor),
                Effect::ClearTaking { actor } => {
                    let activity = self.state.components.activity_mut(actor).unwrap();
                    if matches!(activity, Activity::Take { .. }) {
                        *activity = Activity::Idle;
                    }
                    for due in self.state.due.values_mut() {
                        due.retain(|d| !matches!(d,Due::Physical(id) if *id==actor));
                    }
                    self.state.due.retain(|_, due| !due.is_empty());
                }
                Effect::ClearReport {
                    sender,
                    completes,
                    interrupted,
                } => {
                    let activity = self.state.components.activity_mut(sender).unwrap();
                    if matches!(*activity,Activity::ReportIncident {completes:at,..} if at==completes)
                    {
                        *activity = Activity::Idle;
                    }
                    if let Some(due) = self.state.due.get_mut(&completes) {
                        due.retain(|d| !matches!(d,Due::Physical(id) if *id==sender));
                    }
                    self.state.due.retain(|_, due| !due.is_empty());
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
                    let access = &mut self.state.inventories.get_mut(&inventory).unwrap().access;
                    if allowed {
                        access.insert(recipient);
                    } else {
                        access.remove(&recipient);
                    }
                    if let Some(fact) = self
                        .state
                        .decisions
                        .as_mut()
                        .and_then(|w| w.minds.get_mut(&recipient))
                        .and_then(|m| m.knowledge.get_mut(&inventory))
                    {
                        fact.accessible = allowed;
                        fact.learned_at = self.state.time;
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
                    *self.state.components.activity_mut(actor).unwrap() = Activity::Take {
                        source,
                        destination,
                        quantity,
                        completes,
                    };
                    self.state
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
                    self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
                    self.state
                        .inventories
                        .get_mut(&destination)
                        .unwrap()
                        .quantity
                        .0 += quantity.0;
                    self.state
                        .inventories
                        .get_mut(&source)
                        .unwrap()
                        .last_granted = Some(actor);
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
                    *self.state.components.activity_mut(sender).unwrap() =
                        Activity::ReportIncident {
                            recipient,
                            incident,
                            completes,
                        };
                    self.state
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
                    if let Some(mind) = self
                        .state
                        .decisions
                        .as_mut()
                        .and_then(|w| w.minds.get_mut(&observer))
                    {
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
            .state
            .property
            .as_ref()
            .map(|w| w.pending.values().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        for id in pending {
            self.witness_property_incident(id)?;
        }
        Ok(())
    }
    pub(crate) fn property_actor_talking(&self, actor: PersonId) -> bool {
        self.state
            .property
            .as_ref()
            .is_some_and(|w| w.actor_talking(actor))
    }
    fn witness_property_incident(&mut self, id: IncidentId) -> Result<(), Error> {
        let world = self.state.property.as_ref().ok_or(Error::Unavailable)?;
        let incident = world.incidents.get(&id).ok_or(Error::Unavailable)?;
        let observers = self
            .property_present(incident.area)
            .into_iter()
            .map(|actor| self.property_observer_facts(actor, incident))
            .collect::<Vec<_>>();
        let change = world.prepare_witness(id, self.state.time, &observers)?;
        self.apply_property_change(change)
    }
    pub(crate) fn review_property_resistance(&mut self) -> Result<(), Error> {
        let pending = self
            .state
            .property
            .as_ref()
            .map(|w| w.pending.keys().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        for thief in pending {
            let world = self.state.property.as_ref().unwrap();
            let incident = &world.incidents[&world.pending[&thief]];
            let observers = self
                .property_present(incident.area)
                .into_iter()
                .map(|actor| self.property_observer_facts(actor, incident))
                .collect::<Vec<_>>();
            if let Some(game_property::Effect::Engage { defender, thief }) =
                world.resistance(thief, &observers)
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
        if let Some(world) = &self.state.property {
            let change = world.prepare_cancel(actor, self.state.time);
            let _ = self.apply_property_change(change);
        }
    }
    pub(crate) fn complete_property_action(&mut self, actor: PersonId) -> Result<(), Error> {
        let world = self.state.property.as_ref().ok_or(Error::Unavailable)?;
        let change = if let Some(id) = world.pending.get(&actor) {
            let incident = &world.incidents[id];
            let from = self.property_stock_facts(actor, incident.source)?;
            let to = self.property_stock_facts(actor, incident.destination)?;
            let room = if self.transfer_capacity(incident.destination, incident.quantity.0) {
                incident.quantity
            } else {
                Quantity(0)
            };
            let prefix = self.prepare_property_receipts()?;
            world.prepare_finish_take(self.property_actor_facts(actor), from, to, room, prefix)?
        } else if let Some(report) = world.reports.get(&actor) {
            let mut recipient = self.property_contact_facts(report.recipient);
            recipient.available = self
                .state
                .physical
                .as_ref()
                .and_then(|_| self.state.components.activity(report.recipient))
                .is_some_and(|activity| matches!(activity, Activity::Idle | Activity::Rest));
            world.prepare_finish_report(
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
        let Some(world) = &self.state.property else {
            return Ok(None);
        };
        let through = self.state.next_event - 1;
        if world.last_event == through && world.portions.len() == self.state.inventories.len() {
            return Ok(None);
        }
        let mut receipts = Vec::new();
        let mut touched = BTreeSet::new();
        for event in self.state.events.iter().skip(world.last_event as usize) {
            let change = match event.kind {
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
                        .state
                        .agriculture
                        .as_ref()
                        .and_then(|w| w.field(&self.state.components, field))
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
                    event: event.id.0,
                    change,
                });
            }
        }
        if world.portions.len() != self.state.inventories.len() {
            touched.extend(
                self.state
                    .inventories
                    .keys()
                    .filter(|id| !world.portions.contains_key(id))
                    .copied(),
            );
        }
        let balances = touched
            .into_iter()
            .map(|id| {
                self.state
                    .inventories
                    .get(&id)
                    .map(|stock| (id, stock.quantity))
                    .ok_or(Error::Unavailable)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        world
            .prepare_receipts(&receipts, &balances, through)
            .map(Some)
    }
    pub(crate) fn update_property_events(&mut self) -> Result<(), Error> {
        if let Some(change) = self.prepare_property_receipts()? {
            self.apply_property_change(change)?;
        }
        Ok(())
    }
    pub(crate) fn available_property_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(world) = &self.state.property else {
            return Vec::new();
        };
        if !self.property_able(actor) {
            return Vec::new();
        }
        let area = self.property_area(actor).unwrap();
        let mut actions = Vec::new();
        if let Some(known) = world.known_claims.get(&actor) {
            for claim in known.values().take(8) {
                if !claim.recognized_by.contains(&actor) {
                    actions.push(PropertyAction::Recognize { claim: claim.id });
                }
                if !claim.opposed_by.contains(&actor) {
                    actions.push(PropertyAction::Contest { claim: claim.id });
                }
            }
        }
        let people = self.property_present(area);
        let quantity = self.state.physical.as_ref().unwrap().rules.meal_size;
        let cargo = self.state.people[self.person_index[&actor]].inventory;
        for stock in self
            .state
            .inventories
            .values()
            .filter(|stock| stock.quantity.0 > 0 && self.inventory_area(stock) == Some(area))
            .take(16)
        {
            if self.taking_possible(actor, stock.id, cargo, quantity, true) {
                actions.push(PropertyAction::Take {
                    source: stock.id,
                    destination: cargo,
                    quantity,
                });
            }
            if !stock.access.contains(&actor)
                && self.property_custodian(stock.id).is_some_and(|custodian| {
                    custodian != actor && self.property_area(custodian) == Some(area)
                })
            {
                actions.push(PropertyAction::RequestAccess {
                    inventory: stock.id,
                });
            }
            if self.property_custodian(stock.id) == Some(actor) {
                for recipient in people
                    .iter()
                    .filter(|recipient| **recipient != actor)
                    .take(4)
                {
                    actions.push(PropertyAction::Share {
                        inventory: stock.id,
                        recipient: *recipient,
                        allowed: !stock.access.contains(recipient),
                    });
                }
            }
        }
        actions.truncate(40);
        for (thief, id) in world
            .pending
            .iter()
            .filter(|(thief, id)| **thief != actor && world.incidents[id].area == area)
            .take(8)
        {
            if *thief != actor
                && world.incidents[id].area == area
                && world
                    .knowledge
                    .get(&actor)
                    .is_some_and(|known| known.contains_key(id))
            {
                actions.push(PropertyAction::ResistTaking { thief: *thief });
            }
        }
        if let Some(known) = world.knowledge.get(&actor) {
            for incident in known.keys().rev().take(4) {
                for recipient in people
                    .iter()
                    .filter(|person| {
                        **person != actor && self.conversation_actor_available(**person)
                    })
                    .take(4)
                {
                    actions.push(PropertyAction::ReportIncident {
                        recipient: *recipient,
                        incident: *incident,
                    });
                }
            }
        }
        actions
            .into_iter()
            .take(64)
            .map(|action| Command::Property { action })
            .collect()
    }

    pub fn property_view(&self, actor: PersonId) -> PropertyView {
        let Some(world) = &self.state.property else {
            return PropertyView::default();
        };
        PropertyView {
            claims: world
                .known_claims
                .get(&actor)
                .map(|known| known.values().take(64).cloned().collect())
                .unwrap_or_default(),
            incidents: world
                .knowledge
                .get(&actor)
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
        let Some(physical) = &self.state.physical else {
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
        let Some(world) = &self.state.property else {
            return Vec::new();
        };
        let actor_facts = self.property_actor_facts(actor);
        if !world.acquisition_eligible(actor_facts) {
            return Vec::new();
        }
        let Some(mind) = self
            .state
            .decisions
            .as_ref()
            .and_then(|w| w.minds.get(&actor))
        else {
            return Vec::new();
        };
        let mut destinations = BTreeMap::<Good, InventoryId>::new();
        for id in self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
        {
            let stock = &self.state.inventories[id];
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
            if !world.acquisition_source_eligible(&need, known) {
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
                    .acquisition_opportunities(need, &[opportunity])
                    .into_iter()
                    .map(|action| Command::Property { action }),
            );
        }
        actions
    }
    pub(crate) fn review_property_sharing(&mut self) {
        let actors = self
            .state
            .people
            .iter()
            .filter(|p| p.id != self.state.ruler)
            .map(|p| p.id)
            .collect::<Vec<_>>();
        for actor in actors {
            let Some(world) = &self.state.property else {
                return;
            };
            let Some(area) = self.property_area(actor) else {
                continue;
            };
            let needy = self
                .state
                .decisions
                .as_ref()
                .and_then(|w| w.minds.get(&actor))
                .is_some_and(|m| {
                    m.child_signals.values().any(|signal| {
                        signal.location == Location::AtArea(area) && signal.urgency >= 1_000
                    })
                });
            let actor_facts = self.property_actor_facts(actor);
            if !world.sharing_eligible(actor_facts, needy) {
                continue;
            }
            let recipients = self.property_present(area);
            let stores = self
                .state
                .inventories
                .values()
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
            let minimum = self.state.physical.as_ref().unwrap().rules.meal_size;
            let actions = world.plan_sharing(actor_facts, needy, Good::Food, minimum, &stores);
            for action in actions {
                let _ = self.debug_property_action(actor, action);
            }
        }
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.property else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid property state".into());
    world.validate_owned(state.time)?;
    let people: BTreeMap<_, _> = state
        .people
        .iter()
        .map(|person| (person.id, person))
        .collect();
    state.physical.as_ref().ok_or_else(fail)?;
    if state.decisions.is_none()
        || state.agreements.is_none()
        || state.combat.is_none()
        || world.last_event >= state.next_event
        || world
            .attitudes
            .keys()
            .chain(world.next_search.keys())
            .any(|actor| !people.contains_key(actor))
        || world.portions.len() != state.inventories.len()
    {
        return Err(fail());
    }
    for (id, claim) in &world.claims {
        if id.0 >= state.next_id
            || !people.contains_key(&claim.claimant)
            || !state.inventories.contains_key(&claim.inventory)
            || claim
                .recognized_by
                .iter()
                .chain(&claim.opposed_by)
                .any(|person| !people.contains_key(person))
        {
            return Err(fail());
        }
    }
    for (inventory, portions) in &world.portions {
        let stock = state.inventories.get(inventory).ok_or_else(fail)?;
        let total = portions
            .values()
            .try_fold(0u64, |total, q| total.checked_add(*q).ok_or_else(fail))?;
        if total != stock.quantity.0 {
            return Err(fail());
        }
    }
    for (actor, known) in &world.known_claims {
        if !people.contains_key(actor)
            || known.values().any(|view| {
                view.recognized_by
                    .iter()
                    .chain(&view.opposed_by)
                    .any(|person| !people.contains_key(person))
            })
        {
            return Err(fail());
        }
    }
    for incident in world.incidents.values() {
        if incident.id.0 >= state.next_id
            || !people.contains_key(&incident.actor)
            || !state
                .inventories
                .get(&incident.source)
                .zip(state.inventories.get(&incident.destination))
                .is_some_and(|(source, destination)| {
                    source.good == destination.good
                        && destination.location == InventoryLocation::Carried(incident.actor)
                })
            || !state.areas.iter().any(|area| area.id == incident.area)
        {
            return Err(fail());
        }
    }
    let known_sender = |knowledge: &IncidentKnowledge| !matches!(knowledge.source, IncidentSource::Reported { sender } if !people.contains_key(&sender));
    for (actor, known) in &world.knowledge {
        if !people.contains_key(actor) || known.values().any(|knowledge| !known_sender(knowledge)) {
            return Err(fail());
        }
    }
    for (actor, id) in &world.pending {
        let incident = &world.incidents[id];
        let body = state.components.body(*actor).ok_or_else(fail)?;
        if !body.vitals.alive
            || people
                .get(actor)
                .is_none_or(|person| person.location != Location::AtArea(incident.area))
            || !matches!(*state.components.activity(*actor).ok_or_else(fail)?, Activity::Take { source, destination, quantity, completes }
                if source == incident.source && destination == incident.destination && quantity == incident.quantity && completes == incident.completes)
            || !state.due.get(&incident.completes).is_some_and(|due| {
                due.iter()
                    .any(|event| matches!(event, Due::Physical(id) if id == actor))
            })
        {
            return Err(fail());
        }
    }
    for (sender, report) in &world.reports {
        if !known_sender(&report.snapshot)
            || [*sender, report.recipient].iter().any(|actor| people.get(actor).is_none_or(|person| person.location != Location::AtArea(report.area)))
            || !state.components.body(report.recipient).is_some_and(|body| body.vitals.alive)
            || !state.components.activity(report.recipient).is_some_and(|activity| matches!(activity, Activity::Idle | Activity::Rest))
            || !state.components.body(*sender).is_some_and(|body| body.vitals.alive)
            || !state.components.activity(*sender).is_some_and(|activity| matches!(*activity, Activity::ReportIncident { recipient, incident, completes }
                if recipient == report.recipient && incident == report.snapshot.incident.id && completes == report.completes))
            || !state.due.get(&report.completes).is_some_and(|due| due.iter().any(|event| matches!(event, Due::Physical(id) if id == sender)))
        {
            return Err(fail());
        }
    }
    for (actor, activity) in state.components.activities() {
        match *activity {
            Activity::Take { .. } if !world.pending.contains_key(&actor) => return Err(fail()),
            Activity::ReportIncident {
                recipient,
                incident,
                completes,
            } if !world.reports.get(&actor).is_some_and(|report| {
                report.recipient == recipient
                    && report.snapshot.incident.id == incident
                    && report.completes == completes
            }) =>
            {
                return Err(fail());
            }
            _ => {}
        }
    }
    Ok(())
}
