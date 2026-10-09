//! Assertions follow real fungible portions. Recognition, custody, permission,
//! and physical possession remain separate; observers choose their response.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    pub claimant: PersonId,
    pub inventory: InventoryId,
    pub rights: BTreeSet<ClaimRight>,
    pub evidence: Vec<IncidentId>,
    /// Marker zero is unclaimed output. Positive markers identify snapshots
    /// taken when somebody first asserted a claim, not a second stock ledger.
    pub portions: BTreeSet<ClaimId>,
    pub recognized_by: BTreeSet<PersonId>,
    pub opposed_by: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: IncidentId,
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub area: AreaId,
    pub started_at: SimTime,
    pub changed_at: SimTime,
    pub outcome: IncidentOutcome,
    pub portions: BTreeMap<ClaimId, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentKnowledge {
    pub incident: Incident,
    pub learned_at: SimTime,
    pub source: IncidentSource,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attitude {
    pub exclusive_claims: u16,
    pub sharing: u16,
    pub willingness_to_defend: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub claims: BTreeMap<ClaimId, Claim>,
    pub incidents: BTreeMap<IncidentId, Incident>,
    pub known_claims: BTreeMap<PersonId, BTreeMap<ClaimId, ClaimView>>,
    pub knowledge: BTreeMap<PersonId, BTreeMap<IncidentId, IncidentKnowledge>>,
    pub attitudes: BTreeMap<PersonId, Attitude>,
    /// This partition records provenance, and must sum to physical quantity.
    pub portions: BTreeMap<InventoryId, BTreeMap<ClaimId, u64>>,
    pub pending: BTreeMap<PersonId, IncidentId>,
    pub last_event: u64,
    pub next_search: BTreeMap<PersonId, SimTime>,
}

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
            knowledge: BTreeMap::new(),
            last_observation_event: sim.state.next_event - 1,
            agreements: BTreeMap::new(),
            obligations: BTreeMap::new(),
            amendments: BTreeMap::new(),
            custodians,
            last_event: sim.state.next_event - 1,
            next_search: BTreeMap::new(),
            pending_work: BTreeMap::new(),
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
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
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
            && self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .health
                > 0
    }
    fn taking_possible(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        require_denial: bool,
    ) -> bool {
        if !self.property_able(actor) || quantity.0 == 0 || source == destination {
            return false;
        }
        let Some((from, to)) = self
            .state
            .inventories
            .get(&source)
            .zip(self.state.inventories.get(&destination))
        else {
            return false;
        };
        let physical = self.state.physical.as_ref().unwrap();
        let area = self.property_area(actor);
        (!require_denial || !from.access.contains(&actor))
            && self.inventory_area(from) == area
            && self.inventory_area(to) == area
            && from.good == to.good
            && to.location == InventoryLocation::Carried(actor)
            && to.access.contains(&actor)
            && from.quantity >= quantity
            && self.transfer_capacity(destination, quantity.0)
            && physical.actors[&actor].vitals.fatigue < physical.rules.fatigue_limit
    }

    pub fn debug_set_property_attitude(
        &mut self,
        actor: PersonId,
        attitude: Attitude,
    ) -> Result<(), Error> {
        if [
            attitude.exclusive_claims,
            attitude.sharing,
            attitude.willingness_to_defend,
        ]
        .iter()
        .any(|v| *v > 1_000)
            || !self.person_index.contains_key(&actor)
        {
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
        if self.state.property.is_none() || !self.property_able(actor) {
            return Err(Error::Unavailable);
        }
        let area = self.property_area(actor).unwrap();
        match action {
            PropertyAction::RequestAccess { inventory } => {
                let custodian = self
                    .property_custodian(inventory)
                    .ok_or(Error::Unavailable)?;
                let stock = self
                    .state
                    .inventories
                    .get(&inventory)
                    .ok_or(Error::Unavailable)?;
                if actor == custodian
                    || self.property_area(custodian) != Some(area)
                    || self.inventory_area(stock) != Some(area)
                    || stock.access.contains(&actor)
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(3)?;
                self.state
                    .property
                    .as_mut()
                    .unwrap()
                    .next_search
                    .insert(actor, SimTime(self.state.time.0.saturating_add(120_000)));
                self.emit(EventKind::AccessRequested {
                    actor,
                    custodian,
                    inventory,
                });
                // The request makes need observable. The custodian still chooses
                // using their own attitude, relationship and available supply.
                let world = self.state.property.as_ref().unwrap();
                let willing = custodian != self.state.ruler
                    && self.property_able(custodian)
                    && world
                        .attitudes
                        .get(&custodian)
                        .is_some_and(|attitude| attitude.sharing >= 700)
                    && self.state.inventories[&inventory].quantity.0
                        >= self
                            .state
                            .physical
                            .as_ref()
                            .unwrap()
                            .rules
                            .meal_size
                            .0
                            .saturating_mul(2)
                    && self.state.decisions.as_ref().unwrap().minds[&custodian]
                        .relationships
                        .get(&actor)
                        .is_none_or(|relation| relation.trust >= 0);
                if willing {
                    self.debug_property_action(
                        custodian,
                        PropertyAction::Share {
                            inventory,
                            recipient: actor,
                            allowed: true,
                        },
                    )?;
                } else {
                    self.emit(EventKind::AccessRefused {
                        actor,
                        custodian,
                        inventory,
                    });
                }
            }
            PropertyAction::Claim {
                inventory,
                rights,
                evidence,
            } => {
                let stock = self
                    .state
                    .inventories
                    .get(&inventory)
                    .ok_or(Error::Unavailable)?;
                if self.inventory_area(stock) != Some(area)
                    || stock.quantity.0 == 0
                    || rights.is_empty()
                    || evidence.len() > 16
                    || evidence.iter().any(|id| {
                        !self
                            .state
                            .property
                            .as_ref()
                            .unwrap()
                            .knowledge
                            .get(&actor)
                            .is_some_and(|known| known.contains_key(id))
                    })
                    || self
                        .state
                        .property
                        .as_ref()
                        .unwrap()
                        .claims
                        .values()
                        .any(|claim| {
                            claim.claimant == actor
                                && claim.inventory == inventory
                                && claim.rights == rights
                                && claim.evidence == evidence
                                && self
                                    .state
                                    .property
                                    .as_ref()
                                    .unwrap()
                                    .portions
                                    .get(&inventory)
                                    .is_some_and(|portions| {
                                        portions
                                            .keys()
                                            .all(|portion| claim.portions.contains(portion))
                                    })
                        })
                {
                    return Err(Error::Unavailable);
                }
                self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
                self.ensure_event_capacity(1)?;
                self.update_property_events();
                let id = ClaimId(allocate(&mut self.state)?);
                let world = self.state.property.as_mut().unwrap();
                let portions = world.portions.get_mut(&inventory).unwrap();
                if let Some(unclaimed) = portions.remove(&ClaimId(0)) {
                    portions.insert(id, unclaimed);
                }
                let claim = Claim {
                    id,
                    claimant: actor,
                    inventory,
                    rights: rights.clone(),
                    evidence: evidence.clone(),
                    portions: portions.keys().copied().collect(),
                    recognized_by: BTreeSet::from([actor]),
                    opposed_by: BTreeSet::new(),
                };
                let view = ClaimView {
                    id,
                    claimant: actor,
                    inventory,
                    rights,
                    evidence,
                    recognized_by: BTreeSet::from([actor]),
                    opposed_by: BTreeSet::new(),
                };
                world.claims.insert(id, claim);
                let present = self.property_present(area);
                for person in present {
                    self.state
                        .property
                        .as_mut()
                        .unwrap()
                        .known_claims
                        .entry(person)
                        .or_default()
                        .insert(id, view.clone());
                }
                self.emit(EventKind::PropertyClaimed {
                    claim: id,
                    actor,
                    inventory,
                });
            }
            PropertyAction::Recognize { claim } | PropertyAction::Contest { claim } => {
                let support = matches!(action, PropertyAction::Recognize { .. });
                let mut context = self
                    .state
                    .property
                    .as_ref()
                    .unwrap()
                    .known_claims
                    .get(&actor)
                    .and_then(|known| known.get(&claim))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                self.ensure_event_capacity(1)?;
                let assertion = self
                    .state
                    .property
                    .as_mut()
                    .unwrap()
                    .claims
                    .get_mut(&claim)
                    .ok_or(Error::Unavailable)?;
                if support {
                    assertion.recognized_by.insert(actor);
                    assertion.opposed_by.remove(&actor);
                    context.recognized_by.insert(actor);
                    context.opposed_by.remove(&actor);
                } else {
                    assertion.opposed_by.insert(actor);
                    assertion.recognized_by.remove(&actor);
                    context.opposed_by.insert(actor);
                    context.recognized_by.remove(&actor);
                }
                for person in self.property_present(area) {
                    let known = self
                        .state
                        .property
                        .as_mut()
                        .unwrap()
                        .known_claims
                        .entry(person)
                        .or_default()
                        .entry(claim)
                        .or_insert_with(|| context.clone());
                    if support {
                        known.recognized_by.insert(actor);
                        known.opposed_by.remove(&actor);
                    } else {
                        known.opposed_by.insert(actor);
                        known.recognized_by.remove(&actor);
                    }
                }
                self.emit(EventKind::ClaimRecognized {
                    claim,
                    actor,
                    support,
                });
            }
            PropertyAction::Share {
                inventory,
                recipient,
                allowed,
            } => {
                if self.property_custodian(inventory) != Some(actor)
                    || self.property_area(recipient) != Some(area)
                    || self.inventory_area(
                        self.state
                            .inventories
                            .get(&inventory)
                            .ok_or(Error::Unavailable)?,
                    ) != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(1)?;
                let stock = self.state.inventories.get_mut(&inventory).unwrap();
                if allowed {
                    stock.access.insert(recipient);
                } else {
                    stock.access.remove(&recipient);
                }
                if let Some(fact) = self
                    .state
                    .decisions
                    .as_mut()
                    .and_then(|world| world.minds.get_mut(&recipient))
                    .and_then(|mind| mind.knowledge.get_mut(&inventory))
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
            PropertyAction::Take {
                source,
                destination,
                quantity,
            } => {
                if !self.taking_possible(actor, source, destination, quantity, true) {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(self.state.physical.as_ref().unwrap().rules.loading_time.0)
                        .ok_or(Error::Overflow)?,
                );
                // Worst case includes a fully populated existing combat roster.
                self.state.next_id.checked_add(4).ok_or(Error::Overflow)?;
                self.ensure_event_capacity(self.state.people.len() * 8 + 8)?;
                self.update_property_events();
                self.interrupt_physical(actor);
                let incident = IncidentId(allocate(&mut self.state)?);
                let mut remaining = self.state.property.as_ref().unwrap().portions[&source].clone();
                let portions = remove_portions(&mut remaining, quantity.0);
                self.state.property.as_mut().unwrap().incidents.insert(
                    incident,
                    Incident {
                        id: incident,
                        actor,
                        source,
                        destination,
                        quantity,
                        area,
                        started_at: self.state.time,
                        changed_at: self.state.time,
                        outcome: IncidentOutcome::Attempt,
                        portions,
                    },
                );
                self.state
                    .property
                    .as_mut()
                    .unwrap()
                    .pending
                    .insert(actor, incident);
                self.state
                    .property
                    .as_mut()
                    .unwrap()
                    .next_search
                    .insert(actor, SimTime(self.state.time.0.saturating_add(120_000)));
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .activity = Activity::Take {
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
                self.witness_property_incident(incident);
                self.review_property_resistance();
            }
            PropertyAction::ReportIncident {
                recipient,
                incident,
            } => {
                let known = self
                    .state
                    .property
                    .as_ref()
                    .unwrap()
                    .knowledge
                    .get(&actor)
                    .and_then(|known| known.get(&incident))
                    .ok_or(Error::Unavailable)?;
                let receiver = self
                    .state
                    .physical
                    .as_ref()
                    .unwrap()
                    .actors
                    .get(&recipient)
                    .ok_or(Error::Unavailable)?;
                if actor == recipient
                    || self.property_area(recipient) != Some(area)
                    || !matches!(receiver.activity, Activity::Idle | Activity::Rest)
                    || known.learned_at > self.state.time
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(60_000)
                        .ok_or(Error::Overflow)?,
                );
                self.interrupt_physical(actor);
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .activity = Activity::ReportIncident {
                    recipient,
                    incident,
                    completes,
                };
                self.state
                    .due
                    .entry(completes)
                    .or_default()
                    .push(Due::Physical(actor));
            }
            PropertyAction::ResistTaking { thief } => {
                let incident = self
                    .state
                    .property
                    .as_ref()
                    .unwrap()
                    .pending
                    .get(&thief)
                    .and_then(|id| self.state.property.as_ref().unwrap().incidents.get(id))
                    .ok_or(Error::Unavailable)?;
                if actor == thief
                    || incident.area != area
                    || !self
                        .state
                        .property
                        .as_ref()
                        .unwrap()
                        .knowledge
                        .get(&actor)
                        .is_some_and(|known| known.contains_key(&incident.id))
                {
                    return Err(Error::Unavailable);
                }
                self.debug_start_encounter(vec![(actor, 0), (thief, 1)])?;
            }
        }
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn observe_property(&mut self) {
        let incidents: Vec<_> = self
            .state
            .property
            .as_ref()
            .map(|world| world.pending.values().copied().collect())
            .unwrap_or_default();
        for incident in incidents {
            self.witness_property_incident(incident);
        }
    }
    fn witness_property_incident(&mut self, id: IncidentId) {
        let incident = self.state.property.as_ref().unwrap().incidents[&id].clone();
        for observer in self.property_present(incident.area) {
            let old_outcome = self
                .state
                .property
                .as_ref()
                .unwrap()
                .knowledge
                .get(&observer)
                .and_then(|known| known.get(&id))
                .map(|known| known.incident.outcome);
            self.state
                .property
                .as_mut()
                .unwrap()
                .knowledge
                .entry(observer)
                .or_default()
                .insert(
                    id,
                    IncidentKnowledge {
                        incident: incident.clone(),
                        learned_at: self.state.time,
                        source: IncidentSource::Observed,
                    },
                );
            if incident.outcome == IncidentOutcome::Taken
                && old_outcome != Some(IncidentOutcome::Taken)
            {
                self.interpret_property_incident(observer, &incident);
            }
        }
    }
    fn interpret_property_incident(&mut self, observer: PersonId, incident: &Incident) {
        if observer == incident.actor {
            return;
        }
        let world = self.state.property.as_ref().unwrap();
        let Some(attitude) = world.attitudes.get(&observer) else {
            return;
        };
        let supported = world.known_claims.get(&observer).is_some_and(|known| {
            known.keys().any(|id| {
                let claim = &world.claims[id];
                claim.recognized_by.contains(&observer)
                    && claim.rights.contains(&ClaimRight::Transfer)
                    && claim
                        .portions
                        .iter()
                        .any(|portion| incident.portions.contains_key(portion))
            })
        });
        let owns_custody = self.property_custodian(incident.source) == Some(observer)
            && !self.state.inventories[&incident.source]
                .access
                .contains(&incident.actor);
        let personally_allowed = self.property_custodian(incident.source) == Some(observer)
            && self.state.inventories[&incident.source]
                .access
                .contains(&incident.actor);
        let delta = if !personally_allowed
            && (supported || owns_custody)
            && attitude.exclusive_claims >= 500
        {
            -100
        } else if attitude.sharing >= 700 {
            10
        } else {
            0
        };
        if delta != 0
            && let Some(mind) = self
                .state
                .decisions
                .as_mut()
                .and_then(|world| world.minds.get_mut(&observer))
        {
            let relationship =
                mind.relationships
                    .entry(incident.actor)
                    .or_insert(decisions::Relationship {
                        trust: 0,
                        affection: 0,
                    });
            relationship.trust = (relationship.trust + delta).clamp(-1_000, 1_000);
        }
    }

    pub(crate) fn review_property_resistance(&mut self) {
        let pending: Vec<_> = self
            .state
            .property
            .as_ref()
            .map(|w| w.pending.iter().map(|(actor, id)| (*actor, *id)).collect())
            .unwrap_or_default();
        for (thief, id) in pending {
            let incident = self.state.property.as_ref().unwrap().incidents[&id].clone();
            let defender = self
                .property_present(incident.area)
                .into_iter()
                .filter(|actor| *actor != thief && *actor != self.state.ruler)
                .find(|actor| {
                    if !self.property_able(*actor) {
                        return false;
                    }
                    let world = self.state.property.as_ref().unwrap();
                    let Some(attitude) = world.attitudes.get(actor) else {
                        return false;
                    };
                    let personal = self.property_custodian(incident.source) == Some(*actor);
                    let supported = world.known_claims.get(actor).is_some_and(|known| {
                        known.keys().any(|id| {
                            world.claims[id].recognized_by.contains(actor)
                                && world.claims[id]
                                    .portions
                                    .iter()
                                    .any(|p| incident.portions.contains_key(p))
                        })
                    });
                    (personal || supported) && attitude.willingness_to_defend >= 500
                });
            if let Some(defender) = defender {
                let _ = self.debug_start_encounter(vec![(defender, 0), (thief, 1)]);
            }
        }
    }

    pub(crate) fn interrupt_property_action(&mut self, actor: PersonId) {
        let Some(id) = self
            .state
            .property
            .as_mut()
            .and_then(|w| w.pending.remove(&actor))
        else {
            return;
        };
        let incident = self
            .state
            .property
            .as_mut()
            .unwrap()
            .incidents
            .get_mut(&id)
            .unwrap();
        incident.outcome = IncidentOutcome::Interrupted;
        incident.changed_at = self.state.time;
        self.witness_property_incident(id);
        self.emit(EventKind::TakingInterrupted {
            incident: id,
            actor,
        });
    }

    pub(crate) fn complete_property_action(&mut self, actor: PersonId) {
        let activity = self.state.physical.as_ref().unwrap().actors[&actor]
            .activity
            .clone();
        match activity {
            Activity::Take {
                source,
                destination,
                quantity,
                ..
            } => {
                let Some(id) = self.state.property.as_mut().unwrap().pending.remove(&actor) else {
                    return;
                };
                let success = self.taking_possible(actor, source, destination, quantity, false);
                if success {
                    let mut remaining =
                        self.state.property.as_ref().unwrap().portions[&source].clone();
                    self.state
                        .property
                        .as_mut()
                        .unwrap()
                        .incidents
                        .get_mut(&id)
                        .unwrap()
                        .portions = remove_portions(&mut remaining, quantity.0);
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
                let incident = self
                    .state
                    .property
                    .as_mut()
                    .unwrap()
                    .incidents
                    .get_mut(&id)
                    .unwrap();
                incident.outcome = if success {
                    IncidentOutcome::Taken
                } else {
                    IncidentOutcome::Interrupted
                };
                incident.changed_at = self.state.time;
                self.witness_property_incident(id);
                if success {
                    self.emit(EventKind::Taken {
                        incident: id,
                        actor,
                        source,
                        destination,
                        quantity,
                    });
                } else {
                    self.emit(EventKind::TakingInterrupted {
                        incident: id,
                        actor,
                    });
                }
                self.update_property_events();
            }
            Activity::ReportIncident {
                recipient,
                incident,
                ..
            } => {
                let same_area = self.property_area(actor).is_some()
                    && self.property_area(actor) == self.property_area(recipient);
                let receiver_idle = self
                    .state
                    .physical
                    .as_ref()
                    .unwrap()
                    .actors
                    .get(&recipient)
                    .is_some_and(|body| matches!(body.activity, Activity::Idle | Activity::Rest));
                if same_area && receiver_idle {
                    let remembered =
                        self.state.property.as_ref().unwrap().knowledge[&actor][&incident].clone();
                    let earlier = self
                        .state
                        .property
                        .as_ref()
                        .unwrap()
                        .knowledge
                        .get(&recipient)
                        .and_then(|known| known.get(&incident))
                        .map(|known| known.incident.outcome);
                    let known = self
                        .state
                        .property
                        .as_mut()
                        .unwrap()
                        .knowledge
                        .entry(recipient)
                        .or_default()
                        .entry(incident)
                        .or_insert_with(|| remembered.clone());
                    if known.incident.changed_at <= remembered.incident.changed_at {
                        *known = IncidentKnowledge {
                            incident: remembered.incident.clone(),
                            learned_at: self.state.time,
                            source: IncidentSource::Reported { sender: actor },
                        };
                    }
                    if remembered.incident.outcome == IncidentOutcome::Taken
                        && earlier != Some(IncidentOutcome::Taken)
                    {
                        self.interpret_property_incident(recipient, &remembered.incident);
                    }
                    self.emit(EventKind::IncidentReported {
                        sender: actor,
                        recipient,
                        incident,
                    });
                }
            }
            _ => return,
        }
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Idle;
    }

    pub(crate) fn update_property_events(&mut self) {
        let Some(world) = &self.state.property else {
            return;
        };
        let events: Vec<_> = self
            .state
            .events
            .iter()
            .filter(|event| event.id.0 > world.last_event)
            .cloned()
            .collect();
        // Newly created inventories may contain goods before a creation event.
        // Reconstruct their opening balance from this batch rather than copying
        // a closing balance and then applying the same transfer twice.
        for stock in self.state.inventories.values() {
            if self
                .state
                .property
                .as_ref()
                .unwrap()
                .portions
                .contains_key(&stock.id)
            {
                continue;
            }
            let mut opening = i128::from(stock.quantity.0);
            for event in &events {
                match event.kind {
                    EventKind::Transferred {
                        source,
                        destination,
                        quantity,
                        ..
                    } => {
                        if source == stock.id {
                            opening += i128::from(quantity.0);
                        }
                        if destination == stock.id {
                            opening -= i128::from(quantity.0);
                        }
                    }
                    EventKind::Ate {
                        source, quantity, ..
                    }
                    | EventKind::Consumed {
                        source, quantity, ..
                    }
                    | EventKind::Cared {
                        source, quantity, ..
                    } if source == stock.id => {
                        opening += i128::from(quantity.0);
                    }
                    EventKind::Spoiled {
                        inventory,
                        quantity,
                    } if inventory == stock.id => {
                        opening += i128::from(quantity.0);
                    }
                    EventKind::Produced {
                        output, quantity, ..
                    }
                    | EventKind::ResourceCollected {
                        destination: output,
                        quantity,
                        ..
                    } if output == stock.id => {
                        opening -= i128::from(quantity.0);
                    }
                    EventKind::CropHarvested {
                        field, food, seed, ..
                    } => {
                        if let Some(field) = self
                            .state
                            .environment
                            .as_ref()
                            .and_then(|world| world.fields.get(&field))
                        {
                            if field.food == stock.id {
                                opening -= i128::from(food.0);
                            }
                            if field.seed == stock.id {
                                opening -= i128::from(seed.0);
                            }
                        }
                    }
                    _ => {}
                }
            }
            let opening = u64::try_from(opening)
                .expect("physical events conserve inventory opening balances");
            self.state.property.as_mut().unwrap().portions.insert(
                stock.id,
                if opening > 0 {
                    BTreeMap::from([(ClaimId(0), opening)])
                } else {
                    BTreeMap::new()
                },
            );
        }
        for event in events {
            match event.kind {
                EventKind::Born { child, .. } => {
                    self.state
                        .property
                        .as_mut()
                        .unwrap()
                        .attitudes
                        .entry(child)
                        .or_insert(Attitude {
                            exclusive_claims: 0,
                            sharing: 0,
                            willingness_to_defend: 0,
                        });
                    self.state
                        .property
                        .as_mut()
                        .unwrap()
                        .known_claims
                        .entry(child)
                        .or_default();
                    self.state
                        .property
                        .as_mut()
                        .unwrap()
                        .knowledge
                        .entry(child)
                        .or_default();
                }
                EventKind::Transferred {
                    source,
                    destination,
                    quantity,
                    ..
                } => {
                    let parts = remove_portions(
                        self.state
                            .property
                            .as_mut()
                            .unwrap()
                            .portions
                            .entry(source)
                            .or_default(),
                        quantity.0,
                    );
                    let target = self
                        .state
                        .property
                        .as_mut()
                        .unwrap()
                        .portions
                        .entry(destination)
                        .or_default();
                    for (portion, amount) in parts {
                        *target.entry(portion).or_default() += amount;
                    }
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
                    remove_portions(
                        self.state
                            .property
                            .as_mut()
                            .unwrap()
                            .portions
                            .entry(source)
                            .or_default(),
                        quantity.0,
                    );
                }
                EventKind::Spoiled {
                    inventory,
                    quantity,
                } => {
                    remove_portions(
                        self.state
                            .property
                            .as_mut()
                            .unwrap()
                            .portions
                            .entry(inventory)
                            .or_default(),
                        quantity.0,
                    );
                }
                EventKind::Produced {
                    output, quantity, ..
                }
                | EventKind::ResourceCollected {
                    destination: output,
                    quantity,
                    ..
                } if quantity.0 > 0 => {
                    *self
                        .state
                        .property
                        .as_mut()
                        .unwrap()
                        .portions
                        .entry(output)
                        .or_default()
                        .entry(ClaimId(0))
                        .or_default() += quantity.0;
                }
                EventKind::CropHarvested {
                    field, food, seed, ..
                } => {
                    if let Some(field) = self
                        .state
                        .environment
                        .as_ref()
                        .and_then(|w| w.fields.get(&field))
                    {
                        let outputs = [(field.food, food.0), (field.seed, seed.0)];
                        for (output, amount) in outputs {
                            if amount > 0 {
                                *self
                                    .state
                                    .property
                                    .as_mut()
                                    .unwrap()
                                    .portions
                                    .entry(output)
                                    .or_default()
                                    .entry(ClaimId(0))
                                    .or_default() += amount;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let world = self.state.property.as_mut().unwrap();
        world.last_event = self.state.next_event - 1;
        for stock in self.state.inventories.values() {
            world.portions.entry(stock.id).or_insert_with(|| {
                if stock.quantity.0 > 0 {
                    BTreeMap::from([(ClaimId(0), stock.quantity.0)])
                } else {
                    BTreeMap::new()
                }
            });
        }
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
                        **person != actor
                            && matches!(
                                self.state.physical.as_ref().unwrap().actors[person].activity,
                                Activity::Idle | Activity::Rest
                            )
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

    pub(crate) fn property_candidate(&self, actor: PersonId) -> Option<Command> {
        let world = self.state.property.as_ref()?;
        if !self.property_able(actor)
            || world
                .next_search
                .get(&actor)
                .is_some_and(|time| *time > self.state.time)
        {
            return None;
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        let mind = self.state.decisions.as_ref()?.minds.get(&actor)?;
        if body.vitals.hunger < self.state.physical.as_ref().unwrap().rules.meal_relief {
            return None;
        }
        let destination = self.state.people[self.person_index[&actor]].inventory;
        let quantity = self.state.physical.as_ref().unwrap().rules.meal_size;
        for fact in mind.knowledge.values() {
            if fact.good != Good::Food
                || fact.accessible
                || fact.quantity < quantity
                || Some(fact.area) != self.property_area(actor)
            {
                continue;
            }
            if mind.personality.honesty >= 400 || mind.personality.risk_tolerance < 600 {
                if self
                    .property_custodian(fact.inventory)
                    .is_some_and(|custodian| {
                        custodian != actor
                            && self.property_area(custodian) == self.property_area(actor)
                    })
                {
                    return Some(Command::Property {
                        action: PropertyAction::RequestAccess {
                            inventory: fact.inventory,
                        },
                    });
                }
                continue;
            }
            if self.taking_possible(actor, fact.inventory, destination, quantity, true) {
                return Some(Command::Property {
                    action: PropertyAction::Take {
                        source: fact.inventory,
                        destination,
                        quantity,
                    },
                });
            }
        }
        None
    }

    pub(crate) fn review_property_sharing(&mut self) {
        let Some(world) = &self.state.property else {
            return;
        };
        let candidates: Vec<_> = world
            .attitudes
            .iter()
            .filter(|(actor, attitude)| **actor != self.state.ruler && attitude.sharing >= 700)
            .map(|(actor, _)| *actor)
            .collect();
        for actor in candidates {
            if !self.property_able(actor) {
                continue;
            }
            let area = self.property_area(actor).unwrap();
            let needy = self
                .state
                .decisions
                .as_ref()
                .and_then(|w| w.minds.get(&actor))
                .is_some_and(|m| {
                    m.child_signals
                        .values()
                        .any(|signal| signal.area == area && signal.urgency >= 1_000)
                });
            if !needy {
                continue;
            }
            let recipients = self.property_present(area);
            let stores: Vec<_> = self
                .state
                .inventories
                .values()
                .filter(|stock| {
                    stock.good == Good::Food
                        && stock.quantity.0
                            >= self.state.physical.as_ref().unwrap().rules.meal_size.0
                        && self.property_custodian(stock.id) == Some(actor)
                        && self.inventory_area(stock) == Some(area)
                })
                .map(|stock| stock.id)
                .collect();
            for store in stores {
                for recipient in &recipients {
                    if *recipient != actor
                        && !self.state.inventories[&store].access.contains(recipient)
                    {
                        let _ = self.debug_property_action(
                            actor,
                            PropertyAction::Share {
                                inventory: store,
                                recipient: *recipient,
                                allowed: true,
                            },
                        );
                    }
                }
            }
        }
    }
}

fn remove_portions(
    source: &mut BTreeMap<ClaimId, u64>,
    mut quantity: u64,
) -> BTreeMap<ClaimId, u64> {
    let mut removed = BTreeMap::new();
    for id in source.keys().copied().collect::<Vec<_>>() {
        if quantity == 0 {
            break;
        }
        let amount = source[&id].min(quantity);
        *source.get_mut(&id).unwrap() -= amount;
        quantity -= amount;
        if source[&id] == 0 {
            source.remove(&id);
        }
        removed.insert(id, amount);
    }
    removed
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.property else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid property state".into());
    let people: BTreeMap<_, _> = state
        .people
        .iter()
        .map(|person| (person.id, person))
        .collect();
    let physical = state.physical.as_ref().ok_or_else(fail)?;
    if state.decisions.is_none()
        || state.agreements.is_none()
        || state.combat.is_none()
        || world.last_event >= state.next_event
        || world.attitudes.iter().any(|(actor, attitude)| {
            !people.contains_key(actor)
                || [
                    attitude.exclusive_claims,
                    attitude.sharing,
                    attitude.willingness_to_defend,
                ]
                .iter()
                .any(|v| *v > 1_000)
        })
        || world.next_search.keys().any(|id| !people.contains_key(id))
        || world.portions.len() != state.inventories.len()
    {
        return Err(fail());
    }
    for (id, claim) in &world.claims {
        if *id != claim.id
            || id.0 == 0
            || id.0 >= state.next_id
            || !people.contains_key(&claim.claimant)
            || !state.inventories.contains_key(&claim.inventory)
            || claim.rights.is_empty()
            || claim.portions.is_empty()
            || claim
                .portions
                .iter()
                .any(|portion| portion.0 == 0 || !world.claims.contains_key(portion))
            || claim.evidence.len() > 16
            || claim
                .evidence
                .iter()
                .any(|id| !world.incidents.contains_key(id))
            || claim
                .recognized_by
                .iter()
                .chain(&claim.opposed_by)
                .any(|person| !people.contains_key(person))
            || !claim.recognized_by.is_disjoint(&claim.opposed_by)
        {
            return Err(fail());
        }
    }
    for (inventory, portions) in &world.portions {
        let stock = state.inventories.get(inventory).ok_or_else(fail)?;
        let mut total = 0_u64;
        for (marker, quantity) in portions {
            if *quantity == 0 || marker.0 != 0 && !world.claims.contains_key(marker) {
                return Err(fail());
            }
            total = total.checked_add(*quantity).ok_or_else(fail)?;
        }
        if total != stock.quantity.0 {
            return Err(fail());
        }
    }
    for (actor, known) in &world.known_claims {
        if !people.contains_key(actor) {
            return Err(fail());
        }
        for (id, view) in known {
            let claim = world.claims.get(id).ok_or_else(fail)?;
            if *id != view.id
                || claim.claimant != view.claimant
                || claim.inventory != view.inventory
                || claim.rights != view.rights
                || claim.evidence != view.evidence
                || view
                    .recognized_by
                    .iter()
                    .chain(&view.opposed_by)
                    .any(|person| !people.contains_key(person))
                || !view.recognized_by.is_disjoint(&view.opposed_by)
            {
                return Err(fail());
            }
        }
    }
    let valid_incident = |incident: &Incident| {
        incident.id.0 > 0
            && incident.id.0 < state.next_id
            && people.contains_key(&incident.actor)
            && state
                .inventories
                .get(&incident.source)
                .zip(state.inventories.get(&incident.destination))
                .is_some_and(|(a, b)| {
                    a.good == b.good
                        && a.id != b.id
                        && b.location == InventoryLocation::Carried(incident.actor)
                })
            && state.areas.iter().any(|area| area.id == incident.area)
            && incident.quantity.0 > 0
            && incident.started_at <= incident.changed_at
            && incident.changed_at <= state.time
            && incident.portions.iter().all(|(marker, quantity)| {
                *quantity > 0 && (marker.0 == 0 || world.claims.contains_key(marker))
            })
            && incident
                .portions
                .values()
                .map(|quantity| u128::from(*quantity))
                .sum::<u128>()
                == u128::from(incident.quantity.0)
    };
    for (id, incident) in &world.incidents {
        if *id != incident.id
            || !valid_incident(incident)
            || incident.outcome == IncidentOutcome::Attempt
                && world.pending.get(&incident.actor) != Some(id)
        {
            return Err(fail());
        }
    }
    for (actor, known) in &world.knowledge {
        if !people.contains_key(actor) {
            return Err(fail());
        }
        for (id, knowledge) in known {
            let actual = world.incidents.get(id).ok_or_else(fail)?;
            let remembered = &knowledge.incident;
            if *id != remembered.id
                || !valid_incident(remembered)
                || knowledge.learned_at > state.time
                || remembered.changed_at > knowledge.learned_at
                || remembered.changed_at > actual.changed_at
                || remembered.actor != actual.actor
                || remembered.source != actual.source
                || remembered.destination != actual.destination
                || remembered.quantity != actual.quantity
                || remembered.area != actual.area
                || remembered.started_at != actual.started_at
                || matches!(knowledge.source, IncidentSource::Reported { sender } if !people.contains_key(&sender) || sender == *actor)
            {
                return Err(fail());
            }
        }
    }
    for (actor, id) in &world.pending {
        let incident = world.incidents.get(id).ok_or_else(fail)?;
        let body = physical.actors.get(actor).ok_or_else(fail)?;
        if incident.actor != *actor
            || incident.outcome != IncidentOutcome::Attempt
            || !body.vitals.alive
            || people[actor].location != Location::AtArea(incident.area)
            || !matches!(body.activity, Activity::Take { source, destination, quantity, completes } if source == incident.source && destination == incident.destination && quantity == incident.quantity && completes > state.time && state.due.get(&completes).is_some_and(|due| due.iter().any(|event| matches!(event,Due::Physical(id) if id == actor))))
        {
            return Err(fail());
        }
    }
    for (actor, body) in &physical.actors {
        match body.activity {
            Activity::Take { .. } if !world.pending.contains_key(actor) => return Err(fail()),
            Activity::ReportIncident {
                recipient,
                incident,
                completes,
            } if !people.contains_key(&recipient)
                || actor == &recipient
                || completes <= state.time
                || !world
                    .knowledge
                    .get(actor)
                    .is_some_and(|known| known.contains_key(&incident)) =>
            {
                return Err(fail());
            }
            _ => {}
        }
    }
    Ok(())
}
