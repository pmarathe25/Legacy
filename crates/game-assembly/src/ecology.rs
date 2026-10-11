//! Harvestable natural matter renews from climate, not from a person's demand.
//! Collection creates one physical cargo lot and debits the natural deposit.
use crate::*;
pub use game_ecology::{EcologyRules, Gathering, Plot, Resource, Runtime};

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
            profiles: sim
                .host
                .state
                .people
                .iter()
                .map(|person| {
                    (
                        person.id,
                        combat::Profile {
                            initiative: if person.role == Role::Ruler { 200 } else { 100 },
                        },
                    )
                })
                .collect(),
            maps: BTreeMap::new(),
            road_maps: BTreeMap::new(),
            encounters: BTreeMap::new(),
            captivity: BTreeMap::new(),
        });
        sim.host.state.ecology = Some(Runtime {
            rules: EcologyRules::default(),
            day: sim.host.state.environment.as_ref().unwrap().weather_day,
        });
        let people = sim
            .host
            .state
            .people
            .iter()
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
    fn ecology_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive || body.vitals.health == 0 {
            return None;
        }
        match *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))? {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
    fn ecology_able(&self, actor: PersonId) -> bool {
        self.ecology_area(actor).is_some()
            && self.independent(actor)
            && !self.combat_restricts(actor)
            && !self.care_index.contains_key(&actor)
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
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))?
            .iter()
            .copied()
            .find(|id| {
                game_cargo::inventory(&self.host.storage, *(id))
                    .expect("inventory exists")
                    .good
                    == Good::Tool
                    && self.tool_available_for_planning(actor, *id, resources)
            })
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
            EcologyAction::GrantAccess {
                resource,
                recipient,
                allowed,
            } => {
                let source = self
                    .host
                    .state
                    .ecology
                    .as_ref()
                    .unwrap()
                    .resource(&self.host.storage, resource)
                    .ok_or(Error::Unavailable)?;
                if source.area != area
                    || source.custodian != Some(actor)
                    || self.ecology_area(recipient) != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(1)?;
                let access = &mut self
                    .host
                    .state
                    .ecology
                    .as_mut()
                    .unwrap()
                    .resource_mut(&mut self.host.storage, resource)
                    .unwrap()
                    .access;
                if allowed {
                    access.insert(recipient);
                } else {
                    access.remove(&recipient);
                }
                self.emit(EventKind::ResourceAccessChanged {
                    resource,
                    actor,
                    recipient,
                    allowed,
                });
                self.observe_ecology();
            }
            EcologyAction::RequestAccess { resource } => {
                if game_ecology::requests(&self.host.storage, actor)
                    .and_then(|row| row.next_request.get(&resource))
                    .is_some_and(|t| *t > self.host.clock().time)
                {
                    return Err(Error::Unavailable);
                }
                let next_request = SimTime(
                    self.host
                        .clock()
                        .time
                        .0
                        .checked_add(180_000)
                        .ok_or(Error::Overflow)?,
                );
                let source = self
                    .host
                    .state
                    .ecology
                    .as_ref()
                    .unwrap()
                    .resource(&self.host.storage, resource)
                    .ok_or(Error::Unavailable)?;
                let custodian = source.custodian.ok_or(Error::Unavailable)?;
                if source.area != area
                    || self.ecology_area(custodian) != Some(area)
                    || custodian == actor
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(2)?;
                game_ecology::ensure_requests(&mut self.host.storage, actor)
                    .expect("known requester")
                    .next_request
                    .insert(resource, next_request);
                self.emit(EventKind::ResourceAccessRequested {
                    resource,
                    actor,
                    custodian,
                });
                let mind = &game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    custodian,
                )
                .unwrap();
                if custodian != self.host.state.ruler
                    && self.ecology_able(custodian)
                    && mind.personality.empathy >= 700
                    && mind
                        .relationships
                        .get(&actor)
                        .is_none_or(|relation| relation.trust >= 0)
                {
                    self.debug_ecology_action(
                        custodian,
                        EcologyAction::GrantAccess {
                            resource,
                            recipient: actor,
                            allowed: true,
                        },
                    )?;
                } else {
                    self.emit(EventKind::ResourceAccessRefused {
                        resource,
                        actor,
                        custodian,
                    });
                }
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
        let source = self
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&self.host.storage, resource)
            .ok_or(Error::Unavailable)?;
        let adult = self.host.state.development.as_ref().is_none_or(|_| {
            matches!(
                game_lifecycle::life(&self.host.storage, actor)
                    .unwrap()
                    .stage,
                LifeStage::Adult | LifeStage::Elder
            )
        });
        let physical = self.host.state.physical.as_ref().unwrap();
        let carrying_capacity = physical.rules.carrying_capacity;
        let held = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(|id| {
                u128::from(
                    game_cargo::inventory(&self.host.storage, *(id))
                        .expect("inventory exists")
                        .quantity
                        .0,
                )
            })
            .sum::<u128>();
        let plan =
            self.host.state.ecology.as_ref().unwrap().prepare_gather(
                &self.host.storage,
                actor,
                resource,
                quantity,
                unauthorized,
                game_ecology::GatherFacts {
                    now: self.host.clock().time,
                    area: self.ecology_area(actor).ok_or(Error::Unavailable)?,
                    capable: self.ecology_able(actor)
                        && game_physiology::body(&self.host.storage, actor)
                            .expect("body exists")
                            .vitals
                            .fatigue
                            < physical.rules.fatigue_limit,
                    adult,
                    carrying_room: Quantity(
                        u128::from(carrying_capacity.0).saturating_sub(held) as u64
                    ),
                    tool: self.ecology_tool(actor),
                },
            )?;
        let completes = plan.completes;
        let good = plan.good;
        let custodian = source.custodian;
        let existing = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .find(|id| {
                game_cargo::inventory(&self.host.storage, **id)
                    .expect("inventory exists")
                    .good
                    == good
                    && game_cargo::inventory(&self.host.storage, **id)
                        .expect("inventory exists")
                        .capacity
                        .0
                        - game_cargo::inventory(&self.host.storage, **id)
                            .expect("inventory exists")
                            .quantity
                            .0
                        >= quantity.0
            })
            .copied();
        self.host
            .next_id()
            .checked_add(if existing.is_some() { 2 } else { 3 })
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(128)?;
        let destination = if let Some(existing) = existing {
            existing
        } else {
            self.add_inventory(
                good,
                InventoryLocation::Carried(actor),
                Quantity(0),
                carrying_capacity,
                BTreeSet::from([actor]),
            )?
        };
        self.interrupt_actor_activity(actor);
        game_ecology::insert_gathering(&mut self.host.storage, actor, plan.job(destination))
            .expect("validated gathering");
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Gather {
            resource,
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
        self.emit(EventKind::GatheringStarted {
            actor,
            resource,
            unauthorized,
        });
        if unauthorized
            && let Some(custodian) = custodian
            && custodian != actor
            && custodian != self.host.state.ruler
            && self.ecology_area(custodian) == self.ecology_area(actor)
            && !self.combat_restricts(custodian)
            && self.host.state.combat.is_some()
            && self.can_fight(actor)
            && self.can_fight(custodian)
        {
            let mind = &game_characters::decisions::mind::<game_api::Command>(
                &self.host.storage,
                custodian,
            )
            .unwrap();
            if mind.personality.aggression >= 500 && mind.personality.risk_tolerance >= 500 {
                self.debug_start_encounter(vec![(actor, 0), (custodian, 1)])?;
            }
        }
        self.update_property_events()?;
        Ok(())
    }
    pub(crate) fn interrupt_ecology(&mut self, actor: PersonId) {
        if let Some(world) = &mut self.host.state.ecology {
            world
                .remove_gathering(&mut self.host.storage, actor)
                .expect("registered gathering");
        }
    }
    pub(crate) fn complete_gathering(&mut self, actor: PersonId) {
        let Some(job) = self.host.state.ecology.as_mut().and_then(|world| {
            world
                .remove_gathering(&mut self.host.storage, actor)
                .expect("registered gathering")
        }) else {
            return;
        };
        let source = self
            .host
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&self.host.storage, job.resource)
            .expect("component exists")
            .clone();
        let plan = game_ecology::prepare_collection(
            &source,
            actor,
            &job,
            game_ecology::CollectionFacts {
                now: self.host.clock().time,
                area: self.ecology_area(actor).unwrap_or(source.area),
                capable: self.ecology_able(actor),
                cargo_available: self.transfer_capacity(job.destination, job.quantity.0),
                tool_available: job
                    .tool
                    .is_none_or(|tool| self.tool_available_for(actor, tool)),
            },
        );
        if let Ok(plan) = plan {
            self.host
                .state
                .ecology
                .as_mut()
                .unwrap()
                .resource_mut(&mut self.host.storage, plan.resource)
                .unwrap()
                .remaining = plan.remaining;
            game_cargo::inventory_mut(&mut self.host.storage, plan.destination)
                .unwrap()
                .quantity
                .0 += plan.quantity.0;
            if let Some(contamination) = plan.contamination {
                self.collected_water_quality(plan.destination, plan.quantity, contamination);
            }
            self.emit(EventKind::ResourceCollected {
                actor,
                resource: plan.resource,
                destination: plan.destination,
                good: plan.good,
                quantity: plan.quantity,
            });
        } else {
            self.emit(EventKind::Interrupted { actor });
        }
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
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
        let observers = self
            .host
            .state
            .people
            .iter()
            .filter_map(|person| {
                self.ecology_area(person.id)
                    .map(|area| game_ecology::ObserverArea {
                        actor: person.id,
                        area,
                    })
            })
            .collect::<Vec<_>>();
        let prepared = runtime
            .prepare_local_observations(&self.host.storage, self.host.clock().time, &observers)
            .expect("valid local ecological observations");
        prepared
            .commit(runtime, &mut self.host.storage)
            .expect("fresh local ecological observations");
    }
    pub(crate) fn ecology_view(&self, actor: PersonId) -> EcologyView {
        if self.host.state.ecology.is_none() {
            return EcologyView::default();
        }
        let Some(knowledge) = game_ecology::knowledge(&self.host.storage, actor) else {
            return EcologyView::default();
        };
        EcologyView {
            resources: knowledge
                .known_resources
                .values()
                .rev()
                .take(32)
                .cloned()
                .collect(),
            plots: knowledge
                .known_plots
                .values()
                .rev()
                .take(16)
                .cloned()
                .collect(),
        }
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
        let area = self.ecology_area(actor).unwrap();
        let Some(world) = &self.host.state.ecology else {
            return Vec::new();
        };
        let physical = self.host.state.physical.as_ref().unwrap();
        let held = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(|id| {
                u128::from(
                    game_cargo::inventory(&self.host.storage, *(id))
                        .expect("inventory exists")
                        .quantity
                        .0,
                )
            })
            .sum::<u128>();
        let facts = game_ecology::GatherFacts {
            now: self.host.clock().time,
            area,
            capable: game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .fatigue
                < physical.rules.fatigue_limit,
            adult: self.host.state.development.as_ref().is_none_or(|_| {
                matches!(
                    game_lifecycle::life(&self.host.storage, actor)
                        .unwrap()
                        .stage,
                    LifeStage::Adult | LifeStage::Elder
                )
            }),
            carrying_room: Quantity(
                u128::from(physical.rules.carrying_capacity.0).saturating_sub(held) as u64,
            ),
            tool: self.ecology_tool_for_planning(actor, resources),
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
        use game_logistics::{
            AcquisitionOffer, OfferYield, Prerequisite, PrerequisitePlacement, Requirement,
        };
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
        let available = self.available_ecology_commands_for_planning(actor, resources);
        let local = game_ecology::knowledge(&self.host.storage,actor).map(|row|&row.known_resources).into_iter().flat_map(|known| known.values()).filter(|known| known.area == area && known.kind.good() == good).map(|known| {
            let q = Quantity(quantity.0.min(room.0).min(known.available.0));
            let facts = game_ecology::GatherFacts { now: self.host.clock().time, area,
                capable: self.ecology_able(actor) && self.host.state.physical.as_ref().is_some_and(|_| game_physiology::body(&self.host.storage,actor).expect("body exists").vitals.fatigue < self.body_rules(actor).fatigue_limit),
                adult: self.host.state.development.as_ref().is_none_or(|_| matches!(game_lifecycle::life(&self.host.storage,actor).unwrap().stage, LifeStage::Adult | LifeStage::Elder)),
                carrying_room: room, tool: self.ecology_tool_for_planning(actor, resources) };
            game_ecology::LocalAcquisition { resource: known.id,
                can_prepare: world.gathering_conditions(&self.host.storage, actor, known.id, q, !known.accessible, facts).is_ok(),
                can_request: available.iter().any(|command| matches!(command, Command::Ecology {action: EcologyAction::RequestAccess {resource}} if *resource == known.id)) }
        }).collect::<Vec<_>>();
        let prospects = world.acquisition_prospects(
            &self.host.storage,
            game_ecology::AcquisitionNeed {
                actor,
                area,
                now: self.host.clock().time,
                good,
                quantity,
                room,
            },
            &local,
        );
        let mut offers = prospects
            .into_iter()
            .map(|prospect| AcquisitionOffer {
                key: prospect.resource.0,
                good,
                source: None,
                location: Location::AtArea(prospect.area),
                observed_at: prospect.observed_at,
                accessible: true,
                quantity: OfferYield::Known(prospect.quantity),
                requires_carrying_room: matches!(&prospect.action, EcologyAction::Gather { .. }),
                requirements: if prospect.needs_tool {
                    vec![Prerequisite {
                        requirement: Requirement {
                            good: Good::Tool,
                            quantity: Quantity(1_000),
                            source: None,
                        },
                        placement: PrerequisitePlacement::CarriedInOneLot,
                    }]
                } else {
                    Vec::new()
                },
                action: Command::Ecology {
                    action: prospect.action,
                },
            })
            .collect::<Vec<_>>();
        let areas = mind
            .routes
            .keys()
            .chain(mind.routes.values().flat_map(|links| links.keys()))
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        offers.extend(
            world
                .exploration_areas(
                    &self.host.storage,
                    actor,
                    area,
                    self.host.clock().time,
                    &areas,
                )
                .into_iter()
                .map(|(target, inspected)| AcquisitionOffer {
                    key: target.0,
                    good,
                    source: None,
                    location: Location::AtArea(target),
                    observed_at: inspected,
                    accessible: true,
                    quantity: OfferYield::Unknown,
                    requires_carrying_room: false,
                    requirements: Vec::new(),
                    action: Command::Move {
                        destination: target,
                    },
                }),
        );
        offers
    }
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
        let Some(world) = &self.host.state.ecology else {
            if self.host.storage.iter::<Resource>().next().is_some()
                || self.host.storage.iter::<Plot>().next().is_some()
                || game_ecology::gatherings(&self.host.storage)
                    .next()
                    .is_some()
                || game_ecology::all_knowledge(&self.host.storage)
                    .next()
                    .is_some()
                || game_ecology::all_requests(&self.host.storage)
                    .next()
                    .is_some()
            {
                return Err(Error::InvalidSave(
                    "ecological components have no runtime".into(),
                ));
            }
            return Ok(());
        };
        let bad = || Error::InvalidSave("invalid ecological deposits, plots or collection".into());
        let environment = self.host.state.environment.as_ref();
        if self.host.state.physical.is_none() || self.host.state.decisions.is_none() {
            return Err(bad());
        }
        if world.validate(&self.host.storage).is_err()
            || environment.is_some_and(|w| world.day != self.host.clock().time.0 / w.calendar.day.0)
        {
            return Err(bad());
        }
        let people = self.person_index.keys().copied().collect::<BTreeSet<_>>();
        let area_exists = |area| self.host.state.areas.iter().any(|a| a.id == area);
        for (id, s) in world.resources(&self.host.storage) {
            let id = &id;
            if *id != s.id
                || id.0 >= self.host.next_id()
                || !area_exists(s.area)
                || s.validate().is_err()
                || !s.access.is_subset(&people)
                || s.custodian.is_some_and(|p| !people.contains(&p))
            {
                return Err(bad());
            }
        }
        for (id, p) in world.plots(&self.host.storage) {
            let id = &id;
            if *id != p.id
                || id.0 >= self.host.next_id()
                || !area_exists(p.area)
                || p.validate().is_err()
                || !p.access.is_subset(&people)
                || p.custodian.is_some_and(|id| !people.contains(&id))
                || self
                    .host
                    .state
                    .agriculture
                    .as_ref()
                    .is_some_and(|w| w.field(&self.host.storage, *id).is_some())
                || game_workshop::site(&self.host.storage, *id).is_some()
            {
                return Err(bad());
            }
        }
        for (actor, job) in game_ecology::gatherings(&self.host.storage) {
            let actor = &actor;
            let source = world
                .resource(&self.host.storage, job.resource)
                .ok_or_else(bad)?;
            let destination =
                game_cargo::inventory(&self.host.storage, job.destination).ok_or_else(bad)?;
            if !people.contains(actor)
                || job.quantity.0 == 0
                || job.completes <= self.host.clock().time
                || destination.good != source.kind.good()
                || destination.location != InventoryLocation::Carried(*actor)
                || destination.capacity.0 - destination.quantity.0 < job.quantity.0
                || job.tool.is_some_and(|id| {
                    game_cargo::inventory(&self.host.storage, id).is_none_or(|tool| {
                        tool.good != Good::Tool
                            || tool.location != InventoryLocation::Carried(*actor)
                    })
                })
                || (source.kind == ResourceKind::Coppice) != job.tool.is_some()
                || !matches!(*self.host.storage.activity(*actor).expect("body exists"),
                    Activity::Gather {resource,destination,quantity,completes} if resource==job.resource && destination==job.destination && quantity==job.quantity && completes==job.completes)
                || !self
                    .host
                    .state
                    .due
                    .get(&job.completes)
                    .is_some_and(|due| due.contains(&Due::Physical(*actor)))
            {
                return Err(bad());
            }
        }
        if game_ecology::all_requests(&self.host.storage).any(|(actor, row)| {
            let actor = &actor;
            let known = &row.next_request;
            !people.contains(actor)
                || known
                    .keys()
                    .any(|resource| !world.resource(&self.host.storage, *resource).is_some())
        }) {
            return Err(bad());
        }
        if game_ecology::all_knowledge(&self.host.storage).any(|(actor, row)| {
            let actor = &actor;
            let observed = &row.observed_areas;
            !people.contains(actor)
                || observed.len() > 16
                || observed
                    .iter()
                    .any(|(area, at)| !area_exists(*area) || *at > self.host.clock().time)
        }) {
            return Err(bad());
        }
        for (actor, row) in game_ecology::all_knowledge(&self.host.storage) {
            let actor = &actor;
            let known = &row.known_resources;
            if !people.contains(actor)
                || known.iter().any(|(id, k)| {
                    *id != k.id
                        || !world.resource(&self.host.storage, *id).is_some()
                        || k.observed_at > self.host.clock().time
                        || !area_exists(k.area)
                        || k.contamination > 1_000
                        || k.available
                            > world
                                .resource(&self.host.storage, *id)
                                .expect("component exists")
                                .capacity
                        || k.custodian.is_some_and(|p| !people.contains(&p))
                })
            {
                return Err(bad());
            }
        }
        for (actor, row) in game_ecology::all_knowledge(&self.host.storage) {
            let actor = &actor;
            let known = &row.known_plots;
            if !people.contains(actor)
                || known.iter().any(|(id, k)| {
                    *id != k.id
                        || !(world.plot(&self.host.storage, *id).is_some()
                            || self
                                .host
                                .state
                                .agriculture
                                .as_ref()
                                .is_some_and(|w| w.field(&self.host.storage, *id).is_some()))
                        || k.observed_at > self.host.clock().time
                        || !area_exists(k.area)
                        || k.soil_fertility > 1_000
                        || k.custodian.is_some_and(|p| !people.contains(&p))
                })
            {
                return Err(bad());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::ecology_fixture(63, Execution::default()).unwrap();
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
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
        *game_spatial::location_mut(
            &mut sim.host.storage,
            game_ecs::StableId((sim.host.state.people[sim.person_index[&actor]].id).0),
        )
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
        let distant = sim.host.state.people[5].id;
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
        let second = sim.host.state.people[3].id;
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
        let custodian = sim.host.state.people[3].id;
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
}
