//! Harvestable natural matter renews from climate, not from a person's demand.
//! Collection creates one physical cargo lot and debits the natural deposit.
use crate::*;
pub use game_ecology::{EcologyRules, Gathering, Plot, Resource, Runtime};

/// Boundary adapters return factual rows and public bindings only.
struct EcologyPorts<'a>(&'a Simulation);
impl game_ecology::ObservationPort for EcologyPorts<'_> {
    fn actors(&self) -> Vec<PersonId> {
        self.0.people().map(|person| person.id).collect()
    }
    fn condition(&self, actor: PersonId) -> game_ecology::ActorCondition {
        self.0.ecology_condition(actor)
    }
}
impl game_ecology::SavePort for EcologyPorts<'_> {
    fn context(&self) -> game_ecology::SaveContext {
        game_ecology::SaveContext {
            now: self.0.host.clock().time,
            next_id: self.0.host.next_id(),
            physical_present: self.0.host.state.physical.is_some(),
            decisions_present: self.0.host.state.decisions.is_some(),
            environment_day: self
                .0
                .host
                .state
                .environment
                .as_ref()
                .map(|world| world.weather_day),
        }
    }
    fn actor_exists(&self, actor: PersonId) -> bool {
        self.0.person_index.contains_key(&actor)
    }
    fn area_exists(&self, area: AreaId) -> bool {
        self.0.host.state.areas.iter().any(|node| node.id == area)
    }
    fn occupied_plot(&self, site: SiteId) -> bool {
        self.cultivated_plot(site) || game_workshop::site(&self.0.host.storage, site).is_some()
    }
    fn cultivated_plot(&self, site: SiteId) -> bool {
        self.0
            .host
            .state
            .agriculture
            .as_ref()
            .is_some_and(|world| world.field(&self.0.host.storage, site).is_some())
    }
    fn inventory(&self, id: InventoryId) -> Option<game_ecology::InventoryFacts> {
        let row = game_cargo::inventory(&self.0.host.storage, id)?;
        Some(game_ecology::InventoryFacts {
            good: row.good,
            location: match row.location {
                InventoryLocation::Area(area) => game_core::StockPosition::Area(area),
                InventoryLocation::Carried(actor) => game_core::StockPosition::Carried(actor),
            },
            quantity: row.quantity,
            capacity: row.capacity,
        })
    }
    fn gathering_activity(&self, actor: PersonId) -> Option<game_ecology::GatheringActivity> {
        match self.0.host.storage.activity(actor)? {
            Activity::Gather {
                resource,
                destination,
                quantity,
                completes,
            } => Some(game_ecology::GatheringActivity {
                resource: *resource,
                destination: *destination,
                quantity: *quantity,
                completes: *completes,
            }),
            _ => None,
        }
    }
    fn physical_deadline(&self, actor: PersonId, at: SimTime) -> bool {
        self.0
            .host
            .state
            .due
            .get(&at)
            .is_some_and(|due| due.contains(&Due::Physical(actor)))
    }
}

struct PreparedGatheringCompletion {
    actor: PersonId,
    facts: game_ecology::CollectionFacts,
    activity: Activity,
    ecology: game_ecology::PreparedCollectionFinish,
    cargo: Option<game_cargo::PreparedDeposit>,
    water: Option<game_health::PreparedWaterCollection>,
}

