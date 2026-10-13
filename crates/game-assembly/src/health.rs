//! Water lots retain contaminant load across actual collection and transport.
//! Clinical effects are independent of custody and of who knows the water is unsafe.
use crate::*;

pub use game_health::*;
pub(crate) struct LocalHealthFacts {
    pub(crate) actors: Vec<game_health::LocalHealthActor<game_transport::PositionKey>>,
    interactions: Vec<game_health::HealthInteraction>,
    pub(crate) geometry: Vec<game_health::HealthGeometry>,
}
impl LocalHealthFacts {
    fn index<'a>(
        &'a self,
        sim: &'a Simulation,
    ) -> Result<game_health::LocalHealthIndex<'a, game_transport::PositionKey>, Error> {
        sim.host
            .state
            .health
            .as_ref()
            .ok_or(Error::Unavailable)?
            .prepare_local_index(
                &sim.host.storage,
                sim.host.clock().time,
                &self.actors,
                &self.interactions,
                &self.geometry,
            )
    }
}

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
        let stock = game_cargo::inventory(&self.host.storage, inventory).expect("inventory exists");
        assert_eq!(stock.good, Good::Water);
        let prepared = game_health::prepare_water_collection(
            &self.host.storage,
            game_health::WaterCollectionFacts {
                inventory,
                quantity_after: stock.quantity,
                collected: amount,
                contamination,
                holder: match stock.location {
                    InventoryLocation::Carried(actor) => Some(actor),
                    InventoryLocation::Area(_) => None,
                },
                now: self.host.clock().time,
            },
        )
        .expect("validated configured water collection");
        prepared
            .commit(&mut self.host.storage)
            .expect("registered water collection");
        self.host.state.health.get_or_insert_with(Runtime::default);
    }
    pub(crate) fn prepare_health_water_reaction(
        &self,
        reaction: game_health::WaterReaction,
    ) -> Result<Option<game_health::PreparedHealth>, Error> {
        self.host
            .state
            .health
            .as_ref()
            .map(|owner| {
                owner.prepare_water_reaction(&self.host.storage, self.host.clock().time, reaction)
            })
            .transpose()
    }
    pub(crate) fn update_water_event(&mut self, event: &EventKind) {
        let Some(owner) = &self.host.state.health else {
            return;
        };
        let reaction = match *event {
            EventKind::Transferred {
                actor,
                source,
                destination,
                quantity,
            } if game_cargo::inventory(&self.host.storage, source)
                .expect("inventory exists")
                .good
                == Good::Water =>
            {
                Some(game_health::WaterReaction::Transfer {
                    actor,
                    source,
                    destination,
                    quantity,
                    source_before: Quantity(
                        game_cargo::inventory(&self.host.storage, source)
                            .expect("inventory exists")
                            .quantity
                            .0
                            + quantity.0,
                    ),
                    destination_before: Quantity(
                        game_cargo::inventory(&self.host.storage, destination)
                            .expect("inventory exists")
                            .quantity
                            .0
                            - quantity.0,
                    ),
                })
            }
            EventKind::Consumed {
                source, quantity, ..
            }
            | EventKind::Cared {
                source, quantity, ..
            } if game_cargo::inventory(&self.host.storage, source)
                .expect("inventory exists")
                .good
                == Good::Water =>
            {
                Some(game_health::WaterReaction::Consume {
                    source,
                    quantity,
                    source_before: Quantity(
                        game_cargo::inventory(&self.host.storage, source)
                            .expect("inventory exists")
                            .quantity
                            .0
                            + quantity.0,
                    ),
                })
            }
            EventKind::Produced {
                actor,
                output,
                quantity,
                ..
            } if game_cargo::inventory(&self.host.storage, output)
                .expect("inventory exists")
                .good
                == Good::Water =>
            {
                Some(game_health::WaterReaction::Produce {
                    actor,
                    output,
                    quantity,
                    quantity_before: Quantity(
                        game_cargo::inventory(&self.host.storage, output)
                            .expect("inventory exists")
                            .quantity
                            .0
                            - quantity.0,
                    ),
                })
            }
            EventKind::Spoiled { inventory, .. }
                if game_cargo::inventory(&self.host.storage, inventory)
                    .expect("inventory exists")
                    .good
                    == Good::Water =>
            {
                Some(game_health::WaterReaction::Spoil { inventory })
            }
            _ => None,
        };
        if let Some(reaction) = reaction {
            let prepared = owner
                .prepare_water_reaction(&self.host.storage, self.host.clock().time, reaction)
                .expect("validated physical water change");
            prepared
                .commit(
                    self.host.state.health.as_mut().unwrap(),
                    &mut self.host.storage,
                )
                .expect("registered water reactions");
        }
    }
}
pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.state.health else {
        if game_health::clinicals(state.components).next().is_some()
            || game_health::jobs(state.components).next().is_some()
            || game_health::water_lots(state.components).next().is_some()
            || game_health::shelters(state.components).next().is_some()
            || game_health::occupancies(state.components).next().is_some()
            || game_health::knowledges(state.components).next().is_some()
            || game_health::offer_rows(state.components).next().is_some()
        {
            return Err(Error::InvalidSave(
                "health components require health runtime".into(),
            ));
        }
        return Ok(());
    };
    state
        .state
        .physical
        .as_ref()
        .ok_or_else(|| Error::InvalidSave("health requires physical bodies".into()))?;
    let locations = transport::resolved_locations_for(state)?;
    let people = state
        .people()
        .map(|person| {
            let body = game_physiology::body(state.components, person.id).ok_or_else(|| {
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
    let areas = state.state.areas.iter().map(|area| area.id).collect();
    let inventories = game_cargo::inventories(state.components)
        .map(|(id, stock)| {
            (
                id,
                InventoryCatalog {
                    good: stock.good,
                    quantity: stock.quantity,
                },
            )
        })
        .collect();
    game_health::validate_runtime(
        world,
        state.components,
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
        self.host
            .state
            .health
            .get_or_insert_with(Runtime::default)
            .rules = rules;
        for p_id in self.people().map(|person| person.id).collect::<Vec<_>>() {
            if game_health::clinical(&self.host.storage, p_id).is_none() {
                game_health::insert_clinical(&mut self.host.storage, p_id, Clinical::default())?;
            }
        }
        Ok(())
    }
    pub fn health_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::skills_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
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
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
            .ok_or(Error::Unavailable)?;
        game_health::local_action_area(
            game_health::LocalAgencyFacts {
                alive: body.vitals.alive,
                health: body.vitals.health,
                in_combat: self.in_combat(actor),
            },
            agency,
            || self.combat_restricts(actor),
            || self.independent(actor),
            || self.resolved_location(actor),
        )
    }
    fn medical_level(&self, actor: PersonId) -> u16 {
        game_skills::effective_level(
            self.own_skill_level(actor, Skill::Medicine),
            self.borrowed_skill_level(actor, Skill::Medicine),
        )
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
        game_cargo::inventory(&self.host.storage, id).is_some_and(|stock| {
            game_health::stock_usable(
                game_health::StockAccessFacts {
                    good: stock.good,
                    access: self.has_stock_access(actor, stock),
                    capable: self.health_area(actor, true).is_ok(),
                    place: match stock.location {
                        InventoryLocation::Area(area) => game_health::StockPlaceFacts::Area {
                            same_area: self.resolved_location(actor) == Location::AtArea(area),
                            actor_tactical: self.health_tactical_place(actor).is_some(),
                        },
                        InventoryLocation::Carried(holder) => {
                            game_health::StockPlaceFacts::Carried {
                                same_place: self.transport_same_place(actor, holder),
                                contact_clear: self.health_tactical_clear(
                                    self.health_tactical_place(actor),
                                    self.health_tactical_place(holder),
                                    true,
                                ),
                            }
                        }
                    },
                    available: Quantity(self.unreserved_stock(id, actor)),
                },
                good,
                quantity,
            )
        })
    }

    fn health_tactical_place(&self, actor: PersonId) -> Option<(EncounterId, PersonId)> {
        let (anchor, _) =
            game_spatial::resolved_anchor(&self.host.storage, game_ecs::StableId(actor.0))
                .expect("valid carrier chain");
        let anchor = PersonId(anchor.0);
        match *self.host.storage.activity(anchor).unwrap() {
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
        self.host
            .state
            .combat
            .as_ref()
            .map_or(a.is_none() && b.is_none(), |world| {
                world.tactical_clear(&self.host.storage, a, b, contact)
            })
    }

    fn consume_health_stock(&mut self, actor: PersonId, source: InventoryId, quantity: Quantity) {
        game_cargo::inventory_mut(&mut self.host.storage, source)
            .unwrap()
            .quantity
            .0 -= quantity.0;
        self.emit(EventKind::Consumed {
            actor,
            source,
            quantity,
        });
    }
    pub(crate) fn prepare_health_water_received(
        &self,
        patient: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<Option<game_health::PreparedHealth>, Error> {
        let Some(owner) = &self.host.state.health else {
            return Ok(None);
        };
        let stock = game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
        owner
            .prepare_water_received(
                &self.host.storage,
                patient,
                game_health::StockFacts {
                    id: source,
                    good: stock.good,
                    quantity: stock.quantity,
                    available: stock.quantity,
                    usable: true,
                    tool_available: false,
                },
                quantity,
                self.health_physiology(patient),
                self.host.clock().time,
            )
            .map(Some)
    }
    fn preflight_water_received(
        &self,
        patient: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        self.prepare_health_water_received(patient, source, quantity)
            .map(|_| ())
    }
    pub(crate) fn health_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        self.host.state.health.as_ref()?;
        game_health::job(&self.host.storage, actor)?.required_tool()
    }

    pub(crate) fn health_patient_resting(&self, actor: PersonId) -> bool {
        self.host
            .state
            .health
            .as_ref()
            .is_some_and(|w| w.patient_busy(&self.host.storage, actor))
    }
    pub(crate) fn health_work_exerting(&self, actor: PersonId) -> bool {
        self.host.state.health.as_ref().is_some_and(|owner| {
            owner.work_exerting(&self.host.storage, actor, |job| {
                self.health_job_can_progress(job)
            })
        })
    }

    #[cfg(test)]
    pub(crate) fn cancel_health_activity(&mut self, actor: PersonId) {
        if let Some(world) = &self.host.state.health {
            let change = world.prepare_cancel(&self.host.storage, actor);
            let _ = self.apply_health_change(change);
        }
    }
    fn health_actor_facts(&self, actor: PersonId) -> game_health::ActorFacts {
        if !self.person_index.contains_key(&actor) {
            return game_health::ActorFacts {
                actor,
                now: self.host.clock().time,
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
            now: self.host.clock().time,
            area,
            present: area.is_some(),
            capable: self.health_area(actor, true).is_ok(),
            at_area: matches!(self.resolved_location(actor), Location::AtArea(_)),
            carried: self.person_index.get(&actor).is_some_and(|index| {
                matches!(
                    *game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId((self.person_at(*index).id).0)
                    )
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
        let stock = game_cargo::inventory(&self.host.storage, id).ok_or(Error::Unavailable)?;
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
            game_health::clinical_physiology(
                &self.host.state.health.as_ref().unwrap().rules,
                1_000,
                0,
            )
            .unwrap()
        };
        game_health::PatientFacts {
            id: patient,
            present,
            contact: present
                && self.person_index.contains_key(&giver)
                && self.can_observe_health(giver, patient),
            resting: self
                .host
                .state
                .physical
                .as_ref()
                .and_then(|_| self.host.storage.activity(patient))
                .is_some_and(|activity| *activity == Activity::Rest),
            physiology,
        }
    }
    fn health_leaving_group(&self, actor: PersonId) -> Vec<PersonId> {
        game_transport::carrier_family(&self.host.storage, actor)
    }
    pub(crate) fn health_command(
        &mut self,
        actor: PersonId,
        action: &HealthAction,
    ) -> Result<(), Error> {
        use game_health::ActionFacts;
        if self.host.state.health.is_none() || !self.person_index.contains_key(&actor) {
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
        let prepared = self.host.state.health.as_ref().unwrap().prepare_action(
            &self.host.storage,
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
        let change = change.with_consumption_reactions(&self.host.storage, |id| {
            let stock = game_cargo::inventory(&self.host.storage, id).ok_or(Error::Unavailable)?;
            Ok((stock.good, stock.quantity))
        })?;
        let operations = change.operations(|actor| match self.host.storage.activity(actor) {
            Some(Activity::HealthWork { completes, .. }) => {
                game_health::HealthActivityState::Working {
                    completes: *completes,
                }
            }
            _ => game_health::HealthActivityState::Other,
        });
        let mut next_id = self.host.next_id();
        for effect in change.effects() {
            if let HealthEffect::ShelterBuilt { shelter, .. } = effect {
                if shelter.0 != next_id {
                    return Err(Error::Unavailable);
                }
                next_id = next_id.checked_add(1).ok_or(Error::Overflow)?;
            }
        }
        let interruptions =
            self.prepare_interruptions(&operations.interruptions, commands::ActivityOwner::Health)?;
        let cargo = game_cargo::prepare_consumptions(&self.host.storage, &operations.consumptions)?;
        self.ensure_event_capacity(
            change
                .effects()
                .len()
                .checked_add(
                    usize::try_from(interruptions.event_count()?).map_err(|_| Error::Overflow)?,
                )
                .ok_or(Error::Overflow)?,
        )?;
        let mut final_activities = interruptions
            .projected_activities()
            .map(|(actor, activity)| (actor, activity.clone()))
            .collect::<BTreeMap<_, _>>();
        final_activities.extend(operations.activities.into_iter().map(|(actor, activity)| {
            (
                actor,
                match activity {
                    game_health::HealthActivityChange::Idle => Activity::Idle,
                    game_health::HealthActivityChange::Work { kind, completes } => {
                        Activity::HealthWork { kind, completes }
                    }
                    game_health::HealthActivityChange::Rest => Activity::Rest,
                },
            )
        }));
        let mut batch = game_ecs::Batch::new();
        let finish = change.stage_into(
            self.host.state.health.as_ref().ok_or(Error::Unavailable)?,
            &self.host.storage,
            &mut batch,
        )?;
        let cargo_guard = cargo.stage_guarded_into(&self.host.storage, &mut batch)?;
        let mut interrupted = interruptions.stage_into(self, &mut batch, &final_activities)?;
        finish.validate_batch(
            self.host.state.health.as_ref().unwrap(),
            &self.host.storage,
            &batch,
        )?;
        cargo_guard.validate(&self.host.storage, &batch)?;
        interrupted.validate_batch(self, &batch)?;
        self.host.storage.apply_batch(batch)?;
        self.host.allocate_ids(
            next_id
                .checked_sub(self.host.next_id())
                .ok_or(Error::Overflow)?,
        )?;
        for actor in operations.interruptions {
            interrupted.finish_actor(self, actor);
        }
        let effects = finish.finish();
        for effect in effects {
            match effect {
                HealthEffect::Clinical { actor, event } => match event {
                    ClinicalEvent::Started(kind) => {
                        self.emit_prepared_health(EventKind::IllnessStarted { actor, kind })
                    }
                    ClinicalEvent::Recovered(kind) => {
                        self.emit_prepared_health(EventKind::IllnessRecovered { actor, kind })
                    }
                },
                HealthEffect::Interrupt { .. }
                | HealthEffect::ClearWork { .. }
                | HealthEffect::Start { .. }
                | HealthEffect::Rest { .. } => {}
                HealthEffect::Consume {
                    actor,
                    source,
                    quantity,
                } => self.emit_prepared_water(EventKind::Consumed {
                    actor,
                    source,
                    quantity,
                }),
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
    fn health_activity_facts(&self, actor: PersonId) -> game_health::HealthActivityFacts {
        let activity = self.host.storage.activity(actor);
        game_health::HealthActivityFacts {
            combat: matches!(activity, Some(Activity::Combat { .. })),
            cultivation: matches!(activity, Some(Activity::Cultivate { .. })),
            gathering: matches!(activity, Some(Activity::Gather { .. })),
            construction: matches!(activity, Some(Activity::Construct { .. })),
            play: matches!(activity, Some(Activity::Play { .. })),
        }
    }
    fn health_shelter_actor(&self, actor: PersonId) -> Option<game_health::ShelterActorFacts> {
        let location = game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))?;
        Some(game_health::ShelterActorFacts {
            actor,
            carrier: match location {
                Location::Carried { carrier } => Some(*carrier),
                _ => None,
            },
            alive: game_physiology::body(&self.host.storage, actor)?
                .vitals
                .alive,
            physical_area: match self.resolved_location(actor) {
                Location::AtArea(area) => Some(area),
                _ => None,
            },
            activity: self.health_activity_facts(actor),
        })
    }
    pub(crate) fn effective_shelter(&self, actor: PersonId) -> Option<&Shelter> {
        let id = self
            .host
            .state
            .health
            .as_ref()?
            .effective_shelter(&self.host.storage, actor, |id| {
                self.health_shelter_actor(id)
            })
            .expect("valid shelter chain")?;
        game_health::shelter(&self.host.storage, id)
    }
    fn health_local_actor(
        &self,
        actor: PersonId,
        resolved: &Location,
    ) -> game_health::LocalHealthActor<game_transport::PositionKey> {
        let body = game_physiology::body(&self.host.storage, actor).expect("body exists");
        game_health::LocalHealthActor {
            actor,
            place: transport::location_position(resolved, self.host.clock().time)
                .ok()
                .map(|position| position.key()),
            physical_area: match resolved {
                Location::AtArea(area) => Some(*area),
                _ => None,
            },
            carrier: match game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))
                .expect("location exists")
            {
                Location::Carried { carrier } => Some(*carrier),
                _ => None,
            },
            tactical: self
                .health_tactical_place(actor)
                .map(|(encounter, anchor)| game_health::TacticalIdentity { encounter, anchor }),
            alive: body.vitals.alive,
            moving: self.transport_is_moving(actor),
            activity: self.health_activity_facts(actor),
            health: body.vitals.health,
            max_health: self.body_rules(actor).max_health,
            temperature_c: self
                .local_weather_for_actor(actor)
                .map(|weather| weather.temperature),
        }
    }
    fn health_local_facts(&self) -> Result<LocalHealthFacts, Error> {
        let resolved = transport::resolved_locations_for(&self.canonical_state())?;
        let actors: Vec<_> = self
            .people()
            .map(|person| self.health_local_actor(person.id, &resolved[&person.id]))
            .collect();
        let mut interactions = Vec::new();
        for (actor, activity) in self.host.storage.activities() {
            let second = match *activity {
                Activity::Care { child, .. } | Activity::Comfort { child, .. } => Some(child),
                Activity::Tell { recipient, .. } | Activity::RealmTalk { recipient, .. } => {
                    Some(recipient)
                }
                Activity::ArmyTalk { listener, .. } => Some(listener),
                _ => None,
            };
            if let Some(second) = second {
                interactions.push(game_health::HealthInteraction {
                    first: actor,
                    second,
                });
            }
        }
        if self.host.state.missions.is_some() {
            interactions.extend(
                game_missions::talks(&self.host.storage)
                    .map(|(_, row)| row)
                    .map(|talk| game_health::HealthInteraction {
                        first: talk.speaker,
                        second: talk.listener,
                    }),
            );
        }
        let anchors: BTreeSet<_> = actors.iter().filter_map(|actor| actor.tactical).collect();
        let mut geometry = Vec::new();
        for first in &anchors {
            for second in anchors.range((
                std::ops::Bound::Excluded(*first),
                std::ops::Bound::Unbounded,
            )) {
                if first.encounter == second.encounter {
                    let a = Some((first.encounter, first.anchor));
                    let b = Some((second.encounter, second.anchor));
                    geometry.push(game_health::HealthGeometry {
                        first: *first,
                        second: *second,
                        contact_clear: self.health_tactical_clear(a, b, true),
                        sight_clear: self.health_tactical_clear(a, b, false),
                    });
                }
            }
        }
        Ok(LocalHealthFacts {
            actors,
            interactions,
            geometry,
        })
    }
    pub(crate) fn health_exposures(&self) -> Result<BTreeMap<PersonId, ExposureInput>, Error> {
        if self.host.state.health.is_none() {
            return Ok(BTreeMap::new());
        }
        let facts = self.health_local_facts()?;
        Ok(facts.index(self)?.exposures().collect())
    }
    #[cfg(test)]
    fn oracle_effective_shelter(&self, actor: PersonId) -> Option<&Shelter> {
        self.host.state.health.as_ref()?;
        if let Location::Carried { carrier } =
            *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))
                .expect("location exists")
        {
            return game_physiology::body(&self.host.storage, actor)?
                .vitals
                .alive
                .then(|| self.oracle_effective_shelter(carrier))
                .flatten();
        }
        let shelter = game_health::shelter(
            &self.host.storage,
            game_health::occupancy(&self.host.storage, actor)?.shelter,
        )?;
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive
            || self.resolved_location(actor) != Location::AtArea(shelter.area)
            || matches!(
                *self.host.storage.activity(actor)?,
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
        if matches!(*game_spatial::location(&self.host.storage,game_ecs::StableId((a).0)).expect("location exists"), Location::Carried { carrier } if carrier == b)
            || matches!(*game_spatial::location(&self.host.storage,game_ecs::StableId((b).0)).expect("location exists"), Location::Carried { carrier } if carrier == a)
        {
            return true;
        }
        let sa = self.oracle_effective_shelter(a).map(|s| s.id);
        let sb = self.oracle_effective_shelter(b).map(|s| s.id);
        if sa.is_some() && sa == sb {
            return true;
        }
        if game_health::jobs(&self.host.storage).any(|(_,job)|matches!(job.work,Work::Treat {patient,..} if job.actor==a&&patient==b||job.actor==b&&patient==a)){return true;}
        if sa != sb {
            return false;
        }
        for (giver, receiver) in [(a, b), (b, a)] {
            match *self.host.storage.activity(giver).unwrap() {
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
        self.host.state.missions.as_ref().is_some_and(|_| {
            game_missions::talks(&self.host.storage)
                .map(|(_, row)| row)
                .any(|talk| {
                    talk.speaker == a && talk.listener == b
                        || talk.speaker == b && talk.listener == a
                })
        })
    }
    #[cfg(test)]
    fn health_exposure(&self, actor: PersonId) -> ExposureInput {
        ExposureInput {
            temperature_c: self.local_weather_for_actor(actor).map(|w| w.temperature),
            insulation: self
                .oracle_effective_shelter(actor)
                .map_or(0, |s| s.insulation),
            // Body contributions use weather/shelter, never contact doses.
            infectious_contacts: [0; 2],
        }
    }
    #[cfg(test)]
    fn indexed_health_exposures(&self) -> BTreeMap<PersonId, ExposureInput> {
        self.health_exposures().expect("valid local health facts")
    }
    fn clinical_actor_facts(
        &self,
        actor: PersonId,
        exposure: ExposureInput,
    ) -> game_health::ClinicalActorFacts {
        game_health::ClinicalActorFacts {
            actor,
            alive: game_physiology::body(&self.host.storage, actor)
                .unwrap()
                .vitals
                .alive,
            thirst_enabled: !self.person_at(self.person_index[&actor]).hunger_exempt,
            resting: *self.host.storage.activity(actor).unwrap() == Activity::Rest,
            exposure,
            physiology: self.health_physiology(actor),
        }
    }

    pub(crate) fn health_physiology(&self, actor: PersonId) -> ClinicalPhysiology {
        clinical_physiology(
            &self.host.state.health.as_ref().unwrap().rules,
            self.genetic_metabolism_milli(actor),
            self.genetic_cold_tolerance_centi(actor),
        )
        .expect("validated individual metabolism")
    }
    pub(crate) fn observed_exposure_urgency(&self, actor: PersonId) -> i64 {
        self.host
            .state
            .health
            .as_ref()
            .and_then(|world| {
                game_health::clinical(&self.host.storage, actor).map(|clinical| {
                    game_health::exposure_urgency(clinical.cold, world.rules.cold_harm)
                })
            })
            .unwrap_or(0)
    }
    pub(crate) fn observed_hydration_urgency(&self, target: PersonId) -> i64 {
        self.host
            .state
            .health
            .as_ref()
            .and_then(|_| game_health::clinical(&self.host.storage, target))
            .map_or(0, |state| {
                game_health::hydration_urgency(state, self.health_physiology(target))
            })
    }
    pub(crate) fn health_body_delta(
        &self,
        actor: PersonId,
        elapsed: u64,
        exposure: ExposureInput,
    ) -> Result<HealthDelta, Error> {
        let Some(_) = &self.host.state.health else {
            return Ok(HealthDelta::default());
        };
        let next = SimTime(
            self.host
                .clock()
                .time
                .0
                .checked_add(elapsed)
                .ok_or(Error::Overflow)?,
        );
        self.host.state.health.as_ref().unwrap().actor_body_delta(
            &self.host.storage,
            self.host.clock().time,
            next,
            self.clinical_actor_facts(actor, exposure),
        )
    }

    #[cfg(test)]
    pub(crate) fn evaluate_health(&self, next: SimTime) -> Result<PendingHealth, Error> {
        let exposures = self.health_exposures()?;
        self.evaluate_health_with_exposures(next, &exposures)
    }
    pub(crate) fn evaluate_health_with_exposures(
        &self,
        next: SimTime,
        exposures: &BTreeMap<PersonId, ExposureInput>,
    ) -> Result<PendingHealth, Error> {
        let Some(runtime) = &self.host.state.health else {
            return Ok(None);
        };
        let actors = self
            .people()
            .map(|person| self.clinical_actor_facts(person.id, exposures[&person.id]))
            .collect::<Vec<_>>();
        runtime
            .prepare_actor_interval(
                &self.host.storage,
                self.host.clock().time,
                next,
                &actors,
                |job| {
                    (
                        self.health_job_can_progress(job),
                        self.medical_level(job.actor),
                    )
                },
            )
            .map(Some)
    }

    pub(crate) fn health_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        let Some(owner) = &self.host.state.health else {
            return Ok(limit);
        };
        let exposures = self.health_exposures()?;
        let actors = self
            .people()
            .map(|person| self.clinical_actor_facts(person.id, exposures[&person.id]))
            .collect::<Vec<_>>();
        owner.actor_boundary(
            &self.host.storage,
            self.host.clock().time,
            limit,
            &actors,
            |job| self.health_job_can_progress(job),
        )
    }

    pub(crate) fn commit_health(&mut self, pending: PendingHealth) -> Result<(), Error> {
        let Some(pending) = pending else {
            return Ok(());
        };
        let runtime = self.host.state.health.as_ref().ok_or(Error::Unavailable)?;
        let mut body_reads = game_ecs::ReadSet::new();
        let mut activity_reads = game_ecs::ReadSet::new();
        let mut activity_writes = game_ecs::ReadFootprint::new();
        let mut batch = game_ecs::Batch::new();
        let finish = pending.stage_into(
            runtime,
            &self.host.storage,
            |actor| {
                body_reads
                    .read::<game_physiology::Body>(&self.host.storage, game_ecs::StableId(actor.0))
                    .is_some_and(|body| body.vitals.alive)
            },
            |actor| match activity_reads
                .read::<Activity>(&self.host.storage, game_ecs::StableId(actor.0))
            {
                Some(Activity::HealthWork { kind, .. }) => Some(*kind),
                _ => None,
            },
            &mut batch,
        )?;
        for effect in finish.deadlines() {
            batch.insert(
                game_ecs::StableId(effect.actor.0),
                Activity::HealthWork {
                    kind: effect.kind,
                    completes: effect.completes,
                },
            );
            activity_writes.protect::<Activity>(game_ecs::StableId(effect.actor.0));
        }
        let activity_guard = activity_reads.bind(&self.host.storage, &batch, &activity_writes)?;
        let body_guard =
            body_reads.bind(&self.host.storage, &batch, &game_ecs::ReadFootprint::new())?;
        finish.validate_batch(runtime, &self.host.storage, &batch)?;
        activity_guard.validate(&self.host.storage, &batch)?;
        body_guard.validate(&self.host.storage, &batch)?;
        self.host.storage.apply_batch(batch)?;
        Ok(())
    }
    pub(crate) fn reconcile_health(
        &mut self,
        closing_equipment: &BTreeMap<PersonId, bool>,
    ) -> Result<(), Error> {
        let Some(_) = &self.host.state.health else {
            return Ok(());
        };
        self.ensure_event_capacity(
            self.host
                .state
                .health
                .as_ref()
                .unwrap()
                .reconciliation_event_bound(&self.host.storage),
        )?;
        let change = self
            .host
            .state
            .health
            .as_ref()
            .unwrap()
            .prepare_clinical_reconciliation(
                &self.host.storage,
                self.host.clock().time,
                |actor| {
                    game_physiology::body(&self.host.storage, actor)
                        .unwrap()
                        .vitals
                        .alive
                },
            )?;
        self.apply_health_change(change)?;
        let due = self
            .host
            .state
            .health
            .as_ref()
            .unwrap()
            .due_jobs(&self.host.storage, self.host.clock().time);
        for job in due {
            let valid = game_health::finish_ready(
                &job,
                game_health::FinishReadiness {
                    area: self.health_area(job.actor, true).ok(),
                    equipment_available: closing_equipment
                        .get(&job.actor)
                        .copied()
                        .unwrap_or(false),
                    active_deadline: match *self.host.storage.activity(job.actor).unwrap() {
                        Activity::HealthWork { completes, .. } => Some(completes),
                        _ => None,
                    },
                },
            );
            let facts = match job.work {
                Work::Drink { source, .. } => game_health::FinishFacts::Drink {
                    water: self.health_stock_facts(job.actor, source)?,
                    physiology: self.health_physiology(job.actor),
                },
                Work::Boil { source, .. } => game_health::FinishFacts::Boil {
                    water: self.health_stock_facts(job.actor, source)?,
                },
                Work::Shelter { .. } => game_health::FinishFacts::Shelter {
                    id: ShelterId(self.host.next_id()),
                },
                Work::Treat { patient, .. } => game_health::FinishFacts::Treat {
                    patient: self.health_patient_facts(job.actor, patient),
                },
            };
            let change = self.host.state.health.as_ref().unwrap().prepare_finish(
                &self.host.storage,
                job.actor,
                self.host.clock().time,
                valid,
                facts,
            )?;
            self.apply_health_change(change)?;
        }
        let expired = self
            .host
            .state
            .health
            .as_ref()
            .unwrap()
            .prepare_expire_offers(&self.host.storage, self.host.clock().time);
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
        let r = &self
            .host
            .state
            .health
            .as_ref()
            .ok_or(Error::Unavailable)?
            .rules;
        let needed = shelter_cost(r, capacity, insulation)?;
        if quantity < needed
            || built_at > self.host.clock().time
            || !self.person_index.contains_key(&custodian)
            || access
                .iter()
                .any(|person| !self.person_index.contains_key(person))
            || !game_cargo::inventory(&self.host.storage, source).is_some_and(|stock| {
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
        self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        self.ensure_event_capacity(2)?;
        let id = ShelterId(self.host.allocate_id()?.0);
        self.consume_health_stock(custodian, source, quantity);
        let mut access = access;
        access.insert(custodian);
        game_health::insert_shelter(
            &mut self.host.storage,
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
        )?;
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
        self.host.state.health.as_ref().ok_or(Error::Unavailable)?;
        let s = game_health::shelter(&self.host.storage, shelter).ok_or(Error::Unavailable)?;
        if *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId(
                (self
                    .person_at(*self.person_index.get(&person).ok_or(Error::Unavailable)?)
                    .id)
                    .0,
            ),
        )
        .expect("location exists")
            != Location::AtArea(s.area)
            || !s.access.contains(&person)
            || game_health::occupancy(&self.host.storage, person).is_some()
            || game_health::occupancies(&self.host.storage)
                .filter(|(_, row)| row.shelter == shelter)
                .count()
                >= s.capacity as usize
        {
            return Err(Error::Unavailable);
        }
        game_health::insert_occupancy(&mut self.host.storage, person, Occupancy { shelter })
            .expect("validated shelter");
        Ok(())
    }
    pub(crate) fn dependent_signal_visible(&self, observer: PersonId, subject: PersonId) -> bool {
        if !self.person_index.contains_key(&observer)
            || !self.person_index.contains_key(&subject)
            || game_physiology::body(&self.host.storage, observer).is_none()
            || game_physiology::body(&self.host.storage, subject).is_none()
        {
            return false;
        }
        let a =
            transport::location_position(&self.resolved_location(observer), self.host.clock().time)
                .ok()
                .map(|point| point.key());
        let b =
            transport::location_position(&self.resolved_location(subject), self.host.clock().time)
                .ok()
                .map(|point| point.key());
        game_health::signal_visible_at(
            a.as_ref(),
            b.as_ref(),
            [
                self.effective_shelter(observer).map(|s| s.id),
                self.effective_shelter(subject).map(|s| s.id),
            ],
            self.health_tactical_clear(
                self.health_tactical_place(observer),
                self.health_tactical_place(subject),
                false,
            ),
        )
    }
    fn can_observe_health(&self, observer: PersonId, subject: PersonId) -> bool {
        if !self.person_index.contains_key(&observer)
            || !self.person_index.contains_key(&subject)
            || game_physiology::body(&self.host.storage, observer).is_none()
            || game_physiology::body(&self.host.storage, subject).is_none()
        {
            return false;
        }
        let a =
            transport::location_position(&self.resolved_location(observer), self.host.clock().time)
                .ok()
                .map(|point| point.key());
        let b =
            transport::location_position(&self.resolved_location(subject), self.host.clock().time)
                .ok()
                .map(|point| point.key());
        game_health::can_observe_at(
            a.as_ref(),
            b.as_ref(),
            [
                self.effective_shelter(observer).map(|s| s.id),
                self.effective_shelter(subject).map(|s| s.id),
            ],
            self.health_tactical_clear(
                self.health_tactical_place(observer),
                self.health_tactical_place(subject),
                false,
            ),
            self.transport_is_moving(observer),
        )
    }
    fn visible_symptoms(&self, actor: PersonId) -> SymptomsView {
        self.host
            .state
            .health
            .as_ref()
            .expect("health runtime")
            .symptoms_for(
                &self.host.storage,
                self.host.clock().time,
                actor,
                game_physiology::body(&self.host.storage, actor)
                    .expect("body exists")
                    .vitals
                    .health,
                self.body_rules(actor).max_health,
            )
    }
    #[cfg(test)]
    fn oracle_dependent_signal_visible(&self, observer: PersonId, subject: PersonId) -> bool {
        self.transport_same_place(observer, subject)
            && self.health_tactical_clear(
                self.health_tactical_place(observer),
                self.health_tactical_place(subject),
                false,
            )
            && self
                .oracle_effective_shelter(observer)
                .map(|shelter| shelter.id)
                == self
                    .oracle_effective_shelter(subject)
                    .map(|shelter| shelter.id)
    }
    #[cfg(test)]
    fn oracle_can_observe_health(&self, observer: PersonId, subject: PersonId) -> bool {
        self.oracle_dependent_signal_visible(observer, subject)
            && !self.transport_is_moving(observer)
    }
    #[cfg(test)]
    fn oracle_visible_symptoms(&self, person: PersonId) -> SymptomsView {
        let state = self
            .host
            .state
            .health
            .as_ref()
            .and_then(|_| game_health::clinical(&self.host.storage, person));
        let rules = &self.host.state.health.as_ref().unwrap().rules;
        let has = |kind| {
            state
                .and_then(|c| c.illnesses.get(&kind))
                .is_some_and(|i| i.symptomatic && i.recovers > self.host.clock().time)
        };
        SymptomsView {
            person,
            fever: has(IllnessKind::Respiratory) || has(IllnessKind::Enteric),
            nausea: has(IllnessKind::Enteric),
            cough: has(IllnessKind::Respiratory),
            shivering: state.is_some_and(|c| c.cold >= rules.cold_harm / 2),
            wounded: game_physiology::body(&self.host.storage, person)
                .unwrap()
                .vitals
                .health
                < self.body_rules(person).max_health / 2,
            observed_at: self.host.clock().time,
        }
    }
    pub(crate) fn observe_health(&mut self) {
        if self.host.state.health.is_none() {
            return;
        }
        let prepared = {
            let facts = self.health_local_facts().expect("valid local health facts");
            facts
                .index(self)
                .expect("valid local health index")
                .prepare_observations()
                .expect("valid local observations")
        };
        prepared
            .commit(
                self.host.state.health.as_ref().unwrap(),
                &mut self.host.storage,
            )
            .expect("fresh local observation");
    }
    pub(crate) fn health_view(&self, actor: PersonId) -> Option<HealthView> {
        self.host.state.health.as_ref()?;
        let clinical = game_health::clinical(&self.host.storage, actor)
            .cloned()
            .unwrap_or_default();
        let known = game_health::knowledge(&self.host.storage, actor)
            .cloned()
            .unwrap_or_default();
        Some(HealthView {
            thirst: clinical.thirst,
            cold_exposure: clinical.cold,
            symptoms: self.visible_symptoms(actor),
            sheltered: self.effective_shelter(actor).map(|s| s.id),
            treatment_until: clinical.treatment.as_ref().map(|c| c.until),
            offers: game_health::all_offers(&self.host.storage)
                .map(|(_, _, offer)| offer)
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
        let Some(owner) = &self.host.state.health else {
            return Vec::new();
        };
        let planning = self.health_planning_data(actor, None);
        let input = planning
            .as_ref()
            .map(|facts| facts.input(self.host.clock().time));
        owner
            .available_actions(
                &self.host.storage,
                self.health_actor_facts(actor),
                input.as_ref(),
                |giver| game_health::CaregiverAvailability {
                    capable: self.health_area(giver, true).is_ok(),
                    observable: self.can_observe_health(actor, giver),
                },
                |source| self.health_stock_facts(actor, source),
            )
            .into_iter()
            .map(|action| Command::Health { action })
            .collect()
    }

    fn remembered_health_stocks(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<game_health::RememberedStock> {
        game_health::planning_stocks(
            actor,
            self.host.clock().time,
            self.host
                .state
                .decisions
                .as_ref()
                .and_then(|_| game_characters::decisions::stocks(&self.host.storage, actor))
                .into_iter()
                .flat_map(|stocks| stocks.known.values()),
            |fact| game_health::RememberedStockFacts {
                here: self.health_remembered_source_here(actor, fact),
                held: game_cargo::inventory(&self.host.storage, fact.inventory)
                    .is_some_and(|stock| stock.location == InventoryLocation::Carried(actor)),
                available: Quantity(self.unreserved_stock_for_planning(
                    fact.inventory,
                    actor,
                    resources,
                )),
                tool_available: self.tool_available_for_planning(actor, fact.inventory, resources),
            },
        )
    }

    fn health_planning_data(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<HealthPlanningData> {
        let area = self.health_area(actor, true).ok()?;
        let mind =
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)?;
        let traits = self
            .invocation_personality(actor)
            .unwrap_or_else(|| mind.personality.clone());
        Some(HealthPlanningData {
            area,
            stationary: self.resolved_location(actor) == Location::AtArea(area),
            values: game_health::IntakeValues {
                risk_tolerance: traits.risk_tolerance,
            },
            stocks: self.remembered_health_stocks(actor, resources),
        })
    }
    pub(crate) fn health_candidate(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<Command> {
        let world = self.host.state.health.as_ref()?;
        let facts = self.health_planning_data(actor, resources)?;
        let input = facts.input(self.host.clock().time);
        let carried = matches!(
            *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))
                .expect("location exists"),
            Location::Carried { .. }
        );
        let need = game_health::NeedFacts {
            actor,
            drink_relief: self.health_physiology(actor).drink_relief,
            sheltered: self.effective_shelter(actor).is_some(),
            can_enter_shelter: game_health::can_enter_shelter(facts.stationary, carried),
        };
        match world.plan_need(&self.host.storage, need, &input)? {
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
                    observed_at: self.host.clock().time,
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
                        .consume_supply(&self.host.storage, actor, source, &input, supply.consumer)
                        .map(|action| Command::Health { action }),
                    step => self.delivery_command(actor, step),
                }
            }
        }
    }
    pub(crate) fn health_actor_busy(&self, actor: PersonId) -> bool {
        self.host
            .state
            .health
            .as_ref()
            .is_some_and(|owner| owner.actor_busy(&self.host.storage, actor))
    }

    pub(crate) fn review_health(&mut self) -> Result<(), Error> {
        let now = self.host.clock().time;
        let Some(w) = &self.host.state.health else {
            return Ok(());
        };
        if !w.review_due(now) {
            return Ok(());
        }
        self.preflight_health(now)?;
        self.host.state.health.as_mut().unwrap().record_review(now);
        self.observe_health();
        let jobs = self
            .host
            .state
            .health
            .as_ref()
            .unwrap()
            .review_work(&self.host.storage);
        for job in jobs {
            let body = game_physiology::body(&self.host.storage, job.actor).unwrap();
            let physical = self.body_rules(job.actor);
            if job.should_pause(
                job.actor == self.host.state.ruler && !self.past_life_controls_actor(job.actor),
                game_health::WorkNeeds {
                    hunger: body.vitals.hunger,
                    meal_relief: physical.meal_relief,
                    fatigue: body.vitals.fatigue,
                    fatigue_limit: physical.fatigue_limit,
                    health: body.vitals.health,
                    max_health: physical.max_health,
                },
            ) {
                self.try_interrupt_actor_activity(job.actor)?;
                let _ = self.debug_actor_command(job.actor, Command::Rest);
            }
        }
        let offers = self
            .host
            .state
            .health
            .as_ref()
            .unwrap()
            .review_offers(&self.host.storage);
        for offer in offers {
            let patient = offer.patient();
            let caregiver = offer.caregiver();
            let facts = game_health::OfferReviewFacts {
                player_controlled: patient == self.host.state.ruler
                    && !self.past_life_controls_actor(patient),
                patient_present: self.health_area(patient, false).is_ok(),
                caregiver_capable: self.health_area(caregiver, true).is_ok(),
                contact: self.can_observe_health(patient, caregiver),
                trust: self.health_mind(patient).map(|mind| {
                    mind.relationships
                        .get(&caregiver)
                        .map_or(0, |relation| relation.trust)
                }),
            };
            if let Some(action) = offer.response(facts, || self.visible_symptoms(patient)) {
                let _ = self.health_command(patient, &action);
            }
        }
        let treatments = self
            .host
            .state
            .health
            .as_ref()
            .unwrap()
            .accepted_treatments(&self.host.storage, self.host.state.ruler);
        for (caregiver, action) in treatments {
            let _ = self.health_command(caregiver, &action);
        }
        let healers = self.host.state.health.as_ref().unwrap().review_healers(
            self.people().map(|person| person.id),
            |actor| game_health::HealerQualifications {
                is_ruler: actor == self.host.state.ruler,
                medicine: self.medical_level(actor),
            },
            |actor| self.health_area(actor, true).is_ok(),
            |actor| self.health_actor_busy(actor),
        );
        for caregiver in healers {
            let mind = &game_characters::decisions::mind::<game_api::Command>(
                &self.host.storage,
                caregiver,
            )
            .expect("mind exists");
            let body = game_physiology::body(&self.host.storage, caregiver).unwrap();
            let physical = self.body_rules(caregiver);
            let stocks = self.remembered_health_stocks(caregiver, None);
            let patients = self
                .host
                .state
                .health
                .as_ref()
                .unwrap()
                .remembered_patients(
                    &self.host.storage,
                    caregiver,
                    |patient| self.can_observe_health(caregiver, patient),
                    |patient| self.health_patient_resting(patient),
                );
            let facts = game_health::CareReviewFacts {
                healer: game_health::HealerReviewFacts {
                    is_ruler: caregiver == self.host.state.ruler,
                    medicine: self.medical_level(caregiver),
                    capable: self.health_area(caregiver, true).is_ok(),
                    busy: self.health_actor_busy(caregiver),
                },
                body: game_health::WorkNeeds {
                    hunger: body.vitals.hunger,
                    meal_relief: physical.meal_relief,
                    fatigue: body.vitals.fatigue,
                    fatigue_limit: physical.fatigue_limit,
                    health: body.vitals.health,
                    max_health: physical.max_health,
                },
                empathy: mind.personality.empathy,
                stocks: &stocks,
            };
            if let Some(action) = self
                .host
                .state
                .health
                .as_ref()
                .unwrap()
                .proactive_review_offer(caregiver, facts, &patients)
            {
                let _ = self.health_command(caregiver, &action);
            }
        }
        Ok(())
    }
    pub(crate) fn health_allocation_bound(&self, next: SimTime) -> u64 {
        self.host
            .state
            .health
            .as_ref()
            .map_or(0, |w| w.allocation_bound(&self.host.storage, next))
    }
    pub(crate) fn health_event_bound(&self) -> usize {
        self.host
            .state
            .health
            .as_ref()
            .map_or(0, |owner| owner.review_event_bound(&self.host.storage))
    }
    pub(crate) fn preflight_health(&self, next: SimTime) -> Result<(), Error> {
        let Some(w) = &self.host.state.health else {
            return Ok(());
        };
        w.preflight_time(next)?;
        self.host
            .next_id()
            .checked_add(self.health_allocation_bound(next))
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
        let owner_now = self.host.clock().time;
        if !self.person_index.contains_key(&observer)
            || !game_cargo::inventory(&self.host.storage, source)
                .is_some_and(|stock| stock.good == Good::Water)
        {
            return Err(Error::Unavailable);
        }
        let quantity = game_cargo::inventory(&self.host.storage, source)
            .expect("inventory exists")
            .quantity
            .0;
        self.host.state.health.as_ref().ok_or(Error::Unavailable)?;
        let safe =
            game_health::water_lot(&self.host.storage, source).is_none_or(|lot| lot.load() == 0);
        if !safe {
            return Err(Error::InvalidConfiguration(
                "authoring potable water with real contaminants".into(),
            ));
        }
        let _ = quantity;
        game_health::ensure_knowledge(&mut self.host.storage, observer)
            .expect("known health observer")
            .water
            .insert(
                source,
                WaterSafetyView {
                    inventory: source,
                    purified: true,
                    suspected_unsafe: false,
                    observed_at: owner_now,
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
        if self.host.state.health.is_none() {
            return Ok(());
        }
        self.preflight_water_received(patient, source, quantity)
    }
}

impl Simulation {
    pub(crate) fn health_job_can_progress(&self, job: &Job) -> bool {
        let Some(physical) = &self.host.state.physical else {
            return false;
        };
        let Some(body) = game_physiology::body(&self.host.storage, job.actor) else {
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
        let Some(owner) = &self.host.state.health else {
            return;
        };
        let reaction = match *event {
            EventKind::Born { child, parents, .. } => {
                let gestating = parents
                    .iter()
                    .map(|parent| {
                        (
                            *parent,
                            self.host.state.development.as_ref().is_some_and(|_| {
                                game_lifecycle::life(&self.host.storage, *parent).is_some_and(
                                    |life| life.reproduction == development::Reproduction::Gestates,
                                )
                            }),
                        )
                    })
                    .collect::<Vec<_>>();
                let inherited_shelter = owner.birth_shelter(&gestating, |parent| {
                    self.effective_shelter(parent).map(|shelter| shelter.id)
                });
                Some(game_health::HealthReaction::Born {
                    child,
                    inherited_shelter,
                })
            }
            EventKind::Died { actor } => Some(game_health::HealthReaction::Died { actor }),
            EventKind::IllnessStarted { actor, kind } => {
                Some(game_health::HealthReaction::IllnessStarted { actor, kind })
            }
            _ => None,
        };
        if let Some(reaction) = reaction {
            let prepared = owner
                .prepare_reaction(&self.host.storage, self.host.clock().time, reaction)
                .expect("valid health reaction");
            prepared
                .commit(
                    self.host.state.health.as_mut().unwrap(),
                    &mut self.host.storage,
                )
                .expect("registered health reaction");
        }
    }
}

impl Simulation {
    fn willing_health_supplies(
        &self,
        giver: PersonId,
        patient: PersonId,
    ) -> Option<(InventoryId, InventoryId)> {
        let mind =
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, giver)?;
        let body = game_physiology::body(&self.host.storage, giver)?;
        let owner = self.host.state.health.as_ref()?;
        let physical = self.body_rules(giver);
        let stocks = self.remembered_health_stocks(giver, None);
        let urgencies = mind
            .child_signals
            .values()
            .map(|signal| signal.urgency)
            .collect::<Vec<_>>();
        owner.willing_supplies_for(game_health::CareWillingnessFacts {
            capable: self.health_area(giver, true).is_ok(),
            medicine: self.medical_level(giver),
            busy: self.health_actor_busy(giver),
            body: game_health::WorkNeeds {
                hunger: body.vitals.hunger,
                meal_relief: physical.meal_relief,
                fatigue: body.vitals.fatigue,
                fatigue_limit: physical.fatigue_limit,
                health: body.vitals.health,
                max_health: physical.max_health,
            },
            dependent_urgencies: &urgencies,
            empathy: mind.personality.empathy,
            trust: mind
                .relationships
                .get(&patient)
                .map(|relation| relation.trust),
            stocks: &stocks,
        })
    }
}

impl Simulation {
    fn health_mind(&self, actor: PersonId) -> Option<&decisions::Mind> {
        if self.past_life_controls_actor(actor) {
            let w = self.host.state.invocation.as_ref()?;
            return w.controller_mind::<game_api::Command>(&self.host.storage, actor);
        }
        game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
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
        let actor = sim.host.state.ruler;
        let loci: Vec<_> = sim
            .host
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
            sim.host
                .state
                .genetics
                .as_mut()
                .unwrap()
                .set_alleles(&mut sim.host.storage, actor, &locus, [1_000; 2])
                .unwrap();
        }
        sim
    }

    #[test]
    fn actual_genetic_expression_changes_drinking_and_deprivation_without_rule_mutation() {
        let mut whole = expressed_health_fixture();
        let actor = whole.host.state.ruler;
        let rules = whole.host.state.health.as_ref().unwrap().rules.clone();
        let physiology = whole.health_physiology(actor);
        assert_eq!(
            (
                physiology.thirst_harm,
                physiology.drink_relief,
                physiology.cold_tolerance_centi
            ),
            (120_000, 200_000, 400)
        );
        game_health::clinical_mut(&mut whole.host.storage, actor)
            .unwrap()
            .thirst = 100_000;
        let mut sliced = Simulation::load(&whole.save().unwrap(), Execution::default()).unwrap();
        whole.advance(SimDuration(40_003)).unwrap();
        for elapsed in [1, 19_999, 7_001, 13_002] {
            sliced.advance(SimDuration(elapsed)).unwrap();
        }
        assert_eq!(whole.canonical_state(), sliced.canonical_state());
        assert!(
            game_physiology::body(&whole.host.storage, actor)
                .unwrap()
                .vitals
                .health
                < whole.body_rules(actor).max_health
        );
        assert_eq!(whole.host.state.health.as_ref().unwrap().rules, rules);

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
        game_health::clinical_mut(&mut whole.host.storage, actor)
            .unwrap()
            .thirst = 250_000;
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
            game_health::clinical(&whole.host.storage, actor)
                .unwrap()
                .thirst,
            60_000
        );
        assert_eq!(
            game_cargo::inventory(&whole.host.storage, water)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(whole.host.state.health.as_ref().unwrap().rules, rules);
    }

    #[test]
    fn absent_genetics_and_neutral_traits_produce_identical_clinical_inputs() {
        let mut sim = Simulation::health_fixture(58, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        game_health::clinical_mut(&mut sim.host.storage, actor)
            .unwrap()
            .thirst = 200_000;
        let before = sim.evaluate_health(SimTime(1_003)).unwrap();
        let before_delta = sim
            .health_body_delta(actor, 1_003, sim.health_exposure(actor))
            .unwrap();
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
        assert_eq!(
            before_delta,
            sim.health_body_delta(actor, 1_003, sim.health_exposure(actor))
                .unwrap()
        );
        assert_eq!(before_physiology, sim.health_physiology(actor));
    }

    #[test]
    fn caregiver_self_preservation_uses_individual_metabolic_needs() {
        let mut sim = expressed_health_fixture();
        let actor = sim.host.state.ruler;
        let area = sim.health_area(actor, true).unwrap();
        sim.host
            .state
            .skills
            .as_mut()
            .unwrap()
            .credit(&mut sim.host.storage, actor, Skill::Medicine, 60_000, None)
            .unwrap();
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, actor)
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
            let observed_at = sim.time();
            game_characters::decisions::stocks_mut(&mut sim.host.storage, actor)
                .unwrap()
                .known
                .insert(
                    inventory,
                    StockKnowledge {
                        inventory,
                        area,
                        good,
                        quantity: Quantity(2_000),
                        accessible: true,
                        observed_at,
                        learned_at: observed_at,
                        source: KnowledgeSource::Observed(actor),
                    },
                );
        }
        assert!(sim.willing_health_supplies(actor, actor).is_some());
        game_physiology::body_mut(&mut sim.host.storage, actor)
            .unwrap()
            .vitals
            .hunger = 250_000;
        assert!(250_000 < sim.host.state.physical.as_ref().unwrap().rules.meal_relief);
        assert!(250_000 > sim.body_rules(actor).meal_relief);
        assert!(sim.willing_health_supplies(actor, actor).is_none());
    }
    #[test]
    fn medical_learning_during_a_course_is_identical_across_time_slices() {
        let mut whole = Simulation::health_fixture(9, Execution::default()).unwrap();
        whole
            .host
            .state
            .health
            .as_mut()
            .unwrap()
            .rules
            .treatment_time = SimDuration(120_000);
        let actor = whole.host.state.ruler;
        let area = whole.health_area(actor, true).unwrap();
        whole
            .host
            .state
            .skills
            .as_mut()
            .unwrap()
            .credit(
                &mut whole.host.storage,
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
            game_health::clinical(&whole.host.storage, actor)
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
        let actor = sim.host.state.ruler;
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
        game_health::insert_water_lot(
            &mut sim.host.storage,
            water,
            WaterLot::new(2_000, 2_000_000),
        )
        .unwrap();
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Health {
                action: HealthAction::Drink { source: water },
            },
        })
        .unwrap();
        sim.advance(SimDuration(10_000)).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, water)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            game_health::water_lot(&sim.host.storage, water)
                .unwrap()
                .load(),
            1_000_000
        );
        assert!(!sim.observe().health.unwrap().symptoms.nausea);
        sim.advance(SimDuration(29_999)).unwrap();
        assert!(!sim.observe().health.unwrap().symptoms.nausea);
        sim.advance(SimDuration(1)).unwrap();
        assert!(sim.observe().health.unwrap().symptoms.nausea);
        assert!(
            sim.host
                .events()
                .iter()
                .any(|event| event.at == SimTime(40_000)
                    && event.event
                        == (EventKind::IllnessStarted {
                            actor,
                            kind: IllnessKind::Enteric
                        }))
        );
        let before = game_physiology::body(&sim.host.storage, actor)
            .unwrap()
            .vitals
            .health;
        sim.advance(SimDuration(1_000)).unwrap();
        assert!(
            game_physiology::body(&sim.host.storage, actor)
                .unwrap()
                .vitals
                .health
                < before
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn a_physician_stops_work_at_actual_incapacitation_independently_of_time_slicing() {
        let mut whole = Simulation::health_fixture(7, Execution::default()).unwrap();
        let actor = whole.host.state.ruler;
        let area = whole.health_area(actor, true).unwrap();
        whole
            .host
            .state
            .skills
            .as_mut()
            .unwrap()
            .credit(
                &mut whole.host.storage,
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
        let current = game_physiology::body(&whole.host.storage, actor)
            .unwrap()
            .vitals
            .health;
        let threshold = whole
            .host
            .state
            .combat
            .as_ref()
            .unwrap()
            .rules
            .incapacitation;
        whole
            .debug_injure(actor, current - (threshold + 5_000))
            .unwrap();
        game_health::clinical_mut(&mut whole.host.storage, actor)
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
            whole.host.state.skills.as_ref().unwrap().practice(
                &whole.host.storage,
                actor,
                Skill::Medicine
            ),
            95_000
        );
        assert!(matches!(
            game_health::job(&whole.host.storage, actor).unwrap().work,
            Work::Treat { elapsed: 5_000, .. }
        ));
    }
    #[test]
    fn completed_boiling_does_not_release_a_tool_into_another_workers_elapsed_interval() {
        let mut sim = Simulation::health_fixture(6, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let worker = sim.person_at(3).id;
        let area = sim.health_area(actor, true).unwrap();
        let people: Vec<_> = sim.people().map(|p| p.id).collect();
        for person in people {
            sim.interrupt_actor_activity(person);
        }
        *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((worker).0))
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
        let site = game_workshop::sites(&sim.host.storage)
            .map(|(_, site)| site)
            .find(|site| site.area == area)
            .unwrap()
            .id;
        let output = game_workshop::site(&sim.host.storage, site)
            .expect("site exists")
            .output;
        let before = game_cargo::inventory(&sim.host.storage, output)
            .expect("inventory exists")
            .quantity;
        let (input, input_quantity, output_quantity) = {
            let site = game_workshop::site(&sim.host.storage, site).unwrap();
            let recipe = sim.host.state.workshops.recipe(&site.recipe).unwrap();
            (site.input, recipe.input_quantity, recipe.output_quantity)
        };
        let recipe = sim
            .fixture_recipe(
                input,
                output,
                input_quantity,
                output_quantity,
                SimDuration(30_000),
            )
            .unwrap();
        {
            let site = game_workshop::site_mut(&mut sim.host.storage, site).unwrap();
            site.tool = tool;
            site.recipe = recipe;
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
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, output)
                .expect("inventory exists")
                .quantity,
            before
        );
        assert!(sim.host.events().iter().any(|event| event.event
            == (EventKind::WaterPurified {
                actor,
                source: water
            })));
        assert!(!sim.host.events().iter().any(
            |event| matches!(event.event, EventKind::Produced { actor, .. } if actor == worker)
        ));
    }
    #[test]
    fn unseen_shelter_occupants_do_not_change_affordances_or_private_plans() {
        let mut sim = Simulation::health_fixture(5, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let other = sim.people().find(|p| p.role == Role::Guardian).unwrap().id;
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
        game_health::clinical_mut(&mut sim.host.storage, actor)
            .unwrap()
            .cold = 20_000;
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
        sim.host
            .state
            .physical
            .as_mut()
            .unwrap()
            .rules
            .carrying_capacity = Quantity(20_000);
        sim.initialize_transport(game_transport::Rules {
            adult_mass: Quantity(6_000),
            newborn_mass: Quantity(1_000),
            lift_time: SimDuration(6_000),
            ..game_transport::Rules::default()
        })
        .unwrap();
        let bearer = sim.host.state.ruler;
        let passenger = sim.person_at(3).id;
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
            *game_spatial::location(&sim.host.storage, game_ecs::StableId((passenger).0))
                .expect("location exists"),
            Location::Carried { carrier: bearer }
        );
        (sim, bearer, passenger)
    }
    #[test]
    fn held_body_shares_shelter_and_contact_without_obtaining_access() {
        let (mut sim, bearer, passenger) = held_peer();
        let area = sim.health_area(bearer, true).unwrap();
        let quantity =
            shelter_cost(&sim.host.state.health.as_ref().unwrap().rules, 2, 650).unwrap();
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
                sim.host.clock().time,
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
            !game_health::shelter(&sim.host.storage, shelter)
                .unwrap()
                .access
                .contains(&passenger)
        );
        assert!(
            game_health::knowledge(&sim.host.storage, bearer)
                .unwrap()
                .shelters[&shelter]
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
            !game_health::shelter(&sim.host.storage, shelter)
                .unwrap()
                .access
                .contains(&passenger)
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn roadside_drinking_uses_real_held_water_and_rejects_endpoint_stores() {
        let (mut sim, bearer, passenger) = held_peer();
        let home = sim.health_area(bearer, true).unwrap();
        let destination = sim.host.state.areas[1].id;
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
            let observed_at = sim.time();
            game_characters::decisions::stocks_mut(&mut sim.host.storage, bearer)
                .unwrap()
                .known
                .insert(
                    source,
                    StockKnowledge {
                        inventory: source,
                        area: home,
                        good: Good::Water,
                        quantity: Quantity(2_000),
                        accessible: true,
                        observed_at,
                        learned_at: observed_at,
                        source: KnowledgeSource::Observed(bearer),
                    },
                );
        }
        sim.seed_water_safety(bearer, water).unwrap();
        assert!(
            sim.transport_can_depart(bearer),
            "bodies={:?}, groups={:?}",
            sim.transport_bodies(),
            game_transport::groups(&sim.host.storage).collect::<Vec<_>>()
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
        game_health::clinical_mut(&mut sim.host.storage, bearer)
            .unwrap()
            .thirst = 100_000;
        assert_eq!(
            sim.health_candidate(bearer, None),
            Some(Command::Health {
                action: HealthAction::Drink { source: water }
            })
        );
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.debug_actor_command(
                bearer,
                Command::Health {
                    action: HealthAction::Drink { source: endpoint }
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
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
        assert_eq!(sim.canonical_state(), sliced.canonical_state());
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, water)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, endpoint)
                .expect("inventory exists")
                .quantity,
            Quantity(2_000)
        );
        sim.host.state.health.as_mut().unwrap().rules.treatment_time = SimDuration(10_000);
        sim.host
            .state
            .skills
            .as_mut()
            .unwrap()
            .credit(&mut sim.host.storage, bearer, Skill::Medicine, 60_000, None)
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
        assert_eq!(sim.canonical_state(), sliced.canonical_state());
        assert!(
            game_health::clinical(&sim.host.storage, passenger)
                .unwrap()
                .treatment
                .is_some()
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, herbs)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, water)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
    }
    #[test]
    fn carried_battle_body_is_not_a_strategic_area_contact_or_remote_patient() {
        let (mut sim, bearer, passenger) = held_peer();
        let enemy = sim.person_at(6).id;
        let id = sim
            .debug_start_encounter(vec![(bearer, 0), (enemy, 1)])
            .unwrap();
        let outside = sim.person_at(9).id;
        assert!(sim.in_health_contact(bearer, passenger));
        assert!(!sim.can_observe_health(outside, passenger));
        game_conflict::encounter_mut(&mut sim.host.storage, id)
            .unwrap()
            .fighters
            .get_mut(&enemy)
            .unwrap()
            .position = Point {
            x: 20_000,
            y: 8_000,
        };
        game_health::clinical_mut(&mut sim.host.storage, enemy)
            .unwrap()
            .cold = 120_000;
        assert!(!sim.health_tactical_clear(
            sim.health_tactical_place(passenger),
            sim.health_tactical_place(enemy),
            true
        ));
        sim.observe_health();
        assert!(
            !game_health::knowledge(&sim.host.storage, bearer)
                .unwrap()
                .people[&enemy]
                .shivering
        );
        assert!(sim.visible_symptoms(enemy).shivering);
        assert!(sim.indexed_health_exposures().contains_key(&passenger));
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;

    fn population_fixture(population: usize) -> Simulation {
        let sample = Simulation::health_fixture(71, Execution::default()).unwrap();
        let sample_world = sample.host.state.physical.as_ref().unwrap();
        let template = game_physiology::body(&sample.host.storage, sample.host.state.ruler)
            .unwrap()
            .clone();
        let activity = sample
            .host
            .storage
            .activity(sample.host.state.ruler)
            .unwrap()
            .clone();
        let job = game_workshop::job(&sample.host.storage, sample.host.state.ruler).cloned();
        let mut sim = Simulation::kernel_fixture(71, population, Execution::default()).unwrap();
        sim.initialize_physical_people(sample_world.rules.clone())
            .unwrap();
        for person in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            *game_physiology::body_mut(&mut sim.host.storage, person).unwrap() = template.clone();
            *sim.host.storage.activity_mut(person).unwrap() = activity.clone();
            if let Some(job) = job.clone() {
                game_workshop::insert_job(&mut sim.host.storage, person, job).unwrap();
            } else {
                game_workshop::remove_job(&mut sim.host.storage, person).unwrap();
            }
        }
        sim.initialize_health(Rules::fixture()).unwrap();
        sim
    }

    fn exhaustive_exposures(sim: &Simulation) -> BTreeMap<PersonId, ExposureInput> {
        sim.people()
            .map(|person| {
                let mut exposure = sim.health_exposure(person.id);
                for (index, kind) in [IllnessKind::Enteric, IllnessKind::Respiratory]
                    .into_iter()
                    .enumerate()
                {
                    exposure.infectious_contacts[index] = game_health::clinicals(&sim.host.storage)
                        .filter(|(other, state)| {
                            state.illnesses.get(&kind).is_some_and(|i| {
                                i.symptomatic && i.recovers > sim.host.clock().time
                            }) && sim.in_health_contact(person.id, *other)
                        })
                        .count() as u64;
                }
                (person.id, exposure)
            })
            .collect()
    }

    fn exhaustive_observations(sim: &Simulation) -> BTreeMap<PersonId, Knowledge> {
        let mut known: BTreeMap<_, _> = game_health::knowledges(&sim.host.storage)
            .map(|(id, row)| (id, row.clone()))
            .collect();
        for observer in sim.people().filter(|p| {
            game_physiology::body(&sim.host.storage, p.id)
                .unwrap()
                .vitals
                .alive
        }) {
            let Location::AtArea(area) =
                *game_spatial::location(&sim.host.storage, game_ecs::StableId((observer.id).0))
                    .expect("location exists")
            else {
                continue;
            };
            let people: Vec<_> = sim
                .people()
                .filter(|p| sim.oracle_can_observe_health(observer.id, p.id))
                .map(|p| p.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(64)
                .map(|p| (p, sim.oracle_visible_symptoms(p)))
                .collect();
            let shelters: Vec<_> = game_health::shelters(&sim.host.storage)
                .map(|(_, row)| row)
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
                            known_occupants: game_health::occupancies(&sim.host.storage)
                                .filter(|(p, row)| {
                                    row.shelter == s.id
                                        && sim.oracle_can_observe_health(observer.id, *p)
                                })
                                .map(|(p, _)| p)
                                .collect(),
                            observed_at: sim.time(),
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
        let actor = sim.host.state.ruler;
        let area = sim.host.state.areas[0].id;
        let ids = [
            ShelterId(sim.host.next_id()),
            ShelterId(sim.host.next_id() + 1),
        ];
        sim.host.allocate_ids(2).unwrap();
        for id in ids {
            game_health::insert_shelter(
                &mut sim.host.storage,
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
            )
            .unwrap();
        }
        ids
    }

    #[test]
    fn indexed_contacts_match_exhaustive_pairs_across_varied_opening_states() {
        let mut sim = population_fixture(15);
        let people: Vec<_> = sim.people().map(|p| p.id).collect();
        let shelters = add_shelters(&mut sim);
        for phase in 0..18 {
            let occupants: Vec<_> = game_health::occupancies(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
            for id in occupants {
                game_health::remove_occupancy(&mut sim.host.storage, id).unwrap();
            }
            let ids: Vec<_> = game_health::jobs(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
            for id in ids {
                game_health::remove_job(&mut sim.host.storage, id).unwrap();
            }
            for (index, actor) in people.iter().enumerate() {
                let area = sim.host.state.areas[if index < 6 { 0 } else { (index + phase) % 3 }].id;
                *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId(actor.0))
                    .expect("location exists") = if (index + phase) % 11 == 0 {
                    Location::InTransit {
                        from: area,
                        to: if area == sim.host.state.areas[0].id {
                            sim.host.state.areas[1].id
                        } else {
                            sim.host.state.areas[0].id
                        },
                        departed: SimTime(0),
                        arrives: SimTime(100_000),
                        road_start: None,
                    }
                } else {
                    Location::AtArea(area)
                };
                if area == sim.host.state.areas[0].id && (index + phase) % 3 != 2 {
                    game_health::insert_occupancy(
                        &mut sim.host.storage,
                        *actor,
                        Occupancy {
                            shelter: shelters[(index + phase) % 2],
                        },
                    )
                    .unwrap();
                }
                let max_health = sim.body_rules(*actor).max_health;
                let body = game_physiology::body_mut(&mut sim.host.storage, *actor).unwrap();
                body.vitals.alive = (index + phase) % 13 != 0;
                body.vitals.health = if body.vitals.alive { max_health } else { 0 };
                let recipient = people[(index + 1) % people.len()];
                *sim.host.storage.activity_mut(*actor).unwrap() = match (index + phase) % 6 {
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
                let state = game_health::clinical_mut(&mut sim.host.storage, *actor).unwrap();
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
            game_health::insert_job(
                &mut sim.host.storage,
                Job {
                    actor: people[0],
                    area: sim.host.state.areas[0].id,
                    completes: SimTime(60_000),
                    work: Work::Treat {
                        patient: people[1],
                        skill_ms: 0,
                        elapsed: 0,
                    },
                },
            )
            .unwrap();
            sim.host.state.missions = Some(missions::Runtime {
                last_event: 0,
                last_review: SimTime(0),
            });
            game_missions::insert_talk(
                &mut sim.host.storage,
                people[0],
                missions::PendingTalk {
                    speaker: people[0],
                    listener: people[1],
                    area: sim.host.state.areas[0].id,
                    completes: SimTime(60_000),
                    conversation: missions::Conversation::Objective {
                        mission: MissionId(999),
                        leader: people[0],
                        objective: MissionObjective::Visit {
                            destination: sim.host.state.areas[0].id,
                        },
                    },
                },
            )
            .unwrap();
            assert_eq!(
                sim.indexed_health_exposures(),
                exhaustive_exposures(&sim),
                "phase {phase}"
            );
            let expected = exhaustive_observations(&sim);
            sim.observe_health();
            assert_eq!(
                game_health::knowledges(&sim.host.storage)
                    .map(|(id, row)| (id, row.clone()))
                    .collect::<BTreeMap<_, _>>(),
                expected,
                "phase {phase}"
            );
            sim.cancel_health_activity(people[0]);
            let speakers = game_missions::talks(&sim.host.storage)
                .map(|(id, _)| id)
                .collect::<Vec<_>>();
            for id in speakers {
                game_missions::remove_talk(&mut sim.host.storage, id).unwrap();
            }
            game_physiology::body_mut(&mut sim.host.storage, people[1])
                .unwrap()
                .vitals
                .alive = false;
            sim.on_health_event(&EventKind::Died { actor: people[1] });
            assert_eq!(sim.indexed_health_exposures(), exhaustive_exposures(&sim));
        }
    }

    #[test]
    fn indexed_observations_preserve_the_first64_ids_after_roster_index_reordering() {
        let mut sim = population_fixture(90);
        let area = sim.host.state.areas[0].id;
        for p_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((p_id).0))
                .expect("location exists") = Location::AtArea(area);
        }
        add_shelters(&mut sim);
        let expected = exhaustive_observations(&sim);
        sim.person_ids.reverse();
        sim.person_index = sim
            .people()
            .enumerate()
            .map(|(index, p)| (p.id, index))
            .collect();
        sim.observe_health();
        assert_eq!(
            game_health::knowledges(&sim.host.storage)
                .map(|(id, row)| (id, row.clone()))
                .collect::<BTreeMap<_, _>>(),
            expected
        );
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
            assert_eq!(
                game_health::knowledges(&sim.host.storage)
                    .map(|(id, row)| (id, row.clone()))
                    .collect::<BTreeMap<_, _>>(),
                expected
            );
            let start = Instant::now();
            for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
                std::hint::black_box(
                    sim.health_body_delta(person_id, 1_000, sim.health_exposure(person_id))
                        .unwrap(),
                );
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
        let actor = sim.host.state.ruler;
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_actor_command(person_id, Command::Cancel).unwrap();
            let mut traits = decisions::Personality::restful();
            traits.risk_tolerance = 900;
            sim.debug_set_personality(person_id, traits, decisions::Aspiration::Comfort)
                .unwrap();
        }
        for __stock_id in game_cargo::inventories(&sim.host.storage)
            .filter(|(_, stock)| stock.good == Good::Water)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let stock = game_cargo::inventory_mut(&mut sim.host.storage, __stock_id)
                .expect("inventory exists");
            stock.quantity = Quantity(0);
        }
        sim.initialize_health(Rules::fixture()).unwrap();
        let relief = sim.host.state.health.as_ref().unwrap().rules.drink_relief;
        game_health::clinical_mut(&mut sim.host.storage, actor)
            .unwrap()
            .thirst = relief;
        sim.review_decisions().unwrap();
        let source = game_ecology::knowledge(&sim.host.storage, actor)
            .unwrap()
            .known_resources
            .values()
            .find(|source| source.kind == ResourceKind::Spring && source.area == AreaId(1))
            .unwrap()
            .id;
        let original = sim
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&sim.host.storage, source)
            .expect("component exists")
            .remaining;
        let before = sim.canonical_snapshot().unwrap();
        let command = sim.health_candidate(actor, None).unwrap();
        assert!(
            matches!(command,Command::Ecology {action:EcologyAction::Gather {resource,quantity:Quantity(1_000),unauthorized:false}} if resource==source)
        );
        for _ in 0..20 {
            assert_eq!(sim.health_candidate(actor, None), Some(command.clone()));
        }
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        sim.debug_actor_command(actor, command).unwrap();
        let complete = game_ecology::gathering(&sim.host.storage, actor)
            .unwrap()
            .completes;
        let duration = complete.0 - sim.host.clock().time.0;
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
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, source)
                .expect("component exists")
                .remaining,
            Quantity(original.0 - 1_000)
        );
        assert!(!sim.host.events().iter().any(
            |event| matches!(event.event,EventKind::Drank {actor:drinker,..} if drinker==actor)
        ));
        let command = sim.health_candidate(actor, None).unwrap();
        let Command::Health {
            action: HealthAction::Drink { source: water },
        } = command
        else {
            panic!("collected water must go through native intake: {command:?}");
        };
        let thirst = game_health::clinical(&sim.host.storage, actor)
            .unwrap()
            .thirst;
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
        let duration = sim.host.state.health.as_ref().unwrap().rules.drinking_time;
        sim.advance(duration).unwrap();
        restored.advance(duration).unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, water)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert!(
            game_health::clinical(&sim.host.storage, actor)
                .unwrap()
                .thirst
                < thirst
        );
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::Drank {actor:drinker,source,..} if drinker==actor && source==water)));
    }
    #[test]
    fn a_departed_remembered_stock_cannot_hide_a_real_local_intake_alternative() {
        let mut sim = Simulation::health_fixture(51, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let holder = sim.people().find(|person| person.id != actor).unwrap().id;
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_actor_command(person_id, Command::Cancel).unwrap();
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
        let observed_at = sim.time();
        let (_mind,stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((actor).0)).expect("mind and stock knowledge exist");
        stocks.known.retain(|_, stock| stock.good != Good::Water);
        for id in [moving, alternate] {
            stocks.known.insert(
                id,
                StockKnowledge {
                    inventory: id,
                    area: AreaId(1),
                    good: Good::Water,
                    quantity: Quantity(2_000),
                    accessible: true,
                    observed_at,
                    learned_at: observed_at,
                    source: KnowledgeSource::Observed(actor),
                },
            );
        }
        let relief = sim.host.state.health.as_ref().unwrap().rules.drink_relief;
        game_health::clinical_mut(&mut sim.host.storage, actor)
            .unwrap()
            .thirst = relief;
        assert_eq!(
            sim.health_candidate(actor, None),
            Some(Command::Health {
                action: HealthAction::Drink { source: moving }
            })
        );
        let remembered = game_characters::decisions::stocks(&sim.host.storage, actor)
            .expect("mind exists")
            .known[&moving]
            .clone();
        sim.debug_move_actor(holder, AreaId(3)).unwrap();
        sim.advance(SimDuration(1)).unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.health_candidate(actor, None),
            Some(Command::Health {
                action: HealthAction::Drink { source: alternate }
            })
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert_eq!(
            game_characters::decisions::stocks(&sim.host.storage, actor)
                .expect("mind exists")
                .known[&moving],
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
        let child = sim.host.state.ruler;
        let observer = sim
            .people()
            .find(|person| {
                person.id != child
                    && sim.independent(person.id)
                    && game_physiology::body(&sim.host.storage, person.id)
                        .unwrap()
                        .vitals
                        .alive
            })
            .unwrap()
            .id;
        for person in sim
            .people()
            .filter(|person| {
                game_physiology::body(&sim.host.storage, person.id)
                    .unwrap()
                    .vitals
                    .alive
            })
            .map(|person| person.id)
            .collect::<Vec<_>>()
        {
            sim.debug_actor_command(person, Command::Cancel).unwrap();
        }
        let Location::AtArea(area) = sim.resolved_location(child) else {
            panic!("newborn must have an actual local home");
        };
        let cost = shelter_cost(&sim.host.state.health.as_ref().unwrap().rules, 2, 650).unwrap();
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
                sim.host.clock().time,
            )
            .unwrap();
        sim.set_initial_shelter_occupancy(child, shelter).unwrap();
        let hunger = sim.body_rules(child).hunger_harm;
        game_physiology::body_mut(&mut sim.host.storage, child)
            .unwrap()
            .vitals
            .hunger = hunger;
        let thirst = sim.host.state.health.as_ref().unwrap().rules.thirst_harm;
        game_health::clinical_mut(&mut sim.host.storage, child)
            .unwrap()
            .thirst = thirst;
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, observer)
            .unwrap()
            .child_signals
            .clear();
        assert!(!sim.dependent_signal_visible(observer, child));
        sim.review_decisions().unwrap();
        assert!(
            !game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, observer)
                .expect("mind exists")
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
            matches!(*game_spatial::location(&sim.host.storage,game_ecs::StableId((child).0)).expect("location exists"),Location::Carried {carrier} if carrier==observer)
        );
        sim.debug_actor_command(
            observer,
            Command::Health {
                action: HealthAction::LeaveShelter,
            },
        )
        .unwrap();
        let destination = sim
            .host
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

