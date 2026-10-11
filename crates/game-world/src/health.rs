//! Water lots retain contaminant load across actual collection and transport.
//! Clinical effects are independent of custody and of who knows the water is unsafe.
use crate::*;

pub use game_health::*;
type HealthCohort = (
    game_transport::PositionKey,
    Option<ShelterId>,
    Option<(EncounterId, PersonId)>,
);

struct HealthPlanningData {
    area: AreaId,
    stationary: bool,
    values: game_health::IntakeValues,
    stocks: Vec<game_health::RememberedStock>,
}
impl HealthPlanningData {
    fn input(&self, now: SimTime) -> game_health::HealthPlanning<'_> {
        game_health::HealthPlanning {
            now,
            area: self.area,
            stationary: self.stationary,
            values: self.values,
            stocks: &self.stocks,
        }
    }
}
impl Simulation {
    /// Called after the physical water enters a stock. Existing contamination
    /// is mixed with this source; collection never changes physical quantity.
    pub(crate) fn collected_water_quality(
        &mut self,
        inventory: InventoryId,
        amount: Quantity,
        contamination: u16,
    ) {
        assert!(contamination <= 1_000);
        let stock = &self.state.inventories[&inventory];
        assert_eq!(stock.good, Good::Water);
        assert!(stock.quantity >= amount);
        let quantity = stock.quantity.0;
        let world = self.state.health.get_or_insert_with(Runtime::default);
        let existing = world.water.get(&inventory).map_or(0, WaterLot::load);
        let lot = game_health::collect_water(CollectionInput {
            quantity_after: quantity,
            existing_load: existing,
            collected: amount.0,
            contamination,
        })
        .expect("validated physical collection");
        world.water.insert(inventory, lot);
        if let InventoryLocation::Carried(actor) = stock.location {
            let old = world
                .known
                .get(&actor)
                .and_then(|k| k.water.get(&inventory))
                .cloned();
            let purified = contamination == 0
                && (quantity == amount.0 || old.as_ref().is_some_and(|s| s.purified));
            let suspected_unsafe = contamination > 0 || old.is_some_and(|s| s.suspected_unsafe);
            world.known.entry(actor).or_default().water.insert(
                inventory,
                WaterSafetyView {
                    inventory,
                    purified,
                    suspected_unsafe,
                    observed_at: self.state.time,
                },
            );
        }
    }
    pub(crate) fn update_water_event(&mut self, event: &EventKind) {
        let Some(world) = &mut self.state.health else {
            return;
        };
        match *event {
            EventKind::Transferred {
                source,
                destination,
                quantity,
                ..
            } if self.state.inventories[&source].good == Good::Water => {
                let source_before = self.state.inventories[&source].quantity.0 + quantity.0;
                let destination_before =
                    self.state.inventories[&destination].quantity.0 - quantity.0;
                let a = world
                    .water
                    .get(&source)
                    .cloned()
                    .unwrap_or_else(|| WaterLot::new(source_before, 0));
                let b = world
                    .water
                    .get(&destination)
                    .cloned()
                    .unwrap_or_else(|| WaterLot::new(destination_before, 0));
                let effect = game_health::transfer_water(WaterTransferInput {
                    source: &a,
                    destination: &b,
                    quantity: quantity.0,
                })
                .expect("validated physical transfer");
                world.water.insert(source, effect.source);
                world.water.insert(destination, effect.destination);
            }
            EventKind::Consumed {
                source, quantity, ..
            }
            | EventKind::Cared {
                source, quantity, ..
            } if self.state.inventories[&source].good == Good::Water => {
                let before = self.state.inventories[&source].quantity.0 + quantity.0;
                let lot = world
                    .water
                    .get(&source)
                    .cloned()
                    .unwrap_or_else(|| WaterLot::new(before, 0));
                let effect = game_health::consume_water(&lot, quantity.0)
                    .expect("validated physical consumption");
                world.water.insert(source, effect.remaining);
            }
            EventKind::Produced {
                output, quantity, ..
            } if self.state.inventories[&output].good == Good::Water => {
                let quantity_now = self.state.inventories[&output].quantity.0;
                let load = world.water.get(&output).map_or(0, WaterLot::load);
                let _ = quantity;
                world
                    .water
                    .insert(output, WaterLot::new(quantity_now, load));
            }
            EventKind::Spoiled { inventory, .. }
                if self.state.inventories[&inventory].good == Good::Water =>
            {
                world.water.insert(inventory, WaterLot::new(0, 0));
            }
            _ => {}
        }
    }
}
pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.health else {
        if game_health::clinicals(&state.components).next().is_some()
            || game_health::jobs(&state.components).next().is_some()
        {
            return Err(Error::InvalidSave(
                "health components require health runtime".into(),
            ));
        }
        return Ok(());
    };
    state
        .physical
        .as_ref()
        .ok_or_else(|| Error::InvalidSave("health requires physical bodies".into()))?;
    let locations = transport::resolved_locations_for(state)?;
    let people = state
        .people
        .iter()
        .map(|person| {
            let body = state.components.body(person.id).ok_or_else(|| {
                Error::InvalidSave("health requires a body for each person".into())
            })?;
            let activity = state.components.activity(person.id).ok_or_else(|| {
                Error::InvalidSave("health requires an activity for each person".into())
            })?;
            let health_work = match *activity {
                Activity::HealthWork { kind, completes } => {
                    Some(HealthWorkLink { kind, completes })
                }
                _ => None,
            };
            Ok((
                person.id,
                PersonCatalog {
                    alive: body.vitals.alive,
                    location: locations[&person.id].clone(),
                    health_work,
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let areas = state.areas.iter().map(|area| area.id).collect();
    let inventories = state
        .inventories
        .iter()
        .map(|(id, stock)| {
            (
                *id,
                InventoryCatalog {
                    good: stock.good,
                    quantity: stock.quantity,
                },
            )
        })
        .collect();
    game_health::validate_runtime(
        world,
        &state.components,
        ValidationCatalog {
            now: state.time,
            people: &people,
            areas: &areas,
            inventories: &inventories,
        },
    )
}
pub(crate) type PendingHealth = Option<game_health::PreparedInterval>;

impl Simulation {
    pub fn initialize_health(&mut self, rules: Rules) -> Result<(), Error> {
        validate_rules(&rules)?;
        let world = self.state.health.get_or_insert_with(Runtime::default);
        world.rules = rules;
        for p in &self.state.people {
            if self.state.components.clinical(p.id).is_none() {
                self.state
                    .components
                    .insert_clinical(p.id, Clinical::default())?;
            }
        }
        Ok(())
    }
    pub fn health_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::skills_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "health-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.initialize_health(Rules::fixture())?;
        Ok(sim)
    }
    fn health_area(&self, actor: PersonId, agency: bool) -> Result<AreaId, Error> {
        self.person_index.get(&actor).ok_or(Error::Unavailable)?;
        let body = self
            .state
            .physical
            .as_ref()
            .and_then(|_| self.state.components.body(actor))
            .ok_or(Error::Unavailable)?;
        if !body.vitals.alive
            || self.in_combat(actor)
            || (agency
                && (body.vitals.health == 0
                    || self.combat_restricts(actor)
                    || !self.independent(actor)))
        {
            return Err(Error::Unavailable);
        }
        match self.resolved_location(actor) {
            Location::AtArea(area) => Ok(area),
            Location::StoppedOnRoad { from, .. } => Ok(from),
            _ => Err(Error::Unavailable),
        }
    }
    fn medical_level(&self, actor: PersonId) -> u16 {
        self.own_skill_level(actor, Skill::Medicine)
            .max(self.borrowed_skill_level(actor, Skill::Medicine))
    }
    fn health_remembered_source_here(&self, actor: PersonId, stock: &StockKnowledge) -> bool {
        self.health_stock(actor, stock.inventory, stock.good, Quantity(0))
    }
    fn health_stock(
        &self,
        actor: PersonId,
        id: InventoryId,
        good: Good,
        quantity: Quantity,
    ) -> bool {
        self.state.inventories.get(&id).is_some_and(|stock| {
            stock.good == good
                && self.has_stock_access(actor, stock)
                && self.health_area(actor, true).is_ok()
                && match stock.location {
                    InventoryLocation::Area(area) => {
                        self.resolved_location(actor) == Location::AtArea(area)
                            && self.health_tactical_place(actor).is_none()
                    }
                    InventoryLocation::Carried(holder) => {
                        self.transport_same_place(actor, holder)
                            && self.health_tactical_clear(
                                self.health_tactical_place(actor),
                                self.health_tactical_place(holder),
                                true,
                            )
                    }
                }
                && (quantity.0 == 0 || self.unreserved_stock(id, actor) >= quantity.0)
        })
    }
    fn health_tactical_place(&self, actor: PersonId) -> Option<(EncounterId, PersonId)> {
        let mut anchor = actor;
        while let Location::Carried { carrier } = *self
            .state
            .components
            .location(anchor)
            .expect("location exists")
        {
            anchor = carrier;
        }
        match *self.state.components.activity(anchor).unwrap() {
            Activity::Combat { encounter } => Some((encounter, anchor)),
            _ => None,
        }
    }
    fn health_tactical_clear(
        &self,
        a: Option<(EncounterId, PersonId)>,
        b: Option<(EncounterId, PersonId)>,
        contact: bool,
    ) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some((scene_a, actor_a)), Some((scene_b, actor_b))) if scene_a == scene_b => {
                if actor_a == actor_b {
                    return true;
                }
                let world = self.state.combat.as_ref().unwrap();
                let scene = &world.encounters[&scene_a];
                let (a, b) = (
                    scene.fighters[&actor_a].position,
                    scene.fighters[&actor_b].position,
                );
                world
                    .map(scene.map)
                    .expect("clinical encounter map")
                    .clear(a, b)
                    && (!contact
                        || game_conflict::distance(a, b) <= u128::from(world.rules.reach).pow(2))
            }
            _ => false,
        }
    }
    fn consume_health_stock(&mut self, actor: PersonId, source: InventoryId, quantity: Quantity) {
        self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
        self.emit(EventKind::Consumed {
            actor,
            source,
            quantity,
        });
    }
    fn water_dose(&self, source: InventoryId, quantity: Quantity) -> u128 {
        let lot = self
            .state
            .health
            .as_ref()
            .and_then(|w| w.water.get(&source))
            .cloned()
            .unwrap_or_else(|| WaterLot::new(self.state.inventories[&source].quantity.0, 0));
        game_health::consume_water(&lot, quantity.0)
            .expect("validated physical water quantity")
            .consumed_load
    }
    fn received_water(
        &mut self,
        patient: PersonId,
        source: InventoryId,
        quantity: Quantity,
        dose: u128,
    ) -> Result<(), Error> {
        let w = self.state.health.as_ref().unwrap();
        let state = self
            .state
            .components
            .clinical(patient)
            .cloned()
            .unwrap_or_default();
        let state = game_health::receive_water(
            &state,
            &w.rules,
            self.health_physiology(patient),
            self.state.time,
            source,
            quantity,
            dose,
        )?;
        self.state.components.insert_clinical(patient, state)?;
        Ok(())
    }
    fn preflight_water_received(
        &self,
        patient: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        let _ = (patient, self.water_dose(source, quantity));
        game_health::preflight_intake(
            &self.state.health.as_ref().ok_or(Error::Unavailable)?.rules,
            self.state.time,
        )
    }
    pub(crate) fn health_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        match &{
            self.state.health.as_ref()?;
            self.state.components.health_job(actor)?
        }
        .work
        {
            Work::Boil { tool, .. } | Work::Shelter { tool, .. } => Some(*tool),
            _ => None,
        }
    }
    pub(crate) fn health_patient_resting(&self, actor: PersonId) -> bool {
        self.state
            .health
            .as_ref()
            .is_some_and(|w| w.patient_busy(&self.state.components, actor))
    }
    pub(crate) fn health_work_exerting(&self, actor: PersonId) -> bool {
        self.state
            .health
            .as_ref()
            .and_then(|_| self.state.components.health_job(actor))
            .is_some_and(|job| {
                !matches!(job.work, Work::Drink { .. }) && self.health_job_can_progress(job)
            })
    }
    pub(crate) fn cancel_health_activity(&mut self, actor: PersonId) {
        if let Some(world) = &self.state.health {
            let change = world.prepare_cancel(&self.state.components, actor);
            let _ = self.apply_health_change(change);
        }
    }
    fn health_actor_facts(&self, actor: PersonId) -> game_health::ActorFacts {
        if !self.person_index.contains_key(&actor) {
            return game_health::ActorFacts {
                actor,
                now: self.state.time,
                area: None,
                present: false,
                capable: false,
                at_area: false,
                carried: false,
                tactical: false,
            };
        }
        let area = self.health_area(actor, false).ok();
        game_health::ActorFacts {
            actor,
            now: self.state.time,
            area,
            present: area.is_some(),
            capable: self.health_area(actor, true).is_ok(),
            at_area: matches!(self.resolved_location(actor), Location::AtArea(_)),
            carried: self.person_index.get(&actor).is_some_and(|index| {
                matches!(
                    *self
                        .state
                        .components
                        .location(self.state.people[*index].id)
                        .expect("location exists"),
                    Location::Carried { .. }
                )
            }),
            tactical: self.person_index.contains_key(&actor)
                && self.health_tactical_place(actor).is_some(),
        }
    }
    fn health_stock_facts(
        &self,
        actor: PersonId,
        id: InventoryId,
    ) -> Result<game_health::StockFacts, Error> {
        let stock = self.state.inventories.get(&id).ok_or(Error::Unavailable)?;
        Ok(game_health::StockFacts {
            id,
            good: stock.good,
            quantity: stock.quantity,
            available: Quantity(self.unreserved_stock(id, actor)),
            usable: self.health_stock(actor, id, stock.good, Quantity(0)),
            tool_available: self.tool_available_for(actor, id),
        })
    }
    fn health_patient_facts(
        &self,
        giver: PersonId,
        patient: PersonId,
    ) -> game_health::PatientFacts {
        let present = self.health_area(patient, false).is_ok();
        let physiology = if self.person_index.contains_key(&patient) {
            self.health_physiology(patient)
        } else {
            game_health::clinical_physiology(&self.state.health.as_ref().unwrap().rules, 1_000, 0)
                .unwrap()
        };
        game_health::PatientFacts {
            id: patient,
            present,
            contact: present
                && self.person_index.contains_key(&giver)
                && self.can_observe_health(giver, patient),
            resting: self
                .state
                .physical
                .as_ref()
                .and_then(|_| self.state.components.activity(patient))
                .is_some_and(|activity| *activity == Activity::Rest),
            physiology,
        }
    }
    fn health_leaving_group(&self, actor: PersonId) -> Vec<PersonId> {
        let mut carried: BTreeMap<PersonId, Vec<PersonId>> = BTreeMap::new();
        if let Some(world) = &self.state.transport {
            for group in world.groups.values() {
                carried
                    .entry(group.anchor)
                    .or_default()
                    .push(group.passenger);
            }
        }
        let mut result = Vec::new();
        let mut pending = vec![actor];
        while let Some(actor) = pending.pop() {
            result.push(actor);
            if let Some(passengers) = carried.get(&actor) {
                pending.extend(passengers);
            }
        }
        result
    }
    pub(crate) fn leave_shelter_on_movement(&mut self, actor: PersonId) {
        if let Some(world) = &self.state.health {
            let change =
                world.prepare_leave(&self.state.components, &self.health_leaving_group(actor));
            let _ = self.apply_health_change(change);
        }
    }
    pub(crate) fn health_before_putdown(&self, passenger: PersonId) -> Option<ShelterId> {
        self.effective_shelter(passenger).map(|shelter| shelter.id)
    }
    pub(crate) fn health_after_putdown(&mut self, passenger: PersonId, shelter: Option<ShelterId>) {
        let location = self.resolved_location(passenger);
        let alive = self
            .state
            .physical
            .as_ref()
            .is_some_and(|_| self.state.components.body(passenger).unwrap().vitals.alive);
        let Some(world) = &mut self.state.health else {
            return;
        };
        world.occupancy.remove(&passenger);
        if let Some(shelter) = shelter.filter(|id| {
            alive
                && world
                    .shelters
                    .get(id)
                    .is_some_and(|s| location == Location::AtArea(s.area))
        }) {
            world.occupancy.insert(passenger, shelter);
        }
        self.observe_health();
    }
    pub(crate) fn health_command(
        &mut self,
        actor: PersonId,
        action: &HealthAction,
    ) -> Result<(), Error> {
        use game_health::ActionFacts;
        if self.state.health.is_none() || !self.person_index.contains_key(&actor) {
            return Err(Error::Unavailable);
        }
        let facts = match *action {
            HealthAction::Drink { source } => ActionFacts::Drink {
                water: self.health_stock_facts(actor, source)?,
                physiology: self.health_physiology(actor),
            },
            HealthAction::Boil { source, fuel, tool } => ActionFacts::Boil {
                water: self.health_stock_facts(actor, source)?,
                fuel: self.health_stock_facts(actor, fuel)?,
                tool: self.health_stock_facts(actor, tool)?,
            },
            HealthAction::BuildShelter { timber, tool, .. } => ActionFacts::ShelterBuild {
                timber: self.health_stock_facts(actor, timber)?,
                tool: self.health_stock_facts(actor, tool)?,
            },
            HealthAction::ShelterAccess { person, .. } => ActionFacts::ShelterAccess {
                recipient_exists: self.person_index.contains_key(&person),
            },
            HealthAction::EnterShelter { .. } | HealthAction::LeaveShelter => {
                ActionFacts::ShelterOccupancy {
                    leaving: self.health_leaving_group(actor),
                }
            }
            HealthAction::RequestTreatment { caregiver } => {
                let giver = self.health_actor_facts(caregiver);
                let supplies = self
                    .willing_health_supplies(caregiver, actor)
                    .map(|(herbs, water)| {
                        Ok((
                            self.health_stock_facts(caregiver, herbs)?,
                            self.health_stock_facts(caregiver, water)?,
                        ))
                    })
                    .transpose()?;
                ActionFacts::RequestCare {
                    giver,
                    patient: self.health_patient_facts(caregiver, actor),
                    medicine: if giver.capable {
                        self.medical_level(caregiver)
                    } else {
                        0
                    },
                    supplies,
                }
            }
            HealthAction::OfferTreatment {
                patient,
                herbs,
                water,
            } => ActionFacts::OfferCare {
                patient: self.health_patient_facts(actor, patient),
                medicine: self.medical_level(actor),
                herbs: self.health_stock_facts(actor, herbs)?,
                water: self.health_stock_facts(actor, water)?,
            },
            HealthAction::AcceptTreatment { caregiver } => ActionFacts::AcceptCare {
                giver_present: self.health_area(caregiver, true).is_ok(),
                contact: self.person_index.contains_key(&caregiver)
                    && self.can_observe_health(actor, caregiver),
            },
            HealthAction::RefuseTreatment { .. } => ActionFacts::RefuseCare,
            HealthAction::Treat {
                patient,
                herbs,
                water,
            } => ActionFacts::Treat {
                patient: self.health_patient_facts(actor, patient),
                medicine: self.medical_level(actor),
                herbs: self.health_stock_facts(actor, herbs)?,
                water: self.health_stock_facts(actor, water)?,
            },
        };
        let prepared = self.state.health.as_ref().unwrap().prepare_action(
            &self.state.components,
            self.health_actor_facts(actor),
            action,
            facts,
        )?;
        self.apply_health_change(prepared)?;
        self.observe_health();
        Ok(())
    }
    fn apply_health_change(&mut self, change: game_health::PreparedHealth) -> Result<(), Error> {
        use game_health::HealthEffect;
        let mut next_id = self.state.next_id;
        let mut consumed = BTreeMap::<InventoryId, u128>::new();
        let mut event_bound = 0usize;
        for effect in change.effects() {
            match effect {
                HealthEffect::ShelterBuilt { shelter, .. } => {
                    if shelter.0 != next_id {
                        return Err(Error::Unavailable);
                    }
                    next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
                }
                HealthEffect::Consume {
                    source, quantity, ..
                } => *consumed.entry(*source).or_default() += u128::from(quantity.0),
                _ => {}
            }
            event_bound = event_bound
                .checked_add(match effect {
                    HealthEffect::Interrupt { actor } => {
                        if self
                            .state
                            .physical
                            .as_ref()
                            .and_then(|_| self.state.components.activity(*actor))
                            .is_some_and(|activity| matches!(activity, Activity::HealthWork { .. }))
                        {
                            0
                        } else {
                            128
                        }
                    }
                    HealthEffect::ClearWork { .. }
                    | HealthEffect::Start { .. }
                    | HealthEffect::Rest { .. } => 0,
                    _ => 1,
                })
                .ok_or(Error::Overflow)?;
        }
        if consumed.iter().any(|(id, q)| {
            self.state
                .inventories
                .get(id)
                .is_none_or(|s| u128::from(s.quantity.0) < *q)
        }) {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(event_bound)?;
        let effects = change.commit(
            self.state.health.as_mut().ok_or(Error::Unavailable)?,
            &mut self.state.components,
        )?;
        self.state.next_id = next_id;
        for effect in effects {
            match effect {
                HealthEffect::Interrupt { actor } => {
                    self.interrupt_actor_activity_except_health(actor)
                }
                HealthEffect::ClearWork { actor, completes } => {
                    let activity = self.state.components.activity_mut(actor).unwrap();
                    if matches!(*activity,Activity::HealthWork {completes:at,..} if at==completes) {
                        *activity = Activity::Idle;
                    }
                }
                HealthEffect::Start {
                    actor,
                    kind,
                    completes,
                } => {
                    *self.state.components.activity_mut(actor).unwrap() =
                        Activity::HealthWork { kind, completes };
                }
                HealthEffect::Rest { actor } => {
                    *self.state.components.activity_mut(actor).unwrap() = Activity::Rest;
                }
                HealthEffect::Consume {
                    actor,
                    source,
                    quantity,
                } => self.consume_health_stock(actor, source, quantity),
                HealthEffect::Drank {
                    actor,
                    source,
                    quantity,
                } => {
                    self.emit(EventKind::Drank {
                        actor,
                        source,
                        quantity,
                    });
                }
                HealthEffect::Purified { actor, source } => {
                    self.emit(EventKind::WaterPurified { actor, source });
                }
                HealthEffect::ShelterBuilt { shelter, builder } => {
                    self.emit(EventKind::ShelterBuilt { shelter, builder });
                }
                HealthEffect::ShelterAccess {
                    shelter,
                    actor,
                    person,
                    allowed,
                } => {
                    self.emit(EventKind::ShelterAccessChanged {
                        shelter,
                        actor,
                        person,
                        allowed,
                    });
                }
                HealthEffect::ShelterEntered { shelter, actor } => {
                    self.emit(EventKind::ShelterEntered { shelter, actor });
                }
                HealthEffect::ShelterLeft { shelter, actor } => {
                    self.emit(EventKind::ShelterLeft { shelter, actor });
                }
                HealthEffect::Offered { caregiver, patient } => {
                    self.emit(EventKind::TreatmentOffered { caregiver, patient });
                }
                HealthEffect::Accepted { caregiver, patient } => {
                    self.emit(EventKind::TreatmentAccepted { caregiver, patient });
                }
                HealthEffect::Refused { caregiver, patient } => {
                    self.emit(EventKind::TreatmentRefused { caregiver, patient });
                }
                HealthEffect::Treated { caregiver, patient } => {
                    self.emit(EventKind::TreatmentCompleted { caregiver, patient });
                }
            }
        }
        Ok(())
    }
}

