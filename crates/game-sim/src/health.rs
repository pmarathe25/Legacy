//! Water lots retain contaminant load across actual collection and transport.
//! Clinical effects are independent of custody and of who knows the water is unsafe.
use crate::*;

pub use game_health::*;
type HealthCohort = (AreaId, Option<ShelterId>);

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
        let world = self.state.health.get_or_insert_with(World::default);
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
        return Ok(());
    };
    let physical = state
        .physical
        .as_ref()
        .ok_or_else(|| Error::InvalidSave("health requires physical bodies".into()))?;
    let people = state
        .people
        .iter()
        .map(|person| {
            let body = physical.actors.get(&person.id).ok_or_else(|| {
                Error::InvalidSave("health requires a body for each person".into())
            })?;
            let health_work = match body.activity {
                Activity::HealthWork { kind, completes } => {
                    Some(HealthWorkLink { kind, completes })
                }
                _ => None,
            };
            Ok((
                person.id,
                PersonCatalog {
                    alive: body.vitals.alive,
                    location: person.location.clone(),
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
    game_health::validate_world(
        world,
        ValidationCatalog {
            now: state.time,
            people: &people,
            areas: &areas,
            inventories: &inventories,
        },
    )
}
#[derive(Clone, Debug)]
pub(crate) struct PendingHealth {
    pub clinical: BTreeMap<PersonId, Clinical>,
    pub jobs: BTreeMap<PersonId, Job>,
}

impl Simulation {
    pub fn initialize_health(&mut self, rules: Rules) -> Result<(), Error> {
        validate_rules(&rules)?;
        let world = self.state.health.get_or_insert_with(World::default);
        world.rules = rules;
        for p in &self.state.people {
            world.clinical.entry(p.id).or_default();
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
        let p = &self.state.people[*self.person_index.get(&actor).ok_or(Error::Unavailable)?];
        let body = self
            .state
            .physical
            .as_ref()
            .and_then(|w| w.actors.get(&actor))
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
        match p.location {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }
    fn medical_level(&self, actor: PersonId) -> u16 {
        self.own_skill_level(actor, Skill::Medicine)
            .max(self.borrowed_skill_level(actor, Skill::Medicine))
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
                && self.inventory_area(stock) == self.health_area(actor, true).ok()
                && self.unreserved_stock(id, actor) >= quantity.0
        })
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
        let state = w.clinical.get(&patient).cloned().unwrap_or_default();
        let state =
            game_health::receive_water(&state, &w.rules, self.state.time, source, quantity, dose)?;
        self.state
            .health
            .as_mut()
            .unwrap()
            .clinical
            .insert(patient, state);
        Ok(())
    }
    pub(crate) fn health_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        match &self.state.health.as_ref()?.jobs.get(&actor)?.work {
            Work::Boil { tool, .. } | Work::Shelter { tool, .. } => Some(*tool),
            _ => None,
        }
    }
    pub(crate) fn health_reserved_stock(&self, source: InventoryId, except: PersonId) -> u64 {
        self.state.health.as_ref().map_or(0, |w| {
            w.jobs
                .values()
                .filter(|job| job.actor != except)
                .filter_map(|job| match job.work {
                    Work::Drink {
                        source: id,
                        quantity,
                    }
                    | Work::Boil {
                        source: id,
                        quantity,
                        ..
                    } if id == source => Some(quantity.0),
                    _ => None,
                })
                .fold(0, u64::saturating_add)
        })
    }
    pub(crate) fn health_patient_resting(&self, actor: PersonId) -> bool {
        self.state.health.as_ref().is_some_and(|w| {
            w.jobs
                .values()
                .any(|job| matches!(job.work,Work::Treat {patient,..}if patient==actor))
        })
    }
    pub(crate) fn health_work_exerting(&self, actor: PersonId) -> bool {
        self.state
            .health
            .as_ref()
            .and_then(|w| w.jobs.get(&actor))
            .is_some_and(|job| {
                !matches!(job.work, Work::Drink { .. }) && self.health_job_can_progress(job)
            })
    }
    fn start_health_job(
        &mut self,
        actor: PersonId,
        area: AreaId,
        duration: SimDuration,
        kind: HealthWorkKind,
        work: Work,
    ) -> Result<(), Error> {
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(duration.0)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_actor_activity(actor);
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::HealthWork { kind, completes };
        self.state.health.as_mut().unwrap().jobs.insert(
            actor,
            Job {
                actor,
                area,
                completes,
                work,
            },
        );
        Ok(())
    }
    pub(crate) fn cancel_health_activity(&mut self, actor: PersonId) {
        let Some(w) = &self.state.health else { return };
        let jobs: Vec<_> = w
            .jobs
            .values()
            .filter(|job| {
                job.actor == actor || matches!(job.work,Work::Treat {patient,..}if patient==actor)
            })
            .cloned()
            .collect();
        for job in jobs {
            self.state.health.as_mut().unwrap().jobs.remove(&job.actor);
            if matches!(self.state.physical.as_ref().unwrap().actors[&job.actor].activity,Activity::HealthWork {completes,..}if completes==job.completes)
            {
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&job.actor)
                    .unwrap()
                    .activity = Activity::Idle;
            }
        }
        self.state
            .health
            .as_mut()
            .unwrap()
            .offers
            .retain(|(giver, patient), _| *giver != actor && *patient != actor);
    }
    pub(crate) fn leave_shelter_on_movement(&mut self, actor: PersonId) {
        if let Some(shelter) = self
            .state
            .health
            .as_mut()
            .and_then(|w| w.occupancy.remove(&actor))
        {
            self.emit(EventKind::ShelterLeft { shelter, actor });
        }
    }
    pub(crate) fn health_command(
        &mut self,
        actor: PersonId,
        action: &HealthAction,
    ) -> Result<(), Error> {
        let rules = self
            .state
            .health
            .as_ref()
            .ok_or(Error::Unavailable)?
            .rules
            .clone();
        let area = self.health_area(
            actor,
            !matches!(
                action,
                HealthAction::AcceptTreatment { .. }
                    | HealthAction::RefuseTreatment { .. }
                    | HealthAction::RequestTreatment { .. }
            ),
        )?;
        self.ensure_event_capacity(12)?;
        match *action {
            HealthAction::Drink { source } => {
                if !self.health_stock(actor, source, Good::Water, rules.drink_quantity) {
                    return Err(Error::Unavailable);
                }
                self.preflight_water_received(actor, source, rules.drink_quantity)?;
                self.start_health_job(
                    actor,
                    area,
                    rules.drinking_time,
                    HealthWorkKind::Drink,
                    Work::Drink {
                        source,
                        quantity: rules.drink_quantity,
                    },
                )?;
            }
            HealthAction::Boil { source, fuel, tool } => {
                if !self.health_stock(actor, source, Good::Water, rules.drink_quantity)
                    || !self.health_stock(actor, fuel, Good::Timber, rules.boiling_fuel)
                    || !self.health_stock(actor, tool, Good::Tool, Quantity(1_000))
                    || !self.tool_available_for(actor, tool)
                {
                    return Err(Error::Unavailable);
                }
                self.state
                    .time
                    .0
                    .checked_add(rules.boiling_time.0)
                    .ok_or(Error::Overflow)?;
                self.start_health_job(
                    actor,
                    area,
                    rules.boiling_time,
                    HealthWorkKind::Boil,
                    Work::Boil {
                        source,
                        quantity: rules.drink_quantity,
                        tool,
                    },
                )?;
                self.consume_health_stock(actor, fuel, rules.boiling_fuel);
            }
            HealthAction::BuildShelter {
                timber,
                tool,
                capacity,
                insulation,
            } => {
                let quantity = shelter_cost(&rules, capacity, insulation)?;
                if !self.health_stock(actor, timber, Good::Timber, quantity)
                    || !self.health_stock(actor, tool, Good::Tool, Quantity(1_000))
                    || !self.tool_available_for(actor, tool)
                {
                    return Err(Error::Unavailable);
                }
                self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
                let duration = SimDuration(
                    rules
                        .shelter_time_per_place
                        .0
                        .checked_mul(u64::from(capacity))
                        .ok_or(Error::Overflow)?,
                );
                self.start_health_job(
                    actor,
                    area,
                    duration,
                    HealthWorkKind::Shelter,
                    Work::Shelter {
                        timber_used: quantity,
                        tool,
                        capacity,
                        insulation,
                        started: self.state.time,
                    },
                )?;
                self.consume_health_stock(actor, timber, quantity);
            }
            HealthAction::ShelterAccess {
                shelter,
                person,
                allowed,
            } => {
                let s = self
                    .state
                    .health
                    .as_ref()
                    .unwrap()
                    .shelters
                    .get(&shelter)
                    .ok_or(Error::Unavailable)?;
                if s.custodian != actor
                    || s.area != area
                    || !self.person_index.contains_key(&person)
                    || person == actor
                {
                    return Err(Error::Unavailable);
                }
                let s = self
                    .state
                    .health
                    .as_mut()
                    .unwrap()
                    .shelters
                    .get_mut(&shelter)
                    .unwrap();
                if allowed {
                    s.access.insert(person);
                } else {
                    s.access.remove(&person);
                }
                // Acknowledged access cannot teleport someone out of an occupied building.
                self.emit(EventKind::ShelterAccessChanged {
                    shelter,
                    actor,
                    person,
                    allowed,
                });
            }
            HealthAction::EnterShelter { shelter } => {
                let w = self.state.health.as_ref().unwrap();
                let s = w.shelters.get(&shelter).ok_or(Error::Unavailable)?;
                if s.area != area
                    || !s.access.contains(&actor)
                    || w.occupancy.values().filter(|id| **id == shelter).count()
                        >= s.capacity as usize
                    || w.occupancy.get(&actor) == Some(&shelter)
                {
                    return Err(Error::Unavailable);
                }
                self.leave_shelter_on_movement(actor);
                self.interrupt_actor_activity(actor);
                self.state
                    .health
                    .as_mut()
                    .unwrap()
                    .occupancy
                    .insert(actor, shelter);
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .activity = Activity::Rest;
                self.emit(EventKind::ShelterEntered { shelter, actor });
            }
            HealthAction::LeaveShelter => {
                if !self
                    .state
                    .health
                    .as_ref()
                    .unwrap()
                    .occupancy
                    .contains_key(&actor)
                {
                    return Err(Error::Unavailable);
                }
                self.leave_shelter_on_movement(actor);
            }
            HealthAction::RequestTreatment { caregiver } => {
                if caregiver == actor
                    || self.health_area(caregiver, true).ok() != Some(area)
                    || !self.can_observe_health(actor, caregiver)
                {
                    return Err(Error::Unavailable);
                }
                if let Some((herbs, water)) = self.willing_health_supplies(caregiver, actor) {
                    self.health_command(
                        caregiver,
                        &HealthAction::OfferTreatment {
                            patient: actor,
                            herbs,
                            water,
                        },
                    )?;
                } else {
                    self.emit(EventKind::TreatmentRefused {
                        caregiver,
                        patient: actor,
                    });
                }
            }
            HealthAction::OfferTreatment {
                patient,
                herbs,
                water,
            } => {
                if patient == actor
                    || self.health_area(patient, false).ok() != Some(area)
                    || !self.can_observe_health(actor, patient)
                    || self.medical_level(actor) == 0
                    || !self.health_stock(actor, herbs, Good::Herb, rules.treatment_herbs)
                    || !self.health_stock(actor, water, Good::Water, rules.treatment_water)
                {
                    return Err(Error::Unavailable);
                }
                let expires = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(rules.offer_duration.0)
                        .ok_or(Error::Overflow)?,
                );
                self.state.health.as_mut().unwrap().offers.insert(
                    (actor, patient),
                    Offer {
                        caregiver: actor,
                        patient,
                        herbs,
                        water,
                        expires,
                        accepted: false,
                    },
                );
                self.emit(EventKind::TreatmentOffered {
                    caregiver: actor,
                    patient,
                });
            }
            HealthAction::AcceptTreatment { caregiver } => {
                let offer = self
                    .state
                    .health
                    .as_ref()
                    .unwrap()
                    .offers
                    .get(&(caregiver, actor))
                    .ok_or(Error::Unavailable)?
                    .clone();
                if offer.accepted
                    || offer.expires <= self.state.time
                    || self.health_area(caregiver, true).ok() != Some(area)
                    || !self.can_observe_health(actor, caregiver)
                {
                    return Err(Error::Unavailable);
                }
                // Acceptance names this caregiver and these supplies, rather than giving a permanent mandate.
                self.interrupt_actor_activity(actor);
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .activity = Activity::Rest;
                self.state.health.as_mut().unwrap().offers.insert(
                    (caregiver, actor),
                    Offer {
                        accepted: true,
                        ..offer
                    },
                );
                self.emit(EventKind::TreatmentAccepted {
                    caregiver,
                    patient: actor,
                });
            }
            HealthAction::RefuseTreatment { caregiver } => {
                if self
                    .state
                    .health
                    .as_mut()
                    .unwrap()
                    .offers
                    .remove(&(caregiver, actor))
                    .is_none()
                {
                    return Err(Error::Unavailable);
                }
                self.cancel_health_activity(actor);
                self.emit(EventKind::TreatmentRefused {
                    caregiver,
                    patient: actor,
                });
            }
            HealthAction::Treat {
                patient,
                herbs,
                water,
            } => {
                if self.health_area(patient, false).ok() != Some(area)
                    || !self.can_observe_health(actor, patient)
                    || self.medical_level(actor) == 0
                    || !self.health_stock(actor, herbs, Good::Herb, rules.treatment_herbs)
                    || !self.health_stock(actor, water, Good::Water, rules.treatment_water)
                    || self.health_patient_resting(patient)
                {
                    return Err(Error::Unavailable);
                }
                if patient != actor
                    && !self
                        .state
                        .health
                        .as_ref()
                        .unwrap()
                        .offers
                        .get(&(actor, patient))
                        .is_some_and(|o| {
                            o.accepted
                                && o.herbs == herbs
                                && o.water == water
                                && o.expires > self.state.time
                        })
                {
                    return Err(Error::Unavailable);
                }
                if patient != actor
                    && self.state.physical.as_ref().unwrap().actors[&patient].activity
                        != Activity::Rest
                {
                    return Err(Error::Unavailable);
                }
                let until = self
                    .state
                    .time
                    .0
                    .checked_add(rules.treatment_time.0)
                    .and_then(|t| t.checked_add(rules.treatment_duration.0))
                    .ok_or(Error::Overflow)?;
                let _ = until;
                self.preflight_water_received(patient, water, rules.treatment_water)?;
                let dose = self.water_dose(water, rules.treatment_water);
                self.start_health_job(
                    actor,
                    area,
                    rules.treatment_time,
                    HealthWorkKind::Treat,
                    Work::Treat {
                        patient,
                        skill_ms: 0,
                        elapsed: 0,
                    },
                )?;
                self.consume_health_stock(actor, herbs, rules.treatment_herbs);
                self.consume_health_stock(patient, water, rules.treatment_water);
                self.received_water(patient, water, rules.treatment_water, dose)?;
            }
        }
        self.observe_health();
        Ok(())
    }
    fn preflight_water_received(
        &self,
        patient: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        let _ = self.water_dose(source, quantity);
        let _ = patient;
        let r = &self.state.health.as_ref().unwrap().rules;
        self.state
            .time
            .0
            .checked_add(r.incubation.0)
            .and_then(|t| t.checked_add(r.illness_duration.0))
            .ok_or(Error::Overflow)?;
        Ok(())
    }
}

impl Simulation {
    fn effective_shelter(&self, actor: PersonId) -> Option<&Shelter> {
        let w = self.state.health.as_ref()?;
        let shelter = w.shelters.get(w.occupancy.get(&actor)?)?;
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive
            || self.state.people[self.person_index[&actor]].location
                != Location::AtArea(shelter.area)
            || matches!(
                body.activity,
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
            || self.state.people[self.person_index[&a]].location
                != self.state.people[self.person_index[&b]].location
        {
            return false;
        }
        let (Location::AtArea(pa), Location::AtArea(pb)) = (
            &self.state.people[self.person_index[&a]].location,
            &self.state.people[self.person_index[&b]].location,
        ) else {
            return false;
        };
        if pa != pb {
            return false;
        }
        let sa = self.effective_shelter(a).map(|s| s.id);
        let sb = self.effective_shelter(b).map(|s| s.id);
        if sa.is_some() && sa == sb {
            return true;
        }
        let w = self.state.health.as_ref().unwrap();
        if w.jobs.values().any(|job|matches!(job.work,Work::Treat {patient,..} if job.actor==a&&patient==b||job.actor==b&&patient==a)){return true;}
        if sa != sb {
            return false;
        }
        let physical = self.state.physical.as_ref().unwrap();
        for (giver, receiver) in [(a, b), (b, a)] {
            match physical.actors[&giver].activity {
                Activity::Care { child, .. } | Activity::Comfort { child, .. }
                    if child == receiver =>
                {
                    return true;
                }
                Activity::Tell { recipient, .. } if recipient == receiver => return true,
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
            temperature_c: self
                .state
                .environment
                .as_ref()
                .map_or(12, |w| w.weather.temperature),
            insulation: self.effective_shelter(actor).map_or(0, |s| s.insulation),
            // Body contributions use weather/shelter, never contact doses.
            infectious_contacts: [0; 2],
        }
    }
    fn health_locations(&self) -> BTreeMap<PersonId, HealthCohort> {
        self.state
            .people
            .iter()
            .filter_map(|person| {
                if let Location::AtArea(area) = person.location {
                    Some((
                        person.id,
                        (area, self.effective_shelter(person.id).map(|s| s.id)),
                    ))
                } else {
                    None
                }
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
                let state = world.clinical.get(&person.id);
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
        for (actor, (_, shelter)) in &locations {
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
                        .state
                        .environment
                        .as_ref()
                        .map_or(12, |w| w.weather.temperature),
                    insulation: locations
                        .get(&person.id)
                        .and_then(|(_, shelter)| *shelter)
                        .map_or(0, |id| world.shelters[&id].insulation),
                    infectious_contacts: [0; 2],
                };
                if let Some((_, Some(shelter))) = locations.get(&person.id) {
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
            let (Some((area_a, shelter_a)), Some((area_b, shelter_b))) =
                (locations.get(&a), locations.get(&b))
            else {
                return;
            };
            if area_a != area_b
                || (!treatment && shelter_a != shelter_b)
                || (shelter_a.is_some() && shelter_a == shelter_b)
            {
                return;
            }
            pairs.insert(if a < b { (a, b) } else { (b, a) });
        };
        for job in world.jobs.values() {
            if let Work::Treat { patient, .. } = job.work {
                add_pair(job.actor, patient, true);
            }
        }
        for (actor, body) in &self.state.physical.as_ref().unwrap().actors {
            match body.activity {
                Activity::Care { child, .. } | Activity::Comfort { child, .. } => {
                    add_pair(*actor, child, false)
                }
                Activity::Tell { recipient, .. } => add_pair(*actor, recipient, false),
                _ => {}
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
            alive: self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .alive,
            thirst_enabled: !self.state.people[self.person_index[&actor]].hunger_exempt,
            resting: self.state.physical.as_ref().unwrap().actors[&actor].activity
                == Activity::Rest,
            exposure,
        }
    }
    pub(crate) fn health_body_delta(
        &self,
        actor: PersonId,
        elapsed: u64,
    ) -> Result<HealthDelta, Error> {
        let Some(w) = &self.state.health else {
            return Ok(HealthDelta::default());
        };
        let empty = Clinical::default();
        let state = w.clinical.get(&actor).unwrap_or(&empty);
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
        let Some(w) = &self.state.health else {
            return Ok(PendingHealth {
                clinical: BTreeMap::new(),
                jobs: BTreeMap::new(),
            });
        };
        let elapsed = next
            .0
            .checked_sub(self.state.time.0)
            .ok_or(Error::Overflow)?;
        let mut clinical = w.clinical.clone();
        let exposures = self.indexed_health_exposures();
        let mut jobs = BTreeMap::new();
        for person in &self.state.people {
            if !self.state.physical.as_ref().unwrap().actors[&person.id]
                .vitals
                .alive
            {
                continue;
            }
            let empty = Clinical::default();
            let state = w.clinical.get(&person.id).unwrap_or(&empty);
            clinical.insert(
                person.id,
                game_health::advance_clinical(self.clinical_input(
                    person.id,
                    state,
                    next,
                    exposures[&person.id],
                ))?,
            );
        }
        for (actor, job) in &w.jobs {
            jobs.insert(
                *actor,
                game_health::advance_job(
                    job,
                    elapsed,
                    self.health_job_can_progress(job),
                    self.medical_level(*actor),
                )?,
            );
        }
        Ok(PendingHealth { clinical, jobs })
    }
    pub(crate) fn health_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        let Some(w) = &self.state.health else {
            return Ok(limit);
        };
        let mut boundary = limit;
        let exposures = self.indexed_health_exposures();
        for job in w.jobs.values() {
            if self.health_job_can_progress(job) && job.completes > self.state.time {
                boundary = boundary.min(job.completes);
            }
        }
        // Domain thresholds constrain the interval. The coordinator separately
        // finds combined body limits using game-physiology.
        for person in &self.state.people {
            let empty = Clinical::default();
            let state = w.clinical.get(&person.id).unwrap_or(&empty);
            boundary = boundary.min(game_health::clinical_boundary(
                self.clinical_input(person.id, state, limit, exposures[&person.id]),
                limit,
            )?);
        }
        Ok(boundary)
    }
    pub(crate) fn commit_health(&mut self, pending: PendingHealth) -> Result<(), Error> {
        let Some(w) = &mut self.state.health else {
            return Ok(());
        };
        w.clinical = pending.clinical;
        let live_jobs: BTreeSet<_> = w.jobs.keys().copied().collect();
        w.jobs = pending
            .jobs
            .into_iter()
            .filter(|(actor, _)| {
                live_jobs.contains(actor)
                    && self.state.physical.as_ref().unwrap().actors[actor]
                        .vitals
                        .alive
            })
            .collect();
        for (actor, job) in &w.jobs {
            if let Activity::HealthWork { kind, .. } =
                self.state.physical.as_ref().unwrap().actors[actor].activity
            {
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(actor)
                    .unwrap()
                    .activity = Activity::HealthWork {
                    kind,
                    completes: job.completes,
                };
            }
        }
        Ok(())
    }
    pub(crate) fn reconcile_health(
        &mut self,
        closing_equipment: &BTreeMap<PersonId, bool>,
    ) -> Result<(), Error> {
        let Some(w) = &self.state.health else {
            return Ok(());
        };
        self.ensure_event_capacity(w.clinical.len() * 5 + w.jobs.len() * 5 + 2)?;
        let ids: Vec<_> = w.clinical.keys().copied().collect();
        for actor in ids {
            if !self.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .alive
            {
                self.cancel_health_activity(actor);
                continue;
            }
            let w = self.state.health.as_ref().unwrap();
            let transition = game_health::reconcile_clinical(
                &w.clinical[&actor],
                &w.rules,
                self.state.time,
                true,
            )?;
            self.state
                .health
                .as_mut()
                .unwrap()
                .clinical
                .insert(actor, transition.state);
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
        let due: Vec<_> = self
            .state
            .health
            .as_ref()
            .unwrap()
            .jobs
            .values()
            .filter(|job| job.completes <= self.state.time)
            .cloned()
            .collect();
        for job in due {
            let valid = self.health_area(job.actor, true).ok() == Some(job.area)
                && (!matches!(job.work, Work::Boil { .. } | Work::Shelter { .. })
                    || closing_equipment.get(&job.actor).copied().unwrap_or(false))
                && matches!(self.state.physical.as_ref().unwrap().actors[&job.actor].activity,Activity::HealthWork {completes,..} if completes==job.completes);
            self.state.health.as_mut().unwrap().jobs.remove(&job.actor);
            if !valid {
                continue;
            }
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&job.actor)
                .unwrap()
                .activity = Activity::Idle;
            match job.work {
                Work::Drink { source, quantity } => {
                    if !self.health_stock(job.actor, source, Good::Water, quantity) {
                        continue;
                    }
                    let dose = self.water_dose(source, quantity);
                    self.consume_health_stock(job.actor, source, quantity);
                    self.received_water(job.actor, source, quantity, dose)?;
                    self.emit(EventKind::Drank {
                        actor: job.actor,
                        source,
                        quantity,
                    });
                }
                Work::Boil {
                    source,
                    quantity,
                    tool: _,
                } => {
                    if !self.health_stock(job.actor, source, Good::Water, quantity) {
                        continue;
                    }
                    let before = self.state.inventories[&source].quantity.0;
                    let lot = self
                        .state
                        .health
                        .as_ref()
                        .unwrap()
                        .water
                        .get(&source)
                        .cloned()
                        .unwrap_or_else(|| WaterLot::new(before, 0));
                    let effect = game_health::purify_water(&lot, quantity.0)?;
                    let load = effect.remaining.load();
                    self.state
                        .health
                        .as_mut()
                        .unwrap()
                        .water
                        .insert(source, effect.remaining);
                    let safety = WaterSafetyView {
                        inventory: source,
                        purified: load == 0,
                        suspected_unsafe: load > 0,
                        observed_at: self.state.time,
                    };
                    self.state
                        .health
                        .as_mut()
                        .unwrap()
                        .known
                        .entry(job.actor)
                        .or_default()
                        .water
                        .insert(source, safety);
                    self.emit(EventKind::WaterPurified {
                        actor: job.actor,
                        source,
                    });
                }
                Work::Shelter {
                    timber_used,
                    tool: _,
                    capacity,
                    insulation,
                    started,
                } => {
                    let id = ShelterId(allocate(&mut self.state)?);
                    let s = Shelter {
                        id,
                        area: job.area,
                        custodian: job.actor,
                        access: BTreeSet::from([job.actor]),
                        capacity,
                        insulation,
                        built_at: self.state.time,
                        timber_used,
                        labor_ms: self.state.time.0 - started.0,
                    };
                    self.state.health.as_mut().unwrap().shelters.insert(id, s);
                    self.emit(EventKind::ShelterBuilt {
                        shelter: id,
                        builder: job.actor,
                    });
                }
                Work::Treat {
                    patient,
                    skill_ms,
                    elapsed,
                } => {
                    if self.health_area(patient, false).ok() != Some(job.area)
                        || self.state.physical.as_ref().unwrap().actors[&patient].activity
                            != Activity::Rest
                            && patient != job.actor
                    {
                        continue;
                    }
                    let course = game_health::treatment_course(
                        &self.state.health.as_ref().unwrap().rules,
                        self.state.time,
                        skill_ms,
                        elapsed,
                    )?;
                    self.state
                        .health
                        .as_mut()
                        .unwrap()
                        .clinical
                        .entry(patient)
                        .or_default()
                        .treatment = Some(course);
                    self.emit(EventKind::TreatmentCompleted {
                        caregiver: job.actor,
                        patient,
                    });
                }
            }
        }
        self.state
            .health
            .as_mut()
            .unwrap()
            .offers
            .retain(|_, offer| offer.expires > self.state.time);
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
        if self.state.people[*self.person_index.get(&person).ok_or(Error::Unavailable)?].location
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
    fn can_observe_health(&self, observer: PersonId, subject: PersonId) -> bool {
        self.state.people[self.person_index[&observer]].location
            == self.state.people[self.person_index[&subject]].location
            && matches!(
                self.state.people[self.person_index[&observer]].location,
                Location::AtArea(_)
            )
            && self.effective_shelter(observer).map(|s| s.id)
                == self.effective_shelter(subject).map(|s| s.id)
    }
    fn visible_symptoms(&self, person: PersonId) -> SymptomsView {
        let state = self
            .state
            .health
            .as_ref()
            .and_then(|w| w.clinical.get(&person));
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
            wounded: self.state.physical.as_ref().unwrap().actors[&person]
                .vitals
                .health
                < self.state.physical.as_ref().unwrap().rules.max_health / 2,
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
                (
                    *cohort,
                    actors
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
        for (actor, shelter) in &world.occupancy {
            if let Some(location) = locations.get(actor) {
                occupants
                    .entry((*shelter, *location))
                    .or_default()
                    .insert(*actor);
            }
        }
        let observers: Vec<_> = self
            .state
            .people
            .iter()
            .filter(|person| {
                self.state.physical.as_ref().unwrap().actors[&person.id]
                    .vitals
                    .alive
                    && matches!(person.location, Location::AtArea(_))
            })
            .map(|person| person.id)
            .collect();
        for observer in observers {
            let area = match self.state.people[self.person_index[&observer]].location {
                Location::AtArea(area) => area,
                _ => continue,
            };
            let cohort = locations[&observer];
            let people = &symptoms[&cohort];
            let shelters: Vec<_> = area_shelters
                .get(&area)
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
        let clinical = w.clinical.get(&actor).cloned().unwrap_or_default();
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
                && self.health_area(o.caregiver, true).ok() == Some(area)
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
            let mind = self
                .state
                .decisions
                .as_ref()
                .and_then(|world| world.minds.get(&actor));
            if let Some(mind) = mind {
                for stock in mind.knowledge.values().filter(|stock| {
                    stock.good == Good::Water
                        && stock.accessible
                        && stock.area == area
                        && stock.quantity >= w.rules.drink_quantity
                }) {
                    actions.push(HealthAction::Drink {
                        source: stock.inventory,
                    });
                }
            }
            for shelter in w
                .known
                .get(&actor)
                .into_iter()
                .flat_map(|k| k.shelters.values())
                .filter(|s| {
                    s.area == area
                        && s.accessible
                        && w.occupancy.get(&actor) != Some(&s.id)
                        && s.known_occupants.len() < s.capacity as usize
                })
            {
                actions.push(HealthAction::EnterShelter {
                    shelter: shelter.id,
                });
            }
            if w.occupancy.contains_key(&actor) {
                actions.push(HealthAction::LeaveShelter);
            }
        }
        actions
            .into_iter()
            .map(|action| Command::Health { action })
            .collect()
    }
    pub(crate) fn health_candidate(&self, actor: PersonId) -> Option<Command> {
        let w = self.state.health.as_ref()?;
        let clinical = w.clinical.get(&actor).cloned().unwrap_or_default();
        let area = self.health_area(actor, true).ok()?;
        if w.jobs.contains_key(&actor) || self.health_patient_resting(actor) {
            return None;
        }
        let mind = self.state.decisions.as_ref()?.minds.get(&actor)?;
        let traits = self
            .invocation_personality(actor)
            .unwrap_or_else(|| mind.personality.clone());
        if clinical.thirst >= w.rules.drink_relief / 4 {
            let stock = mind
                .knowledge
                .values()
                .filter(|s| {
                    s.good == Good::Water
                        && s.accessible
                        && s.area == area
                        && s.quantity >= w.rules.drink_quantity
                })
                .min_by_key(|s| s.inventory)?;
            let safety = w
                .known
                .get(&actor)
                .and_then(|k| k.water.get(&stock.inventory));
            if !safety.is_some_and(|s| s.purified)
                && (safety.is_some_and(|s| s.suspected_unsafe) || traits.risk_tolerance < 500)
            {
                let fuel = mind.knowledge.values().find(|s| {
                    s.good == Good::Timber
                        && s.area == area
                        && s.accessible
                        && s.quantity >= w.rules.boiling_fuel
                });
                let tool = mind.knowledge.values().find(|s| {
                    s.good == Good::Tool
                        && s.area == area
                        && s.accessible
                        && s.quantity >= Quantity(1_000)
                });
                if let (Some(fuel), Some(tool)) = (fuel, tool) {
                    if stock.quantity > w.rules.drink_quantity {
                        return Some(Command::Carry {
                            source: stock.inventory,
                            quantity: w.rules.drink_quantity,
                        });
                    }
                    return Some(Command::Health {
                        action: HealthAction::Boil {
                            source: stock.inventory,
                            fuel: fuel.inventory,
                            tool: tool.inventory,
                        },
                    });
                }
            }
            return Some(Command::Health {
                action: HealthAction::Drink {
                    source: stock.inventory,
                },
            });
        }
        if clinical.cold >= w.rules.cold_harm / 4
            && self.effective_shelter(actor).is_none()
            && let Some(shelter) = w
                .known
                .get(&actor)
                .into_iter()
                .flat_map(|k| k.shelters.values())
                .filter(|s| {
                    s.area == area && s.accessible && s.known_occupants.len() < s.capacity as usize
                })
                .max_by_key(|s| (s.insulation, s.id))
        {
            return Some(Command::Health {
                action: HealthAction::EnterShelter {
                    shelter: shelter.id,
                },
            });
        }
        None
    }
    pub(crate) fn health_actor_busy(&self, actor: PersonId) -> bool {
        self.state
            .health
            .as_ref()
            .is_some_and(|w| w.jobs.contains_key(&actor) || self.health_patient_resting(actor))
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
            .health
            .as_ref()
            .unwrap()
            .jobs
            .values()
            .cloned()
            .collect();
        for job in jobs {
            if job.actor == self.state.ruler && !self.controls_actor(job.actor)
                || matches!(job.work, Work::Drink { .. })
            {
                continue;
            }
            let body = &self.state.physical.as_ref().unwrap().actors[&job.actor];
            let physical = &self.state.physical.as_ref().unwrap().rules;
            if body.vitals.fatigue >= physical.fatigue_limit / 2
                || body.vitals.hunger >= physical.meal_relief
                || body.vitals.health <= physical.max_health / 3
            {
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
            if offer.patient == self.state.ruler && !self.controls_actor(offer.patient)
                || offer.accepted
                || self.health_area(offer.patient, false).ok()
                    != self.health_area(offer.caregiver, true).ok()
            {
                continue;
            }
            let Some(patient) = self.health_mind(offer.patient) else {
                continue;
            };
            let symptoms = self.visible_symptoms(offer.patient);
            let want = symptoms.fever || symptoms.wounded || symptoms.shivering;
            let trust = patient
                .relationships
                .get(&offer.caregiver)
                .map_or(0, |r| r.trust);
            let action = if want && trust >= -100 {
                HealthAction::AcceptTreatment {
                    caregiver: offer.caregiver,
                }
            } else {
                HealthAction::RefuseTreatment {
                    caregiver: offer.caregiver,
                }
            };
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
            let body = &self.state.physical.as_ref().unwrap().actors[&caregiver];
            let rules = &self.state.health.as_ref().unwrap().rules;
            if mind.personality.empathy < 300
                || body.vitals.hunger >= self.state.physical.as_ref().unwrap().rules.meal_relief
                || body.vitals.fatigue
                    >= self.state.physical.as_ref().unwrap().rules.fatigue_limit / 2
            {
                continue;
            }
            let area = self.health_area(caregiver, true)?;
            let herbs = mind.knowledge.values().find(|s| {
                s.good == Good::Herb
                    && s.accessible
                    && s.area == area
                    && s.quantity >= rules.treatment_herbs
            });
            let water = mind.knowledge.values().find(|s| {
                s.good == Good::Water
                    && s.accessible
                    && s.area == area
                    && s.quantity >= rules.treatment_water
            });
            let (Some(herbs), Some(water)) = (herbs, water) else {
                continue;
            };
            let herbs = herbs.inventory;
            let water = water.inventory;
            let patient = self
                .state
                .health
                .as_ref()
                .unwrap()
                .known
                .get(&caregiver)
                .into_iter()
                .flat_map(|k| k.people.values())
                .filter(|s| {
                    s.person != caregiver
                        && (s.fever || s.wounded || s.shivering)
                        && self.can_observe_health(caregiver, s.person)
                        && !self
                            .state
                            .health
                            .as_ref()
                            .unwrap()
                            .offers
                            .contains_key(&(caregiver, s.person))
                        && !self.health_patient_resting(s.person)
                })
                .min_by_key(|s| s.person)
                .map(|s| s.person);
            if let Some(patient) = patient {
                let _ = self.health_command(
                    caregiver,
                    &HealthAction::OfferTreatment {
                        patient,
                        herbs,
                        water,
                    },
                );
            }
        }
        Ok(())
    }
    pub(crate) fn health_allocation_bound(&self, next: SimTime) -> u64 {
        self.state.health.as_ref().map_or(0, |w| {
            w.jobs
                .values()
                .filter(|job| job.completes <= next && matches!(job.work, Work::Shelter { .. }))
                .count() as u64
        })
    }
    pub(crate) fn health_event_bound(&self) -> usize {
        self.state.health.as_ref().map_or(0, |w| {
            w.clinical.len() * 8 + w.jobs.len() * 8 + w.offers.len() * 4
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
        if self.health_area(job.actor, true).ok() != Some(job.area) {
            return false;
        }
        if !matches!(job.work, Work::Drink { .. })
            && self.state.physical.as_ref().unwrap().actors[&job.actor]
                .vitals
                .fatigue
                >= self.state.physical.as_ref().unwrap().rules.fatigue_limit
        {
            return false;
        }
        match job.work {
            Work::Boil { tool, .. } | Work::Shelter { tool, .. } => {
                self.tool_available_for(job.actor, tool)
            }
            Work::Treat { patient, .. } => {
                self.health_area(patient, false).ok() == Some(job.area)
                    && self.can_observe_health(job.actor, patient)
                    && (patient == job.actor
                        || self.state.physical.as_ref().unwrap().actors[&patient].activity
                            == Activity::Rest)
            }
            Work::Drink { .. } => true,
        }
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
                self.state
                    .health
                    .as_mut()
                    .unwrap()
                    .clinical
                    .entry(child)
                    .or_default();
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
                    .health
                    .as_ref()
                    .unwrap()
                    .clinical
                    .get(&actor)
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
        let job = self.state.health.as_ref()?.jobs.get(&actor)?;
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
        if self.medical_level(giver) == 0 || self.health_actor_busy(giver) {
            return None;
        }
        let area = self.health_area(giver, true).ok()?;
        let mind = &self.state.decisions.as_ref()?.minds[&giver];
        let body = &self.state.physical.as_ref()?.actors[&giver];
        let r = &self.state.health.as_ref()?.rules;
        if body.vitals.hunger >= self.state.physical.as_ref()?.rules.meal_relief
            || body.vitals.fatigue >= self.state.physical.as_ref()?.rules.fatigue_limit / 2
            || mind.child_signals.values().any(|s| s.urgency >= 20_000)
        {
            return None;
        }
        let goodwill = i64::from(mind.personality.empathy)
            + i64::from(mind.relationships.get(&patient).map_or(0, |v| v.trust));
        if goodwill < 300 {
            return None;
        }
        let herbs = mind
            .knowledge
            .values()
            .find(|s| {
                s.good == Good::Herb
                    && s.accessible
                    && s.area == area
                    && s.quantity >= r.treatment_herbs
            })?
            .inventory;
        let water = mind
            .knowledge
            .values()
            .find(|s| {
                s.good == Good::Water
                    && s.accessible
                    && s.area == area
                    && s.quantity >= r.treatment_water
            })?
            .inventory;
        Some((herbs, water))
    }
}

impl Simulation {
    fn health_mind(&self, actor: PersonId) -> Option<&decisions::Mind> {
        if self.controls_actor(actor) {
            let w = self.state.invocation.as_ref()?;
            let active = w.active.as_ref()?;
            let life = active.dominant?;
            return Some(&w.history.get(&life)?.mind);
        }
        self.state.decisions.as_ref()?.minds.get(&actor)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
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
            .credit(actor, Skill::Medicine, 90_000, None);
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
            whole.state.health.as_ref().unwrap().clinical[&actor]
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
        let before = sim.state.physical.as_ref().unwrap().actors[&actor]
            .vitals
            .health;
        sim.advance(SimDuration(1_000)).unwrap();
        assert!(
            sim.state.physical.as_ref().unwrap().actors[&actor]
                .vitals
                .health
                < before
        );
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
            .credit(actor, Skill::Medicine, 90_000, None);
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
        let current = whole.state.physical.as_ref().unwrap().actors[&actor]
            .vitals
            .health;
        let threshold = whole.state.combat.as_ref().unwrap().rules.incapacitation;
        whole
            .debug_injure(actor, current - (threshold + 5_000))
            .unwrap();
        whole
            .state
            .health
            .as_mut()
            .unwrap()
            .clinical
            .get_mut(&actor)
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
            whole
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Medicine),
            95_000
        );
        assert!(matches!(
            whole.state.health.as_ref().unwrap().jobs[&actor].work,
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
        sim.state.people[sim.person_index[&worker]].location = Location::AtArea(area);
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
            .physical
            .as_ref()
            .unwrap()
            .sites
            .values()
            .find(|site| site.area == area)
            .unwrap()
            .id;
        let output = sim.state.physical.as_ref().unwrap().sites[&site].output;
        let before = sim.state.inventories[&output].quantity;
        {
            let site = sim
                .state
                .physical
                .as_mut()
                .unwrap()
                .sites
                .get_mut(&site)
                .unwrap();
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
        sim.state
            .health
            .as_mut()
            .unwrap()
            .clinical
            .get_mut(&actor)
            .unwrap()
            .cold = 20_000;
        sim.observe_health();
        let expected = Command::Health {
            action: HealthAction::EnterShelter { shelter },
        };
        assert!(sim.available_health_commands(actor).contains(&expected));
        assert_eq!(sim.health_candidate(actor), Some(expected.clone()));
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
        assert_eq!(sim.health_candidate(actor), Some(expected.clone()));
        assert_eq!(
            sim.debug_actor_command(actor, expected),
            Err(Error::Unavailable)
        );
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;

    fn population_fixture(population: usize) -> Simulation {
        let sample = Simulation::health_fixture(71, Execution::default()).unwrap();
        let sample_world = sample.state.physical.as_ref().unwrap();
        let template = sample_world.actors[&sample.state.ruler].clone();
        let mut sim = Simulation::kernel_fixture(71, population, Execution::default()).unwrap();
        sim.state.physical = Some(physical::World {
            rules: sample_world.rules.clone(),
            actors: sim
                .state
                .people
                .iter()
                .map(|p| (p.id, template.clone()))
                .collect(),
            sites: BTreeMap::new(),
        });
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
                        .health
                        .as_ref()
                        .unwrap()
                        .clinical
                        .iter()
                        .filter(|(other, state)| {
                            state
                                .illnesses
                                .get(&kind)
                                .is_some_and(|i| i.symptomatic && i.recovers > sim.state.time)
                                && sim.in_health_contact(person.id, **other)
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
        for observer in sim.state.people.iter().filter(|p| {
            sim.state.physical.as_ref().unwrap().actors[&p.id]
                .vitals
                .alive
        }) {
            let Location::AtArea(area) = observer.location else {
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
            sim.state.health.as_mut().unwrap().jobs.clear();
            for (index, actor) in people.iter().enumerate() {
                let area = sim.state.areas[if index < 6 { 0 } else { (index + phase) % 3 }].id;
                sim.state.people[sim.person_index[actor]].location = if (index + phase) % 11 == 0 {
                    Location::InTransit {
                        from: area,
                        to: sim.state.areas[1].id,
                        departed: SimTime(0),
                        arrives: SimTime(100_000),
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
                let body = sim
                    .state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(actor)
                    .unwrap();
                body.vitals.alive = (index + phase) % 13 != 0;
                let recipient = people[(index + 1) % people.len()];
                body.activity = match (index + phase) % 6 {
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
                let state = sim
                    .state
                    .health
                    .as_mut()
                    .unwrap()
                    .clinical
                    .get_mut(actor)
                    .unwrap();
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
            sim.state.health.as_mut().unwrap().jobs.insert(
                people[0],
                Job {
                    actor: people[0],
                    area: sim.state.areas[0].id,
                    completes: SimTime(60_000),
                    work: Work::Treat {
                        patient: people[1],
                        skill_ms: 0,
                        elapsed: 0,
                    },
                },
            );
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
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&people[1])
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
            p.location = Location::AtArea(area);
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