pub(crate) struct PlacementFacts<'a>(pub(crate) &'a Simulation);
impl game_health::ShelterPlacementFacts for PlacementFacts<'_> {
    fn actor(&self, actor: PersonId) -> Option<game_health::ShelterActorFacts> {
        self.0.health_shelter_actor(actor)
    }
}
impl Simulation {
    pub(crate) fn health_projected_facts(
        &self,
        motion: &game_transport::PreparedMotion,
        activities: &BTreeMap<PersonId, Activity>,
    ) -> Result<LocalHealthFacts, Error> {
        let mut facts = self.health_local_facts()?;
        for actor in &mut facts.actors {
            let (anchor, location) = motion.projected_anchor(&self.host.storage, actor.actor)?;
            actor.carrier = match motion.projected_location(&self.host.storage, actor.actor) {
                Some(Location::Carried { carrier }) => Some(*carrier),
                _ => None,
            };
            actor.physical_area = match location {
                Location::AtArea(area) => Some(*area),
                _ => None,
            };
            actor.place = game_transport::position_from_location(location, self.host.clock().time)
                .ok()
                .map(|point| point.key());
            actor.moving = matches!(location, Location::InTransit { .. });
            actor.tactical = self
                .host
                .state
                .combat
                .as_ref()
                .and_then(|world| world.tactical_placement(&self.host.storage, anchor))
                .map(|place| game_health::TacticalIdentity {
                    encounter: place.encounter,
                    anchor,
                });
            if let Some(activity) = activities.get(&actor.actor) {
                actor.activity = game_health::HealthActivityFacts {
                    combat: matches!(activity, Activity::Combat { .. }),
                    cultivation: matches!(activity, Activity::Cultivate { .. }),
                    gathering: matches!(activity, Activity::Gather { .. }),
                    construction: matches!(activity, Activity::Construct { .. }),
                    play: matches!(activity, Activity::Play { .. }),
                };
            }
        }
        Ok(facts)
    }
}