impl Simulation {
    fn effective_shelter(&self, actor: PersonId) -> Option<&Shelter> {
        let w = self.state.health.as_ref()?;
        if let Location::Carried { carrier } = *self
            .state
            .components
            .location(actor)
            .expect("location exists")
        {
            return self
                .state
                .components
                .body(actor)?
                .vitals
                .alive
                .then(|| self.effective_shelter(carrier))
                .flatten();
        }
        let shelter = w.shelters.get(w.occupancy.get(&actor)?)?;
        let body = self.state.components.body(actor)?;
        if !body.vitals.alive
            || self.resolved_location(actor) != Location::AtArea(shelter.area)
            || matches!(
                *self.state.components.activity(actor)?,
                Activity::Combat { .. }
                    | Activity::Cultivate { .. }
                    | Activity::Gather { .. }
                    | Activity::Construct { .. }
                    | Activity::Play { .. }
            )
        {
            None
        } else {
            Some(shelter)
        }
    }
    #[cfg(test)]
    fn in_health_contact(&self, a: PersonId, b: PersonId) -> bool {
        if a == b
            || !self.transport_same_place(a, b)
            || !self.health_tactical_clear(
                self.health_tactical_place(a),
                self.health_tactical_place(b),
                true,
            )
        {
            return false;
        }
        if matches!(*self.state.components.location(a).expect("location exists"), Location::Carried { carrier } if carrier == b)
            || matches!(*self.state.components.location(b).expect("location exists"), Location::Carried { carrier } if carrier == a)
        {
            return true;
        }
        let sa = self.effective_shelter(a).map(|s| s.id);
        let sb = self.effective_shelter(b).map(|s| s.id);
        if sa.is_some() && sa == sb {
            return true;
        }
        if self.state.components.health_jobs().any(|(_,job)|matches!(job.work,Work::Treat {patient,..} if job.actor==a&&patient==b||job.actor==b&&patient==a)){return true;}
        if sa != sb {
            return false;
        }
        for (giver, receiver) in [(a, b), (b, a)] {
            match *self.state.components.activity(giver).unwrap() {
                Activity::Care { child, .. } | Activity::Comfort { child, .. }
                    if child == receiver =>
                {
                    return true;
                }
                Activity::Tell { recipient, .. } | Activity::RealmTalk { recipient, .. }
                    if recipient == receiver =>
                {
                    return true;
                }
                Activity::ArmyTalk { listener, .. } if listener == receiver => return true,
                _ => {}
            }
        }
        self.state.missions.as_ref().is_some_and(|w| {
            w.talks.values().any(|talk| {
                talk.speaker == a && talk.listener == b || talk.speaker == b && talk.listener == a
            })
        })
    }
    fn health_exposure(&self, actor: PersonId) -> ExposureInput {
        ExposureInput {
            temperature_c: self.local_weather_for_actor(actor).map(|w| w.temperature),
            insulation: self.effective_shelter(actor).map_or(0, |s| s.insulation),
            // Body contributions use weather/shelter, never contact doses.
            infectious_contacts: [0; 2],
        }
    }
    fn health_locations(&self) -> BTreeMap<PersonId, HealthCohort> {
        let resolved =
            transport::resolved_locations_for(&self.state).expect("valid physical locations");
        let mut shelters = BTreeMap::new();
        let mut tactical = BTreeMap::new();
        for person in &self.state.people {
            let mut current = person.id;
            let mut chain = Vec::new();
            let mut shelter = loop {
                if let Some(shelter) = shelters.get(&current) {
                    break *shelter;
                }
                match *self
                    .state
                    .components
                    .location(current)
                    .expect("location exists")
                {
                    Location::Carried { carrier } => {
                        chain.push(current);
                        current = carrier;
                    }
                    _ => {
                        let shelter = self.effective_shelter(current).map(|s| s.id);
                        shelters.insert(current, shelter);
                        tactical.insert(current, self.health_tactical_place(current));
                        break shelter;
                    }
                }
            };
            for carried in chain.into_iter().rev() {
                if !self.state.components.body(carried).unwrap().vitals.alive {
                    shelter = None;
                }
                shelters.insert(carried, shelter);
                tactical.insert(carried, tactical[&current]);
            }
        }
        self.state
            .people
            .iter()
            .filter_map(|person| {
                let position =
                    transport::location_position(&resolved[&person.id], self.state.time).ok();
                position.map(|position| {
                    (
                        person.id,
                        (position.key(), shelters[&person.id], tactical[&person.id]),
                    )
                })
            })
            .collect()
    }
    /// Derived from one immutable opening state. Shared shelter contacts are
    /// aggregated, while actual interactions outside them are deduplicated.
    fn indexed_health_exposures(&self) -> BTreeMap<PersonId, ExposureInput> {
        let world = self.state.health.as_ref().unwrap();
        let locations = self.health_locations();
        let infectious: BTreeMap<_, _> = self
            .state
            .people
            .iter()
            .map(|person| {
                let state = self.state.components.clinical(person.id);
                let flags = [IllnessKind::Enteric, IllnessKind::Respiratory].map(|kind| {
                    u64::from(
                        state
                            .and_then(|s| s.illnesses.get(&kind))
                            .is_some_and(|i| i.symptomatic && i.recovers > self.state.time),
                    )
                });
                (person.id, flags)
            })
            .collect();
        let mut shared: BTreeMap<ShelterId, [u64; 2]> = BTreeMap::new();
        for (actor, (_, shelter, _)) in &locations {
            if let Some(shelter) = shelter {
                let count = shared.entry(*shelter).or_default();
                for (total, flag) in count.iter_mut().zip(infectious[actor]) {
                    *total += flag;
                }
            }
        }
        let mut exposures: BTreeMap<_, _> = self
            .state
            .people
            .iter()
            .map(|person| {
                let mut exposure = ExposureInput {
                    temperature_c: self
                        .local_weather_for_actor(person.id)
                        .map(|w| w.temperature),
                    insulation: locations
                        .get(&person.id)
                        .and_then(|(_, shelter, _)| *shelter)
                        .map_or(0, |id| world.shelters[&id].insulation),
                    infectious_contacts: [0; 2],
                };
                if let Some((_, Some(shelter), _)) = locations.get(&person.id) {
                    exposure.infectious_contacts = shared[shelter];
                    for (count, own) in exposure
                        .infectious_contacts
                        .iter_mut()
                        .zip(infectious[&person.id])
                    {
                        *count -= own;
                    }
                }
                (person.id, exposure)
            })
            .collect();
        let mut pairs = BTreeSet::new();
        let mut add_pair = |a: PersonId, b: PersonId, treatment: bool| {
            if a == b {
                return;
            }
            let (Some((area_a, shelter_a, tactical_a)), Some((area_b, shelter_b, tactical_b))) =
                (locations.get(&a), locations.get(&b))
            else {
                return;
            };
            if area_a != area_b
                || !self.health_tactical_clear(*tactical_a, *tactical_b, true)
                || (!treatment && shelter_a != shelter_b)
                || (shelter_a.is_some() && shelter_a == shelter_b)
            {
                return;
            }
            pairs.insert(if a < b { (a, b) } else { (b, a) });
        };
        for (_, job) in self.state.components.health_jobs() {
            if let Work::Treat { patient, .. } = job.work {
                add_pair(job.actor, patient, true);
            }
        }
        for (actor, activity) in self.state.components.activities() {
            match *activity {
                Activity::Care { child, .. } | Activity::Comfort { child, .. } => {
                    add_pair(actor, child, false)
                }
                Activity::Tell { recipient, .. } | Activity::RealmTalk { recipient, .. } => {
                    add_pair(actor, recipient, false)
                }
                Activity::ArmyTalk { listener, .. } => add_pair(actor, listener, false),
                _ => {}
            }
        }
        for person in &self.state.people {
            if let Location::Carried { carrier } = *self
                .state
                .components
                .location(person.id)
                .expect("location exists")
            {
                add_pair(person.id, carrier, true);
            }
        }
        if let Some(missions) = &self.state.missions {
            for talk in missions.talks.values() {
                add_pair(talk.speaker, talk.listener, false);
            }
        }
        for (a, b) in pairs {
            for (receiver, source) in [(a, b), (b, a)] {
                let exposure = exposures.get_mut(&receiver).unwrap();
                for (count, flag) in exposure
                    .infectious_contacts
                    .iter_mut()
                    .zip(infectious[&source])
                {
                    *count += flag;
                }
            }
        }
        exposures
    }
    fn clinical_input<'a>(
        &'a self,
        actor: PersonId,
        state: &'a Clinical,
        next: SimTime,
        exposure: ExposureInput,
    ) -> ClinicalInput<'a> {
        ClinicalInput {
            rules: &self.state.health.as_ref().unwrap().rules,
            state,
            now: self.state.time,
            next,
            alive: self.state.components.body(actor).unwrap().vitals.alive,
            thirst_enabled: !self.state.people[self.person_index[&actor]].hunger_exempt,
            resting: *self.state.components.activity(actor).unwrap() == Activity::Rest,
            exposure,
            physiology: self.health_physiology(actor),
        }
    }
    fn health_physiology(&self, actor: PersonId) -> ClinicalPhysiology {
        clinical_physiology(
            &self.state.health.as_ref().unwrap().rules,
            self.genetic_metabolism_milli(actor),
            self.genetic_cold_tolerance_centi(actor),
        )
        .expect("validated individual metabolism")
    }
    pub(crate) fn observed_exposure_urgency(&self, actor: PersonId) -> i64 {
        self.state
            .health
            .as_ref()
            .and_then(|world| {
                self.state.components.clinical(actor).map(|clinical| {
                    game_health::exposure_urgency(clinical.cold, world.rules.cold_harm)
                })
            })
            .unwrap_or(0)
    }
    pub(crate) fn observed_hydration_urgency(&self, target: PersonId) -> i64 {
        self.state
            .health
            .as_ref()
            .and_then(|_| self.state.components.clinical(target))
            .map_or(0, |state| {
                game_health::hydration_urgency(state, self.health_physiology(target))
            })
    }
    pub(crate) fn health_body_delta(
        &self,
        actor: PersonId,
        elapsed: u64,
    ) -> Result<HealthDelta, Error> {
        let Some(_) = &self.state.health else {
            return Ok(HealthDelta::default());
        };
        let empty = Clinical::default();
        let state = self.state.components.clinical(actor).unwrap_or(&empty);
        let next = SimTime(
            self.state
                .time
                .0
                .checked_add(elapsed)
                .ok_or(Error::Overflow)?,
        );
        game_health::clinical_contribution(self.clinical_input(
            actor,
            state,
            next,
            self.health_exposure(actor),
        ))
    }
    pub(crate) fn evaluate_health(&self, next: SimTime) -> Result<PendingHealth, Error> {
        let Some(runtime) = &self.state.health else {
            return Ok(None);
        };
        let empty = Clinical::default();
        let exposures = self.indexed_health_exposures();
        let mut clinical = Vec::new();
        for person in &self.state.people {
            if !self.state.components.body(person.id).unwrap().vitals.alive {
                continue;
            }
            let state = game_health::clinical(&self.state.components, person.id).unwrap_or(&empty);
            clinical.push((
                person.id,
                self.clinical_input(person.id, state, next, exposures[&person.id]),
            ));
        }
        let jobs: Vec<_> = game_health::jobs(&self.state.components)
            .map(|(actor, job)| {
                (
                    actor,
                    game_health::JobInput {
                        job,
                        can_progress: self.health_job_can_progress(job),
                        medical_level: self.medical_level(actor),
                    },
                )
            })
            .collect();
        runtime
            .prepare_interval(
                &self.state.components,
                self.state.time,
                next,
                &clinical,
                &jobs,
            )
            .map(Some)
    }
    pub(crate) fn health_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        let Some(_) = &self.state.health else {
            return Ok(limit);
        };
        let mut boundary = limit;
        let exposures = self.indexed_health_exposures();
        for (_, job) in self.state.components.health_jobs() {
            if self.health_job_can_progress(job) && job.completes > self.state.time {
                boundary = boundary.min(job.completes);
            }
        }
        // Domain thresholds constrain the interval. The coordinator separately
        // finds combined body limits using game-physiology.
        for person in &self.state.people {
            let empty = Clinical::default();
            let state = self.state.components.clinical(person.id).unwrap_or(&empty);
            boundary = boundary.min(game_health::clinical_boundary(
                self.clinical_input(person.id, state, limit, exposures[&person.id]),
                limit,
            )?);
        }
        Ok(boundary)
    }
    pub(crate) fn commit_health(&mut self, pending: PendingHealth) -> Result<(), Error> {
        let Some(pending) = pending else {
            return Ok(());
        };
        let runtime = self.state.health.as_ref().ok_or(Error::Unavailable)?;
        let living: BTreeSet<_> = self
            .state
            .components
            .bodies()
            .filter(|(_, body)| body.vitals.alive)
            .map(|(actor, _)| actor)
            .collect();
        pending.commit(runtime, &mut self.state.components, |actor| {
            living.contains(&actor)
        })?;
        let deadlines: Vec<_> = game_health::jobs(&self.state.components)
            .map(|(actor, job)| (actor, job.completes))
            .collect();
        for (actor, completes) in deadlines {
            if let Activity::HealthWork { kind, .. } =
                *self.state.components.activity(actor).unwrap()
            {
                *self.state.components.activity_mut(actor).unwrap() =
                    Activity::HealthWork { kind, completes };
            }
        }
        Ok(())
    }
    pub(crate) fn reconcile_health(
        &mut self,
        closing_equipment: &BTreeMap<PersonId, bool>,
    ) -> Result<(), Error> {
        let Some(_) = &self.state.health else {
            return Ok(());
        };
        self.ensure_event_capacity(
            self.state.components.clinicals().count() * 5
                + self.state.components.health_jobs().count() * 5
                + 2,
        )?;
        let ids: Vec<_> = self
            .state
            .components
            .clinicals()
            .map(|(id, _)| id)
            .collect();
        for actor in ids {
            if !self.state.components.body(actor).unwrap().vitals.alive {
                self.cancel_health_activity(actor);
                continue;
            }
            let w = self.state.health.as_ref().unwrap();
            let transition = game_health::reconcile_clinical(
                self.state.components.clinical(actor).unwrap(),
                &w.rules,
                self.state.time,
                true,
            )?;
            self.state
                .components
                .insert_clinical(actor, transition.state)?;
            for event in transition.events {
                match event {
                    ClinicalEvent::Started(kind) => {
                        self.emit(EventKind::IllnessStarted { actor, kind })
                    }
                    ClinicalEvent::Recovered(kind) => {
                        self.emit(EventKind::IllnessRecovered { actor, kind })
                    }
                }
            }
        }
        let due = self
            .state
            .components
            .health_jobs()
            .map(|(_, job)| job)
            .filter(|job| job.completes <= self.state.time)
            .cloned()
            .collect::<Vec<_>>();
        for job in due {
            let valid = self.health_area(job.actor, true).ok() == Some(job.area)
                && (!matches!(job.work, Work::Boil { .. } | Work::Shelter { .. })
                    || closing_equipment.get(&job.actor).copied().unwrap_or(false))
                && matches!(*self.state.components.activity(job.actor).unwrap(),Activity::HealthWork {completes,..} if completes==job.completes);
            let facts = match job.work {
                Work::Drink { source, .. } => game_health::FinishFacts::Drink {
                    water: self.health_stock_facts(job.actor, source)?,
                    physiology: self.health_physiology(job.actor),
                },
                Work::Boil { source, .. } => game_health::FinishFacts::Boil {
                    water: self.health_stock_facts(job.actor, source)?,
                },
                Work::Shelter { .. } => game_health::FinishFacts::Shelter {
                    id: ShelterId(self.state.next_id),
                },
                Work::Treat { patient, .. } => game_health::FinishFacts::Treat {
                    patient: self.health_patient_facts(job.actor, patient),
                },
            };
            let change = self.state.health.as_ref().unwrap().prepare_finish(
                &self.state.components,
                job.actor,
                self.state.time,
                valid,
                facts,
            )?;
            self.apply_health_change(change)?;
        }
        let expired = self
            .state
            .health
            .as_ref()
            .unwrap()
            .prepare_expire_offers(self.state.time);
        self.apply_health_change(expired)?;
        self.observe_health();
        Ok(())
    }
}

