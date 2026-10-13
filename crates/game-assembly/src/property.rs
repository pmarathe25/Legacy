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
        let custodian = sim.person_at(3).id;
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
            .people()
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
            let (_mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((person).0)).expect("mind and stock knowledge exist");
            if let Some(fact) = stocks.known.get_mut(&food) {
                fact.accessible = person == custodian;
            }
        }
        Ok(sim)
    }

    pub(crate) fn property_area(&self, actor: PersonId) -> Option<AreaId> {
        game_property::local_area(
            game_physiology::body(&self.host.storage, actor).is_some_and(|body| body.vitals.alive),
            game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)),
        )
    }
    fn property_custodian(&self, inventory: InventoryId) -> Option<PersonId> {
        game_property::physical_custodian(&self.host.storage, inventory)
    }
    pub(crate) fn property_present(&self, area: AreaId) -> Vec<PersonId> {
        self.people()
            .filter(|person| self.property_area(person.id) == Some(area))
            .map(|p| p.id)
            .collect()
    }
    fn property_able(&self, actor: PersonId) -> bool {
        game_property::action_capable(game_property::ReadinessFacts {
            area: self.property_area(actor),
            independent: self.independent(actor),
            combat_restricted: self.combat_restricts(actor),
            receiving_care: self.care_index.contains_key(&actor),
            health: game_physiology::body(&self.host.storage, actor)
                .map_or(0, |body| body.vitals.health),
        })
    }
    fn property_actor_facts(&self, actor: PersonId) -> game_property::ActorFacts {
        game_property::ActorFacts {
            actor,
            now: self.host.clock().time,
            area: self.property_area(actor),
            capable: self.property_able(actor),
            exertion_ready: game_property::exertion_ready(
                game_physiology::body(&self.host.storage, actor)
                    .map_or(u64::MAX, |body| body.vitals.fatigue),
                self.host
                    .state
                    .physical
                    .as_ref()
                    .map(|owner| owner.rules.fatigue_limit),
            ),
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
    pub(crate) fn property_observer_facts(
        &self,
        observer: PersonId,
        incident: &Incident,
    ) -> game_property::ObserverFacts {
        game_property::observer_facts(
            observer,
            game_property::ObserverContext {
                local: self.property_area(observer) == Some(incident.area),
                source_custodian: self.property_custodian(incident.source),
                taking_actor_allowed: game_cargo::inventory(&self.host.storage, incident.source)
                    .is_some_and(|stock| stock.access.contains(&incident.actor)),
                player_controlled: observer == self.host.state.ruler,
                capable: self.property_able(observer),
            },
        )
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
        self.debug_property_action_with_maintenance(actor, action, false)
    }
    pub(crate) fn property_command(
        &mut self,
        actor: PersonId,
        action: PropertyAction,
    ) -> Result<(), Error> {
        self.debug_property_action_with_maintenance(actor, action, true)
    }
    fn debug_property_action_with_maintenance(
        &mut self,
        actor: PersonId,
        action: PropertyAction,
        routed_maintenance: bool,
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
                    reserve: game_property::retention_reserve(
                        self.host
                            .state
                            .physical
                            .as_ref()
                            .ok_or(Error::Unavailable)?
                            .rules
                            .meal_size,
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
                duration: game_property::Rules::default().report_duration,
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
        self.apply_property_change_with_revision(change, 1, routed_maintenance)
    }
    fn apply_property_change(
        &mut self,
        change: game_property::PreparedProperty,
    ) -> Result<(), Error> {
        self.apply_property_change_with_revision(change, 0, false)
    }
    fn apply_property_change_with_revision(
        &mut self,
        mut change: game_property::PreparedProperty,
        origin_revisions: u64,
        routed_maintenance: bool,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        use game_property::Effect;
        let review_resistance = change.requires_resistance_review();
        let engagement = change
            .select_projected_resistance(
                &self.host.storage,
                |incident| {
                    self.property_present(incident.area)
                        .into_iter()
                        .map(|actor| self.property_observer_facts(actor, incident))
                        .collect()
                },
                |defender, thief| {
                    self.preflight_start_encounter(&[(defender, 0), (thief, 1)])
                        .is_ok()
                },
            )?
            .and_then(|effect| match effect {
                Effect::Engage { defender, thief } => Some((defender, thief)),
                _ => None,
            });
        if let Some((defender, thief)) = engagement {
            change.cancel_projected(&self.host.storage, defender, owner_now);
            change.cancel_projected(&self.host.storage, thief, owner_now);
        }
        let witnessed = change
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                Effect::StartTake { incident, .. }
                | Effect::TakingInterrupted { incident, .. }
                | Effect::Taken { incident, .. } => Some(*incident),
                _ => None,
            })
            .collect::<Vec<_>>();
        for incident_id in witnessed {
            let incident = change
                .projected_incident(&self.host.storage, incident_id)
                .ok_or(Error::Unavailable)?;
            let observers = self
                .property_present(incident.area)
                .into_iter()
                .map(|actor| self.property_observer_facts(actor, incident))
                .collect::<Vec<_>>();
            change.witness_projected(
                self.host
                    .state
                    .property
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &self.host.storage,
                incident_id,
                owner_now,
                &observers,
            )?;
        }
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
                    Effect::StartTake { .. } | Effect::Engage { .. } => self.people().len() * 8 + 8,
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
        let encounter = if let Some((defender, thief)) = engagement {
            let participants = vec![(defender, 0), (thief, 1)];
            let opening = self.prepare_encounter_opening(&participants)?;
            let area = opening.area();
            let map = self.prepare_combat_location_map_at(opening.location(), next_id)?;
            let id = EncounterId(
                next_id
                    .checked_add(u64::from(map.is_new()))
                    .ok_or(Error::Overflow)?,
            );
            next_id = id.0.checked_add(1).ok_or(Error::Overflow)?;
            let fatigue = participants
                .iter()
                .map(|(actor, _)| {
                    (
                        *actor,
                        game_physiology::body(&self.host.storage, *actor)
                            .ok_or(Error::Unavailable)
                            .map(|b| b.vitals.fatigue),
                    )
                })
                .map(|(actor, fatigue)| fatigue.map(|fatigue| (actor, fatigue)))
                .collect::<Result<Vec<_>, _>>()?;
            let start = self
                .host
                .state
                .combat
                .as_ref()
                .ok_or(Error::Unavailable)?
                .prepare_start(
                    &self.host.storage,
                    id,
                    opening,
                    map.map(),
                    &participants,
                    &fatigue,
                    owner_now,
                )?;
            self.host.preflight_records(0, 0, 1)?;
            Some((id, area, participants, map, start))
        } else {
            None
        };
        self.ensure_event_capacity(event_bound)?;
        let mut interrupted_actors = change
            .effects()
            .iter()
            .filter_map(|effect| {
                if let Effect::Interrupt { actor } = effect {
                    Some(*actor)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if let Some((_, _, participants, _, _)) = &encounter {
            interrupted_actors.extend(participants.iter().map(|(actor, _)| *actor));
        }
        let interruptions =
            self.prepare_interruptions(&interrupted_actors, commands::ActivityOwner::Property)?;
        let activities = game_property::prepare_activity_effects(
            &self.host.storage,
            change.effects(),
            interruptions
                .projected_activities()
                .map(|(actor, activity)| (actor, activity.clone())),
            |activity| matches!(activity, Activity::Take { .. }),
            |activity| {
                if let Activity::ReportIncident { completes, .. } = activity {
                    Some(*completes)
                } else {
                    None
                }
            },
            Activity::Idle,
            |effect| match effect {
                Effect::StartTake {
                    source,
                    destination,
                    quantity,
                    completes,
                    ..
                } => Some(Activity::Take {
                    source: *source,
                    destination: *destination,
                    quantity: *quantity,
                    completes: *completes,
                }),
                Effect::StartReport {
                    recipient,
                    incident,
                    completes,
                    ..
                } => Some(Activity::ReportIncident {
                    recipient: *recipient,
                    incident: *incident,
                    completes: *completes,
                }),
                _ => None,
            },
        )?;
        let mut batch = game_ecs::Batch::new();
        let mut final_activities = activities.replacements().clone();
        if let Some((id, _, participants, _, _)) = &encounter {
            for (actor, _) in participants {
                final_activities.insert(*actor, Activity::Combat { encounter: *id });
            }
        }
        let encounter = encounter
            .map(|(id, area, participants, map, start)| {
                let map_guard = map.stage_into(&self.host.storage, &mut batch)?;
                let start_guard = start.stage_into(
                    self.host.state.combat.as_ref().ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )?;
                Ok::<_, Error>((id, area, participants, map_guard, start_guard))
            })
            .transpose()?;
        let mut interrupted = interruptions.stage_into(self, &mut batch, &final_activities)?;
        let activity_guard = activities.bind(&self.host.storage, &batch)?;
        let mut permission_memories =
            BTreeMap::<PersonId, Vec<(InventoryId, bool, PersonId)>>::new();
        for effect in change.effects() {
            if let Effect::Permission {
                actor,
                inventory,
                recipient,
                allowed,
            } = effect
            {
                permission_memories
                    .entry(*recipient)
                    .or_default()
                    .push((*inventory, *allowed, *actor));
            }
        }
        let mut memory_guards = Vec::new();
        if self.host.state.decisions.is_some() {
            for (recipient, reports) in permission_memories {
                let prepared = game_characters::decisions::prepare_permission_memories::<Command>(
                    &self.host.storage,
                    recipient,
                    &reports,
                    owner_now,
                )?;
                memory_guards.push(prepared.stage_into(&self.host.storage, &mut batch)?);
            }
        }
        let relationship_guard = if self.host.state.decisions.is_some() {
            let effects = change.effects().iter().filter_map(|effect| match effect {
                Effect::Relationship {
                    observer,
                    target,
                    trust,
                } => Some(game_characters::decisions::TrustDelta {
                    observer: *observer,
                    subject: *target,
                    delta: *trust,
                }),
                _ => None,
            });
            Some(
                game_characters::receivers::prepare_trust_reactions::<Command>(
                    &self.host.storage,
                    effects,
                )?
                .stage_into(&self.host.storage, &mut batch)?,
            )
        } else {
            None
        };
        let mut health_finishes = Vec::new();
        for effect in change.effects() {
            if let Effect::Transfer {
                actor,
                source,
                destination,
                quantity,
            } = effect
            {
                let from =
                    game_cargo::inventory(&self.host.storage, *source).ok_or(Error::Unavailable)?;
                let to = game_cargo::inventory(&self.host.storage, *destination)
                    .ok_or(Error::Unavailable)?;
                if from.good == Good::Water {
                    let reaction = game_health::WaterReaction::Transfer {
                        actor: *actor,
                        source: *source,
                        destination: *destination,
                        quantity: *quantity,
                        source_before: from.quantity,
                        destination_before: to.quantity,
                    };
                    if let Some(health) = self.prepare_health_water_reaction(reaction)? {
                        health_finishes.push(health.stage_into(
                            self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                            &self.host.storage,
                            &mut batch,
                        )?);
                    }
                }
            }
        }
        let (finish, cargo_guards) = change.stage_material_into(
            self.host
                .state
                .property
                .as_ref()
                .ok_or(Error::Unavailable)?,
            &self.host.storage,
            &mut batch,
            self.host.next_event(),
        )?;
        for guard in &cargo_guards {
            guard.validate(&self.host.storage, &batch)?;
        }
        finish.validate_runtime(
            self.host
                .state
                .property
                .as_ref()
                .ok_or(Error::Unavailable)?,
        )?;
        finish.validate_batch(&self.host.storage, &batch)?;
        for guard in &memory_guards {
            guard.validate(&batch)?;
        }
        if let Some(guard) = &relationship_guard {
            guard.validate(&self.host.storage, &batch)?;
        }
        for health in &health_finishes {
            health.validate_batch(
                self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
        }
        interrupted.validate_batch(self, &batch)?;
        activity_guard.validate(&self.host.storage, &batch)?;
        if let Some((_, _, _, map, start)) = &encounter {
            map.validate(&self.host.storage, &batch)?;
            start.validate(&self.host.storage, &batch)?;
        }
        let allocated_ids = next_id
            .checked_sub(self.host.next_id())
            .ok_or(Error::Overflow)?;
        self.host.preflight_records(
            allocated_ids,
            0,
            origin_revisions
                .checked_add(u64::from(encounter.is_some()))
                .ok_or(Error::Overflow)?,
        )?;
        let encounter_id = encounter.as_ref().map(|(id, _, _, _, _)| *id);
        let finish_origin = move |sim: &mut Simulation| {
            for health in health_finishes {
                let _ = health.finish();
            }
            let effects = finish.finish(
                sim.host
                    .state
                    .property
                    .as_mut()
                    .expect("validated property runtime"),
            );
            for effect in effects {
                match effect {
                    Effect::Interrupt { actor } => interrupted.finish_actor(sim, actor),
                    Effect::ClearTaking { actor } => {
                        for due in sim.host.state.due.values_mut() {
                            due.retain(|d| !matches!(d,Due::Physical(id) if *id==actor));
                        }
                        sim.host.state.due.retain(|_, due| !due.is_empty());
                    }
                    Effect::ClearReport {
                        sender,
                        completes,
                        interrupted,
                    } => {
                        if let Some(due) = sim.host.state.due.get_mut(&completes) {
                            due.retain(|d| !matches!(d,Due::Physical(id) if *id==sender));
                        }
                        sim.host.state.due.retain(|_, due| !due.is_empty());
                        if interrupted {
                            sim.emit(EventKind::Interrupted { actor: sender });
                        }
                    }
                    Effect::ClaimPublished {
                        claim,
                        actor,
                        inventory,
                    } => {
                        sim.emit(EventKind::PropertyClaimed {
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
                        sim.emit(EventKind::ClaimRecognized {
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
                        sim.emit(EventKind::AccessRequested {
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
                        sim.emit(EventKind::AccessRefused {
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
                        sim.emit(EventKind::PermissionShared {
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
                        sim.host
                            .state
                            .due
                            .entry(completes)
                            .or_default()
                            .push(Due::Physical(actor));
                        sim.emit(EventKind::TakingAttempted {
                            incident,
                            actor,
                            source,
                            destination,
                            quantity,
                        });
                    }
                    Effect::TakingInterrupted { incident, actor } => {
                        sim.emit(EventKind::TakingInterrupted { incident, actor });
                    }
                    Effect::Transfer {
                        actor,
                        source,
                        destination,
                        quantity,
                    } => {
                        sim.emit_prepared_water(EventKind::Transferred {
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
                        sim.emit(EventKind::Taken {
                            incident,
                            actor,
                            source,
                            destination,
                            quantity,
                        });
                    }
                    Effect::StartReport {
                        sender, completes, ..
                    } => {
                        sim.host
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
                        sim.emit(EventKind::IncidentReported {
                            sender,
                            recipient,
                            incident,
                        });
                    }
                    Effect::Engage { defender, thief } => {
                        let _ = (defender, thief);
                    }
                    Effect::Relationship { .. } => {}
                }
            }
            if let Some((id, area, participants, _, _)) = encounter {
                for (actor, _) in participants {
                    interrupted.finish_actor(sim, actor);
                }
                sim.emit(EventKind::EncounterStarted {
                    encounter: id,
                    area,
                });
            }
        };
        if encounter_id.is_some() || routed_maintenance {
            let mut extra = BTreeSet::new();
            if review_resistance {
                for (thief, incident_id) in game_property::pending_rows(&self.host.storage) {
                    extra.extend([
                        game_ecs::StableId(thief.0),
                        game_ecs::StableId(incident_id.0),
                    ]);
                    if let Some(incident) = game_property::incident(&self.host.storage, incident_id)
                    {
                        extra.extend([
                            game_ecs::StableId(incident.area.0),
                            game_ecs::StableId(incident.source.0),
                            game_ecs::StableId(incident.destination.0),
                        ]);
                        extra.extend(
                            self.property_present(incident.area)
                                .into_iter()
                                .map(|actor| game_ecs::StableId(actor.0)),
                        );
                    }
                }
            }
            let contacts = self.physical_maintenance_contacts(&game_ecs::Batch::new())?;
            let inputs =
                self.physical_maintenance_agreement_inputs(&game_ecs::Batch::new(), &[])?;
            let followups = self.prepare_property_source_followups(
                encounter_id,
                &batch,
                allocated_ids,
                origin_revisions
                    .checked_add(u64::from(encounter_id.is_some()))
                    .ok_or(Error::Overflow)?,
                extra,
                finish_origin,
                move |preview| {
                    if review_resistance {
                        preview.review_property_resistance()?;
                    }
                    if routed_maintenance {
                        preview.update_property_events()?;
                        let mut maintenance_batch = game_ecs::Batch::new();
                        let maintenance = preview
                            .prepare_physical_agreement_maintenance_with_context(
                                &mut maintenance_batch,
                                &[],
                                &contacts,
                                &inputs,
                            )?;
                        maintenance.validate(preview, &maintenance_batch)?;
                        let mut transaction = game_world::Transaction::new(
                            preview.host.transaction_opening(),
                            maintenance_batch,
                        );
                        transaction.events = maintenance.events().to_vec();
                        transaction.guards = maintenance.guards();
                        preview.host.commit_transaction(transaction)?;
                        maintenance.finish(preview);
                    }
                    Ok(())
                },
            )?;
            let (transaction, finalization) = followups.into_transaction(self)?;
            self.host.commit_transaction(transaction)?;
            finalization.finish(self);
        } else {
            self.host.storage.apply_batch(batch)?;
            self.host
                .allocate_ids(allocated_ids)
                .expect("preflighted property identifiers");
            self.host
                .bump_revision(origin_revisions)
                .expect("preflighted property source revision");
            finish_origin(self);
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
    pub(crate) fn prepare_property_receipts(
        &self,
    ) -> Result<Option<game_property::PreparedProperty>, Error> {
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
        for event in self.host.events_since(world.last_event) {
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
        let cargo = self.person_at(self.person_index[&actor]).inventory;
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
        let Some(_mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return Vec::new();
        };
        let destinations = game_cargo::carried_destinations(
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .filter_map(|id| game_cargo::inventory(&self.host.storage, *id)),
        );
        let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
            .expect("stock knowledge exists");
        let mut actions = Vec::new();
        for known in stocks.known.values() {
            let destination = destinations.get(&known.good).copied();
            let Some(need) = world.acquisition_need(
                &self.host.storage,
                actor_facts,
                known,
                good,
                quantity,
                destination,
            ) else {
                continue;
            };
            let quantity = need.quantity;
            let opportunity = game_property::NeedOpportunity {
                known: known.clone(),
                local_custodian: self
                    .property_custodian(known.inventory)
                    .is_some_and(|c| c != actor && self.property_area(c) == actor_facts.area),
                can_take: destination.is_some_and(|destination| {
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
        let actors = self.people().map(|p| p.id).collect::<Vec<_>>();
        for actor in actors {
            let Some(world) = &self.host.state.property else {
                return;
            };
            let Some(area) = self.property_area(actor) else {
                continue;
            };
            let needy = game_characters::orchestration::needy_child_present::<Command>(
                &self.host.storage,
                actor,
                area,
            );
            let actor_facts = self.property_actor_facts(actor);
            if !world.autonomous_sharing_eligible(
                &self.host.storage,
                actor_facts,
                self.host.state.ruler,
                needy,
            ) {
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
            people: state.people().map(|person| person.id).collect(),
            areas: state.state.areas.iter().map(|area| area.id).collect(),
        },
        actions,
    )
}

#[cfg(test)]
mod atomic_tests;
