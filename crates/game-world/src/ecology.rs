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
        sim.state.scenario = ScenarioInfo {
            id: "ecology-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let maximum = sim.state.physical.as_ref().unwrap().rules.max_health;
        sim.state.combat = Some(combat::World {
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
        sim.state.ecology = Some(Runtime {
            rules: EcologyRules::default(),
            jobs: BTreeMap::new(),
            known: BTreeMap::new(),
            known_plots: BTreeMap::new(),
            observed_areas: BTreeMap::new(),
            day: sim.state.environment.as_ref().unwrap().weather_day,
            next_request: BTreeMap::new(),
        });
        let people = sim
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
        if self.state.ecology.is_none()
            || self.state.environment.is_none()
            || remaining > capacity
            || capacity.0 == 0
            || contamination > 1_000
            || !self.state.areas.iter().any(|node| node.id == area)
            || access
                .iter()
                .any(|actor| !self.person_index.contains_key(actor))
            || custodian.is_some_and(|actor| !self.person_index.contains_key(&actor))
        {
            return Err(Error::InvalidFixture("invalid natural resource".into()));
        }
        let id = ResourceId(allocate(&mut self.state)?);
        self.state.ecology.as_ref().unwrap().insert_resource(
            &mut self.state.components,
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
        if self.state.ecology.is_none()
            || fertility > 1_000
            || !self.state.areas.iter().any(|node| node.id == area)
            || access
                .iter()
                .any(|actor| !self.person_index.contains_key(actor))
            || custodian.is_some_and(|actor| !self.person_index.contains_key(&actor))
        {
            return Err(Error::InvalidFixture("invalid natural plot".into()));
        }
        let id = SiteId(allocate(&mut self.state)?);
        self.state.ecology.as_ref().unwrap().insert_plot(
            &mut self.state.components,
            Plot {
                id,
                area,
                fertility,
                soil_water: 75,
                water_capacity: 120,
                access,
                custodian,
            },
        )?;
        Ok(id)
    }
    fn ecology_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = self.state.components.body(actor)?;
        if !body.vitals.alive || body.vitals.health == 0 {
            return None;
        }
        match *self.state.components.location(actor)? {
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
        self.state.ecology.as_ref()?.jobs.get(&actor)?.tool
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
                self.state.inventories[id].good == Good::Tool
                    && self.tool_available_for_planning(actor, *id, resources)
            })
    }
    pub fn debug_ecology_action(
        &mut self,
        actor: PersonId,
        action: impl std::borrow::Borrow<EcologyAction>,
    ) -> Result<(), Error> {
        let action = action.borrow().clone();
        if !self.ecology_able(actor) || self.state.ecology.is_none() {
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
                    .state
                    .ecology
                    .as_ref()
                    .unwrap()
                    .resource(&self.state.components, resource)
                    .ok_or(Error::Unavailable)?;
                if source.area != area
                    || source.custodian != Some(actor)
                    || self.ecology_area(recipient) != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(1)?;
                let access = &mut self
                    .state
                    .ecology
                    .as_mut()
                    .unwrap()
                    .resource_mut(&mut self.state.components, resource)
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
                if self
                    .state
                    .ecology
                    .as_ref()
                    .unwrap()
                    .next_request
                    .get(&actor)
                    .and_then(|k| k.get(&resource))
                    .is_some_and(|t| *t > self.state.time)
                {
                    return Err(Error::Unavailable);
                }
                let next_request = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(180_000)
                        .ok_or(Error::Overflow)?,
                );
                let source = self
                    .state
                    .ecology
                    .as_ref()
                    .unwrap()
                    .resource(&self.state.components, resource)
                    .ok_or(Error::Unavailable)?;
                let custodian = source.custodian.ok_or(Error::Unavailable)?;
                if source.area != area
                    || self.ecology_area(custodian) != Some(area)
                    || custodian == actor
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(2)?;
                self.state
                    .ecology
                    .as_mut()
                    .unwrap()
                    .next_request
                    .entry(actor)
                    .or_default()
                    .insert(resource, next_request);
                self.emit(EventKind::ResourceAccessRequested {
                    resource,
                    actor,
                    custodian,
                });
                let mind = &self.state.decisions.as_ref().unwrap().minds[&custodian];
                if custodian != self.state.ruler
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
        self.revision += 1;
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
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&self.state.components, resource)
            .ok_or(Error::Unavailable)?;
        let adult = self.state.development.as_ref().is_none_or(|world| {
            matches!(
                world.lives[&actor].stage,
                LifeStage::Adult | LifeStage::Elder
            )
        });
        let physical = self.state.physical.as_ref().unwrap();
        let carrying_capacity = physical.rules.carrying_capacity;
        let held = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(|id| u128::from(self.state.inventories[id].quantity.0))
            .sum::<u128>();
        let plan =
            self.state.ecology.as_ref().unwrap().prepare_gather(
                &self.state.components,
                actor,
                resource,
                quantity,
                unauthorized,
                game_ecology::GatherFacts {
                    now: self.state.time,
                    area: self.ecology_area(actor).ok_or(Error::Unavailable)?,
                    capable: self.ecology_able(actor)
                        && self
                            .state
                            .components
                            .body(actor)
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
                self.state.inventories[id].good == good
                    && self.state.inventories[id].capacity.0 - self.state.inventories[id].quantity.0
                        >= quantity.0
            })
            .copied();
        self.state
            .next_id
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
        self.state
            .ecology
            .as_mut()
            .unwrap()
            .jobs
            .insert(actor, plan.job(destination));
        *self.state.components.activity_mut(actor).unwrap() = Activity::Gather {
            resource,
            destination,
            quantity,
            completes,
        };
        self.state
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
            && custodian != self.state.ruler
            && self.ecology_area(custodian) == self.ecology_area(actor)
            && !self.combat_restricts(custodian)
            && self.state.combat.is_some()
            && self.can_fight(actor)
            && self.can_fight(custodian)
        {
            let mind = &self.state.decisions.as_ref().unwrap().minds[&custodian];
            if mind.personality.aggression >= 500 && mind.personality.risk_tolerance >= 500 {
                self.debug_start_encounter(vec![(actor, 0), (custodian, 1)])?;
            }
        }
        self.update_property_events()?;
        Ok(())
    }
    pub(crate) fn interrupt_ecology(&mut self, actor: PersonId) {
        if let Some(world) = &mut self.state.ecology {
            world.jobs.remove(&actor);
        }
    }
    pub(crate) fn complete_gathering(&mut self, actor: PersonId) {
        let Some(job) = self
            .state
            .ecology
            .as_mut()
            .and_then(|world| world.jobs.remove(&actor))
        else {
            return;
        };
        let source = self
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&self.state.components, job.resource)
            .expect("component exists")
            .clone();
        let plan = game_ecology::prepare_collection(
            &source,
            actor,
            &job,
            game_ecology::CollectionFacts {
                now: self.state.time,
                area: self.ecology_area(actor).unwrap_or(source.area),
                capable: self.ecology_able(actor),
                cargo_available: self.transfer_capacity(job.destination, job.quantity.0),
                tool_available: job
                    .tool
                    .is_none_or(|tool| self.tool_available_for(actor, tool)),
            },
        );
        if let Ok(plan) = plan {
            self.state
                .ecology
                .as_mut()
                .unwrap()
                .resource_mut(&mut self.state.components, plan.resource)
                .unwrap()
                .remaining = plan.remaining;
            self.state
                .inventories
                .get_mut(&plan.destination)
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
        *self.state.components.activity_mut(actor).unwrap() = Activity::Idle;
    }
    pub(crate) fn evaluate_ecology(
        &self,
        day: Option<&game_environment::LocalDayFacts>,
    ) -> Result<Option<game_ecology::PreparedAdvance>, Error> {
        let (Some(world), Some(day)) = (&self.state.ecology, day) else {
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
                &self.state.components,
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
            self.state
                .ecology
                .as_mut()
                .ok_or(Error::Unavailable)?
                .commit_advance(&mut self.state.components, output)?;
        }
        Ok(())
    }
    pub(crate) fn observe_ecology(&mut self) {
        let Some(world) = &self.state.ecology else {
            return;
        };
        let resources = world
            .resources(&self.state.components)
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        let plots = world
            .plots(&self.state.components)
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        for actor in self
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>()
        {
            let Some(area) = self.ecology_area(actor) else {
                continue;
            };
            self.state
                .ecology
                .as_mut()
                .unwrap()
                .observe_area(actor, area, self.state.time);
            for source in resources.iter().filter(|source| source.area == area) {
                self.state
                    .ecology
                    .as_mut()
                    .unwrap()
                    .known
                    .entry(actor)
                    .or_default()
                    .insert(
                        source.id,
                        ResourceView {
                            id: source.id,
                            area,
                            kind: source.kind,
                            available: source.remaining,
                            accessible: source.access.contains(&actor),
                            custodian: source.custodian,
                            contamination: source.contamination,
                            observed_at: self.state.time,
                        },
                    );
            }
            for plot in plots.iter().filter(|plot| plot.area == area) {
                self.state
                    .ecology
                    .as_mut()
                    .unwrap()
                    .known_plots
                    .entry(actor)
                    .or_default()
                    .insert(
                        plot.id,
                        PlotView {
                            id: plot.id,
                            area,
                            soil_fertility: plot.fertility,
                            soil_water: plot.soil_water,
                            accessible: plot.access.contains(&actor),
                            custodian: plot.custodian,
                            observed_at: self.state.time,
                        },
                    );
            }
        }
    }
    pub(crate) fn ecology_view(&self, actor: PersonId) -> EcologyView {
        let Some(world) = &self.state.ecology else {
            return EcologyView::default();
        };
        EcologyView {
            resources: world
                .known
                .get(&actor)
                .map(|known| known.values().rev().take(32).cloned().collect())
                .unwrap_or_default(),
            plots: world
                .known_plots
                .get(&actor)
                .map(|known| known.values().rev().take(16).cloned().collect())
                .unwrap_or_default(),
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
        let Some(world) = &self.state.ecology else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        for source in world
            .resources(&self.state.components)
            .map(|(_, value)| value)
            .filter(|s| s.area == area)
            .take(16)
        {
            if !source.access.contains(&actor)
                && source
                    .custodian
                    .is_some_and(|c| c != actor && self.ecology_area(c) == Some(area))
            {
                commands.push(Command::Ecology {
                    action: EcologyAction::RequestAccess {
                        resource: source.id,
                    },
                });
            }
        }
        commands
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
        let Some(world) = &self.state.ecology else {
            return Vec::new();
        };
        let physical = self.state.physical.as_ref().unwrap();
        let held = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(|id| u128::from(self.state.inventories[id].quantity.0))
            .sum::<u128>();
        let facts = game_ecology::GatherFacts {
            now: self.state.time,
            area,
            capable: self
                .state
                .components
                .body(actor)
                .expect("body exists")
                .vitals
                .fatigue
                < physical.rules.fatigue_limit,
            adult: self.state.development.as_ref().is_none_or(|w| {
                matches!(w.lives[&actor].stage, LifeStage::Adult | LifeStage::Elder)
            }),
            carrying_room: Quantity(
                u128::from(physical.rules.carrying_capacity.0).saturating_sub(held) as u64,
            ),
            tool: self.ecology_tool_for_planning(actor, resources),
        };
        let mut opportunities = Vec::new();
        for source in world
            .resources(&self.state.components)
            .map(|(_, value)| value)
            .filter(|source| source.area == area)
            .take(16)
        {
            let Ok(upper_bound) =
                world.gather_upper_bound(&self.state.components, actor, source.id, facts)
            else {
                continue;
            };
            let unauthorized = !source.access.contains(&actor);
            let suggested = self
                .quantity_suggestions(
                    actor,
                    source.kind.good(),
                    upper_bound,
                    Quantity(1),
                    None,
                    None,
                )
                .into_iter()
                .filter(|quantity| {
                    world
                        .prepare_gather(
                            &self.state.components,
                            actor,
                            source.id,
                            *quantity,
                            unauthorized,
                            facts,
                        )
                        .is_ok()
                })
                .collect::<Vec<_>>();
            if !suggested.is_empty() {
                opportunities.push(QuantityOpportunity {
                    action: QuantityAction::Gather {
                        resource: source.id,
                        unauthorized,
                    },
                    upper_bound,
                    increment: Quantity(1),
                    suggested,
                });
            }
        }
        opportunities
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
        let Some(world) = &self.state.ecology else {
            return Vec::new();
        };
        let Some(area) = self.ecology_area(actor) else {
            return Vec::new();
        };
        let Some(mind) = self
            .state
            .decisions
            .as_ref()
            .and_then(|w| w.minds.get(&actor))
        else {
            return Vec::new();
        };
        let available = self.available_ecology_commands_for_planning(actor, resources);
        let local = world.known.get(&actor).into_iter().flat_map(|known| known.values()).filter(|known| known.area == area && known.kind.good() == good).map(|known| {
            let q = Quantity(quantity.0.min(room.0).min(known.available.0));
            let facts = game_ecology::GatherFacts { now: self.state.time, area,
                capable: self.ecology_able(actor) && self.state.physical.as_ref().is_some_and(|_| self.state.components.body(actor).expect("body exists").vitals.fatigue < self.body_rules(actor).fatigue_limit),
                adult: self.state.development.as_ref().is_none_or(|w| matches!(w.lives[&actor].stage, LifeStage::Adult | LifeStage::Elder)),
                carrying_room: room, tool: self.ecology_tool_for_planning(actor, resources) };
            game_ecology::LocalAcquisition { resource: known.id,
                can_prepare: world.gathering_conditions(&self.state.components, actor, known.id, q, !known.accessible, facts).is_ok(),
                can_request: available.iter().any(|command| matches!(command, Command::Ecology {action: EcologyAction::RequestAccess {resource}} if *resource == known.id)) }
        }).collect::<Vec<_>>();
        let prospects = world.acquisition_prospects(
            game_ecology::AcquisitionNeed {
                actor,
                area,
                now: self.state.time,
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
                .exploration_areas(actor, area, self.state.time, &areas)
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
        let Some(world) = &self.state.ecology else {
            return Vec::new();
        };
        if !self.ecology_able(actor) || world.jobs.contains_key(&actor) {
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
        let Some(area) = self.ecology_area(actor) else {
            return Vec::new();
        };
        let Some(known) = world.known.get(&actor) else {
            return Vec::new();
        };
        let mut commands = self
            .available_ecology_commands_for_planning(actor, resources)
            .into_iter()
            .filter(|command| match command {
                Command::Ecology {
                    action:
                        EcologyAction::Gather { resource, .. }
                        | EcologyAction::RequestAccess { resource },
                } => known.contains_key(resource),
                _ => false,
            })
            .collect::<Vec<_>>();
        let destinations = known
            .values()
            .filter(|source| {
                source.available.0 > 0
                    && source.area != area
                    && decisions::known_path(&mind.routes, area, source.area).is_ok()
            })
            .map(|source| source.area)
            .collect::<BTreeSet<_>>();
        commands.extend(
            destinations
                .into_iter()
                .map(|destination| Command::Move { destination }),
        );
        commands
    }
    pub(crate) fn validate_ecology(&self) -> Result<(), Error> {
        let Some(world) = &self.state.ecology else {
            if self.state.components.iter::<Resource>().next().is_some()
                || self.state.components.iter::<Plot>().next().is_some()
            {
                return Err(Error::InvalidSave(
                    "ecological components have no runtime".into(),
                ));
            }
            return Ok(());
        };
        let bad = || Error::InvalidSave("invalid ecological deposits, plots or collection".into());
        let environment = self.state.environment.as_ref();
        if self.state.physical.is_none() || self.state.decisions.is_none() {
            return Err(bad());
        }
        if world.validate(&self.state.components).is_err()
            || environment.is_some_and(|w| world.day != self.state.time.0 / w.calendar.day.0)
        {
            return Err(bad());
        }
        let people = self.person_index.keys().copied().collect::<BTreeSet<_>>();
        let area_exists = |area| self.state.areas.iter().any(|a| a.id == area);
        for (id, s) in world.resources(&self.state.components) {
            let id = &id;
            if *id != s.id
                || id.0 >= self.state.next_id
                || !area_exists(s.area)
                || s.validate().is_err()
                || !s.access.is_subset(&people)
                || s.custodian.is_some_and(|p| !people.contains(&p))
            {
                return Err(bad());
            }
        }
        for (id, p) in world.plots(&self.state.components) {
            let id = &id;
            if *id != p.id
                || id.0 >= self.state.next_id
                || !area_exists(p.area)
                || p.validate().is_err()
                || !p.access.is_subset(&people)
                || p.custodian.is_some_and(|id| !people.contains(&id))
                || self
                    .state
                    .agriculture
                    .as_ref()
                    .is_some_and(|w| w.field(&self.state.components, *id).is_some())
                || self.state.components.site(*id).is_some()
            {
                return Err(bad());
            }
        }
        for (actor, job) in &world.jobs {
            let source = world
                .resource(&self.state.components, job.resource)
                .ok_or_else(bad)?;
            let destination = self
                .state
                .inventories
                .get(&job.destination)
                .ok_or_else(bad)?;
            if !people.contains(actor)
                || job.quantity.0 == 0
                || job.completes <= self.state.time
                || destination.good != source.kind.good()
                || destination.location != InventoryLocation::Carried(*actor)
                || destination.capacity.0 - destination.quantity.0 < job.quantity.0
                || job.tool.is_some_and(|id| {
                    self.state.inventories.get(&id).is_none_or(|tool| {
                        tool.good != Good::Tool
                            || tool.location != InventoryLocation::Carried(*actor)
                    })
                })
                || (source.kind == ResourceKind::Coppice) != job.tool.is_some()
                || !matches!(*self.state.components.activity(*actor).expect("body exists"),
                    Activity::Gather {resource,destination,quantity,completes} if resource==job.resource && destination==job.destination && quantity==job.quantity && completes==job.completes)
                || !self
                    .state
                    .due
                    .get(&job.completes)
                    .is_some_and(|due| due.contains(&Due::Physical(*actor)))
            {
                return Err(bad());
            }
        }
        if world.next_request.iter().any(|(actor, known)| {
            !people.contains(actor)
                || known
                    .keys()
                    .any(|resource| !world.resource(&self.state.components, *resource).is_some())
        }) {
            return Err(bad());
        }
        if world.observed_areas.iter().any(|(actor, observed)| {
            !people.contains(actor)
                || observed.len() > 16
                || observed
                    .iter()
                    .any(|(area, at)| !area_exists(*area) || *at > self.state.time)
        }) {
            return Err(bad());
        }
        for (actor, known) in &world.known {
            if !people.contains(actor)
                || known.iter().any(|(id, k)| {
                    *id != k.id
                        || !world.resource(&self.state.components, *id).is_some()
                        || k.observed_at > self.state.time
                        || !area_exists(k.area)
                        || k.contamination > 1_000
                        || k.available
                            > world
                                .resource(&self.state.components, *id)
                                .expect("component exists")
                                .capacity
                        || k.custodian.is_some_and(|p| !people.contains(&p))
                })
            {
                return Err(bad());
            }
        }
        for (actor, known) in &world.known_plots {
            if !people.contains(actor)
                || known.iter().any(|(id, k)| {
                    *id != k.id
                        || !(world.plot(&self.state.components, *id).is_some()
                            || self
                                .state
                                .agriculture
                                .as_ref()
                                .is_some_and(|w| w.field(&self.state.components, *id).is_some()))
                        || k.observed_at > self.state.time
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
        for actor in sim.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
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
        sim.state
            .ecology
            .as_ref()
            .unwrap()
            .resources(&sim.state.components)
            .map(|(_, value)| value)
            .find(|r| r.kind == kind && r.area == area)
            .unwrap()
            .id
    }
    fn local(sim: &mut Simulation, actor: PersonId, area: AreaId) {
        *sim.state
            .components
            .location_mut(sim.state.people[sim.person_index[&actor]].id)
            .expect("location exists") = Location::AtArea(area);
        sim.observe_ecology();
    }
    #[test]
    fn gathering_debits_deposit_only_after_real_labor_and_preserves_physical_quantity() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let before = sim
            .state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&sim.state.components, spring)
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
        let destination = sim.state.ecology.as_ref().unwrap().jobs[&actor].destination;
        assert_eq!(sim.state.inventories[&destination].quantity, Quantity(0));
        sim.advance(SimDuration(59_999)).unwrap();
        assert_eq!(
            sim.state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.state.components, spring)
                .expect("component exists")
                .remaining,
            before
        );
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(
            sim.state.inventories[&destination].quantity,
            Quantity(1_000)
        );
        assert_eq!(
            sim.state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.state.components, spring)
                .expect("component exists")
                .remaining,
            Quantity(before.0 - 1_000)
        );
        sim.validate_ecology().unwrap();
    }
    #[test]
    fn pending_collection_restores_across_partial_time_and_worker_order_without_new_yield() {
        let mut whole = fixture();
        let actor = whole.state.ruler;
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
        assert_eq!(whole.state, split.state);
        let state = split.state.clone();
        for _ in 0..10 {
            let _ = split.ecology_view(actor);
            let _ = split.available_ecology_commands(actor);
        }
        assert_eq!(split.state, state);
    }
    #[test]
    fn cancelling_does_not_create_material_and_observations_remain_personal() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let distant = sim.state.people[5].id;
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
        let destination = sim.state.ecology.as_ref().unwrap().jobs[&actor].destination;
        sim.advance(SimDuration(30_000)).unwrap();
        sim.debug_actor_command(actor, Command::Cancel).unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        assert_eq!(sim.state.inventories[&destination].quantity, Quantity(0));
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
        let actor = sim.state.ruler;
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
        sim.state
            .ecology
            .as_mut()
            .unwrap()
            .resource_mut(&mut sim.state.components, coppice)
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
        let second = sim.state.people[3].id;
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
            sim.state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.state.components, coppice)
                .expect("component exists")
                .remaining,
            Quantity(0)
        );
    }
    #[test]
    fn unauthorized_gathering_can_trigger_actual_combat_before_any_goods_move() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let custodian = sim.state.people[3].id;
        local(&mut sim, custodian, AreaId(1));
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let source = sim
            .state
            .ecology
            .as_mut()
            .unwrap()
            .resource_mut(&mut sim.state.components, spring)
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
        assert!(
            !sim.state
                .ecology
                .as_ref()
                .unwrap()
                .jobs
                .contains_key(&actor)
        );
        assert_eq!(
            sim.state
                .ecology
                .as_ref()
                .unwrap()
                .resource(&sim.state.components, spring)
                .expect("component exists")
                .remaining,
            before
        );
    }
    #[test]
    fn warm_wet_days_renew_depleted_sources_while_cold_days_do_not() {
        let mut sim = fixture();
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        sim.state
            .ecology
            .as_mut()
            .unwrap()
            .resource_mut(&mut sim.state.components, spring)
            .unwrap()
            .remaining = Quantity(0);
        let warm = game_environment::LocalDayFacts {
            day: 1,
            day_of_year: 0,
            cells: sim
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
        sim.ecology_queries.refresh(&sim.state.components).unwrap();
        let renewed = sim.evaluate_ecology(Some(&warm)).unwrap().unwrap();
        assert_eq!(
            renewed
                .changed_resources()
                .find(|(id, _)| **id == spring)
                .map_or_else(
                    || sim
                        .state
                        .ecology
                        .as_ref()
                        .unwrap()
                        .resource(&sim.state.components, spring)
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
                        .state
                        .ecology
                        .as_ref()
                        .unwrap()
                        .resource(&sim.state.components, spring)
                        .unwrap()
                        .remaining,
                    |(_, source)| source.remaining
                ),
            Quantity(0)
        );
    }
}