impl Simulation {
    #[allow(clippy::too_many_arguments)]
    pub fn seed_shelter(
        &mut self,
        area: AreaId,
        custodian: PersonId,
        access: BTreeSet<PersonId>,
        capacity: u32,
        insulation: u16,
        source: InventoryId,
        quantity: Quantity,
        built_at: SimTime,
    ) -> Result<ShelterId, Error> {
        let r = &self.state.health.as_ref().ok_or(Error::Unavailable)?.rules;
        let needed = shelter_cost(r, capacity, insulation)?;
        if quantity < needed
            || built_at > self.state.time
            || !self.person_index.contains_key(&custodian)
            || access
                .iter()
                .any(|person| !self.person_index.contains_key(person))
            || !self.state.inventories.get(&source).is_some_and(|stock| {
                stock.good == Good::Timber
                    && stock.location == InventoryLocation::Area(area)
                    && stock.quantity >= quantity
            })
        {
            return Err(Error::InvalidConfiguration(
                "invalid initial shelter history".into(),
            ));
        }
        let labor_ms = r
            .shelter_time_per_place
            .0
            .checked_mul(u64::from(capacity))
            .ok_or(Error::Overflow)?;
        self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
        self.ensure_event_capacity(2)?;
        let id = ShelterId(allocate(&mut self.state)?);
        self.consume_health_stock(custodian, source, quantity);
        let mut access = access;
        access.insert(custodian);
        self.state.health.as_mut().unwrap().shelters.insert(
            id,
            Shelter {
                id,
                area,
                custodian,
                access,
                capacity,
                insulation,
                built_at,
                timber_used: quantity,
                labor_ms,
            },
        );
        self.emit(EventKind::ShelterBuilt {
            shelter: id,
            builder: custodian,
        });
        Ok(id)
    }
    pub fn set_initial_shelter_occupancy(
        &mut self,
        person: PersonId,
        shelter: ShelterId,
    ) -> Result<(), Error> {
        let w = self.state.health.as_ref().ok_or(Error::Unavailable)?;
        let s = w.shelters.get(&shelter).ok_or(Error::Unavailable)?;
        if *self
            .state
            .components
            .location(
                self.state.people[*self.person_index.get(&person).ok_or(Error::Unavailable)?].id,
            )
            .expect("location exists")
            != Location::AtArea(s.area)
            || !s.access.contains(&person)
            || w.occupancy.contains_key(&person)
            || w.occupancy.values().filter(|id| **id == shelter).count() >= s.capacity as usize
        {
            return Err(Error::Unavailable);
        }
        self.state
            .health
            .as_mut()
            .unwrap()
            .occupancy
            .insert(person, shelter);
        Ok(())
    }
    pub(crate) fn dependent_signal_visible(&self, observer: PersonId, subject: PersonId) -> bool {
        self.transport_same_place(observer, subject)
            && self.health_tactical_clear(
                self.health_tactical_place(observer),
                self.health_tactical_place(subject),
                false,
            )
            && self.effective_shelter(observer).map(|shelter| shelter.id)
                == self.effective_shelter(subject).map(|shelter| shelter.id)
    }
    fn can_observe_health(&self, observer: PersonId, subject: PersonId) -> bool {
        self.dependent_signal_visible(observer, subject) && !self.transport_is_moving(observer)
    }
    fn visible_symptoms(&self, person: PersonId) -> SymptomsView {
        let state = self
            .state
            .health
            .as_ref()
            .and_then(|_| self.state.components.clinical(person));
        let rules = &self.state.health.as_ref().unwrap().rules;
        let has = |kind| {
            state
                .and_then(|c| c.illnesses.get(&kind))
                .is_some_and(|i| i.symptomatic && i.recovers > self.state.time)
        };
        SymptomsView {
            person,
            fever: has(IllnessKind::Respiratory) || has(IllnessKind::Enteric),
            nausea: has(IllnessKind::Enteric),
            cough: has(IllnessKind::Respiratory),
            shivering: state.is_some_and(|c| c.cold >= rules.cold_harm / 2),
            wounded: self.state.components.body(person).unwrap().vitals.health
                < self.body_rules(person).max_health / 2,
            observed_at: self.state.time,
        }
    }
    pub(crate) fn observe_health(&mut self) {
        if self.state.health.is_none() {
            return;
        }
        let locations = self.health_locations();
        let mut cohorts: BTreeMap<HealthCohort, BTreeSet<PersonId>> = BTreeMap::new();
        for (actor, location) in &locations {
            cohorts.entry(*location).or_default().insert(*actor);
        }
        let symptoms: BTreeMap<_, _> = cohorts
            .iter()
            .map(|(cohort, actors)| {
                let visible: BTreeSet<_> = if cohort.2.is_some() {
                    cohorts
                        .iter()
                        .filter(|(other, _)| {
                            other.0 == cohort.0
                                && other.1 == cohort.1
                                && self.health_tactical_clear(cohort.2, other.2, false)
                        })
                        .flat_map(|(_, actors)| actors.iter().copied())
                        .collect()
                } else {
                    actors.clone()
                };
                (
                    *cohort,
                    visible
                        .iter()
                        .take(64)
                        .map(|actor| (*actor, self.visible_symptoms(*actor)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let world = self.state.health.as_ref().unwrap();
        let mut area_shelters: BTreeMap<AreaId, Vec<ShelterId>> = BTreeMap::new();
        for shelter in world.shelters.values() {
            area_shelters
                .entry(shelter.area)
                .or_default()
                .push(shelter.id);
        }
        let mut occupants: BTreeMap<(ShelterId, HealthCohort), BTreeSet<PersonId>> =
            BTreeMap::new();
        for (actor, location) in &locations {
            let shelter = if matches!(
                *self
                    .state
                    .components
                    .location(*actor)
                    .expect("location exists"),
                Location::Carried { .. }
            ) {
                location.1
            } else {
                world.occupancy.get(actor).copied()
            };
            if let Some(shelter) = shelter {
                occupants
                    .entry((shelter, *location))
                    .or_default()
                    .insert(*actor);
            }
        }
        let observers: Vec<_> = self
            .state
            .people
            .iter()
            .filter(|person| {
                self.state.components.body(person.id).unwrap().vitals.alive
                    && !self.transport_is_moving(person.id)
                    && locations.contains_key(&person.id)
            })
            .map(|person| person.id)
            .collect();
        for observer in observers {
            let area = match locations[&observer].0 {
                game_transport::PositionKey::Area(area) if locations[&observer].2.is_none() => {
                    Some(area)
                }
                game_transport::PositionKey::Area(_) => None,
                game_transport::PositionKey::Road { .. } => None,
            };
            let cohort = locations[&observer];
            let people = &symptoms[&cohort];
            let shelters: Vec<_> = area
                .and_then(|area| area_shelters.get(&area))
                .into_iter()
                .flatten()
                .map(|id| {
                    let s = &self.state.health.as_ref().unwrap().shelters[id];
                    let known_occupants =
                        occupants.get(&(*id, cohort)).cloned().unwrap_or_default();
                    (
                        s.id,
                        ShelterView {
                            id: s.id,
                            area: s.area,
                            capacity: s.capacity,
                            insulation: s.insulation,
                            custodian: s.custodian,
                            accessible: s.access.contains(&observer),
                            known_occupants,
                            observed_at: self.state.time,
                        },
                    )
                })
                .collect();
            let knowledge = self
                .state
                .health
                .as_mut()
                .unwrap()
                .known
                .entry(observer)
                .or_default();
            for (person, symptoms) in people {
                knowledge.people.insert(*person, symptoms.clone());
            }
            for (id, s) in shelters {
                knowledge.shelters.insert(id, s);
            }
        }
    }
    pub(crate) fn health_view(&self, actor: PersonId) -> Option<HealthView> {
        let w = self.state.health.as_ref()?;
        let clinical = self
            .state
            .components
            .clinical(actor)
            .cloned()
            .unwrap_or_default();
        let known = w.known.get(&actor).cloned().unwrap_or_default();
        Some(HealthView {
            thirst: clinical.thirst,
            cold_exposure: clinical.cold,
            symptoms: self.visible_symptoms(actor),
            sheltered: self.effective_shelter(actor).map(|s| s.id),
            treatment_until: clinical.treatment.as_ref().map(|c| c.until),
            offers: w
                .offers
                .values()
                .filter(|o| o.caregiver == actor || o.patient == actor)
                .map(|o| TreatmentOfferView {
                    caregiver: o.caregiver,
                    patient: o.patient,
                    herbs: o.herbs,
                    water: o.water,
                    expires: o.expires,
                    accepted: o.accepted,
                })
                .collect(),
            observed_people: known
                .people
                .values()
                .filter(|p| p.person != actor)
                .cloned()
                .collect(),
            shelters: known.shelters.into_values().collect(),
            water: known.water.into_values().collect(),
        })
    }
    pub(crate) fn available_health_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(w) = &self.state.health else {
            return Vec::new();
        };
        let Ok(area) = self.health_area(actor, false) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        for offer in w.offers.values().filter(|o| {
            o.patient == actor
                && o.expires > self.state.time
                && self.health_area(o.caregiver, true).is_ok()
        }) {
            if !offer.accepted {
                actions.push(HealthAction::AcceptTreatment {
                    caregiver: offer.caregiver,
                });
            }
            actions.push(HealthAction::RefuseTreatment {
                caregiver: offer.caregiver,
            });
        }
        for person in w
            .known
            .get(&actor)
            .into_iter()
            .flat_map(|k| k.people.keys())
            .filter(|person| {
                **person != actor
                    && self.can_observe_health(actor, **person)
                    && self.health_area(**person, true).is_ok()
            })
        {
            actions.push(HealthAction::RequestTreatment { caregiver: *person });
        }
        if self.health_area(actor, true).is_ok() {
            let can_enter = self.resolved_location(actor) == Location::AtArea(area)
                && !matches!(
                    *self
                        .state
                        .components
                        .location(actor)
                        .expect("location exists"),
                    Location::Carried { .. }
                );
            let mind = self
                .state
                .decisions
                .as_ref()
                .and_then(|world| world.minds.get(&actor));
            if let Some(mind) = mind {
                for stock in mind.knowledge.values().filter(|stock| {
                    stock.good == Good::Water
                        && stock.accessible
                        && self.health_remembered_source_here(actor, stock)
                        && self.health_stock(
                            actor,
                            stock.inventory,
                            Good::Water,
                            w.rules.drink_quantity,
                        )
                }) {
                    actions.push(HealthAction::Drink {
                        source: stock.inventory,
                    });
                }
            }
            if let Some(planning) = self.health_planning_data(actor, None) {
                let input = planning.input(self.state.time);
                for stock in &planning.stocks {
                    let Some(action) = w.boiling_action(
                        &self.state.components,
                        actor,
                        stock.fact.inventory,
                        &input,
                    ) else {
                        continue;
                    };
                    let HealthAction::Boil { source, fuel, tool } = action else {
                        continue;
                    };
                    let facts = self.health_stock_facts(actor, source).and_then(|water| {
                        self.health_stock_facts(actor, fuel).and_then(|fuel| {
                            self.health_stock_facts(actor, tool)
                                .map(|tool| game_health::ActionFacts::Boil { water, fuel, tool })
                        })
                    });
                    if facts.is_ok_and(|facts| {
                        w.prepare_action(
                            &self.state.components,
                            self.health_actor_facts(actor),
                            &action,
                            facts,
                        )
                        .is_ok()
                    }) {
                        actions.push(action);
                    }
                }
            }
            for shelter in w
                .known
                .get(&actor)
                .into_iter()
                .flat_map(|k| k.shelters.values())
                .filter(|s| {
                    can_enter
                        && s.area == area
                        && s.accessible
                        && w.occupancy.get(&actor) != Some(&s.id)
                        && s.known_occupants.len() < s.capacity as usize
                })
            {
                actions.push(HealthAction::EnterShelter {
                    shelter: shelter.id,
                });
            }
            if can_enter && w.occupancy.contains_key(&actor) {
                actions.push(HealthAction::LeaveShelter);
            }
        }
        actions
            .into_iter()
            .map(|action| Command::Health { action })
            .collect()
    }
    fn remembered_health_stocks(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<game_health::RememberedStock> {
        self.state
            .decisions
            .as_ref()
            .and_then(|w| w.minds.get(&actor))
            .into_iter()
            .flat_map(|mind| mind.knowledge.values())
            .filter(|s| matches!(s.good, Good::Water | Good::Timber | Good::Tool | Good::Herb))
            .map(|fact| {
                let here = self.health_remembered_source_here(actor, fact);
                let mut observed = fact.clone();
                if here {
                    observed.quantity = Quantity(self.unreserved_stock_for_planning(
                        fact.inventory,
                        actor,
                        resources,
                    ));
                    observed.accessible = true;
                    observed.observed_at = self.state.time;
                    observed.learned_at = self.state.time;
                    observed.source = KnowledgeSource::Observed(actor);
                }
                game_health::RememberedStock {
                    fact: observed,
                    here,
                    held: self
                        .state
                        .inventories
                        .get(&fact.inventory)
                        .is_some_and(|stock| stock.location == InventoryLocation::Carried(actor)),
                }
            })
            .collect()
    }
    fn health_planning_data(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<HealthPlanningData> {
        let area = self.health_area(actor, true).ok()?;
        let mind = self.state.decisions.as_ref()?.minds.get(&actor)?;
        let traits = self
            .invocation_personality(actor)
            .unwrap_or_else(|| mind.personality.clone());
        Some(HealthPlanningData {
            area,
            stationary: self.resolved_location(actor) == Location::AtArea(area),
            values: game_health::IntakeValues {
                risk_tolerance: traits.risk_tolerance,
            },
            stocks: self
                .remembered_health_stocks(actor, resources)
                .into_iter()
                .filter(|stock| {
                    stock.fact.good != Good::Tool
                        || self.tool_available_for_planning(actor, stock.fact.inventory, resources)
                })
                .collect(),
        })
    }
    pub(crate) fn health_candidate(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let world = self.state.health.as_ref()?;
        let facts = self.health_planning_data(actor, resources)?;
        let input = facts.input(self.state.time);
        let need = game_health::NeedFacts {
            actor,
            drink_relief: self.health_physiology(actor).drink_relief,
            sheltered: self.effective_shelter(actor).is_some(),
            can_enter_shelter: facts.stationary
                && !matches!(
                    *self
                        .state
                        .components
                        .location(actor)
                        .expect("location exists"),
                    Location::Carried { .. }
                ),
        };
        match world.plan_need(&self.state.components, need, &input)? {
            game_health::HealthIntent::Action(action) => Some(Command::Health { action }),
            game_health::HealthIntent::Supply(supply) => {
                let delivery = game_logistics::Delivery::new(
                    game_logistics::Requirement {
                        good: supply.good,
                        quantity: supply.quantity,
                        source: supply.source,
                    },
                    game_logistics::Destination::Contact(actor),
                )
                .ok()?;
                let destination = game_logistics::KnownDestination {
                    destination: delivery.destination,
                    location: self.delivery_observed_location(actor),
                    observed_at: self.state.time,
                    accessible: true,
                    room: None,
                    direct_transfer: !supply.carried,
                };
                match self.plan_delivery_for_planning(
                    actor,
                    &delivery,
                    &destination,
                    supply.quantity,
                    &[],
                    resources,
                ) {
                    game_logistics::Step::Arrived { source, .. } => world
                        .consume_supply(
                            &self.state.components,
                            actor,
                            source,
                            &input,
                            supply.consumer,
                        )
                        .map(|action| Command::Health { action }),
                    step => self.delivery_command(actor, step),
                }
            }
        }
    }
    pub(crate) fn health_actor_busy(&self, actor: PersonId) -> bool {
        self.state.health.as_ref().is_some_and(|_| {
            self.state.components.health_job(actor).is_some() || self.health_patient_resting(actor)
        })
    }
    pub(crate) fn review_health(&mut self) -> Result<(), Error> {
        let Some(w) = &self.state.health else {
            return Ok(());
        };
        if w.last_review == self.state.time || !self.state.time.0.is_multiple_of(60_000) {
            return Ok(());
        }
        self.preflight_health(self.state.time)?;
        self.state.health.as_mut().unwrap().last_review = self.state.time;
        self.observe_health();
        let jobs: Vec<_> = self
            .state
            .components
            .health_jobs()
            .map(|(_, job)| job)
            .cloned()
            .collect();
        for job in jobs {
            if job.actor == self.state.ruler && !self.past_life_controls_actor(job.actor) {
                continue;
            }
            let body = self.state.components.body(job.actor).unwrap();
            let physical = self.body_rules(job.actor);
            if game_health::should_pause_work(
                &job.work,
                game_health::WorkNeeds {
                    hunger: body.vitals.hunger,
                    meal_relief: physical.meal_relief,
                    fatigue: body.vitals.fatigue,
                    fatigue_limit: physical.fatigue_limit,
                    health: body.vitals.health,
                    max_health: physical.max_health,
                },
            ) {
                self.interrupt_actor_activity(job.actor);
                let _ = self.debug_actor_command(job.actor, Command::Rest);
            }
        }
        let offers: Vec<_> = self
            .state
            .health
            .as_ref()
            .unwrap()
            .offers
            .values()
            .cloned()
            .collect();
        for offer in offers {
            if offer.patient == self.state.ruler && !self.past_life_controls_actor(offer.patient)
                || offer.accepted
                || self.health_area(offer.patient, false).is_err()
                || self.health_area(offer.caregiver, true).is_err()
                || !self.can_observe_health(offer.patient, offer.caregiver)
            {
                continue;
            }
            let Some(patient) = self.health_mind(offer.patient) else {
                continue;
            };
            let symptoms = self.visible_symptoms(offer.patient);
            let trust = patient
                .relationships
                .get(&offer.caregiver)
                .map_or(0, |r| r.trust);
            let action = game_health::offer_response(offer.caregiver, &symptoms, trust);
            let _ = self.health_command(offer.patient, &action);
        }
        let offers: Vec<_> = self
            .state
            .health
            .as_ref()
            .unwrap()
            .offers
            .values()
            .filter(|o| o.accepted && o.caregiver != self.state.ruler)
            .cloned()
            .collect();
        for offer in offers {
            let _ = self.health_command(
                offer.caregiver,
                &HealthAction::Treat {
                    patient: offer.patient,
                    herbs: offer.herbs,
                    water: offer.water,
                },
            );
        }
        let healers: Vec<_> = self
            .state
            .people
            .iter()
            .filter(|p| {
                p.id != self.state.ruler
                    && self.medical_level(p.id) > 0
                    && self.health_area(p.id, true).is_ok()
                    && !self.health_actor_busy(p.id)
            })
            .map(|p| p.id)
            .collect();
        for caregiver in healers {
            let mind = &self.state.decisions.as_ref().unwrap().minds[&caregiver];
            let body = self.state.components.body(caregiver).unwrap();
            let rules = &self.state.health.as_ref().unwrap().rules;
            let physical = self.body_rules(caregiver);
            let stocks = self.remembered_health_stocks(caregiver, None);
            let patients = self
                .state
                .health
                .as_ref()
                .unwrap()
                .known
                .get(&caregiver)
                .into_iter()
                .flat_map(|k| k.people.values())
                .map(|symptoms| game_health::PatientOpportunity {
                    symptoms: symptoms.clone(),
                    contact: self.can_observe_health(caregiver, symptoms.person),
                    offer_exists: self
                        .state
                        .health
                        .as_ref()
                        .unwrap()
                        .offers
                        .contains_key(&(caregiver, symptoms.person)),
                    busy: self.health_patient_resting(symptoms.person),
                })
                .collect::<Vec<_>>();
            let facts = game_health::WillingCare {
                capable: self.health_area(caregiver, true).is_ok(),
                medicine: self.medical_level(caregiver),
                busy: self.health_actor_busy(caregiver),
                hungry: body.vitals.hunger >= physical.meal_relief,
                fatigued: body.vitals.fatigue >= physical.fatigue_limit / 2,
                urgent_dependent: false,
                empathy: mind.personality.empathy,
                trust: 0,
                stocks: &stocks,
            };
            if let Some(action) = game_health::proactive_offer(caregiver, rules, facts, &patients) {
                let _ = self.health_command(caregiver, &action);
            }
        }
        Ok(())
    }
    pub(crate) fn health_allocation_bound(&self, next: SimTime) -> u64 {
        self.state
            .health
            .as_ref()
            .map_or(0, |w| w.allocation_bound(&self.state.components, next))
    }
    pub(crate) fn health_event_bound(&self) -> usize {
        self.state.health.as_ref().map_or(0, |w| {
            self.state.components.clinicals().count() * 8
                + self.state.components.health_jobs().count() * 8
                + w.offers.len() * 4
        })
    }
    pub(crate) fn preflight_health(&self, next: SimTime) -> Result<(), Error> {
        let Some(w) = &self.state.health else {
            return Ok(());
        };
        validate_rules(&w.rules)?;
        self.state
            .next_id
            .checked_add(self.health_allocation_bound(next))
            .ok_or(Error::Overflow)?;
        next.0
            .checked_add(w.rules.incubation.0)
            .and_then(|t| t.checked_add(w.rules.illness_duration.0))
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(self.health_event_bound())
    }
}

impl Simulation {
    pub fn seed_water_safety(
        &mut self,
        observer: PersonId,
        source: InventoryId,
    ) -> Result<(), Error> {
        if !self.person_index.contains_key(&observer)
            || !self
                .state
                .inventories
                .get(&source)
                .is_some_and(|stock| stock.good == Good::Water)
        {
            return Err(Error::Unavailable);
        }
        let quantity = self.state.inventories[&source].quantity.0;
        let world = self.state.health.as_mut().ok_or(Error::Unavailable)?;
        let safe = world.water.get(&source).is_none_or(|lot| lot.load() == 0);
        if !safe {
            return Err(Error::InvalidConfiguration(
                "authoring potable water with real contaminants".into(),
            ));
        }
        let _ = quantity;
        world.known.entry(observer).or_default().water.insert(
            source,
            WaterSafetyView {
                inventory: source,
                purified: true,
                suspected_unsafe: false,
                observed_at: self.state.time,
            },
        );
        Ok(())
    }
    pub(crate) fn preflight_water_care(
        &self,
        patient: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        if self.state.health.is_none() {
            return Ok(());
        }
        self.preflight_water_received(patient, source, quantity)
    }
    /// Called immediately before actual water consumption, while the lot still
    /// contains the exact fraction about to be delivered to the child.
    pub(crate) fn health_water_given(
        &mut self,
        patient: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        if self.state.health.is_none() {
            return Ok(());
        }
        self.preflight_water_received(patient, source, quantity)?;
        let dose = self.water_dose(source, quantity);
        self.received_water(patient, source, quantity, dose)
    }
}

impl Simulation {
    fn health_job_can_progress(&self, job: &Job) -> bool {
        let Some(physical) = &self.state.physical else {
            return false;
        };
        let Some(body) = self.state.components.body(job.actor) else {
            return false;
        };
        let patient = job
            .patient()
            .map(|id| self.health_patient_facts(job.actor, id));
        game_health::can_progress(
            job,
            game_health::ProgressFacts {
                area: self.health_area(job.actor, true).ok(),
                fatigue: body.vitals.fatigue,
                fatigue_limit: physical.rules.fatigue_limit,
                tool_available: job
                    .required_tool()
                    .is_some_and(|tool| self.tool_available_for(job.actor, tool)),
                patient_present: patient.is_some_and(|p| p.present),
                patient_contact: patient.is_some_and(|p| p.contact),
                patient_resting: patient.is_some_and(|p| p.resting),
            },
        )
    }
}

impl Simulation {
    pub(crate) fn on_health_event(&mut self, event: &EventKind) {
        if self.state.health.is_none() {
            return;
        }
        match *event {
            EventKind::Departed { actor, .. } => self.leave_shelter_on_movement(actor),
            EventKind::Born { child, parents, .. } => {
                let mother = parents
                    .iter()
                    .find(|parent| {
                        self.state.development.as_ref().is_some_and(|w| {
                            w.lives.get(parent).is_some_and(|life| {
                                life.reproduction == development::Reproduction::Gestates
                            })
                        })
                    })
                    .copied();
                if let Some(shelter) =
                    mother.and_then(|parent| self.effective_shelter(parent).map(|s| s.id))
                {
                    self.state
                        .health
                        .as_mut()
                        .unwrap()
                        .occupancy
                        .insert(child, shelter);
                }
                if self.state.components.clinical(child).is_none() {
                    self.state
                        .components
                        .insert_clinical(child, Clinical::default())
                        .expect("validated newborn clinical row");
                }
            }
            EventKind::Died { actor } => {
                self.state.health.as_mut().unwrap().occupancy.remove(&actor);
            }
            EventKind::IllnessStarted {
                actor,
                kind: IllnessKind::Enteric,
            } => {
                if let Some((source, drank)) = self
                    .state
                    .components
                    .clinical(actor)
                    .and_then(|c| c.last_drink)
                    && self.state.time.0.saturating_sub(drank.0)
                        <= self.state.health.as_ref().unwrap().rules.incubation.0
                    && !self
                        .state
                        .health
                        .as_ref()
                        .unwrap()
                        .known
                        .get(&actor)
                        .and_then(|k| k.water.get(&source))
                        .is_some_and(|s| s.purified)
                {
                    self.state
                        .health
                        .as_mut()
                        .unwrap()
                        .known
                        .entry(actor)
                        .or_default()
                        .water
                        .insert(
                            source,
                            WaterSafetyView {
                                inventory: source,
                                purified: false,
                                suspected_unsafe: true,
                                observed_at: self.state.time,
                            },
                        );
                }
            }

            EventKind::Transferred {
                actor,
                source,
                destination,
                quantity,
            } if self.state.inventories[&source].good == Good::Water => {
                let source_known = self
                    .state
                    .health
                    .as_ref()
                    .unwrap()
                    .known
                    .get(&actor)
                    .and_then(|k| k.water.get(&source))
                    .cloned();
                let destination_known = self
                    .state
                    .health
                    .as_ref()
                    .unwrap()
                    .known
                    .get(&actor)
                    .and_then(|k| k.water.get(&destination))
                    .cloned();
                if let Some(source_known) = source_known {
                    let old_quantity = self.state.inventories[&destination].quantity.0 - quantity.0;
                    let purified = source_known.purified
                        && (old_quantity == 0
                            || destination_known.as_ref().is_some_and(|d| d.purified));
                    let suspected_unsafe = source_known.suspected_unsafe
                        || destination_known.is_some_and(|d| d.suspected_unsafe);
                    self.state
                        .health
                        .as_mut()
                        .unwrap()
                        .known
                        .entry(actor)
                        .or_default()
                        .water
                        .insert(
                            destination,
                            WaterSafetyView {
                                inventory: destination,
                                purified,
                                suspected_unsafe,
                                observed_at: self.state.time,
                            },
                        );
                }
            }
            EventKind::Produced { actor, output, .. }
                if self.state.inventories[&output].good == Good::Water =>
            {
                self.state
                    .health
                    .as_mut()
                    .unwrap()
                    .known
                    .entry(actor)
                    .or_default()
                    .water
                    .remove(&output);
            }
            _ => {}
        }
    }
}

impl Simulation {
    pub(crate) fn health_practice_skill(&self, actor: PersonId) -> Option<Skill> {
        let job = {
            self.state.health.as_ref()?;
            self.state.components.health_job(actor)?
        };
        if !self.health_job_can_progress(job) {
            return None;
        }
        match job.work {
            Work::Treat { .. } | Work::Boil { .. } => Some(Skill::Medicine),
            Work::Shelter { .. } => Some(Skill::Craft),
            Work::Drink { .. } => None,
        }
    }
}

impl Simulation {
    fn willing_health_supplies(
        &self,
        giver: PersonId,
        patient: PersonId,
    ) -> Option<(InventoryId, InventoryId)> {
        let mind = self.state.decisions.as_ref()?.minds.get(&giver)?;
        let body = self.state.components.body(giver)?;
        let rules = &self.state.health.as_ref()?.rules;
        let physical = self.body_rules(giver);
        let stocks = self.remembered_health_stocks(giver, None);
        game_health::willing_supplies(
            rules,
            game_health::WillingCare {
                capable: self.health_area(giver, true).is_ok(),
                medicine: self.medical_level(giver),
                busy: self.health_actor_busy(giver),
                hungry: body.vitals.hunger >= physical.meal_relief,
                fatigued: body.vitals.fatigue >= physical.fatigue_limit / 2,
                urgent_dependent: mind.child_signals.values().any(|s| s.urgency >= 20_000),
                empathy: mind.personality.empathy,
                trust: mind.relationships.get(&patient).map_or(0, |r| r.trust),
                stocks: &stocks,
            },
        )
    }
}

impl Simulation {
    fn health_mind(&self, actor: PersonId) -> Option<&decisions::Mind> {
        if self.past_life_controls_actor(actor) {
            let w = self.state.invocation.as_ref()?;
            let active = w.control.active.as_ref()?;
            let life = active.dominant?;
            return Some(&w.history.get(&life)?.mind);
        }
        self.state.decisions.as_ref()?.minds.get(&actor)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    fn expressed_health_fixture() -> Simulation {
        let mut sim = Simulation::health_fixture(57, Execution::default()).unwrap();
        let mut content = genetics::Content::read(genetics::Config::default().content_dir).unwrap();
        content.physiology.data.expression_period_ms = 1_000_000;
        for definition in &mut content.physiology.data.traits {
            match definition.kind {
                game_physiology::traits::PhysicalTrait::Metabolism => definition.spread = 500,
                game_physiology::traits::PhysicalTrait::ColdAdaptation => definition.spread = 400,
                _ => {}
            }
        }
        sim.initialize_trait_content(content).unwrap();
        let actor = sim.state.ruler;
        let loci: Vec<_> = sim
            .state
            .physical_traits
            .as_ref()
            .unwrap()
            .definitions
            .traits
            .iter()
            .filter(|t| {
                matches!(
                    t.kind,
                    game_physiology::traits::PhysicalTrait::Metabolism
                        | game_physiology::traits::PhysicalTrait::ColdAdaptation
                )
            })
            .map(|t| game_genetics::LocusId(t.inheritance.locus.clone()))
            .collect();
        for locus in loci {
            sim.state
                .genetics
                .as_mut()
                .unwrap()
                .set_alleles(actor, &locus, [1_000; 2])
                .unwrap();
        }
        sim
    }

    #[test]
    fn actual_genetic_expression_changes_drinking_and_deprivation_without_rule_mutation() {
        let mut whole = expressed_health_fixture();
        let actor = whole.state.ruler;
        let rules = whole.state.health.as_ref().unwrap().rules.clone();
        let physiology = whole.health_physiology(actor);
        assert_eq!(
            (
                physiology.thirst_harm,
                physiology.drink_relief,
                physiology.cold_tolerance_centi
            ),
            (120_000, 200_000, 400)
        );
        whole.state.components.clinical_mut(actor).unwrap().thirst = 100_000;
        let mut sliced = Simulation::load(&whole.save().unwrap(), Execution::default()).unwrap();
        whole.advance(SimDuration(40_003)).unwrap();
        for elapsed in [1, 19_999, 7_001, 13_002] {
            sliced.advance(SimDuration(elapsed)).unwrap();
        }
        assert_eq!(whole.canonical_state(), sliced.canonical_state());
        assert!(
            whole.state.components.body(actor).unwrap().vitals.health
                < whole.body_rules(actor).max_health
        );
        assert_eq!(whole.state.health.as_ref().unwrap().rules, rules);

        let area = whole.health_area(actor, true).unwrap();
        let water = whole
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(area),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        whole.state.components.clinical_mut(actor).unwrap().thirst = 250_000;
        whole
            .debug_actor_command(
                actor,
                Command::Health {
                    action: HealthAction::Drink { source: water },
                },
            )
            .unwrap();
        whole.advance(rules.drinking_time).unwrap();
        assert_eq!(
            whole.state.components.clinical(actor).unwrap().thirst,
            60_000
        );
        assert_eq!(whole.state.inventories[&water].quantity, Quantity(0));
        assert_eq!(whole.state.health.as_ref().unwrap().rules, rules);
    }

    #[test]
    fn absent_genetics_and_neutral_traits_produce_identical_clinical_inputs() {
        let mut sim = Simulation::health_fixture(58, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        sim.state.components.clinical_mut(actor).unwrap().thirst = 200_000;
        let before = sim.evaluate_health(SimTime(1_003)).unwrap();
        let before_delta = sim.health_body_delta(actor, 1_003).unwrap();
        let before_physiology = sim.health_physiology(actor);
        let mut content = genetics::Content::read(genetics::Config::default().content_dir).unwrap();
        for definition in &mut content.physiology.data.traits {
            if matches!(
                definition.kind,
                game_physiology::traits::PhysicalTrait::Metabolism
                    | game_physiology::traits::PhysicalTrait::ColdAdaptation
            ) {
                definition.spread = 0;
            }
        }
        sim.initialize_trait_content(content).unwrap();
        let after = sim.evaluate_health(SimTime(1_003)).unwrap();
        assert_eq!(before, after);
        assert_eq!(before_delta, sim.health_body_delta(actor, 1_003).unwrap());
        assert_eq!(before_physiology, sim.health_physiology(actor));
    }

    #[test]
    fn caregiver_self_preservation_uses_individual_metabolic_needs() {
        let mut sim = expressed_health_fixture();
        let actor = sim.state.ruler;
        let area = sim.health_area(actor, true).unwrap();
        sim.state
            .skills
            .as_mut()
            .unwrap()
            .credit(
                &mut sim.state.components,
                actor,
                Skill::Medicine,
                60_000,
                None,
            )
            .unwrap();
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap()
            .personality
            .empathy = 900;
        for good in [Good::Herb, Good::Water] {
            let inventory = sim
                .debug_add_inventory(
                    good,
                    InventoryLocation::Area(area),
                    Quantity(2_000),
                    Quantity(2_000),
                    BTreeSet::from([actor]),
                )
                .unwrap();
            sim.state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
                .unwrap()
                .knowledge
                .insert(
                    inventory,
                    StockKnowledge {
                        inventory,
                        area,
                        good,
                        quantity: Quantity(2_000),
                        accessible: true,
                        observed_at: sim.state.time,
                        learned_at: sim.state.time,
                        source: KnowledgeSource::Observed(actor),
                    },
                );
        }
        assert!(sim.willing_health_supplies(actor, actor).is_some());
        sim.state.components.body_mut(actor).unwrap().vitals.hunger = 250_000;
        assert!(250_000 < sim.state.physical.as_ref().unwrap().rules.meal_relief);
        assert!(250_000 > sim.body_rules(actor).meal_relief);
        assert!(sim.willing_health_supplies(actor, actor).is_none());
    }
    #[test]
    fn medical_learning_during_a_course_is_identical_across_time_slices() {
        let mut whole = Simulation::health_fixture(9, Execution::default()).unwrap();
        whole.state.health.as_mut().unwrap().rules.treatment_time = SimDuration(120_000);
        let actor = whole.state.ruler;
        let area = whole.health_area(actor, true).unwrap();
        whole
            .state
            .skills
            .as_mut()
            .unwrap()
            .credit(
                &mut whole.state.components,
                actor,
                Skill::Medicine,
                90_000,
                None,
            )
            .unwrap();
        let herbs = whole
            .debug_add_inventory(
                Good::Herb,
                InventoryLocation::Area(area),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let water = whole
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(area),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        whole.debug_injure(actor, 20_000).unwrap();
        whole
            .debug_actor_command(
                actor,
                Command::Health {
                    action: HealthAction::Treat {
                        patient: actor,
                        herbs,
                        water,
                    },
                },
            )
            .unwrap();
        let mut sliced = Simulation::load(&whole.save().unwrap(), Execution::default()).unwrap();
        whole.advance(SimDuration(120_000)).unwrap();
        for elapsed in [1_003, 18_997, 43_001, 56_999] {
            sliced.advance(SimDuration(elapsed)).unwrap();
        }
        assert_eq!(whole.canonical_state(), sliced.canonical_state());
        assert_eq!(
            whole
                .state
                .components
                .clinical(actor)
                .unwrap()
                .treatment
                .as_ref()
                .unwrap()
                .healing,
            1_050
        );
    }
    #[test]
    fn unsafe_water_produces_real_dose_then_incubation_and_physical_illness() {
        let mut sim = Simulation::health_fixture(8, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        let area = sim.health_area(actor, true).unwrap();
        let water = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(area),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        sim.state
            .health
            .as_mut()
            .unwrap()
            .water
            .insert(water, WaterLot::new(2_000, 2_000_000));
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Health {
                action: HealthAction::Drink { source: water },
            },
        })
        .unwrap();
        sim.advance(SimDuration(10_000)).unwrap();
        assert_eq!(sim.state.inventories[&water].quantity, Quantity(1_000));
        assert_eq!(
            sim.state.health.as_ref().unwrap().water[&water].load(),
            1_000_000
        );
        assert!(!sim.observe().health.unwrap().symptoms.nausea);
        sim.advance(SimDuration(29_999)).unwrap();
        assert!(!sim.observe().health.unwrap().symptoms.nausea);
        sim.advance(SimDuration(1)).unwrap();
        assert!(sim.observe().health.unwrap().symptoms.nausea);
        assert!(
            sim.state
                .events
                .iter()
                .any(|event| event.time == SimTime(40_000)
                    && event.kind
                        == (EventKind::IllnessStarted {
                            actor,
                            kind: IllnessKind::Enteric
                        }))
        );
        let before = sim.state.components.body(actor).unwrap().vitals.health;
        sim.advance(SimDuration(1_000)).unwrap();
        assert!(sim.state.components.body(actor).unwrap().vitals.health < before);
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn a_physician_stops_work_at_actual_incapacitation_independently_of_time_slicing() {
        let mut whole = Simulation::health_fixture(7, Execution::default()).unwrap();
        let actor = whole.state.ruler;
        let area = whole.health_area(actor, true).unwrap();
        whole
            .state
            .skills
            .as_mut()
            .unwrap()
            .credit(
                &mut whole.state.components,
                actor,
                Skill::Medicine,
                90_000,
                None,
            )
            .unwrap();
        let herbs = whole
            .debug_add_inventory(
                Good::Herb,
                InventoryLocation::Area(area),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let water = whole
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(area),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let current = whole.state.components.body(actor).unwrap().vitals.health;
        let threshold = whole.state.combat.as_ref().unwrap().rules.incapacitation;
        whole
            .debug_injure(actor, current - (threshold + 5_000))
            .unwrap();
        whole
            .state
            .components
            .clinical_mut(actor)
            .unwrap()
            .illnesses
            .insert(
                IllnessKind::Respiratory,
                Infection {
                    kind: IllnessKind::Respiratory,
                    acquired: SimTime(0),
                    onset: SimTime(0),
                    recovers: SimTime(240_000),
                    symptomatic: true,
                },
            );
        whole
            .debug_actor_command(
                actor,
                Command::Health {
                    action: HealthAction::Treat {
                        patient: actor,
                        herbs,
                        water,
                    },
                },
            )
            .unwrap();
        let mut sliced = Simulation::load(&whole.save().unwrap(), Execution::default()).unwrap();
        whole.advance(SimDuration(20_000)).unwrap();
        for elapsed in [2_500, 2_500, 15_000] {
            sliced.advance(SimDuration(elapsed)).unwrap();
        }
        assert_eq!(whole.canonical_state(), sliced.canonical_state());
        assert_eq!(
            whole.state.skills.as_ref().unwrap().practice(
                &whole.state.components,
                actor,
                Skill::Medicine
            ),
            95_000
        );
        assert!(matches!(
            whole.state.components.health_job(actor).unwrap().work,
            Work::Treat { elapsed: 5_000, .. }
        ));
    }
    #[test]
    fn completed_boiling_does_not_release_a_tool_into_another_workers_elapsed_interval() {
        let mut sim = Simulation::health_fixture(6, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        let worker = sim.state.people[3].id;
        let area = sim.health_area(actor, true).unwrap();
        let people: Vec<_> = sim.state.people.iter().map(|p| p.id).collect();
        for person in people {
            sim.interrupt_actor_activity(person);
        }
        *sim.state
            .components
            .location_mut(worker)
            .expect("location exists") = Location::AtArea(area);
        let tool = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Area(area),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([actor, worker]),
            )
            .unwrap();
        let carried = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(worker),
                Quantity(0),
                Quantity(1_000),
                BTreeSet::from([worker]),
            )
            .unwrap();
        let water = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(area),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let fuel = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let site = sim
            .state
            .components
            .sites()
            .map(|(_, site)| site)
            .find(|site| site.area == area)
            .unwrap()
            .id;
        let output = sim.state.components.site(site).expect("site exists").output;
        let before = sim.state.inventories[&output].quantity;
        {
            let site = sim.state.components.site_mut(site).unwrap();
            site.tool = tool;
            site.duration = SimDuration(30_000);
            site.access.insert(worker);
        }
        sim.debug_actor_command(
            actor,
            Command::Health {
                action: HealthAction::Boil {
                    source: water,
                    fuel,
                    tool,
                },
            },
        )
        .unwrap();
        sim.debug_actor_command(worker, Command::Work { site })
            .unwrap();
        sim.debug_schedule_transfers(
            SimTime(30_000),
            vec![TransferIntent {
                actor: worker,
                source: tool,
                destination: carried,
                quantity: Quantity(1_000),
            }],
        )
        .unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        assert_eq!(sim.state.inventories[&output].quantity, before);
        assert!(sim.state.events.iter().any(|event| event.kind
            == (EventKind::WaterPurified {
                actor,
                source: water
            })));
        assert!(!sim.state.events.iter().any(
            |event| matches!(event.kind, EventKind::Produced { actor, .. } if actor == worker)
        ));
    }
    #[test]
    fn unseen_shelter_occupants_do_not_change_affordances_or_private_plans() {
        let mut sim = Simulation::health_fixture(5, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        let other = sim
            .state
            .people
            .iter()
            .find(|p| p.role == Role::Guardian)
            .unwrap()
            .id;
        let area = sim.health_area(actor, true).unwrap();
        let timber = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let shelter = sim
            .seed_shelter(
                area,
                actor,
                BTreeSet::from([actor, other]),
                1,
                1_000,
                timber,
                Quantity(2_000),
                SimTime(0),
            )
            .unwrap();
        sim.state.components.clinical_mut(actor).unwrap().cold = 20_000;
        sim.observe_health();
        let expected = Command::Health {
            action: HealthAction::EnterShelter { shelter },
        };
        assert!(sim.available_health_commands(actor).contains(&expected));
        assert_eq!(sim.health_candidate(actor, None), Some(expected.clone()));
        sim.set_initial_shelter_occupancy(other, shelter).unwrap();
        sim.observe_health();
        assert!(
            sim.observe()
                .health
                .unwrap()
                .shelters
                .iter()
                .find(|s| s.id == shelter)
                .unwrap()
                .known_occupants
                .is_empty()
        );
        assert!(sim.available_health_commands(actor).contains(&expected));
        assert_eq!(sim.health_candidate(actor, None), Some(expected.clone()));
        assert_eq!(
            sim.debug_actor_command(actor, expected),
            Err(Error::Unavailable)
        );
    }
}

#[cfg(test)]
mod transport_health_tests {
    use super::*;
    fn held_peer() -> (Simulation, PersonId, PersonId) {
        let mut sim = Simulation::health_fixture(23, Execution::default()).unwrap();
        // This fixture's medic can support the adult and the actual supply pack.
        sim.state.physical.as_mut().unwrap().rules.carrying_capacity = Quantity(20_000);
        sim.initialize_transport(game_transport::Rules {
            adult_mass: Quantity(6_000),
            newborn_mass: Quantity(1_000),
            lift_time: SimDuration(6_000),
            ..game_transport::Rules::default()
        })
        .unwrap();
        let bearer = sim.state.ruler;
        let passenger = sim.state.people[3].id;
        for actor in [bearer, passenger] {
            sim.debug_actor_command(actor, Command::Rest).unwrap();
        }
        sim.debug_actor_command(
            bearer,
            Command::Transport {
                action: game_transport::Action::OfferCarry { passenger },
            },
        )
        .unwrap();
        sim.debug_actor_command(
            passenger,
            Command::Transport {
                action: game_transport::Action::AcceptCarry {
                    bearers: vec![bearer],
                },
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            *sim.state
                .components
                .location(passenger)
                .expect("location exists"),
            Location::Carried { carrier: bearer }
        );
        (sim, bearer, passenger)
    }
    #[test]
    fn held_body_shares_shelter_and_contact_without_obtaining_access() {
        let (mut sim, bearer, passenger) = held_peer();
        let area = sim.health_area(bearer, true).unwrap();
        let quantity = shelter_cost(&sim.state.health.as_ref().unwrap().rules, 2, 650).unwrap();
        let timber = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                quantity,
                quantity,
                BTreeSet::from([bearer]),
            )
            .unwrap();
        let shelter = sim
            .seed_shelter(
                area,
                bearer,
                BTreeSet::new(),
                2,
                650,
                timber,
                quantity,
                sim.state.time,
            )
            .unwrap();
        sim.debug_actor_command(
            bearer,
            Command::Health {
                action: HealthAction::EnterShelter { shelter },
            },
        )
        .unwrap();
        assert_eq!(sim.effective_shelter(passenger).unwrap().id, shelter);
        assert_eq!(sim.health_exposure(passenger).insulation, 650);
        assert!(
            !sim.state.health.as_ref().unwrap().shelters[&shelter]
                .access
                .contains(&passenger)
        );
        assert!(
            sim.state.health.as_ref().unwrap().known[&bearer].shelters[&shelter]
                .known_occupants
                .contains(&passenger)
        );
        sim.debug_actor_command(
            bearer,
            Command::Transport {
                action: game_transport::Action::PutDown { passenger },
            },
        )
        .unwrap();
        assert_eq!(sim.effective_shelter(passenger).unwrap().id, shelter);
        assert!(
            !sim.state.health.as_ref().unwrap().shelters[&shelter]
                .access
                .contains(&passenger)
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn roadside_drinking_uses_real_held_water_and_rejects_endpoint_stores() {
        let (mut sim, bearer, passenger) = held_peer();
        let home = sim.health_area(bearer, true).unwrap();
        let destination = sim.state.areas[1].id;
        let endpoint = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(home),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([bearer]),
            )
            .unwrap();
        let water = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Carried(bearer),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([bearer]),
            )
            .unwrap();
        for source in [endpoint, water] {
            sim.state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&bearer)
                .unwrap()
                .knowledge
                .insert(
                    source,
                    StockKnowledge {
                        inventory: source,
                        area: home,
                        good: Good::Water,
                        quantity: Quantity(2_000),
                        accessible: true,
                        observed_at: sim.state.time,
                        learned_at: sim.state.time,
                        source: KnowledgeSource::Observed(bearer),
                    },
                );
        }
        sim.seed_water_safety(bearer, water).unwrap();
        assert!(
            sim.transport_can_depart(bearer),
            "bodies={:?}, groups={:?}",
            sim.transport_bodies(),
            sim.state.transport.as_ref().unwrap().groups
        );
        sim.debug_move_actor(bearer, destination).unwrap();
        sim.advance(SimDuration(10_000)).unwrap();
        sim.debug_actor_command(
            bearer,
            Command::Transport {
                action: game_transport::Action::Stop,
            },
        )
        .unwrap();
        assert!(matches!(
            sim.resolved_location(bearer),
            Location::StoppedOnRoad { .. }
        ));
        sim.state.components.clinical_mut(bearer).unwrap().thirst = 100_000;
        assert_eq!(
            sim.health_candidate(bearer, None),
            Some(Command::Health {
                action: HealthAction::Drink { source: water }
            })
        );
        let before = sim.state.clone();
        assert_eq!(
            sim.debug_actor_command(
                bearer,
                Command::Health {
                    action: HealthAction::Drink { source: endpoint }
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
        sim.debug_actor_command(
            bearer,
            Command::Health {
                action: HealthAction::Drink { source: water },
            },
        )
        .unwrap();
        let mut sliced = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(SimDuration(10_000)).unwrap();
        for elapsed in [1, 997, 9_002] {
            sliced.advance(SimDuration(elapsed)).unwrap();
        }
        assert_eq!(sim.state, sliced.state);
        assert_eq!(sim.state.inventories[&water].quantity, Quantity(1_000));
        assert_eq!(sim.state.inventories[&endpoint].quantity, Quantity(2_000));
        sim.state.health.as_mut().unwrap().rules.treatment_time = SimDuration(10_000);
        sim.state
            .skills
            .as_mut()
            .unwrap()
            .credit(
                &mut sim.state.components,
                bearer,
                Skill::Medicine,
                60_000,
                None,
            )
            .unwrap();
        let herbs = sim
            .debug_add_inventory(
                Good::Herb,
                InventoryLocation::Carried(bearer),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([bearer]),
            )
            .unwrap();
        sim.debug_actor_command(
            bearer,
            Command::Health {
                action: HealthAction::OfferTreatment {
                    patient: passenger,
                    herbs,
                    water,
                },
            },
        )
        .unwrap();
        sim.debug_actor_command(
            passenger,
            Command::Health {
                action: HealthAction::AcceptTreatment { caregiver: bearer },
            },
        )
        .unwrap();
        sim.debug_actor_command(
            bearer,
            Command::Health {
                action: HealthAction::Treat {
                    patient: passenger,
                    herbs,
                    water,
                },
            },
        )
        .unwrap();
        let mut sliced = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(SimDuration(10_000)).unwrap();
        for elapsed in [503, 4_997, 4_500] {
            sliced.advance(SimDuration(elapsed)).unwrap();
        }
        assert_eq!(sim.state, sliced.state);
        assert!(
            sim.state
                .components
                .clinical(passenger)
                .unwrap()
                .treatment
                .is_some()
        );
        assert_eq!(sim.state.inventories[&herbs].quantity, Quantity(0));
        assert_eq!(sim.state.inventories[&water].quantity, Quantity(0));
    }
    #[test]
    fn carried_battle_body_is_not_a_strategic_area_contact_or_remote_patient() {
        let (mut sim, bearer, passenger) = held_peer();
        let enemy = sim.state.people[6].id;
        let id = sim
            .debug_start_encounter(vec![(bearer, 0), (enemy, 1)])
            .unwrap();
        let outside = sim.state.people[9].id;
        assert!(sim.in_health_contact(bearer, passenger));
        assert!(!sim.can_observe_health(outside, passenger));
        sim.state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&id)
            .unwrap()
            .fighters
            .get_mut(&enemy)
            .unwrap()
            .position = Point {
            x: 20_000,
            y: 8_000,
        };
        sim.state.components.clinical_mut(enemy).unwrap().cold = 120_000;
        assert!(!sim.health_tactical_clear(
            sim.health_tactical_place(passenger),
            sim.health_tactical_place(enemy),
            true
        ));
        sim.observe_health();
        assert!(!sim.state.health.as_ref().unwrap().known[&bearer].people[&enemy].shivering);
        assert!(sim.visible_symptoms(enemy).shivering);
        assert!(sim.indexed_health_exposures().contains_key(&passenger));
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;

    fn population_fixture(population: usize) -> Simulation {
        let sample = Simulation::health_fixture(71, Execution::default()).unwrap();
        let sample_world = sample.state.physical.as_ref().unwrap();
        let template = sample
            .state
            .components
            .body(sample.state.ruler)
            .unwrap()
            .clone();
        let activity = sample
            .state
            .components
            .activity(sample.state.ruler)
            .unwrap()
            .clone();
        let job = sample.state.components.job(sample.state.ruler).cloned();
        let mut sim = Simulation::kernel_fixture(71, population, Execution::default()).unwrap();
        sim.initialize_physical_people(sample_world.rules.clone())
            .unwrap();
        for person in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            *sim.state.components.body_mut(person).unwrap() = template.clone();
            *sim.state.components.activity_mut(person).unwrap() = activity.clone();
            sim.state.components.set_job(person, job.clone()).unwrap();
        }
        sim.initialize_health(Rules::fixture()).unwrap();
        sim
    }

    fn exhaustive_exposures(sim: &Simulation) -> BTreeMap<PersonId, ExposureInput> {
        sim.state
            .people
            .iter()
            .map(|person| {
                let mut exposure = sim.health_exposure(person.id);
                for (index, kind) in [IllnessKind::Enteric, IllnessKind::Respiratory]
                    .into_iter()
                    .enumerate()
                {
                    exposure.infectious_contacts[index] = sim
                        .state
                        .components
                        .clinicals()
                        .filter(|(other, state)| {
                            state
                                .illnesses
                                .get(&kind)
                                .is_some_and(|i| i.symptomatic && i.recovers > sim.state.time)
                                && sim.in_health_contact(person.id, *other)
                        })
                        .count() as u64;
                }
                (person.id, exposure)
            })
            .collect()
    }

    fn exhaustive_observations(sim: &Simulation) -> BTreeMap<PersonId, Knowledge> {
        let world = sim.state.health.as_ref().unwrap();
        let mut known = world.known.clone();
        for observer in sim
            .state
            .people
            .iter()
            .filter(|p| sim.state.components.body(p.id).unwrap().vitals.alive)
        {
            let Location::AtArea(area) = *sim
                .state
                .components
                .location(observer.id)
                .expect("location exists")
            else {
                continue;
            };
            let people: Vec<_> = sim
                .state
                .people
                .iter()
                .filter(|p| sim.can_observe_health(observer.id, p.id))
                .map(|p| p.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(64)
                .map(|p| (p, sim.visible_symptoms(p)))
                .collect();
            let shelters: Vec<_> = world
                .shelters
                .values()
                .filter(|s| s.area == area)
                .map(|s| {
                    (
                        s.id,
                        ShelterView {
                            id: s.id,
                            area: s.area,
                            capacity: s.capacity,
                            insulation: s.insulation,
                            custodian: s.custodian,
                            accessible: s.access.contains(&observer.id),
                            known_occupants: world
                                .occupancy
                                .iter()
                                .filter(|(p, id)| {
                                    **id == s.id && sim.can_observe_health(observer.id, **p)
                                })
                                .map(|(p, _)| *p)
                                .collect(),
                            observed_at: sim.state.time,
                        },
                    )
                })
                .collect();
            let knowledge = known.entry(observer.id).or_default();
            knowledge.people.extend(people);
            knowledge.shelters.extend(shelters);
        }
        known
    }

    fn add_shelters(sim: &mut Simulation) -> [ShelterId; 2] {
        let actor = sim.state.ruler;
        let area = sim.state.areas[0].id;
        let ids = [
            ShelterId(sim.state.next_id),
            ShelterId(sim.state.next_id + 1),
        ];
        sim.state.next_id += 2;
        let world = sim.state.health.as_mut().unwrap();
        for id in ids {
            world.shelters.insert(
                id,
                Shelter {
                    id,
                    area,
                    custodian: actor,
                    access: BTreeSet::from([actor]),
                    capacity: 64,
                    insulation: 650,
                    built_at: SimTime(0),
                    timber_used: Quantity(64_000),
                    labor_ms: 60_000,
                },
            );
        }
        ids
    }

    #[test]
    fn indexed_contacts_match_exhaustive_pairs_across_varied_opening_states() {
        let mut sim = population_fixture(15);
        let people: Vec<_> = sim.state.people.iter().map(|p| p.id).collect();
        let shelters = add_shelters(&mut sim);
        for phase in 0..18 {
            sim.state.health.as_mut().unwrap().occupancy.clear();
            let ids: Vec<_> = sim
                .state
                .components
                .health_jobs()
                .map(|(id, _)| id)
                .collect();
            for id in ids {
                sim.state.components.remove_health_job(id).unwrap();
            }
            for (index, actor) in people.iter().enumerate() {
                let area = sim.state.areas[if index < 6 { 0 } else { (index + phase) % 3 }].id;
                *sim.state
                    .components
                    .location_mut(*actor)
                    .expect("location exists") = if (index + phase) % 11 == 0 {
                    Location::InTransit {
                        from: area,
                        to: if area == sim.state.areas[0].id {
                            sim.state.areas[1].id
                        } else {
                            sim.state.areas[0].id
                        },
                        departed: SimTime(0),
                        arrives: SimTime(100_000),
                        road_start: None,
                    }
                } else {
                    Location::AtArea(area)
                };
                if area == sim.state.areas[0].id && (index + phase) % 3 != 2 {
                    sim.state
                        .health
                        .as_mut()
                        .unwrap()
                        .occupancy
                        .insert(*actor, shelters[(index + phase) % 2]);
                }
                let max_health = sim.body_rules(*actor).max_health;
                let body = sim.state.components.body_mut(*actor).unwrap();
                body.vitals.alive = (index + phase) % 13 != 0;
                body.vitals.health = if body.vitals.alive { max_health } else { 0 };
                let recipient = people[(index + 1) % people.len()];
                *sim.state.components.activity_mut(*actor).unwrap() = match (index + phase) % 6 {
                    0 => Activity::Care {
                        child: recipient,
                        source: InventoryId(1),
                        completes: SimTime(60_000),
                    },
                    1 => Activity::Comfort {
                        child: recipient,
                        completes: SimTime(60_000),
                    },
                    2 => Activity::Tell {
                        recipient,
                        fact: StockKnowledge {
                            inventory: InventoryId(1),
                            area,
                            good: Good::Water,
                            quantity: Quantity(0),
                            accessible: false,
                            observed_at: SimTime(0),
                            learned_at: SimTime(0),
                            source: KnowledgeSource::Observed(*actor),
                        },
                        completes: SimTime(60_000),
                    },
                    3 => Activity::Rest,
                    4 => Activity::Play {
                        interest: ChildhoodInterest::Movement,
                        completes: SimTime(60_000),
                    },
                    _ => Activity::Idle,
                };
                let state = sim.state.components.clinical_mut(*actor).unwrap();
                state.illnesses.clear();
                for (kind_index, kind) in [IllnessKind::Enteric, IllnessKind::Respiratory]
                    .into_iter()
                    .enumerate()
                {
                    state.illnesses.insert(
                        kind,
                        Infection {
                            kind,
                            acquired: SimTime(0),
                            onset: SimTime(0),
                            recovers: SimTime(if (index + phase + kind_index) % 5 == 0 {
                                0
                            } else {
                                120_000
                            }),
                            symptomatic: (index + phase + kind_index) % 3 != 0,
                        },
                    );
                }
            }
            sim.state
                .components
                .insert_health_job(Job {
                    actor: people[0],
                    area: sim.state.areas[0].id,
                    completes: SimTime(60_000),
                    work: Work::Treat {
                        patient: people[1],
                        skill_ms: 0,
                        elapsed: 0,
                    },
                })
                .unwrap();
            sim.state.missions = Some(missions::World {
                missions: BTreeMap::new(),
                recruitments: BTreeMap::new(),
                known: BTreeMap::new(),
                talks: BTreeMap::from([(
                    people[0],
                    missions::PendingTalk {
                        speaker: people[0],
                        listener: people[1],
                        area: sim.state.areas[0].id,
                        completes: SimTime(60_000),
                        conversation: missions::Conversation::Objective {
                            mission: MissionId(999),
                            leader: people[0],
                            objective: MissionObjective::Visit {
                                destination: sim.state.areas[0].id,
                            },
                        },
                    },
                )]),
                last_event: 0,
                last_review: SimTime(0),
            });
            assert_eq!(
                sim.indexed_health_exposures(),
                exhaustive_exposures(&sim),
                "phase {phase}"
            );
            let expected = exhaustive_observations(&sim);
            sim.observe_health();
            assert_eq!(
                sim.state.health.as_ref().unwrap().known,
                expected,
                "phase {phase}"
            );
            sim.cancel_health_activity(people[0]);
            sim.state.missions.as_mut().unwrap().talks.clear();
            sim.state
                .components
                .body_mut(people[1])
                .unwrap()
                .vitals
                .alive = false;
            sim.on_health_event(&EventKind::Died { actor: people[1] });
            assert_eq!(sim.indexed_health_exposures(), exhaustive_exposures(&sim));
        }
    }

    #[test]
    fn indexed_observations_preserve_the_first64_ids_after_storage_reordering() {
        let mut sim = population_fixture(90);
        let area = sim.state.areas[0].id;
        for p in &mut sim.state.people {
            *sim.state
                .components
                .location_mut(p.id)
                .expect("location exists") = Location::AtArea(area);
        }
        add_shelters(&mut sim);
        let expected = exhaustive_observations(&sim);
        sim.state.people.reverse();
        sim.person_index = sim
            .state
            .people
            .iter()
            .enumerate()
            .map(|(index, p)| (p.id, index))
            .collect();
        sim.observe_health();
        assert_eq!(sim.state.health.as_ref().unwrap().known, expected);
        assert!(expected.values().all(|k| k.people.len() == 64));
    }

    #[test]
    #[ignore = "explicit release workload measurement"]
    fn health_index_workload_timings() {
        for population in [1_000, 10_000] {
            let mut sim = population_fixture(population);
            let start = Instant::now();
            let indexed = sim.indexed_health_exposures();
            let index_time = start.elapsed();
            let start = Instant::now();
            let exhaustive = exhaustive_exposures(&sim);
            let reference_time = start.elapsed();
            assert_eq!(indexed, exhaustive);
            let start = Instant::now();
            let expected = exhaustive_observations(&sim);
            let observe_reference = start.elapsed();
            let start = Instant::now();
            sim.observe_health();
            let observe_indexed = start.elapsed();
            assert_eq!(sim.state.health.as_ref().unwrap().known, expected);
            let start = Instant::now();
            for person in &sim.state.people {
                std::hint::black_box(sim.health_body_delta(person.id, 1_000).unwrap());
            }
            let body_contributions = start.elapsed();
            println!(
                "population={population} exposure_index={index_time:?} exposure_exhaustive={reference_time:?} observe_index={observe_indexed:?} observe_exhaustive={observe_reference:?} body_contributions={body_contributions:?}"
            );
        }
    }
}

#[cfg(test)]
mod supply_composition_tests {
    use super::*;
    #[test]
    fn self_intake_composes_real_collection_then_native_consumption_across_restore_and_slices() {
        let mut sim = Simulation::ecology_fixture(29, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        for person in sim.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            let mut traits = decisions::Personality::restful();
            traits.risk_tolerance = 900;
            sim.debug_set_personality(person.id, traits, decisions::Aspiration::Comfort)
                .unwrap();
        }
        for stock in sim
            .state
            .inventories
            .values_mut()
            .filter(|stock| stock.good == Good::Water)
        {
            stock.quantity = Quantity(0);
        }
        sim.initialize_health(Rules::fixture()).unwrap();
        let relief = sim.state.health.as_ref().unwrap().rules.drink_relief;
        sim.state.components.clinical_mut(actor).unwrap().thirst = relief;
        sim.review_decisions().unwrap();
        let source = sim.state.ecology.as_ref().unwrap().known[&actor]
            .values()
            .find(|source| source.kind == ResourceKind::Spring && source.area == AreaId(1))
            .unwrap()
            .id;
        let original = sim
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&sim.state.components, source)
            .expect("component exists")
            .remaining;
        let before = sim.state.clone();
        let command = sim.health_candidate(actor, None).unwrap();
        assert!(
            matches!(command,Command::Ecology {action:EcologyAction::Gather {resource,quantity:Quantity(1_000),unauthorized:false}} if resource==source)
        );
        for _ in 0..20 {
            assert_eq!(sim.health_candidate(actor, None), Some(command.clone()));
        }
        assert_eq!(sim.state, before);
        sim.debug_actor_command(actor, command).unwrap();
        let complete = sim.state.ecology.as_ref().unwrap().jobs[&actor].completes;
        let duration = complete.0 - sim.state.time.0;
        let mut restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(SimDuration(duration)).unwrap();
        restored.advance(SimDuration(duration / 2)).unwrap();
        restored = Simulation::load(
            &restored.save().unwrap(),
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        restored
            .advance(SimDuration(duration - duration / 2))
            .unwrap();
        assert_eq!(sim.state, restored.state);
        assert_eq!(
            sim.state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.state.components, source)
                .expect("component exists")
                .remaining,
            Quantity(original.0 - 1_000)
        );
        assert!(!sim.state.events.iter().any(
            |event| matches!(event.kind,EventKind::Drank {actor:drinker,..} if drinker==actor)
        ));
        let command = sim.health_candidate(actor, None).unwrap();
        let Command::Health {
            action: HealthAction::Drink { source: water },
        } = command
        else {
            panic!("collected water must go through native intake: {command:?}");
        };
        let thirst = sim.state.components.clinical(actor).unwrap().thirst;
        for world in [&mut sim, &mut restored] {
            world
                .debug_actor_command(
                    actor,
                    Command::Health {
                        action: HealthAction::Drink { source: water },
                    },
                )
                .unwrap();
        }
        let duration = sim.state.health.as_ref().unwrap().rules.drinking_time;
        sim.advance(duration).unwrap();
        restored.advance(duration).unwrap();
        assert_eq!(sim.state, restored.state);
        assert_eq!(sim.state.inventories[&water].quantity, Quantity(0));
        assert!(sim.state.components.clinical(actor).unwrap().thirst < thirst);
        assert!(sim.state.events.iter().any(|event|matches!(event.kind,EventKind::Drank {actor:drinker,source,..} if drinker==actor && source==water)));
    }
    #[test]
    fn a_departed_remembered_stock_cannot_hide_a_real_local_intake_alternative() {
        let mut sim = Simulation::health_fixture(51, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        let holder = sim
            .state
            .people
            .iter()
            .find(|person| person.id != actor)
            .unwrap()
            .id;
        for person in sim.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
        }
        let mut traits = decisions::Personality::restful();
        traits.risk_tolerance = 900;
        sim.debug_set_personality(actor, traits, decisions::Aspiration::Comfort)
            .unwrap();
        let moving = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Carried(holder),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([actor, holder]),
            )
            .unwrap();
        let alternate = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        mind.knowledge.retain(|_, stock| stock.good != Good::Water);
        for id in [moving, alternate] {
            mind.knowledge.insert(
                id,
                StockKnowledge {
                    inventory: id,
                    area: AreaId(1),
                    good: Good::Water,
                    quantity: Quantity(2_000),
                    accessible: true,
                    observed_at: sim.state.time,
                    learned_at: sim.state.time,
                    source: KnowledgeSource::Observed(actor),
                },
            );
        }
        let relief = sim.state.health.as_ref().unwrap().rules.drink_relief;
        sim.state.components.clinical_mut(actor).unwrap().thirst = relief;
        assert_eq!(
            sim.health_candidate(actor, None),
            Some(Command::Health {
                action: HealthAction::Drink { source: moving }
            })
        );
        let remembered =
            sim.state.decisions.as_ref().unwrap().minds[&actor].knowledge[&moving].clone();
        sim.debug_move_actor(holder, AreaId(3)).unwrap();
        sim.advance(SimDuration(1)).unwrap();
        let before = sim.state.clone();
        assert_eq!(
            sim.health_candidate(actor, None),
            Some(Command::Health {
                action: HealthAction::Drink { source: alternate }
            })
        );
        assert_eq!(sim.state, before);
        assert_eq!(
            sim.state.decisions.as_ref().unwrap().minds[&actor].knowledge[&moving],
            remembered
        );
        let facts = sim.remembered_health_stocks(actor, None);
        assert!(
            !facts
                .iter()
                .find(|stock| stock.fact.inventory == moving)
                .unwrap()
                .here
        );
        assert_eq!(
            facts
                .iter()
                .find(|stock| stock.fact.inventory == moving)
                .unwrap()
                .fact
                .area,
            AreaId(1)
        );
    }
    #[test]
    fn private_shelter_hides_new_dependent_needs_and_real_bearers_keep_contact_while_moving() {
        let mut sim = Simulation::childhood_fixture(11, Execution::default()).unwrap();
        sim.initialize_health(Rules::fixture()).unwrap();
        let child = sim.state.ruler;
        let observer = sim
            .state
            .people
            .iter()
            .find(|person| {
                person.id != child
                    && sim.independent(person.id)
                    && sim.state.components.body(person.id).unwrap().vitals.alive
            })
            .unwrap()
            .id;
        for person in sim
            .state
            .people
            .clone()
            .into_iter()
            .filter(|person| sim.state.components.body(person.id).unwrap().vitals.alive)
            .map(|person| person.id)
            .collect::<Vec<_>>()
        {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
        }
        let Location::AtArea(area) = sim.resolved_location(child) else {
            panic!("newborn must have an actual local home");
        };
        let cost = shelter_cost(&sim.state.health.as_ref().unwrap().rules, 2, 650).unwrap();
        let timber = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                cost,
                cost,
                BTreeSet::from([observer]),
            )
            .unwrap();
        let shelter = sim
            .seed_shelter(
                area,
                observer,
                BTreeSet::from([child, observer]),
                2,
                650,
                timber,
                cost,
                sim.state.time,
            )
            .unwrap();
        sim.set_initial_shelter_occupancy(child, shelter).unwrap();
        let hunger = sim.body_rules(child).hunger_harm;
        sim.state.components.body_mut(child).unwrap().vitals.hunger = hunger;
        let thirst = sim.state.health.as_ref().unwrap().rules.thirst_harm;
        sim.state.components.clinical_mut(child).unwrap().thirst = thirst;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&observer)
            .unwrap()
            .child_signals
            .clear();
        assert!(!sim.dependent_signal_visible(observer, child));
        sim.review_decisions().unwrap();
        assert!(
            !sim.state.decisions.as_ref().unwrap().minds[&observer]
                .child_signals
                .contains_key(&child)
        );
        sim.debug_actor_command(
            observer,
            Command::Health {
                action: HealthAction::EnterShelter { shelter },
            },
        )
        .unwrap();
        assert!(sim.dependent_signal_visible(observer, child));
        sim.initialize_transport(game_transport::Rules {
            lift_time: SimDuration(6_000),
            ..game_transport::Rules::default()
        })
        .unwrap();
        sim.debug_actor_command(
            observer,
            Command::Transport {
                action: game_transport::Action::OfferCarry { passenger: child },
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(
            matches!(*sim.state.components.location(child).expect("location exists"),Location::Carried {carrier} if carrier==observer)
        );
        sim.debug_actor_command(
            observer,
            Command::Health {
                action: HealthAction::LeaveShelter,
            },
        )
        .unwrap();
        let destination = sim
            .state
            .areas
            .iter()
            .find(|place| place.id == area)
            .unwrap()
            .links[0]
            .destination;
        sim.debug_move_actor(observer, destination).unwrap();
        sim.advance(SimDuration(1)).unwrap();
        assert!(sim.transport_is_moving(observer));
        assert!(sim.dependent_signal_visible(observer, child));
        assert!(!sim.can_observe_health(observer, child));
    }
}
