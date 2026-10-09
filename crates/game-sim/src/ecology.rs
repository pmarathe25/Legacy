//! Harvestable natural matter renews from climate, not from a person's demand.
//! Collection creates one physical cargo lot and debits the natural deposit.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub id: ResourceId,
    pub area: AreaId,
    pub kind: ResourceKind,
    pub remaining: Quantity,
    pub capacity: Quantity,
    pub daily_growth: Quantity,
    pub growth_remainder: u64,
    pub decay_permille: u16,
    pub soil_water: u64,
    pub water_capacity: u64,
    pub optimal_water: u64,
    pub minimum_temperature: i32,
    pub optimum_temperature: i32,
    pub contamination: u16,
    pub runoff_contamination: u16,
    pub labor_per_unit: SimDuration,
    pub workers: usize,
    pub access: BTreeSet<PersonId>,
    pub custodian: Option<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plot {
    pub id: SiteId,
    pub area: AreaId,
    pub fertility: u16,
    pub soil_water: u64,
    pub water_capacity: u64,
    pub access: BTreeSet<PersonId>,
    pub custodian: Option<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gathering {
    pub resource: ResourceId,
    pub destination: InventoryId,
    pub quantity: Quantity,
    pub tool: Option<InventoryId>,
    pub completes: SimTime,
    pub unauthorized: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub resources: BTreeMap<ResourceId, Resource>,
    pub plots: BTreeMap<SiteId, Plot>,
    pub jobs: BTreeMap<PersonId, Gathering>,
    pub known: BTreeMap<PersonId, BTreeMap<ResourceId, ResourceView>>,
    pub known_plots: BTreeMap<PersonId, BTreeMap<SiteId, PlotView>>,
    pub day: u64,
    #[serde(default)]
    pub next_request: BTreeMap<PersonId, BTreeMap<ResourceId, SimTime>>,
}
impl Simulation {
    pub fn ecology_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        Self::ecology_with_config(seed, execution, environment::Config::default())
    }
    pub fn ecology_with_config(
        seed: u64,
        execution: Execution,
        config: environment::Config,
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
            encounters: BTreeMap::new(),
            captivity: BTreeMap::new(),
        });
        sim.state.ecology = Some(World {
            resources: BTreeMap::new(),
            plots: BTreeMap::new(),
            jobs: BTreeMap::new(),
            known: BTreeMap::new(),
            known_plots: BTreeMap::new(),
            day: 0,
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
        self.state.ecology.as_mut().unwrap().resources.insert(
            id,
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
        );
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
        self.state.ecology.as_mut().unwrap().plots.insert(
            id,
            Plot {
                id,
                area,
                fertility,
                soil_water: 75,
                water_capacity: 120,
                access,
                custodian,
            },
        );
        Ok(id)
    }
    fn ecology_area(&self, actor: PersonId) -> Option<AreaId> {
        let body = self.state.physical.as_ref()?.actors.get(&actor)?;
        if !body.vitals.alive || body.vitals.health == 0 {
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
        self.inventory_index
            .get(&InventoryLocation::Carried(actor))?
            .iter()
            .copied()
            .find(|id| {
                self.state.inventories[id].good == Good::Tool && self.tool_available_for(actor, *id)
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
                    .resources
                    .get(&resource)
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
                    .resources
                    .get_mut(&resource)
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
                    .resources
                    .get(&resource)
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
            .resources
            .get(&resource)
            .ok_or(Error::Unavailable)?;
        let reserved = self
            .state
            .ecology
            .as_ref()
            .unwrap()
            .jobs
            .iter()
            .filter(|(person, job)| **person != actor && job.resource == resource)
            .map(|(_, job)| u128::from(job.quantity.0))
            .sum::<u128>();
        let working = self
            .state
            .ecology
            .as_ref()
            .unwrap()
            .jobs
            .iter()
            .filter(|(person, job)| **person != actor && job.resource == resource)
            .count();
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
        if quantity.0 == 0
            || source.area != self.ecology_area(actor).unwrap()
            || (!unauthorized && !source.access.contains(&actor))
            || working >= source.workers
            || u128::from(quantity.0) + reserved > u128::from(source.remaining.0)
            || held + u128::from(quantity.0) > u128::from(physical.rules.carrying_capacity.0)
            || physical.actors[&actor].vitals.fatigue >= physical.rules.fatigue_limit
            || (!adult && (source.kind == ResourceKind::Coppice || quantity.0 > 1_000))
        {
            return Err(Error::Unavailable);
        }
        let tool = if source.kind == ResourceKind::Coppice {
            Some(self.ecology_tool(actor).ok_or(Error::Unavailable)?)
        } else {
            None
        };
        let duration = u64::try_from(
            (u128::from(source.labor_per_unit.0) * u128::from(quantity.0)).div_ceil(1_000),
        )
        .map_err(|_| Error::Overflow)?;
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(duration.max(1))
                .ok_or(Error::Overflow)?,
        );
        let good = source.kind.good();
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
        self.interrupt_physical(actor);
        self.state.ecology.as_mut().unwrap().jobs.insert(
            actor,
            Gathering {
                resource,
                destination,
                quantity,
                tool,
                completes,
                unauthorized,
            },
        );
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .activity = Activity::Gather {
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
        self.update_property_events();
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
        let source = self.state.ecology.as_ref().unwrap().resources[&job.resource].clone();
        if self.ecology_able(actor)
            && self.ecology_area(actor) == Some(source.area)
            && source.remaining >= job.quantity
            && (job.unauthorized || source.access.contains(&actor))
            && self.transfer_capacity(job.destination, job.quantity.0)
            && job
                .tool
                .is_none_or(|tool| self.tool_available_for(actor, tool))
        {
            self.state
                .ecology
                .as_mut()
                .unwrap()
                .resources
                .get_mut(&job.resource)
                .unwrap()
                .remaining
                .0 -= job.quantity.0;
            self.state
                .inventories
                .get_mut(&job.destination)
                .unwrap()
                .quantity
                .0 += job.quantity.0;
            if source.kind == ResourceKind::Spring {
                self.collected_water_quality(job.destination, job.quantity, source.contamination);
            }
            self.emit(EventKind::ResourceCollected {
                actor,
                resource: job.resource,
                destination: job.destination,
                good: source.kind.good(),
                quantity: job.quantity,
            });
        } else {
            self.emit(EventKind::Interrupted { actor });
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
    pub(crate) fn evaluate_ecology(&self, next: SimTime) -> Result<Option<World>, Error> {
        let Some(mut world) = self.state.ecology.clone() else {
            return Ok(None);
        };
        let environment = self.state.environment.as_ref().ok_or(Error::Unavailable)?;
        let day = next.0 / environment.calendar.day.0;
        if day == world.day {
            return Ok(Some(world));
        }
        if day != world.day + 1 {
            return Err(Error::InvalidSave("ecology skipped a calendar day".into()));
        }
        for source in world.resources.values_mut() {
            source.soil_water = source
                .soil_water
                .saturating_add(environment.weather.rain)
                .min(source.water_capacity)
                .saturating_sub(environment.weather.evaporation);
            let warmth = i64::from(environment.weather.temperature - source.minimum_temperature)
                .max(0)
                .min(i64::from(
                    source.optimum_temperature - source.minimum_temperature,
                )) as u64
                * 1_000
                / u64::try_from(source.optimum_temperature - source.minimum_temperature)
                    .map_err(|_| Error::Overflow)?;
            let moisture = (u128::from(source.soil_water.min(source.optimal_water)) * 1_000
                / u128::from(source.optimal_water)) as u64;
            let growth =
                u128::from(source.daily_growth.0) * u128::from(warmth) * u128::from(moisture)
                    + u128::from(source.growth_remainder);
            let available = source.capacity.0 - source.remaining.0;
            let added = u64::try_from((growth / 1_000_000).min(u128::from(available)))
                .map_err(|_| Error::Overflow)?;
            source.growth_remainder = if added == available {
                0
            } else {
                (growth % 1_000_000) as u64
            };
            let decay =
                (u128::from(source.remaining.0) * u128::from(source.decay_permille) / 1_000) as u64;
            let previous = source.remaining.0 - decay;
            source.remaining.0 = previous.saturating_add(added).min(source.capacity.0);
            if source.kind == ResourceKind::Spring && added > 0 {
                source.contamination = ((u128::from(previous) * u128::from(source.contamination)
                    + u128::from(added) * u128::from(source.runoff_contamination))
                    / u128::from(source.remaining.0.max(1)))
                    as u16;
            }
        }
        for plot in world.plots.values_mut() {
            plot.soil_water = plot
                .soil_water
                .saturating_add(if environment.weather.temperature > 0 {
                    environment.weather.rain
                } else {
                    0
                })
                .min(plot.water_capacity)
                .saturating_sub(environment.weather.evaporation);
            if environment.weather.temperature > environment.crop.base_temperature
                && plot.soil_water > 0
            {
                plot.fertility = plot
                    .fertility
                    .saturating_add(environment.crop.soil_recovery)
                    .min(1_000);
            }
        }
        world.day = day;
        Ok(Some(world))
    }
    pub(crate) fn commit_ecology(&mut self, output: Option<World>) {
        if let Some(output) = output {
            self.state.ecology = Some(output);
        }
    }
    pub(crate) fn observe_ecology(&mut self) {
        let Some(world) = &self.state.ecology else {
            return;
        };
        let resources = world.resources.values().cloned().collect::<Vec<_>>();
        let plots = world.plots.values().cloned().collect::<Vec<_>>();
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
    pub(crate) fn available_ecology_commands(&self, actor: PersonId) -> Vec<Command> {
        if !self.ecology_able(actor) {
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
        let room = u128::from(physical.rules.carrying_capacity.0).saturating_sub(held);
        let mut commands = Vec::new();
        for source in world.resources.values().filter(|s| s.area == area).take(16) {
            let reserved = world
                .jobs
                .iter()
                .filter(|(id, j)| **id != actor && j.resource == source.id)
                .map(|(_, j)| u128::from(j.quantity.0))
                .sum::<u128>();
            let available = u128::from(source.remaining.0).saturating_sub(reserved);
            if available >= 1_000
                && room >= 1_000
                && physical.actors[&actor].vitals.fatigue < physical.rules.fatigue_limit
                && (source.kind != ResourceKind::Coppice || self.ecology_tool(actor).is_some())
            {
                commands.push(Command::Ecology {
                    action: EcologyAction::Gather {
                        resource: source.id,
                        quantity: Quantity(1_000),
                        unauthorized: !source.access.contains(&actor),
                    },
                });
            }
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
    pub(crate) fn ecology_candidate(&self, actor: PersonId) -> Option<Command> {
        let world = self.state.ecology.as_ref()?;
        if !self.ecology_able(actor) || world.jobs.contains_key(&actor) {
            return None;
        }
        let physical = self.state.physical.as_ref()?;
        let body = &physical.actors[&actor];
        if body.vitals.fatigue >= physical.rules.fatigue_limit / 2 {
            return None;
        }
        let mind = &self.state.decisions.as_ref()?.minds[&actor];
        let hungry = body.vitals.hunger >= physical.rules.meal_relief;
        let local_food = mind.knowledge.values().any(|f| {
            f.good == Good::Food && f.accessible && f.quantity >= physical.rules.meal_size
        });
        let desired = if hungry && !local_food {
            Good::Food
        } else if mind.dream == decisions::Dream::Independence && mind.personality.ambition >= 600 {
            Good::Timber
        } else {
            return None;
        };
        let held = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .any(|id| {
                self.state.inventories[id].good == desired
                    && self.state.inventories[id].quantity.0
                        >= if desired == Good::Timber {
                            5_000
                        } else {
                            1_000
                        }
            });
        if held {
            return None;
        }
        let area = self.ecology_area(actor)?;
        let source = world
            .known
            .get(&actor)?
            .values()
            .filter(|s| s.kind.good() == desired && s.available.0 >= 1_000)
            .filter(|s| {
                s.accessible
                    || s.custodian.is_some()
                    || mind.personality.honesty < 250 && mind.personality.risk_tolerance >= 700
            })
            .min_by_key(|s| (s.area != area, s.id))?;
        if source.area != area {
            return decisions::known_path(&mind.routes, area, source.area)
                .ok()
                .map(|_| Command::Move {
                    destination: source.area,
                });
        }
        if !source.accessible
            && (mind.personality.honesty >= 250 || mind.personality.risk_tolerance < 700)
        {
            return self.available_ecology_commands(actor).into_iter().find(|c|matches!(c,Command::Ecology {action:EcologyAction::RequestAccess {resource}} if *resource==source.id));
        }
        self.available_ecology_commands(actor)
            .into_iter()
            .find(|command| {
                matches!(command,
            Command::Ecology {action:EcologyAction::Gather {resource,..}} if *resource==source.id)
            })
    }
    pub(crate) fn validate_ecology(&self) -> Result<(), Error> {
        let Some(world) = &self.state.ecology else {
            return Ok(());
        };
        let bad = || Error::InvalidSave("invalid ecological deposits, plots or collection".into());
        let environment = self.state.environment.as_ref().ok_or_else(bad)?;
        if self.state.physical.is_none() || self.state.decisions.is_none() {
            return Err(bad());
        }
        if world.day != self.state.time.0 / environment.calendar.day.0 {
            return Err(bad());
        }
        let people = self.person_index.keys().copied().collect::<BTreeSet<_>>();
        let area_exists = |area| self.state.areas.iter().any(|a| a.id == area);
        for (id, s) in &world.resources {
            if *id != s.id
                || id.0 >= self.state.next_id
                || s.capacity.0 == 0
                || s.remaining > s.capacity
                || !area_exists(s.area)
                || s.decay_permille > 1_000
                || s.contamination > 1_000
                || s.runoff_contamination > 1_000
                || s.growth_remainder >= 1_000_000
                || s.water_capacity == 0
                || s.soil_water > s.water_capacity
                || s.optimal_water == 0
                || s.optimal_water > s.water_capacity
                || s.minimum_temperature >= s.optimum_temperature
                || !(-100..=100).contains(&s.minimum_temperature)
                || !(-100..=100).contains(&s.optimum_temperature)
                || s.labor_per_unit.0 == 0
                || s.workers == 0
                || !s.access.is_subset(&people)
                || s.custodian.is_some_and(|p| !people.contains(&p))
            {
                return Err(bad());
            }
        }
        for (id, p) in &world.plots {
            if *id != p.id
                || id.0 >= self.state.next_id
                || !area_exists(p.area)
                || p.fertility > 1_000
                || p.water_capacity == 0
                || p.soil_water > p.water_capacity
                || !p.access.is_subset(&people)
                || p.custodian.is_some_and(|id| !people.contains(&id))
                || environment.fields.contains_key(id)
                || self.state.physical.as_ref().unwrap().sites.contains_key(id)
            {
                return Err(bad());
            }
        }
        for (actor, job) in &world.jobs {
            let source = world.resources.get(&job.resource).ok_or_else(bad)?;
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
                || !matches!(self.state.physical.as_ref().unwrap().actors[actor].activity,
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
                    .any(|resource| !world.resources.contains_key(resource))
        }) {
            return Err(bad());
        }
        for (actor, known) in &world.known {
            if !people.contains(actor)
                || known.iter().any(|(id, k)| {
                    *id != k.id
                        || !world.resources.contains_key(id)
                        || k.observed_at > self.state.time
                        || !area_exists(k.area)
                        || k.contamination > 1_000
                        || k.available > world.resources[id].capacity
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
                        || !(world.plots.contains_key(id) || environment.fields.contains_key(id))
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
            sim.debug_physical_command(actor, Command::Cancel).unwrap();
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Dream::Comfort,
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
            .resources
            .values()
            .find(|r| r.kind == kind && r.area == area)
            .unwrap()
            .id
    }
    fn local(sim: &mut Simulation, actor: PersonId, area: AreaId) {
        sim.state.people[sim.person_index[&actor]].location = Location::AtArea(area);
        sim.observe_ecology();
    }
    #[test]
    fn gathering_debits_deposit_only_after_real_labor_and_preserves_physical_quantity() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let spring = resource(&sim, ResourceKind::Spring, AreaId(1));
        let before = sim.state.ecology.as_ref().unwrap().resources[&spring].remaining;
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
            sim.state.ecology.as_ref().unwrap().resources[&spring].remaining,
            before
        );
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(
            sim.state.inventories[&destination].quantity,
            Quantity(1_000)
        );
        assert_eq!(
            sim.state.ecology.as_ref().unwrap().resources[&spring].remaining,
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
        sim.debug_physical_command(actor, Command::Cancel).unwrap();
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
            .resources
            .get_mut(&coppice)
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
            sim.state.ecology.as_ref().unwrap().resources[&coppice].remaining,
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
            .resources
            .get_mut(&spring)
            .unwrap();
        source.access = BTreeSet::from([custodian]);
        source.custodian = Some(custodian);
        let before = source.remaining;
        let mut mind = decisions::Personality::restful();
        mind.aggression = 900;
        mind.risk_tolerance = 900;
        sim.debug_set_personality(custodian, mind, decisions::Dream::Comfort)
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
            sim.state.ecology.as_ref().unwrap().resources[&spring].remaining,
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
            .resources
            .get_mut(&spring)
            .unwrap()
            .remaining = Quantity(0);
        let world = sim.state.environment.as_mut().unwrap();
        world.weather.temperature = 20;
        world.weather.rain = 50;
        world.weather.evaporation = 0;
        let day = world.calendar.day;
        let renewed = sim.evaluate_ecology(SimTime(day.0)).unwrap().unwrap();
        assert_eq!(renewed.resources[&spring].remaining, Quantity(10_000));
        sim.state.environment.as_mut().unwrap().weather.temperature = -5;
        let frozen = sim.evaluate_ecology(SimTime(day.0)).unwrap().unwrap();
        assert_eq!(frozen.resources[&spring].remaining, Quantity(0));
    }
}