impl Simulation {
    pub fn ecology_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        Self::ecology_with_config(seed, execution, agriculture::FarmingConfig::default())
    }
    pub fn ecology_with_config(
        seed: u64,
        execution: Execution,
        config: agriculture::FarmingConfig,
    ) -> Result<Self, Error> {
        let mut sim = Self::farming_with_config(seed, execution, config)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "ecology-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let maximum = sim.host.state.physical.as_ref().unwrap().rules.max_health;
        sim.host.state.combat = Some(combat::World {
            rules: combat::Rules {
                encounter_limit: 128,
                round: SimDuration(6_000),
                movement: 5_000,
                reach: 2_000,
                damage: maximum / 4,
                defence: maximum / 6,
                incapacitation: maximum / 4,
            },
        });
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            let person_role = sim.person(person_id).role;
            game_conflict::insert_profile(
                &mut sim.host.storage,
                person_id,
                combat::Profile {
                    initiative: if person_role == Role::Ruler { 200 } else { 100 },
                },
            )?;
        }
        sim.host.state.ecology = Some(Runtime {
            rules: EcologyRules::default(),
            day: sim.host.state.environment.as_ref().unwrap().weather_day,
        });
        let people = sim
            .people()
            .map(|person| person.id)
            .collect::<BTreeSet<_>>();
        for (kind, area, capacity, growth, contamination) in [
            (ResourceKind::Coppice, AreaId(3), 20_000, 250, 0),
            (ResourceKind::Berries, AreaId(3), 6_000, 5_000, 0),
            (ResourceKind::Herbs, AreaId(3), 3_000, 80, 0),
            (ResourceKind::Spring, AreaId(1), 50_000, 10_000, 0),
            (ResourceKind::Spring, AreaId(3), 30_000, 5_000, 200),
        ] {
            sim.debug_add_resource(
                kind,
                area,
                Quantity(capacity),
                Quantity(capacity),
                Quantity(growth),
                people.clone(),
                None,
                contamination,
            )?;
        }
        sim.observe_ecology();
        Ok(sim)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn debug_add_resource(
        &mut self,
        kind: ResourceKind,
        area: AreaId,
        capacity: Quantity,
        remaining: Quantity,
        daily_growth: Quantity,
        access: BTreeSet<PersonId>,
        custodian: Option<PersonId>,
        contamination: u16,
    ) -> Result<ResourceId, Error> {
        if self.host.state.ecology.is_none()
            || self.host.state.environment.is_none()
            || remaining > capacity
            || capacity.0 == 0
            || contamination > 1_000
            || !self.host.state.areas.iter().any(|node| node.id == area)
            || access
                .iter()
                .any(|actor| !self.person_index.contains_key(actor))
            || custodian.is_some_and(|actor| !self.person_index.contains_key(&actor))
        {
            return Err(Error::InvalidFixture("invalid natural resource".into()));
        }
        let id = ResourceId(self.host.allocate_id()?.0);
        self.host.state.ecology.as_ref().unwrap().insert_resource(
            &mut self.host.storage,
            Resource {
                id,
                area,
                kind,
                remaining,
                capacity,
                daily_growth,
                growth_remainder: 0,
                decay_permille: if kind == ResourceKind::Berries {
                    100
                } else {
                    0
                },
                soil_water: 75,
                water_capacity: 120,
                optimal_water: 50,
                minimum_temperature: 0,
                optimum_temperature: 20,
                contamination,
                runoff_contamination: contamination,
                labor_per_unit: if kind == ResourceKind::Coppice {
                    SimDuration(3_600_000)
                } else {
                    MINUTE
                },
                workers: if kind == ResourceKind::Coppice { 2 } else { 4 },
                access,
                custodian,
            },
        )?;
        Ok(id)
    }
    pub fn debug_add_plot(
        &mut self,
        area: AreaId,
        custodian: Option<PersonId>,
        access: BTreeSet<PersonId>,
        fertility: u16,
    ) -> Result<SiteId, Error> {
        if self.host.state.ecology.is_none()
            || fertility > 1_000
            || !self.host.state.areas.iter().any(|node| node.id == area)
            || access
                .iter()
                .any(|actor| !self.person_index.contains_key(actor))
            || custodian.is_some_and(|actor| !self.person_index.contains_key(&actor))
        {
            return Err(Error::InvalidFixture("invalid natural plot".into()));
        }
        let id = SiteId(self.host.allocate_id()?.0);
        self.host.state.ecology.as_ref().unwrap().insert_plot_soil(
            &mut self.host.storage,
            Plot {
                id,
                area,
                access,
                custodian,
            },
            game_land::Soil {
                fertility,
                water: 75,
                capacity: 120,
            },
        )?;
        Ok(id)
    }
    fn ecology_condition(&self, actor: PersonId) -> game_ecology::ActorCondition {
        let rules = self.host.state.physical.as_ref().map(|world| &world.rules);
        game_ecology::ActorCondition {
            location: game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
                .cloned(),
            vitals: game_physiology::body(&self.host.storage, actor)
                .map(|body| body.vitals.clone()),
            stage: self.host.state.development.as_ref().and_then(|_| {
                game_lifecycle::life(&self.host.storage, actor).map(|life| life.stage)
            }),
            development_present: self.host.state.development.is_some(),
            combat_restricted: self.combat_restricts(actor),
            receiving_care: self.care_index.contains_key(&actor),
            fatigue_limit: rules.map_or(0, |rules| rules.fatigue_limit),
            carrying_capacity: rules.map_or(Quantity(0), |rules| rules.carrying_capacity),
        }
    }
    fn ecology_area(&self, actor: PersonId) -> Option<AreaId> {
        self.ecology_condition(actor).observation_area()
    }
    fn ecology_able(&self, actor: PersonId) -> bool {
        self.ecology_condition(actor).able()
    }
    pub(crate) fn ecology_job_tool(&self, actor: PersonId) -> Option<InventoryId> {
        game_ecology::gathering(&self.host.storage, actor)?.tool
    }
    fn ecology_tool(&self, actor: PersonId) -> Option<InventoryId> {
        self.ecology_tool_for_planning(actor, None)
    }

    fn ecology_tool_for_planning(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Option<InventoryId> {
        game_ecology::Runtime::select_tool(
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))?
                .iter()
                .copied()
                .map(|id| game_ecology::ToolCandidate {
                    id,
                    good: game_cargo::inventory(&self.host.storage, id)
                        .expect("inventory exists")
                        .good,
                }),
            |id| self.tool_available_for_planning(actor, id, resources),
        )
    }
    pub fn debug_ecology_action(
        &mut self,
        actor: PersonId,
        action: impl std::borrow::Borrow<EcologyAction>,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let action = action.borrow().clone();
        if !self.ecology_able(actor) || self.host.state.ecology.is_none() {
            return Err(Error::Unavailable);
        }
        let area = self.ecology_area(actor).unwrap();
        match action {
            EcologyAction::GrantAccess { .. } | EcologyAction::RequestAccess { .. } => {
                let runtime = self.host.state.ecology.as_ref().unwrap();
                let prepared = runtime.prepare_access(
                    &self.host.storage,
                    self.host.clock().time,
                    &action,
                    game_ecology::AccessFacts {
                        actor,
                        area,
                        capable: self.ecology_able(actor),
                    },
                    |person| self.ecology_area(person),
                )?;
                let effect = prepared.effect().clone();
                let (response, refused) = match effect {
                    game_ecology::AccessEffect::Requested {
                        resource,
                        actor,
                        custodian,
                    } => {
                        let mind = game_characters::decisions::mind::<game_api::Command>(
                            &self.host.storage,
                            custodian,
                        )
                        .unwrap();
                        let response = game_characters::resource_responses::permission_response(
                            &mind.personality,
                            mind.relationships.get(&actor),
                            game_characters::resource_responses::ResponseFacts {
                                autonomous: custodian != self.host.state.ruler,
                                capable: self.ecology_able(custodian),
                            },
                        );
                        match response {
                            game_characters::resource_responses::PermissionResponse::Grant => (
                                Some(runtime.prepare_access(
                                    &self.host.storage,
                                    self.host.clock().time,
                                    &EcologyAction::GrantAccess {
                                        resource,
                                        recipient: actor,
                                        allowed: true,
                                    },
                                    game_ecology::AccessFacts {
                                        actor: custodian,
                                        area,
                                        capable: self.ecology_able(custodian),
                                    },
                                    |person| self.ecology_area(person),
                                )?),
                                None,
                            ),
                            game_characters::resource_responses::PermissionResponse::Refuse => {
                                (None, Some((resource, actor, custodian)))
                            }
                        }
                    }
                    game_ecology::AccessEffect::Changed { .. } => (None, None),
                };
                self.ensure_event_capacity(if response.is_some() || refused.is_some() {
                    2
                } else {
                    1
                })?;
                // Match the previous nested command's additional revision, but
                // admit it before any request or access component is published.
                let response_revision = u64::from(response.is_some());
                self.host.preflight_records(0, 0, 1 + response_revision)?;
                let mut batch = game_ecs::Batch::new();
                let (effects, guard) = prepared.stage_with_response(
                    response,
                    runtime,
                    &self.host.storage,
                    &mut batch,
                )?;
                guard.validate(&self.host.storage, &batch)?;
                self.host.storage.apply_batch(batch)?;
                for effect in effects {
                    match effect {
                        game_ecology::AccessEffect::Requested {
                            resource,
                            actor,
                            custodian,
                        } => {
                            self.emit(EventKind::ResourceAccessRequested {
                                resource,
                                actor,
                                custodian,
                            });
                        }
                        game_ecology::AccessEffect::Changed {
                            resource,
                            actor,
                            recipient,
                            allowed,
                        } => {
                            self.emit(EventKind::ResourceAccessChanged {
                                resource,
                                actor,
                                recipient,
                                allowed,
                            });
                            self.observe_ecology();
                        }
                    }
                }
                if let Some((resource, actor, custodian)) = refused {
                    self.emit(EventKind::ResourceAccessRefused {
                        resource,
                        actor,
                        custodian,
                    });
                }
                self.host.bump_revision(response_revision)?;
            }
            EcologyAction::Gather {
                resource,
                quantity,
                unauthorized,
            } => self.start_gathering(actor, resource, quantity, unauthorized)?,
        }
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn start_gathering(
        &mut self,
        actor: PersonId,
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
    ) -> Result<(), Error> {
        let runtime = self.host.state.ecology.as_ref().ok_or(Error::Unavailable)?;
        let condition = self.ecology_condition(actor);
        let carrying_capacity = condition.carrying_capacity;
        let held = game_cargo::carried_quantity_in(
            &self.host.storage,
            actor,
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .copied(),
        )?;
        let prepared = runtime.prepare_gathering_start(
            &self.host.storage,
            actor,
            resource,
            quantity,
            unauthorized,
            condition.gather_facts(
                self.host.clock().time,
                u128::from(held.0),
                self.ecology_tool(actor),
            )?,
        )?;
        let completes = prepared.plan().completes;
        let good = prepared.plan().good;
        let opposition = prepared.opposition();
        let existing = game_cargo::compatible_carried_destination_in(
            &self.host.storage,
            actor,
            good,
            quantity,
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .copied(),
        );
        let destination = existing.unwrap_or(InventoryId(self.host.next_id()));
        let creation = if existing.is_none() {
            Some(game_cargo::prepare_inventory_creation(
                &self.host.storage,
                Inventory {
                    id: destination,
                    location: InventoryLocation::Carried(actor),
                    quantity: Quantity(0),
                    capacity: carrying_capacity,
                    access: BTreeSet::from([actor]),
                    last_granted: None,
                    good,
                    spoils_at: None,
                },
            )?)
        } else {
            None
        };
        let creation_count = u64::from(creation.is_some());
        let resistance = opposition.filter(|custodian| {
            let mind = game_characters::decisions::mind::<game_api::Command>(
                &self.host.storage,
                *custodian,
            )
            .unwrap();
            game_characters::resource_responses::unauthorized_use_reaction(
                &mind.personality,
                game_characters::resource_responses::OppositionFacts {
                    manual: *custodian == self.host.state.ruler,
                    co_present: self.ecology_area(*custodian) == self.ecology_area(actor),
                    restricted: self.combat_restricts(*custodian),
                    combat_available: self.host.state.combat.is_some(),
                    user_can_fight: self
                        .host
                        .state
                        .combat
                        .as_ref()
                        .is_some_and(|_| self.can_fight(actor)),
                    custodian_can_fight: self
                        .host
                        .state
                        .combat
                        .as_ref()
                        .is_some_and(|_| self.can_fight(*custodian)),
                },
            ) == game_characters::resource_responses::UnauthorizedUseResponse::Resist
        });
        // The receiving Conflict owner admits its complete opening before the
        // natural-resource attempt, empty compartment or receipt is published.
        let encounter = resistance
            .map(|custodian| {
                let participants = [(actor, 0), (custodian, 1)];
                let opening = self.prepare_encounter_opening(&participants)?;
                let area = opening.area();
                let first_id = self
                    .host
                    .next_id()
                    .checked_add(creation_count)
                    .ok_or(Error::Overflow)?;
                let scene = self.prepare_combat_location_map_at(opening.location(), first_id)?;
                let map_count = u64::from(scene.is_new());
                let id = EncounterId(first_id.checked_add(map_count).ok_or(Error::Overflow)?);
                let fatigue = participants
                    .iter()
                    .map(|(actor, _)| {
                        (
                            *actor,
                            game_physiology::body(&self.host.storage, *actor)
                                .expect("admitted combat body")
                                .vitals
                                .fatigue,
                        )
                    })
                    .collect::<Vec<_>>();
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
                        scene.map(),
                        &participants,
                        &fatigue,
                        self.host.clock().time,
                    )?;
                Ok::<_, Error>((custodian, id, area, map_count + 1, scene, start))
            })
            .transpose()?;
        let mut actors = vec![actor];
        if let Some((custodian, ..)) = &encounter {
            actors.push(*custodian);
        }
        let interruptions =
            self.prepare_interruptions(&actors, commands::ActivityOwner::Ecology)?;
        let event_count = usize::try_from(interruptions.event_count()?)
            .map_err(|_| Error::Overflow)?
            .checked_add(if encounter.is_some() { 3 } else { 1 })
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(event_count)?;
        let ids = creation_count
            .checked_add(encounter.as_ref().map_or(0, |entry| entry.3))
            .ok_or(Error::Overflow)?;
        let encounter_revision = u64::from(encounter.is_some());
        self.host.preflight_records(
            ids,
            u64::try_from(event_count).map_err(|_| Error::Overflow)?,
            1 + encounter_revision,
        )?;
        let transaction_opening = self.host.transaction_opening();
        let mut batch = game_ecs::Batch::new();
        let mut guards = Vec::new();
        let mut final_activities = BTreeMap::new();
        let opened = if let Some((custodian, id, area, _, scene, start)) = encounter {
            let (effect, guard) =
                prepared.stage_opposed(&self.host.storage, custodian, &mut batch)?;
            guards.push(guard);
            guards.push(scene.stage_into(&self.host.storage, &mut batch)?);
            guards.push(start.stage_into(
                self.host.state.combat.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                &mut batch,
            )?);
            for participant in &actors {
                final_activities.insert(*participant, Activity::Combat { encounter: id });
            }
            Some((custodian, id, area, effect))
        } else {
            guards.push(prepared.stage_into(&self.host.storage, destination, &mut batch)?);
            final_activities.insert(
                actor,
                Activity::Gather {
                    resource,
                    destination,
                    quantity,
                    completes,
                },
            );
            None
        };
        if let Some(creation) = creation {
            guards.push(creation.stage_guarded_into(&self.host.storage, &mut batch)?);
        }
        let mut interrupted = interruptions.stage_into(self, &mut batch, &final_activities)?;
        interrupted.validate_batch(self, &batch)?;
        if let Some((custodian, id, area, effect)) = opened {
            let mut events = interrupted.events_for(&[actor]);
            events.push(EventKind::GatheringStarted {
                actor: effect.actor,
                resource: effect.resource,
                unauthorized: effect.unauthorized,
            });
            events.push(EventKind::Interrupted {
                actor: effect.actor,
            });
            events.extend(interrupted.events_for(&[custodian]));
            events.push(EventKind::EncounterStarted {
                encounter: id,
                area,
            });
            for guard in &guards {
                guard.validate(&self.host.storage, &batch)?;
            }
            let followups = self.prepare_combat_opening_followups_with_property(
                id,
                &batch,
                ids,
                &events,
                encounter_revision,
            )?;
            let (transaction, finish) = followups.into_transaction(self)?;
            // Preserve the source owner's untouched-resource and scoped
            // reservation witnesses across the admitted NPC preview.
            for guard in &guards {
                guard.validate(&self.host.storage, &batch)?;
            }
            interrupted.validate_batch(self, &batch)?;
            self.host.preflight_records(
                transaction.ids,
                u64::try_from(transaction.events.len()).map_err(|_| Error::Overflow)?,
                transaction
                    .revisions
                    .checked_add(1)
                    .ok_or(Error::Overflow)?,
            )?;
            self.host.commit_transaction(transaction)?;
            if creation_count != 0 {
                let location = game_cargo::inventory(&self.host.storage, destination)
                    .expect("admitted receiving compartment")
                    .location
                    .clone();
                self.inventory_index
                    .entry(location)
                    .or_default()
                    .push(destination);
            }
            interrupted.finish_actor_metadata(self, actor);
            interrupted.finish_actor_metadata(self, custodian);
            finish.finish(self);
            return Ok(());
        } else {
            let mut transaction = game_world::Transaction::new(transaction_opening, batch);
            transaction.ids = ids;
            transaction.guards = guards;
            self.host.commit_transaction(transaction)?;
            if creation_count != 0 {
                self.inventory_index
                    .entry(InventoryLocation::Carried(actor))
                    .or_default()
                    .push(destination);
            }
            interrupted.finish_actor(self, actor);
            self.emit(EventKind::GatheringStarted {
                actor,
                resource,
                unauthorized,
            });
            self.host
                .state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(actor));
        }
        self.update_property_events()?;
        Ok(())
    }
    fn gathering_facts(&self, actor: PersonId) -> Result<game_ecology::CollectionFacts, Error> {
        let runtime = self.host.state.ecology.as_ref().ok_or(Error::Unavailable)?;
        let job = runtime
            .gathering(&self.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        let source = runtime
            .resource(&self.host.storage, job.resource)
            .ok_or(Error::Unavailable)?;
        Ok(game_ecology::CollectionFacts {
            now: self.host.clock().time,
            area: self.ecology_area(actor).unwrap_or(source.area),
            capable: self.ecology_able(actor),
            cargo_available: self.transfer_capacity(job.destination, job.quantity.0)
                && game_cargo::deposit_available(
                    &self.host.storage,
                    job.destination,
                    source.kind.good(),
                    job.quantity,
                ),
            tool_available: job
                .tool
                .is_none_or(|tool| self.tool_available_after_release(actor, tool)),
        })
    }
    fn prepare_gathering_completion(
        &self,
        actor: PersonId,
    ) -> Result<Option<PreparedGatheringCompletion>, Error> {
        let Some(runtime) = self.host.state.ecology.as_ref() else {
            return Ok(None);
        };
        if runtime.gathering(&self.host.storage, actor).is_none() {
            return Ok(None);
        }
        let facts = self.gathering_facts(actor)?;
        let Some(ecology) = runtime.prepare_collection_finish(&self.host.storage, actor, facts)?
        else {
            return Ok(None);
        };
        let (cargo, water) = match ecology.outcome() {
            game_ecology::CollectionOutcome::Collected(effect) => {
                let cargo = game_cargo::prepare_deposit(
                    &self.host.storage,
                    effect.destination,
                    effect.good,
                    effect.quantity,
                )?;
                let water = effect
                    .contamination
                    .map(|contamination| {
                        game_health::prepare_water_collection(
                            &self.host.storage,
                            game_health::WaterCollectionFacts {
                                inventory: effect.destination,
                                quantity_after: cargo.quantity_after(),
                                collected: effect.quantity,
                                contamination,
                                holder: match *cargo.location() {
                                    InventoryLocation::Carried(holder) => Some(holder),
                                    InventoryLocation::Area(_) => None,
                                },
                                now: self.host.clock().time,
                            },
                        )
                    })
                    .transpose()?;
                (Some(cargo), water)
            }
            game_ecology::CollectionOutcome::Interrupted { .. } => (None, None),
        };
        let activity = self
            .host
            .storage
            .activity(actor)
            .ok_or(Error::Unavailable)?
            .clone();
        Ok(Some(PreparedGatheringCompletion {
            actor,
            facts,
            activity,
            ecology,
            cargo,
            water,
        }))
    }
    fn commit_gathering_completion(
        &mut self,
        prepared: PreparedGatheringCompletion,
    ) -> Result<(), Error> {
        let runtime = self.host.state.ecology.as_ref().ok_or(Error::Unavailable)?;
        prepared.ecology.validate(runtime, &self.host.storage)?;
        if self.gathering_facts(prepared.actor)? != prepared.facts
            || self.host.storage.activity(prepared.actor) != Some(&prepared.activity)
        {
            return Err(Error::Unavailable);
        }
        if let Some(cargo) = &prepared.cargo {
            cargo.validate(&self.host.storage)?;
        }
        if let Some(water) = &prepared.water {
            water.validate(&self.host.storage)?;
        }
        self.ensure_event_capacity(1)?;
        let mut batch = game_ecs::Batch::new();
        let (outcome, ecology_guard) =
            prepared
                .ecology
                .stage_into(runtime, &self.host.storage, &mut batch)?;
        let cargo_guard = prepared
            .cargo
            .map(|cargo| cargo.stage_into(&self.host.storage, &mut batch))
            .transpose()?;
        let has_water = prepared.water.is_some();
        let water_guard = prepared
            .water
            .map(|water| water.stage_into(&self.host.storage, &mut batch))
            .transpose()?;
        batch.insert(game_ecs::StableId(prepared.actor.0), Activity::Idle);
        ecology_guard.validate(&self.host.storage, &batch)?;
        if let Some(guard) = cargo_guard {
            guard.validate(&self.host.storage, &batch)?;
        }
        if let Some(guard) = water_guard {
            guard.validate(&self.host.storage, &batch)?;
        }
        self.host.storage.apply_batch(batch)?;
        if has_water {
            self.host
                .state
                .health
                .get_or_insert_with(health::Runtime::default);
        }
        match outcome {
            game_ecology::CollectionOutcome::Collected(effect) => {
                self.emit(EventKind::ResourceCollected {
                    actor: prepared.actor,
                    resource: effect.resource,
                    destination: effect.destination,
                    good: effect.good,
                    quantity: effect.quantity,
                })
            }
            game_ecology::CollectionOutcome::Interrupted { actor } => {
                self.emit(EventKind::Interrupted { actor })
            }
        }
        Ok(())
    }
    pub(crate) fn complete_gathering(&mut self, actor: PersonId) -> Result<(), Error> {
        if let Some(prepared) = self.prepare_gathering_completion(actor)? {
            self.commit_gathering_completion(prepared)?;
        }
        Ok(())
    }
    /// Closing collection uses fresh post-renewal/post-transfer facts. Admit
    /// all possible accepted shapes before the clock: fixed schema validators
    /// allow source/stock quantities within capacities, water load<=quantity*1000
    /// and own observation identity. The existing physical event envelope covers
    /// its single receipt. No collection creates a stable entity or changes good,
    /// capacity or identity; unavailable closing facts yield Interrupted.
    pub(crate) fn preflight_ecology_collections(
        &self,
        next: SimTime,
        due: &[Due],
        renewed: Option<&game_ecology::PreparedAdvance>,
    ) -> Result<(), Error> {
        let Some(runtime) = self.host.state.ecology.as_ref() else {
            return Ok(());
        };
        let changed: BTreeMap<_, _> = renewed
            .into_iter()
            .flat_map(|rows| rows.changed_resources())
            .map(|(id, row)| (*id, row))
            .collect();
        for deadline in due {
            let Due::Physical(actor) = deadline else {
                continue;
            };
            if !matches!(
                self.host.storage.activity(*actor),
                Some(Activity::Gather { .. })
            ) {
                continue;
            }
            let Some(job) = runtime.gathering(&self.host.storage, *actor) else {
                continue;
            };
            let source = changed
                .get(&job.resource)
                .copied()
                .or_else(|| runtime.resource(&self.host.storage, job.resource))
                .ok_or(Error::Unavailable)?;
            let mut batch = game_ecs::Batch::new();
            let collection = runtime.append_collection_schema_bounds(
                &self.host.storage,
                *actor,
                source,
                &mut batch,
            )?;
            let bounds = game_cargo::append_deposit_schema_bounds(
                &self.host.storage,
                collection.destination,
                collection.good,
                &mut batch,
            )?;
            if collection.water {
                game_health::append_water_collection_schema_bounds(
                    &self.host.storage,
                    collection.destination,
                    bounds.capacity,
                    match bounds.location {
                        InventoryLocation::Carried(holder) => Some(holder),
                        InventoryLocation::Area(_) => None,
                    },
                    next,
                    &mut batch,
                )?;
            }
            batch.insert(game_ecs::StableId(actor.0), Activity::Idle);
            self.host.storage.validate_batch(&batch)?;
        }
        Ok(())
    }
    pub(crate) fn evaluate_ecology(
        &self,
        day: Option<&game_environment::LocalDayFacts>,
    ) -> Result<Option<game_ecology::PreparedAdvance>, Error> {
        let (Some(world), Some(day)) = (&self.host.state.ecology, day) else {
            return Ok(None);
        };
        let day = game_ecology::LocalWeatherDay {
            day: day.day,
            day_of_year: day.day_of_year,
            cells: day
                .cells
                .iter()
                .map(|(id, w)| {
                    (
                        *id,
                        game_ecology::WeatherFacts {
                            temperature: w.temperature,
                            rain: w.rain,
                            evaporation: w.evaporation,
                        },
                    )
                })
                .collect(),
        };
        world
            .prepare_advance(
                &self.host.storage,
                &self.ecology_queries,
                Some(&day),
                |inputs| {
                    self.engine
                        .executor()
                        .map_ordered(inputs, |input| input.evaluate())
                },
                |inputs| {
                    self.engine
                        .executor()
                        .map_ordered(inputs, |input| input.evaluate())
                },
            )
            .map(Some)
    }
    pub(crate) fn commit_ecology(
        &mut self,
        output: Option<game_ecology::PreparedAdvance>,
    ) -> Result<(), Error> {
        if let Some(output) = output {
            self.host
                .state
                .ecology
                .as_mut()
                .ok_or(Error::Unavailable)?
                .commit_advance(&mut self.host.storage, output)?;
        }
        Ok(())
    }
    pub(crate) fn observe_ecology(&mut self) {
        let Some(runtime) = &self.host.state.ecology else {
            return;
        };
        let prepared = runtime
            .prepare_observations(
                &self.host.storage,
                self.host.clock().time,
                &EcologyPorts(self),
            )
            .expect("valid local ecological observations");
        prepared
            .commit(runtime, &mut self.host.storage)
            .expect("fresh local ecological observations");
    }
    pub(crate) fn ecology_view(&self, actor: PersonId) -> EcologyView {
        self.host
            .state
            .ecology
            .as_ref()
            .map_or_else(EcologyView::default, |runtime| {
                runtime.view(&self.host.storage, actor)
            })
    }
    #[cfg(test)]
    pub(crate) fn available_ecology_commands(&self, actor: PersonId) -> Vec<Command> {
        self.available_ecology_commands_for_planning(actor, None)
    }

    fn available_ecology_commands_for_planning(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        for opportunity in self.ecology_quantity_opportunities_for_planning(actor, resources) {
            commands.extend(
                opportunity
                    .suggested
                    .iter()
                    .map(|quantity| opportunity.action.command(*quantity)),
            );
        }
        commands.extend(self.available_ecology_access_commands(actor));
        commands
    }
    pub(crate) fn available_ecology_access_commands(&self, actor: PersonId) -> Vec<Command> {
        if !self.ecology_able(actor) {
            return Vec::new();
        }
        let area = self.ecology_area(actor).unwrap();
        let Some(world) = &self.host.state.ecology else {
            return Vec::new();
        };
        world
            .local_access_actions(&self.host.storage, actor, area, |custodian| {
                self.ecology_area(custodian)
            })
            .into_iter()
            .map(|action| Command::Ecology { action })
            .collect()
    }
    pub(crate) fn ecology_quantity_opportunities(
        &self,
        actor: PersonId,
    ) -> Vec<QuantityOpportunity> {
        self.ecology_quantity_opportunities_for_planning(actor, None)
    }

    fn ecology_quantity_opportunities_for_planning(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<QuantityOpportunity> {
        if !self.ecology_able(actor) || self.past_life_controls_actor(actor) {
            return Vec::new();
        }
        let Some(world) = &self.host.state.ecology else {
            return Vec::new();
        };
        let Ok(held) = game_cargo::carried_quantity_in(
            &self.host.storage,
            actor,
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .copied(),
        ) else {
            return Vec::new();
        };
        let Ok(facts) = self.ecology_condition(actor).gather_facts(
            self.host.clock().time,
            u128::from(held.0),
            self.ecology_tool_for_planning(actor, resources),
        ) else {
            return Vec::new();
        };
        world
            .local_gather_opportunities(&self.host.storage, actor, facts, |good, upper_bound| {
                self.quantity_suggestions(actor, good, upper_bound, Quantity(1), None, None)
            })
            .into_iter()
            .map(|opportunity| QuantityOpportunity {
                action: QuantityAction::Gather {
                    resource: opportunity.resource,
                    unauthorized: opportunity.unauthorized,
                },
                upper_bound: opportunity.upper_bound,
                increment: Quantity(1),
                suggested: opportunity.suggested,
            })
            .collect()
    }
    pub(crate) fn ecology_acquisition_offers(
        &self,
        actor: PersonId,
        good: Good,
        quantity: Quantity,
        room: Quantity,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<game_logistics::AcquisitionOffer<Command>> {
        let Some(world) = &self.host.state.ecology else {
            return Vec::new();
        };
        let Some(area) = self.ecology_area(actor) else {
            return Vec::new();
        };
        let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return Vec::new();
        };
        let local_actions = self
            .available_ecology_commands_for_planning(actor, resources)
            .into_iter()
            .filter_map(|command| match command {
                Command::Ecology { action } => Some(action),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut condition = self.ecology_condition(actor);
        if self.host.state.physical.is_some() {
            condition.fatigue_limit = self.body_rules(actor).fatigue_limit;
        }
        let Ok(mut facts) = condition.gather_facts(
            self.host.clock().time,
            0,
            self.ecology_tool_for_planning(actor, resources),
        ) else {
            return Vec::new();
        };
        facts.carrying_room = room;
        let areas = mind
            .routes
            .keys()
            .chain(mind.routes.values().flat_map(|links| links.keys()))
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        world
            .acquisition_offers(
                &self.host.storage,
                game_ecology::AcquisitionNeed {
                    actor,
                    area,
                    now: self.host.clock().time,
                    good,
                    quantity,
                    room,
                },
                facts,
                &local_actions,
                &areas,
            )
            .into_iter()
            .map(|offer| {
                offer.map_action(|action| match action {
                    game_ecology::AcquisitionAction::Resource(action) => {
                        Command::Ecology { action }
                    }
                    game_ecology::AcquisitionAction::Explore { destination } => {
                        Command::Move { destination }
                    }
                })
            })
            .collect()
    }
    pub(crate) fn ecology_provision_opportunities(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<(Command, game_logistics::Requirement)> {
        let Some(world) = &self.host.state.ecology else {
            return Vec::new();
        };
        if !self.ecology_able(actor) {
            return Vec::new();
        }
        let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return Vec::new();
        };
        let Some(area) = self.ecology_area(actor) else {
            return Vec::new();
        };
        let local_actions = self
            .available_ecology_commands_for_planning(actor, resources)
            .into_iter()
            .filter_map(|command| match command {
                Command::Ecology { action } => Some(action),
                _ => None,
            })
            .collect::<Vec<_>>();
        world
            .provision_opportunities(
                &self.host.storage,
                actor,
                area,
                &local_actions,
                self.host.state.physical.as_ref().unwrap().rules.meal_size,
                |destination| decisions::known_path(&mind.routes, area, destination).is_ok(),
            )
            .into_iter()
            .map(|opportunity| {
                let command = match opportunity.action {
                    game_ecology::AcquisitionAction::Resource(action) => {
                        Command::Ecology { action }
                    }
                    game_ecology::AcquisitionAction::Explore { destination } => {
                        Command::Move { destination }
                    }
                };
                (command, opportunity.requirement)
            })
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn ecology_opportunities(
        &self,
        actor: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> Vec<Command> {
        let Some(world) = &self.host.state.ecology else {
            return Vec::new();
        };
        if !self.ecology_able(actor) {
            return Vec::new();
        }
        let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
        }) else {
            return Vec::new();
        };
        let Some(area) = self.ecology_area(actor) else {
            return Vec::new();
        };
        let local_actions = self
            .available_ecology_commands_for_planning(actor, resources)
            .into_iter()
            .filter_map(|command| match command {
                Command::Ecology { action } => Some(action),
                _ => None,
            })
            .collect::<Vec<_>>();
        let options = world.autonomous_options(
            &self.host.storage,
            actor,
            area,
            &local_actions,
            |destination| decisions::known_path(&mind.routes, area, destination).is_ok(),
        );
        options
            .actions
            .into_iter()
            .map(|action| Command::Ecology { action })
            .chain(
                options
                    .destinations
                    .into_iter()
                    .map(|destination| Command::Move { destination }),
            )
            .collect()
    }
    pub(crate) fn validate_ecology(&self) -> Result<(), Error> {
        game_ecology::validate_save(
            self.host.state.ecology.as_ref(),
            &self.host.storage,
            &EcologyPorts(self),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::ecology_fixture(63, Execution::default()).unwrap();
        for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            sim.debug_actor_command(actor, Command::Cancel).unwrap();
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        sim
    }
    fn resource(sim: &Simulation, kind: ResourceKind, area: AreaId) -> ResourceId {
        sim.host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resources(&sim.host.storage)
            .map(|(_, value)| value)
            .find(|r| r.kind == kind && r.area == area)
            .unwrap()
            .id
    }
    fn local(sim: &mut Simulation, actor: PersonId, area: AreaId) {
        *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((actor).0))
            .expect("location exists") = Location::AtArea(area);
        sim.observe_ecology();
    }
    #[test]
    fn gathering_debits_deposit_only_after_real_labor_and_preserves_physical_quantity() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let before = sim
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&sim.host.storage, spring)
            .expect("component exists")
            .remaining;
        sim.debug_ecology_action(
            actor,
            EcologyAction::Gather {
                resource: spring,
                quantity: Quantity(1_000),
                unauthorized: false,
            },
        )
        .unwrap();
        let destination = game_ecology::gathering(&sim.host.storage, actor)
            .unwrap()
            .destination;
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        sim.advance(SimDuration(59_999)).unwrap();
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, spring)
                .expect("component exists")
                .remaining,
            before
        );
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, spring)
                .expect("component exists")
                .remaining,
            Quantity(before.0 - 1_000)
        );
        sim.validate_ecology().unwrap();
    }
    #[test]
    fn pending_collection_restores_across_partial_time_and_worker_order_without_new_yield() {
        let mut whole = fixture();
        let actor = whole.host.state.ruler;
        let source = resource(&whole, ResourceKind::Spring, AreaId(1));
        whole
            .debug_ecology_action(
                actor,
                EcologyAction::Gather {
                    resource: source,
                    quantity: Quantity(1_000),
                    unauthorized: false,
                },
            )
            .unwrap();
        let mut split = Simulation::load(
            &whole.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        split.advance(SimDuration(17_001)).unwrap();
        split = Simulation::load(
            &split.save().unwrap(),
            Execution {
                workers: 2,
                reverse_jobs: true,
            },
        )
        .unwrap();
        split.advance(SimDuration(42_999)).unwrap();
        whole.advance(MINUTE).unwrap();
        assert_eq!(whole.canonical_state(), split.canonical_state());
        let state = split.canonical_snapshot().unwrap();
        for _ in 0..10 {
            let _ = split.ecology_view(actor);
            let _ = split.available_ecology_commands(actor);
        }
        assert_eq!(split.canonical_snapshot().unwrap(), state);
    }
    #[test]
    fn cancelling_does_not_create_material_and_observations_remain_personal() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let distant = sim.person_at(5).id;
        local(&mut sim, distant, AreaId(3));
        let old = sim.ecology_view(distant);
        sim.debug_ecology_action(
            actor,
            EcologyAction::Gather {
                resource: spring,
                quantity: Quantity(1_000),
                unauthorized: false,
            },
        )
        .unwrap();
        let destination = game_ecology::gathering(&sim.host.storage, actor)
            .unwrap()
            .destination;
        sim.advance(SimDuration(30_000)).unwrap();
        sim.debug_actor_command(actor, Command::Cancel).unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            sim.ecology_view(distant)
                .resources
                .iter()
                .find(|s| s.id == spring),
            old.resources.iter().find(|s| s.id == spring)
        );
    }
    #[test]
    fn forest_needs_reachable_equipment_and_limited_yield_is_reserved_once() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        local(&mut sim, actor, AreaId(3));
        let coppice = resource(&sim, ResourceKind::Coppice, AreaId(3));
        assert_eq!(
            sim.debug_ecology_action(
                actor,
                EcologyAction::Gather {
                    resource: coppice,
                    quantity: Quantity(1_000),
                    unauthorized: false
                }
            ),
            Err(Error::Unavailable)
        );
        sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Carried(actor),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
        sim.host
            .state
            .ecology
            .as_mut()
            .unwrap()
            .resource_mut(&mut sim.host.storage, coppice)
            .unwrap()
            .remaining = Quantity(1_000);
        sim.debug_ecology_action(
            actor,
            EcologyAction::Gather {
                resource: coppice,
                quantity: Quantity(1_000),
                unauthorized: false,
            },
        )
        .unwrap();
        let second = sim.person_at(3).id;
        local(&mut sim, second, AreaId(3));
        sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Carried(second),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([second]),
        )
        .unwrap();
        assert_eq!(
            sim.debug_ecology_action(
                second,
                EcologyAction::Gather {
                    resource: coppice,
                    quantity: Quantity(1_000),
                    unauthorized: false
                }
            ),
            Err(Error::Unavailable)
        );
        sim.advance(SimDuration(3_600_000)).unwrap();
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, coppice)
                .expect("component exists")
                .remaining,
            Quantity(0)
        );
    }
    #[test]
    fn unauthorized_gathering_can_trigger_actual_combat_before_any_goods_move() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let custodian = sim.person_at(3).id;
        local(&mut sim, custodian, AreaId(1));
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let source = sim
            .host
            .state
            .ecology
            .as_mut()
            .unwrap()
            .resource_mut(&mut sim.host.storage, spring)
            .unwrap();
        source.access = BTreeSet::from([custodian]);
        source.custodian = Some(custodian);
        let before = source.remaining;
        let mut mind = decisions::Personality::restful();
        mind.aggression = 900;
        mind.risk_tolerance = 900;
        sim.debug_set_personality(custodian, mind, decisions::Aspiration::Comfort)
            .unwrap();
        let first_event = sim.host.events().len();
        sim.debug_ecology_action(
            actor,
            EcologyAction::Gather {
                resource: spring,
                quantity: Quantity(1_000),
                unauthorized: true,
            },
        )
        .unwrap();
        assert!(sim.in_combat(actor));
        assert!(!game_ecology::gathering(&sim.host.storage, actor).is_some());
        let events = &sim.host.events()[first_event..];
        let started = events.iter().position(|event| matches!(event.event, EventKind::GatheringStarted { actor: who, .. } if who == actor)).unwrap();
        let interrupted = events.iter().position(|event| matches!(event.event, EventKind::Interrupted { actor: who } if who == actor)).unwrap();
        let resisted = events
            .iter()
            .position(|event| matches!(event.event, EventKind::EncounterStarted { .. }))
            .unwrap();
        assert!(started < interrupted && interrupted < resisted);
        assert!(
            !sim.host
                .state
                .due
                .values()
                .flatten()
                .any(|deadline| matches!(deadline, Due::Physical(who) if *who == actor))
        );
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, spring)
                .expect("component exists")
                .remaining,
            before
        );
    }
    #[test]
    fn refused_resistance_opening_preserves_gathering_compartments_ids_and_receipts() {
        for exhausted_revisions in [false, true] {
            let mut sim = fixture();
            let actor = sim.host.state.ruler;
            let custodian = sim.person_at(3).id;
            local(&mut sim, custodian, AreaId(1));
            let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
            let source = sim
                .host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource_mut(&mut sim.host.storage, spring)
                .unwrap();
            source.access = BTreeSet::from([custodian]);
            source.custodian = Some(custodian);
            let mut mind = decisions::Personality::restful();
            mind.aggression = 900;
            mind.risk_tolerance = 900;
            sim.debug_set_personality(custodian, mind, decisions::Aspiration::Comfort)
                .unwrap();
            if exhausted_revisions {
                let mut clock = sim.host.clock();
                clock.revision = u64::MAX - 1;
                game_sim::ClockWorld::publish_clock(&mut sim.host, clock);
            } else {
                sim.host
                    .storage
                    .remove::<game_conflict::Profile>(game_ecs::StableId(custodian.0))
                    .unwrap();
            }
            let before = sim.canonical_snapshot().unwrap();
            let result = sim.debug_ecology_action(
                actor,
                EcologyAction::Gather {
                    resource: spring,
                    quantity: Quantity(1_000),
                    unauthorized: true,
                },
            );
            assert!(result.is_err());
            assert_eq!(sim.canonical_snapshot().unwrap(), before);
            assert!(game_ecology::gathering(&sim.host.storage, actor).is_none());
        }
    }
    #[test]
    fn exhausted_npc_opening_prefix_preserves_the_entire_resource_attempt() {
        let mut sim = fixture();
        let actor = sim.person_at(3).id;
        let custodian = sim.person_at(4).id;
        local(&mut sim, actor, AreaId(1));
        local(&mut sim, custodian, AreaId(1));
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let source = sim
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource_mut(&mut sim.host.storage, spring)
            .unwrap();
        source.access = BTreeSet::from([custodian]);
        source.custodian = Some(custodian);
        let mut mind = decisions::Personality::restful();
        mind.aggression = 900;
        mind.risk_tolerance = 900;
        sim.debug_set_personality(custodian, mind, decisions::Aspiration::Comfort)
            .unwrap();
        let mut clock = sim.host.clock();
        // The origin fits, but two autonomous turns plus the enclosing source
        // command exceed this revision envelope.
        clock.revision = u64::MAX - 3;
        game_sim::ClockWorld::publish_clock(&mut sim.host, clock);
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.debug_ecology_action(
                actor,
                EcologyAction::Gather {
                    resource: spring,
                    quantity: Quantity(1_000),
                    unauthorized: true
                }
            ),
            Err(Error::Overflow)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }
    #[test]
    fn warm_wet_days_renew_depleted_sources_while_cold_days_do_not() {
        let mut sim = fixture();
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        sim.host
            .state
            .ecology
            .as_mut()
            .unwrap()
            .resource_mut(&mut sim.host.storage, spring)
            .unwrap()
            .remaining = Quantity(0);
        let warm = game_environment::LocalDayFacts {
            day: 1,
            day_of_year: 0,
            cells: sim
                .host
                .state
                .areas
                .iter()
                .map(|area| {
                    (
                        area.id,
                        game_environment::Weather {
                            temperature: 20,
                            rain: 50,
                            evaporation: 0,
                        },
                    )
                })
                .collect(),
        };
        sim.ecology_queries.refresh(&sim.host.storage).unwrap();
        let renewed = sim.evaluate_ecology(Some(&warm)).unwrap().unwrap();
        assert_eq!(
            renewed
                .changed_resources()
                .find(|(id, _)| **id == spring)
                .map_or_else(
                    || sim
                        .host
                        .state
                        .ecology
                        .as_ref()
                        .unwrap()
                        .resource(&sim.host.storage, spring)
                        .unwrap()
                        .remaining,
                    |(_, source)| source.remaining
                ),
            Quantity(10_000)
        );
        let cold = game_environment::LocalDayFacts {
            cells: warm
                .cells
                .iter()
                .map(|(id, w)| {
                    (
                        *id,
                        game_environment::Weather {
                            temperature: -5,
                            ..w.clone()
                        },
                    )
                })
                .collect(),
            ..warm
        };
        let frozen = sim.evaluate_ecology(Some(&cold)).unwrap().unwrap();
        assert_eq!(
            frozen
                .changed_resources()
                .find(|(id, _)| **id == spring)
                .map_or_else(
                    || sim
                        .host
                        .state
                        .ecology
                        .as_ref()
                        .unwrap()
                        .resource(&sim.host.storage, spring)
                        .unwrap()
                        .remaining,
                    |(_, source)| source.remaining
                ),
            Quantity(0)
        );
    }
    #[test]
    fn a_scheduled_transfer_fills_the_compartment_before_collection_and_interrupts_without_debit() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let source = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                Quantity(1000),
                Quantity(1000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let destination = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Carried(actor),
                Quantity(0),
                Quantity(1000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let before = sim
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&sim.host.storage, spring)
            .unwrap()
            .remaining;
        sim.debug_ecology_action(
            actor,
            EcologyAction::Gather {
                resource: spring,
                quantity: Quantity(1000),
                unauthorized: false,
            },
        )
        .unwrap();
        let job = game_ecology::gathering(&sim.host.storage, actor).unwrap();
        assert_eq!(job.destination, destination);
        let completes = job.completes;
        sim.debug_schedule_transfers(
            completes,
            vec![TransferIntent {
                actor,
                source,
                destination,
                quantity: Quantity(1000),
            }],
        )
        .unwrap();
        sim.advance(SimDuration(completes.0 - sim.host.clock().time.0))
            .unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .unwrap()
                .quantity,
            Quantity(0)
        );
        let stock = game_cargo::inventory(&sim.host.storage, destination).unwrap();
        assert_eq!(stock.quantity, stock.capacity);
        assert_eq!(
            sim.host
                .state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.host.storage, spring)
                .unwrap()
                .remaining,
            before
        );
        assert!(game_ecology::gathering(&sim.host.storage, actor).is_none());
        let events = sim.canonical_state().events;
        let moved=events.iter().position(|event|matches!(event.event,EventKind::Transferred{actor:who,source:from,destination:to,..} if who==actor && from==source && to==destination)).unwrap();
        let stopped = events
            .iter()
            .position(|event| matches!(event.event,EventKind::Interrupted{actor:who} if who==actor))
            .unwrap();
        assert!(moved < stopped);
        assert!(!events.iter().any(|event|matches!(event.event,EventKind::ResourceCollected{actor:who,resource,..} if who==actor && resource==spring)));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn a_prepared_collection_rejects_stale_stock_or_foreign_place_before_any_publication() {
        for change_stock in [false, true] {
            let mut sim = fixture();
            let actor = sim.host.state.ruler;
            let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
            sim.debug_ecology_action(
                actor,
                EcologyAction::Gather {
                    resource: spring,
                    quantity: Quantity(1000),
                    unauthorized: false,
                },
            )
            .unwrap();
            let job = game_ecology::gathering(&sim.host.storage, actor).unwrap();
            let destination = job.destination;
            // This is the enclosing interval's closing read phase, after clock publication.
            let mut clock = sim.host.clock();
            clock.time = job.completes;
            game_sim::ClockWorld::publish_clock(&mut sim.host, clock);
            let prepared = sim.prepare_gathering_completion(actor).unwrap().unwrap();
            assert!(prepared.cargo.is_some());
            if change_stock {
                game_cargo::inventory_mut(&mut sim.host.storage, destination)
                    .unwrap()
                    .quantity = Quantity(1);
            } else {
                *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId(actor.0))
                    .unwrap() = Location::AtArea(AreaId(2));
            }
            let before = sim.canonical_snapshot().unwrap();
            assert_eq!(
                sim.commit_gathering_completion(prepared),
                Err(Error::Unavailable)
            );
            assert_eq!(sim.canonical_snapshot().unwrap(), before);
            assert!(game_ecology::gathering(&sim.host.storage, actor).is_some());
        }
    }
    #[test]
    fn pending_collection_and_all_other_state_survive_an_unrepresentable_public_advance() {
        let mut sim = fixture();
        let actor = sim.host.state.ruler;
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        sim.debug_ecology_action(
            actor,
            EcologyAction::Gather {
                resource: spring,
                quantity: Quantity(1000),
                unauthorized: false,
            },
        )
        .unwrap();
        sim.advance(SimDuration(1)).unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.advance(SimDuration(u64::MAX)), Err(Error::Overflow));
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert!(game_ecology::gathering(&sim.host.storage, actor).is_some());
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
}
