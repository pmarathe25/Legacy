//! Physical rules with explicit scenario rates. The survival fixture is test data,
//! not the setting's eventual biology, farming cycle, or autonomous social policy.
use crate::*;
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub hunger_harm: u64,
    pub fatigue_limit: u64,
    pub max_health: u64,
    pub meal_relief: u64,
    pub meal_size: Quantity,
    pub eating_time: SimDuration,
    pub loading_time: SimDuration,
    pub carrying_capacity: Quantity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub site: SiteId,
    pub remaining: u64,
    pub rate: SkillRate,
    pub body_rate: SkillRate,
    pub work_remainder: u64,
    pub tool: Option<InventoryId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub fatigue_remainder: u16,
    pub health_remainder: u16,
    pub vitals: Vitals,
    pub activity: Activity,
    pub job: Option<Job>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    pub id: SiteId,
    pub area: AreaId,
    pub input: InventoryId,
    pub input_quantity: Quantity,
    pub tool: InventoryId,
    pub output: InventoryId,
    pub output_quantity: Quantity,
    pub duration: SimDuration,
    pub access: BTreeSet<PersonId>,
    pub last_granted: Option<PersonId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub actors: BTreeMap<PersonId, Actor>,
    pub sites: BTreeMap<SiteId, Site>,
}

#[derive(Clone)]
pub(crate) struct Outcome {
    id: PersonId,
    vitals: Vitals,
    fatigue_remainder: u16,
    health_remainder: u16,
    remaining: Option<u64>,
    work_remainder: Option<u64>,
}

impl Simulation {
    pub(crate) fn body_rules(&self, actor: PersonId) -> Rules {
        rules_for(&self.state, actor)
    }
    pub(crate) fn apply_genetic_body_change(
        &mut self,
        actor: PersonId,
        old_health_milli: u16,
        new_health_milli: u16,
    ) {
        let Some(body) = self
            .state
            .physical
            .as_mut()
            .and_then(|world| world.actors.get_mut(&actor))
        else {
            return;
        };
        if old_health_milli == 0 {
            return;
        }
        let health = (u128::from(body.vitals.health) * 1_000 + u128::from(body.health_remainder))
            * u128::from(new_health_milli)
            / u128::from(old_health_milli);
        body.vitals.health = (health / 1_000).min(u128::from(u64::MAX)) as u64;
        body.health_remainder = (health % 1_000) as u16;
        // Changing expression preserves existing injury and never revives death.
        if !body.vitals.alive {
            body.vitals.health = 0;
            body.health_remainder = 0;
        }
    }
    pub fn survival_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::kernel_fixture(seed, 15, execution)?;
        sim.state.scenario = scenarios().remove(1);
        let village = sim.state.areas[0].id;
        let forest = sim.state.areas[2].id;
        let people: BTreeSet<_> = sim.state.people.iter().map(|p| p.id).collect();
        let rules = Rules {
            hunger_harm: 600_000,
            fatigue_limit: 300_000,
            max_health: 180_000,
            meal_relief: 300_000,
            meal_size: Quantity(1_000),
            eating_time: MINUTE,
            loading_time: MINUTE,
            carrying_capacity: Quantity(10_000),
        };
        let actors = sim
            .state
            .people
            .iter()
            .map(|p| {
                (
                    p.id,
                    Actor {
                        fatigue_remainder: 0,
                        health_remainder: 0,
                        vitals: Vitals {
                            hunger: 0,
                            fatigue: 0,
                            health: rules.max_health,
                            alive: true,
                        },
                        activity: Activity::Idle,
                        job: None,
                    },
                )
            })
            .collect();
        sim.state.physical = Some(World {
            rules,
            actors,
            sites: BTreeMap::new(),
        });
        let food = *sim.state.inventories.keys().next_back().unwrap();
        sim.state.inventories.get_mut(&food).unwrap().quantity = Quantity(100_000);
        let timber = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(village),
            Quantity(20_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let tools = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(village),
            Quantity(1_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let forest_tools = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(forest),
            Quantity(1_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let deposit = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(forest),
            Quantity(20_000),
            Quantity(100_000),
            people.clone(),
        )?;
        let output = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(forest),
            Quantity(0),
            Quantity(100_000),
            people.clone(),
        )?;
        for (area, input, tool, output, output_quantity) in [
            (village, timber, tools, food, 1_000),
            (forest, deposit, forest_tools, output, 1_000),
        ] {
            let id = SiteId(allocate(&mut sim.state)?);
            sim.state.physical.as_mut().unwrap().sites.insert(
                id,
                Site {
                    id,
                    area,
                    input,
                    input_quantity: Quantity(1_000),
                    tool,
                    output,
                    output_quantity: Quantity(output_quantity),
                    duration: MINUTE,
                    access: people.clone(),
                    last_granted: None,
                },
            );
        }
        // Each good has a separate physical stack. Carrying capacity is shared
        // across every stack carried by the same person.
        for p in sim.state.people.clone() {
            sim.debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(p.id),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([p.id]),
            )?;
        }
        Ok(sim)
    }

    pub(crate) fn physical_view(&self) -> Option<PhysicalView> {
        let world = self.state.physical.as_ref()?;
        let actor = &world.actors[&self.state.ruler];
        let ruler = &self.state.people[self.person_index[&self.state.ruler]];
        if !actor.vitals.alive {
            return Some(PhysicalView {
                vitals: actor.vitals.clone(),
                activity: actor.activity.clone(),
                local_stocks: Vec::new(),
                local_sites: Vec::new(),
            });
        }
        let area = if let Location::AtArea(a) = ruler.location {
            Some(a)
        } else {
            None
        };
        Some(PhysicalView {
            vitals: actor.vitals.clone(),
            activity: actor.activity.clone(),
            local_stocks: self
                .inventory_index
                .get(&InventoryLocation::Carried(ruler.id))
                .into_iter()
                .flatten()
                .chain(
                    area.and_then(|a| self.inventory_index.get(&InventoryLocation::Area(a)))
                        .into_iter()
                        .flatten(),
                )
                .map(|id| &self.state.inventories[id])
                .map(|i| StockView {
                    id: i.id,
                    good: i.good,
                    quantity: i.quantity,
                    can_access: self.has_stock_access(ruler.id, i),
                    capacity: i.capacity,
                    position: match i.location {
                        InventoryLocation::Area(id) => StockPosition::Area(id),
                        InventoryLocation::Carried(id) => StockPosition::Carried(id),
                    },
                })
                .collect(),
            local_sites: world
                .sites
                .values()
                .filter(|s| Some(s.area) == area && s.access.contains(&ruler.id))
                .map(|s| s.id)
                .collect(),
        })
    }

    pub(crate) fn physically_working(&self, actor: PersonId) -> bool {
        self.state.physical.as_ref().is_none_or(|w| {
            let a = &w.actors[&actor];
            a.vitals.alive && a.vitals.health > 0 && matches!(a.activity, Activity::Work { .. })
        })
    }

    pub(crate) fn start_physical_action(
        &mut self,
        actor: PersonId,
        command: Command,
    ) -> Result<(), Error> {
        if self.combat_restricts(actor)
            && !matches!(
                command,
                Command::Rest
                    | Command::Eat { .. }
                    | Command::Tell { .. }
                    | Command::FreeCaptive { .. }
            )
        {
            return Err(Error::Unavailable);
        }
        if self.in_combat(actor) {
            return Err(Error::Unavailable);
        }
        if !self.independent(actor) && !matches!(command, Command::Signal { .. } | Command::Cancel)
        {
            return Err(Error::Unavailable);
        }
        if self.care_index.contains_key(&actor)
            && !matches!(command, Command::Signal { .. } | Command::Cancel)
        {
            return Err(Error::Unavailable);
        }
        let world = self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let a = world.actors.get(&actor).ok_or(Error::Unavailable)?;
        let p = &self.state.people[self.person_index[&actor]];
        if !a.vitals.alive || a.vitals.health == 0 {
            return Err(Error::Unavailable);
        }
        let Location::AtArea(area) = p.location else {
            return Err(Error::Unavailable);
        };
        let rules = &world.rules;
        let (activity, new_job, consume) = match command {
            Command::Signal { preference } => {
                let life = self
                    .state
                    .development
                    .as_ref()
                    .and_then(|w| w.lives.get(&actor))
                    .ok_or(Error::Unavailable)?;
                if !matches!(life.stage, LifeStage::Infant | LifeStage::Child)
                    || !matches!(
                        preference,
                        ChildhoodInterest::Food | ChildhoodInterest::Company
                    )
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(6_000)
                        .ok_or(Error::Overflow)?,
                );
                completes.0.checked_add(300_000).ok_or(Error::Overflow)?;
                (
                    Activity::Express {
                        preference,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Play { interest } => {
                let life = self
                    .state
                    .development
                    .as_ref()
                    .and_then(|w| w.lives.get(&actor))
                    .ok_or(Error::Unavailable)?;
                if life.stage != LifeStage::Child || a.vitals.fatigue >= rules.fatigue_limit {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(MINUTE.0)
                        .ok_or(Error::Overflow)?,
                );
                (
                    Activity::Play {
                        interest,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Rest => (Activity::Rest, None, None),
            Command::Cancel => (Activity::Idle, None, None),
            Command::FreeCaptive { target } => {
                if !self.can_release(actor, target) {
                    return Err(Error::Unavailable);
                }
                self.preflight_force_intervention()?;
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(rules.loading_time.0)
                        .ok_or(Error::Overflow)?,
                );
                (Activity::Unbind { target, completes }, None, None)
            }
            Command::Comfort { child } => {
                let life = self
                    .state
                    .development
                    .as_ref()
                    .and_then(|w| w.lives.get(&child))
                    .ok_or(Error::Unavailable)?;
                if !matches!(life.stage, LifeStage::Infant | LifeStage::Child)
                    || child == actor
                    || !world.actors[&child].vitals.alive
                    || self.state.people[self.person_index[&child]].location
                        != Location::AtArea(area)
                    || self.care_index.contains_key(&child)
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(MINUTE.0)
                        .ok_or(Error::Overflow)?,
                );
                (Activity::Comfort { child, completes }, None, None)
            }
            Command::Care { child, source } => {
                let life = self
                    .state
                    .development
                    .as_ref()
                    .and_then(|w| w.lives.get(&child))
                    .ok_or(Error::Unavailable)?;
                let receiver = &self.state.physical.as_ref().unwrap().actors[&child];
                let food = self
                    .state
                    .inventories
                    .get(&source)
                    .ok_or(Error::Unavailable)?;
                if !matches!(life.stage, LifeStage::Infant | LifeStage::Child)
                    || actor == child
                    || !receiver.vitals.alive
                    || !matches!(receiver.activity, Activity::Idle | Activity::Rest)
                    || self.care_index.get(&child).is_some_and(|id| *id != actor)
                    || self.state.people[self.person_index[&child]].location
                        != Location::AtArea(area)
                    || !matches!(food.good, Good::Food | Good::Water)
                    || !self.can_use(actor, food)
                    || self.unreserved_stock(source, actor) < rules.meal_size.0
                {
                    return Err(Error::Unavailable);
                }
                if food.good == Good::Water {
                    self.preflight_water_care(child, source, rules.meal_size)?;
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(rules.eating_time.0)
                        .ok_or(Error::Overflow)?,
                );
                (
                    Activity::Care {
                        child,
                        source,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Tell {
                recipient,
                inventory,
            } => {
                let remembered = self
                    .state
                    .decisions
                    .as_ref()
                    .and_then(|w| w.minds.get(&actor))
                    .and_then(|m| m.knowledge.get(&inventory))
                    .cloned();
                let visible = self.state.inventories.get(&inventory).filter(|i| {
                    i.location == InventoryLocation::Area(area)
                        || i.location == InventoryLocation::Carried(actor)
                });
                let fact = visible
                    .map(|i| StockKnowledge {
                        inventory,
                        area,
                        good: i.good,
                        quantity: i.quantity,
                        accessible: self.has_stock_access(actor, i),
                        observed_at: self.state.time,
                        learned_at: self.state.time,
                        source: KnowledgeSource::Observed(actor),
                    })
                    .or(remembered)
                    .ok_or(Error::Unavailable)?;
                if self.state.decisions.is_none() {
                    return Err(Error::Unavailable);
                }
                let receiver = self
                    .person_index
                    .get(&recipient)
                    .map(|i| &self.state.people[*i])
                    .ok_or(Error::Unavailable)?;
                if recipient == actor || receiver.location != Location::AtArea(area) {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(60_000)
                        .ok_or(Error::Overflow)?,
                );
                (
                    Activity::Tell {
                        recipient,
                        fact,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Eat { source } => {
                let inventory = self
                    .state
                    .inventories
                    .get(&source)
                    .ok_or(Error::Unavailable)?;
                if inventory.good != Good::Food
                    || !self.can_use(actor, inventory)
                    || self.unreserved_stock(source, actor) < rules.meal_size.0
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(rules.eating_time.0)
                        .ok_or(Error::Overflow)?,
                );
                (Activity::Eat { source, completes }, None, None)
            }
            Command::Load {
                source,
                destination,
                quantity,
            } => {
                let source_inventory = self
                    .state
                    .inventories
                    .get(&source)
                    .ok_or(Error::Unavailable)?;
                let destination_inventory = self
                    .state
                    .inventories
                    .get(&destination)
                    .ok_or(Error::Unavailable)?;
                if source == destination
                    || !self.record_transfer_allowed(source, quantity)
                    || quantity.0 == 0
                    || source_inventory.good != destination_inventory.good
                    || (source_inventory.location != InventoryLocation::Carried(actor)
                        && destination_inventory.location != InventoryLocation::Carried(actor))
                    || !self.can_use(actor, source_inventory)
                    || !(self.can_use(actor, destination_inventory)
                        || self.agreement_accepts_delivery(actor, destination, quantity)
                        || self.force_accepts_food_delivery(actor, source, destination, quantity))
                    || self.unreserved_stock(source, actor) < quantity.0
                    || destination_inventory.capacity.0 - destination_inventory.quantity.0
                        < quantity.0
                    || (source_inventory.location != destination_inventory.location
                        && !self.transfer_capacity(destination, quantity.0))
                    || a.vitals.fatigue >= rules.fatigue_limit
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(rules.loading_time.0)
                        .ok_or(Error::Overflow)?,
                );
                (
                    Activity::Load {
                        source,
                        destination,
                        quantity,
                        completes,
                    },
                    None,
                    None,
                )
            }
            Command::Work { site } => {
                if self.state.development.as_ref().is_some_and(|w| {
                    !matches!(w.lives[&actor].stage, LifeStage::Adult | LifeStage::Elder)
                }) {
                    return Err(Error::Unavailable);
                }
                let site_record = world.sites.get(&site).ok_or(Error::Unavailable)?;
                let input = &self.state.inventories[&site_record.input];
                let tool_id = self
                    .select_work_tool(actor, site)
                    .ok_or(Error::Unavailable)?;
                let tool = &self.state.inventories[&tool_id];
                let output = &self.state.inventories[&site_record.output];
                if site_record.area != area || !site_record.access.contains(&actor) || !self.can_use(actor, tool) || tool.quantity.0 < 1_000
                    || a.vitals.fatigue >= rules.fatigue_limit
                    || world.actors.iter().any(|(id, other)| *id != actor && matches!(other.activity, Activity::Work { site: other_site } if other_site == site))
                    || output.capacity.0 - output.quantity.0 < site_record.output_quantity.0 { return Err(Error::Unavailable); }
                if a.job.as_ref().is_some_and(|job| job.site == site) {
                    (Activity::Work { site }, None, None)
                } else {
                    if !self.can_use(actor, input)
                        || self.unreserved_stock(site_record.input, actor)
                            < site_record.input_quantity.0
                    {
                        return Err(Error::Unavailable);
                    }
                    (
                        Activity::Work { site },
                        Some(Job {
                            site,
                            remaining: site_record.duration.0,
                            rate: self.effective_work_rate(actor, Skill::Craft),
                            body_rate: SkillRate {
                                numerator: u64::from(self.genetic_strength_milli(actor)),
                                denominator: 1_000,
                            },
                            work_remainder: 0,
                            tool: Some(tool_id),
                        }),
                        Some((site_record.input, site_record.input_quantity)),
                    )
                }
            }
            _ => return Err(Error::Unavailable),
        };
        if let Command::Work { site } = command
            && let Some((_, quantity)) = &consume
        {
            self.preflight_enterprise_input(site, *quantity)?;
        }
        self.ensure_event_capacity(3)?;
        self.interrupt_actor_activity(actor);
        if let Some((source, quantity)) = consume {
            self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
            self.emit(EventKind::Consumed {
                actor,
                source,
                quantity,
            });
            if let Command::Work { site } = command {
                self.record_enterprise_input(site, quantity);
            }
        }
        let resumed_tool = if let Command::Work { site } = command {
            self.select_work_tool(actor, site)
        } else {
            None
        };
        let a = self
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap();
        a.activity = activity.clone();
        if let Activity::Tell { ref fact, .. } = activity {
            self.state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
                .unwrap()
                .knowledge
                .insert(fact.inventory, fact.clone());
        }
        if let Activity::Care { child, .. } | Activity::Comfort { child, .. } = activity {
            self.care_index.insert(child, actor);
        }
        if let Some(job) = new_job {
            a.job = Some(job);
        }
        if let Some(tool) = resumed_tool
            && let Some(job) = &mut a.job
        {
            job.tool = Some(tool);
        }
        if matches!(command, Command::Cancel) {
            a.job = None;
        }
        let deadline = match activity {
            Activity::Eat { completes, .. }
            | Activity::Load { completes, .. }
            | Activity::Tell { completes, .. }
            | Activity::Care { completes, .. }
            | Activity::Unbind { completes, .. }
            | Activity::Comfort { completes, .. }
            | Activity::Express { completes, .. }
            | Activity::Play { completes, .. } => Some(completes),
            _ => None,
        };
        if let Some(deadline) = deadline {
            self.state
                .due
                .entry(deadline)
                .or_default()
                .push(Due::Physical(actor));
        }
        if let Activity::Unbind { target, completes } = activity {
            self.force_release_started(actor, target, completes)?;
        }
        self.update_property_events();
        self.reconcile_agreements()?;
        self.observe_agreement_events();
        self.reconcile_institutions()?;
        self.revision += 1;
        Ok(())
    }

    fn can_use(&self, actor: PersonId, inventory: &Inventory) -> bool {
        let p = &self.state.people[self.person_index[&actor]];
        if let Location::AtArea(area) = p.location {
            self.has_stock_access(actor, inventory) && self.inventory_area(inventory) == Some(area)
        } else {
            false
        }
    }

    pub(crate) fn unreserved_stock(&self, source: InventoryId, excluding: PersonId) -> u64 {
        let reserved: u128 = self
            .state
            .physical
            .as_ref()
            .unwrap()
            .actors
            .iter()
            .filter(|(id, _)| **id != excluding)
            .map(|(_, a)| {
                u128::from(match a.activity {
                    Activity::Eat { source: s, .. } | Activity::Care { source: s, .. }
                        if s == source =>
                    {
                        self.state.physical.as_ref().unwrap().rules.meal_size.0
                    }
                    Activity::Load {
                        source: s,
                        quantity,
                        ..
                    } if s == source => quantity.0,
                    _ => 0,
                })
            })
            .sum();
        u128::from(self.state.inventories[&source].quantity.0)
            .saturating_sub(reserved + u128::from(self.health_reserved_stock(source, excluding)))
            as u64
    }

    pub(crate) fn transfer_capacity(&self, destination: InventoryId, amount: u64) -> bool {
        if self.state.physical.is_none() {
            return true;
        }
        let inventory = &self.state.inventories[&destination];
        if let InventoryLocation::Carried(actor) = inventory.location {
            let carried: u128 = self
                .inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .map(|id| u128::from(self.state.inventories[id].quantity.0))
                .sum();
            carried + u128::from(amount) <= u128::from(self.body_rules(actor).carrying_capacity.0)
        } else {
            true
        }
    }

    pub(crate) fn physical_boundary(&self, limit: SimTime) -> SimTime {
        let Some(world) = &self.state.physical else {
            return limit;
        };
        let mut dt = limit.0 - self.state.time.0;
        for (id, a) in &world.actors {
            if !a.vitals.alive {
                continue;
            }
            if matches!(a.activity, Activity::Work { .. })
                && let Some(job) = &a.job
            {
                dt = dt.min(
                    skills::work_duration(
                        job.remaining,
                        self.invocation_work_rate(*id, job.rate, job.body_rate),
                        job.work_remainder,
                    )
                    .expect("validated frozen skill rate"),
                );
            }
        }
        // Inactive/dead records never impose a zero-length scheduling loop.
        SimTime(self.state.time.0 + dt.max(1).min(limit.0 - self.state.time.0))
    }

    fn body_context(
        &self,
        actor: PersonId,
    ) -> Result<
        (
            game_physiology::BodySnapshot,
            game_physiology::BodyLimits,
            game_physiology::ActivityFlags,
            bool,
        ),
        Error,
    > {
        let world = self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let body = &world.actors[&actor];
        let rules = self.body_rules(actor);
        let person = &self.state.people[self.person_index[&actor]];
        let exerting = matches!(
            body.activity,
            Activity::Work { .. }
                | Activity::Gather { .. }
                | Activity::Load { .. }
                | Activity::Take { .. }
                | Activity::Construct { .. }
                | Activity::Combat { .. }
                | Activity::Play { .. }
                | Activity::Cultivate { .. }
        ) || matches!(person.location, Location::InTransit { .. })
            || self.health_work_exerting(actor)
            || self.social_activity_is_practical(actor);
        let resting = body.activity == Activity::Rest || self.health_patient_resting(actor);
        Ok((
            game_physiology::BodySnapshot {
                vitals: body.vitals.clone(),
                fatigue_remainder: body.fatigue_remainder,
                health_remainder: body.health_remainder,
            },
            game_physiology::BodyLimits {
                hunger_harm: rules.hunger_harm,
                fatigue_limit: rules.fatigue_limit,
                max_health: rules.max_health,
            },
            game_physiology::ActivityFlags { exerting, resting },
            person.hunger_exempt,
        ))
    }

    /// Assemble facts from domain owners; arithmetic and clamping live in the
    /// independently compiled physiology kernel.
    pub(crate) fn integrated_vitals(
        &self,
        actor: PersonId,
        elapsed: u64,
    ) -> Result<(Vitals, u16, u16), Error> {
        let (snapshot, limits, activity, exempt) = self.body_context(actor)?;
        let health = self.health_body_delta(actor, elapsed)?;
        let (strain, injury) = self.invocation_rates(actor);
        let contributions = [
            game_physiology::Contribution {
                health_milli: health.health_milli,
                fatigue_milli: health.fatigue_milli,
            },
            game_physiology::Contribution {
                health_milli: i128::from(elapsed)
                    .checked_mul(i128::from(injury))
                    .and_then(i128::checked_neg)
                    .ok_or(Error::Overflow)?,
                fatigue_milli: i128::from(elapsed)
                    .checked_mul(i128::from(strain))
                    .ok_or(Error::Overflow)?,
            },
        ];
        let result = game_physiology::integrate(
            &snapshot,
            limits,
            game_physiology::BodyInput {
                elapsed_ms: elapsed,
                hunger_exempt: exempt,
                immortal: exempt,
                activity,
                contributions: &contributions,
            },
        )?;
        Ok((
            result.vitals,
            result.fatigue_remainder,
            result.health_remainder,
        ))
    }

    pub(crate) fn body_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        let Some(world) = &self.state.physical else {
            return Ok(limit);
        };
        let mut next = limit;
        // Hunger changes the healing/harm slope. Clip that transition before
        // searching crossings so a recovery followed by starvation cannot hide
        // an earlier boundary at the interval endpoint.
        for (actor, body) in &world.actors {
            let exempt = self.state.people[self.person_index[actor]].hunger_exempt;
            let rules = self.body_rules(*actor);
            if body.vitals.alive && !exempt && body.vitals.hunger < rules.hunger_harm {
                let remaining = rules.hunger_harm - body.vitals.hunger;
                if remaining < next.0 - self.state.time.0 {
                    next = SimTime(self.state.time.0 + remaining);
                }
            }
        }
        for (actor, body) in &world.actors {
            if !body.vitals.alive {
                continue;
            }
            let elapsed = next.0 - self.state.time.0;
            let before = &body.vitals;
            let rules = self.body_rules(*actor);
            let incapacitation = self.state.combat.as_ref().map(|w| w.rules.incapacitation);
            let crossed = |dt| {
                let (after, _, _) = self.integrated_vitals(*actor, dt)?;
                Ok(!after.alive
                    || before.fatigue < world.rules.fatigue_limit
                        && after.fatigue >= world.rules.fatigue_limit
                    || before.fatigue > 0 && after.fatigue == 0
                    || before.health < rules.max_health && after.health >= rules.max_health
                    || incapacitation.is_some_and(|threshold| {
                        (before.health > threshold) != (after.health > threshold)
                    }))
            };
            if let Some(dt) = game_physiology::first_crossing(elapsed, crossed)? {
                next = SimTime(self.state.time.0 + dt);
            }
        }
        Ok(next)
    }

    pub(crate) fn evaluate_physical(&self, elapsed: u64) -> Result<Vec<Outcome>, Error> {
        let Some(world) = &self.state.physical else {
            return Ok(Vec::new());
        };
        let equipment = self.equipment_availability();
        let work = |(id, a): (&PersonId, &Actor)| -> Result<Outcome, Error> {
            let (vitals, fatigue_remainder, health_remainder) =
                self.integrated_vitals(*id, elapsed)?;
            let (remaining, work_remainder) = if let Some(job) = &a.job {
                if matches!(a.activity, Activity::Work { .. })
                    && equipment.get(id).copied().unwrap_or(false)
                {
                    let (work, remainder) = skills::advance_work(
                        elapsed,
                        self.invocation_work_rate(*id, job.rate, job.body_rate),
                        job.work_remainder,
                    )?;
                    (Some(job.remaining.saturating_sub(work)), Some(remainder))
                } else {
                    (Some(job.remaining), Some(job.work_remainder))
                }
            } else {
                (None, None)
            };
            Ok(Outcome {
                id: *id,
                vitals,
                fatigue_remainder,
                health_remainder,
                remaining,
                work_remainder,
            })
        };
        self.pool.install(|| {
            if world.actors.len() < 256 || self.execution.workers == 1 {
                world.actors.iter().map(work).collect()
            } else {
                world.actors.par_iter().map(work).collect()
            }
        })
    }

    pub(crate) fn complete_physical(&mut self, actor: PersonId) {
        let activity = self.state.physical.as_ref().unwrap().actors[&actor]
            .activity
            .clone();
        let mut success = false;
        if matches!(
            activity,
            Activity::Social { .. } | Activity::Instruction { .. } | Activity::Practice { .. }
        ) {
            self.complete_social(actor);
            return;
        }
        if matches!(activity, Activity::EnforcementTalk { .. }) {
            self.complete_enforcement_talk(actor);
            return;
        }
        if matches!(activity, Activity::Gather { .. }) {
            self.complete_gathering(actor);
            return;
        }
        if matches!(
            activity,
            Activity::Escape { .. } | Activity::Restrain { .. }
        ) {
            self.complete_force(actor);
            return;
        }
        if matches!(activity, Activity::ReportAgreement { .. }) {
            self.complete_agreement_report(actor);
            return;
        }
        if matches!(activity, Activity::Construct { .. }) {
            self.complete_construction(actor);
            return;
        }
        if matches!(activity, Activity::Lesson { .. }) {
            self.complete_lesson(actor);
            return;
        }
        if matches!(
            activity,
            Activity::Take { .. } | Activity::ReportIncident { .. }
        ) {
            self.complete_property_action(actor);
            return;
        }
        if matches!(activity, Activity::Cultivate { .. }) {
            self.complete_cultivation(actor);
            return;
        }
        if let Activity::Conceive { pregnancy, .. } = activity {
            self.complete_conception(pregnancy);
            return;
        }
        if activity == Activity::Idle {
            return;
        }
        match activity {
            Activity::Comfort { child, .. } => {
                self.care_index.remove(&child);
                if self.state.physical.as_ref().unwrap().actors[&child]
                    .vitals
                    .alive
                    && self.state.people[self.person_index[&child]].location
                        == self.state.people[self.person_index[&actor]].location
                {
                    let life = self
                        .state
                        .development
                        .as_mut()
                        .unwrap()
                        .lives
                        .get_mut(&child)
                        .unwrap();
                    if life
                        .preference
                        .is_some_and(|(p, _)| p == ChildhoodInterest::Company)
                    {
                        life.preference = None;
                    }
                    let relationship = self
                        .state
                        .decisions
                        .as_mut()
                        .unwrap()
                        .minds
                        .get_mut(&child)
                        .unwrap()
                        .relationships
                        .entry(actor)
                        .or_insert(decisions::Relationship {
                            trust: 0,
                            affection: 0,
                        });
                    relationship.affection = (relationship.affection + 50).min(1_000);
                    self.emit(EventKind::Comforted {
                        caregiver: actor,
                        child,
                    });
                    success = true;
                }
            }
            Activity::Express { preference, .. } => {
                self.state
                    .development
                    .as_mut()
                    .unwrap()
                    .lives
                    .get_mut(&actor)
                    .unwrap()
                    .preference = Some((preference, SimTime(self.state.time.0 + 300_000)));
                self.emit(EventKind::Expressed { actor, preference });
                success = true;
            }
            Activity::Play { interest, .. } => {
                let experience = self
                    .state
                    .decisions
                    .as_mut()
                    .unwrap()
                    .minds
                    .get_mut(&actor)
                    .unwrap()
                    .interests
                    .entry(interest)
                    .or_default();
                let gain = 100_u64.min(1_000 - *experience);
                *experience += gain;
                self.emit(EventKind::Practiced {
                    actor,
                    interest,
                    gain,
                });
                success = true;
            }
            Activity::Eat { source, .. } => {
                let quantity = self.state.physical.as_ref().unwrap().rules.meal_size;
                if self.can_use(actor, &self.state.inventories[&source])
                    && self.state.inventories[&source].quantity >= quantity
                {
                    let stock = self.state.inventories.get_mut(&source).unwrap();
                    stock.quantity.0 -= quantity.0;
                    stock.last_granted = Some(actor);
                    self.emit(EventKind::Ate {
                        actor,
                        source,
                        quantity,
                    });
                    success = true;
                }
            }
            Activity::Load {
                source,
                destination,
                quantity,
                ..
            } => {
                let intent = TransferIntent {
                    actor,
                    source,
                    destination,
                    quantity,
                };
                let granted = self.resolve_transfers(vec![intent])[0].1;
                if granted {
                    self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
                    self.state
                        .inventories
                        .get_mut(&source)
                        .unwrap()
                        .last_granted = Some(actor);
                    self.state
                        .inventories
                        .get_mut(&destination)
                        .unwrap()
                        .quantity
                        .0 += quantity.0;
                    self.emit(EventKind::Transferred {
                        actor,
                        source,
                        destination,
                        quantity,
                    });
                    success = true;
                }
            }
            Activity::Unbind { target, .. } => {
                if self.can_release(actor, target) {
                    self.release_captive(actor, target);
                    success = true;
                }
            }
            Activity::Tell {
                recipient, fact, ..
            } => {
                let sender = &self.state.people[self.person_index[&actor]];
                let receiver = &self.state.people[self.person_index[&recipient]];
                let receiver_body = &self.state.physical.as_ref().unwrap().actors[&recipient];
                if sender.location == receiver.location
                    && matches!(sender.location, Location::AtArea(_))
                    && receiver_body.vitals.alive
                    && receiver_body.vitals.health > 0
                    && matches!(receiver_body.activity, Activity::Idle | Activity::Rest)
                {
                    success = self.receive_report(actor, recipient, fact);
                }
            }
            Activity::Care { child, source, .. } => {
                self.care_index.remove(&child);
                let recipient = &self.state.physical.as_ref().unwrap().actors[&child];
                let quantity = self.state.physical.as_ref().unwrap().rules.meal_size;
                if recipient.vitals.alive
                    && self.state.people[self.person_index[&child]].location
                        == self.state.people[self.person_index[&actor]].location
                    && self.can_use(actor, &self.state.inventories[&source])
                    && self.state.inventories[&source].quantity >= quantity
                {
                    if self.state.inventories[&source].good == Good::Water
                        && self.health_water_given(child, source, quantity).is_err()
                    {
                        self.emit(EventKind::Interrupted { actor });
                        self.state
                            .physical
                            .as_mut()
                            .unwrap()
                            .actors
                            .get_mut(&actor)
                            .unwrap()
                            .activity = Activity::Idle;
                        return;
                    }
                    let food = self.state.inventories.get_mut(&source).unwrap();
                    food.quantity.0 -= quantity.0;
                    food.last_granted = Some(actor);
                    self.state
                        .development
                        .as_mut()
                        .unwrap()
                        .lives
                        .get_mut(&child)
                        .unwrap()
                        .caregivers
                        .insert(actor);
                    let relationship = self
                        .state
                        .decisions
                        .as_mut()
                        .unwrap()
                        .minds
                        .get_mut(&child)
                        .unwrap()
                        .relationships
                        .entry(actor)
                        .or_insert(decisions::Relationship {
                            trust: 0,
                            affection: 0,
                        });
                    relationship.trust = (relationship.trust + 50).min(1_000);
                    relationship.affection = (relationship.affection + 50).min(1_000);
                    let life = self
                        .state
                        .development
                        .as_mut()
                        .unwrap()
                        .lives
                        .get_mut(&child)
                        .unwrap();
                    if life
                        .preference
                        .is_some_and(|(p, _)| p == ChildhoodInterest::Food)
                    {
                        life.preference = None;
                    }
                    self.emit(EventKind::Cared {
                        caregiver: actor,
                        child,
                        source,
                        quantity,
                    });
                    success = true;
                }
            }
            _ => {}
        }
        if !success {
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
        // Physiology commits after physical effects; apply hunger relief to that
        // interval's evaluated output in commit_physical using this event.
    }

    pub(crate) fn commit_physical(
        &mut self,
        outcomes: Vec<Outcome>,
        equipment: &BTreeMap<PersonId, bool>,
    ) {
        for out in outcomes {
            let actor = out.id;
            let world = self.state.physical.as_ref().unwrap();
            let previous = &world.actors[&actor];
            let previously_alive = previous.vitals.alive;
            let rules = self.body_rules(actor);
            let previously_injured = previous.vitals.health < rules.max_health;
            let mut vitals = out.vitals;
            let ate = self
                .state
                .events
                .iter()
                .rev()
                .take_while(|e| e.time == self.state.time)
                .any(|e| {
                    matches!(e.kind, EventKind::Ate { actor: id, .. } if id == actor)
                        || matches!(e.kind, EventKind::Cared { child: id, source, .. } if id == actor && self.state.inventories[&source].good == Good::Food)
                });
            if ate {
                vitals.hunger = vitals.hunger.saturating_sub(rules.meal_relief);
            }
            let job_done =
                out.remaining == Some(0) && matches!(previous.activity, Activity::Work { .. });
            let interrupted = (self.social_activity_is_practical(actor)
                && (vitals.fatigue >= rules.fatigue_limit
                    || self.social_selected_tool(actor).is_some()
                        && !equipment.get(&actor).copied().unwrap_or(false)))
                || (self.health_work_exerting(actor) && vitals.fatigue >= rules.fatigue_limit)
                || (self.health_selected_tool(actor).is_some()
                    && !equipment.get(&actor).copied().unwrap_or(false))
                || (matches!(previous.activity, Activity::Work { .. })
                    && !equipment.get(&actor).copied().unwrap_or(false))
                || vitals.fatigue >= rules.fatigue_limit
                    && matches!(
                        previous.activity,
                        Activity::Work { .. }
                            | Activity::Gather { .. }
                            | Activity::Load { .. }
                            | Activity::Take { .. }
                            | Activity::Construct { .. }
                            | Activity::Play { .. }
                            | Activity::Cultivate { .. }
                    );
            let a = self
                .state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap();
            a.vitals = vitals.clone();
            a.fatigue_remainder = out.fatigue_remainder;
            a.health_remainder = out.health_remainder;
            if let (Some(job), Some(remaining)) = (&mut a.job, out.remaining) {
                job.remaining = remaining;
                job.work_remainder = out.work_remainder.unwrap();
            }
            if job_done && equipment.get(&actor).copied().unwrap_or(false) {
                let site_id = a.job.as_ref().unwrap().site;
                let site = self.state.physical.as_ref().unwrap().sites[&site_id].clone();
                let output = self.state.inventories.get_mut(&site.output).unwrap();
                if output.capacity.0 - output.quantity.0 >= site.output_quantity.0 {
                    output.quantity.0 += site.output_quantity.0;
                    let a = self
                        .state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&actor)
                        .unwrap();
                    a.job = None;
                    a.activity = Activity::Idle;
                    self.emit(EventKind::Produced {
                        actor,
                        site: site_id,
                        output: site.output,
                        quantity: site.output_quantity,
                    });
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .sites
                        .get_mut(&site_id)
                        .unwrap()
                        .last_granted = Some(actor);
                } else {
                    self.interrupt_actor_activity(actor);
                    self.emit(EventKind::Interrupted { actor });
                }
            } else if interrupted {
                self.interrupt_actor_activity(actor);
                self.emit(EventKind::Interrupted { actor });
            }
            if previously_alive && !vitals.alive {
                self.interrupt_actor_activity(actor);
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .job = None;
                self.state.people[self.person_index[&actor]].route.clear();
                for due in self.state.due.values_mut() {
                    due.retain(|d| !matches!(d, Due::Arrive(id) | Due::Age(id) if *id == actor));
                }
                self.state.due.retain(|_, due| !due.is_empty());
                self.emit(EventKind::Died { actor });
                for people in self.dependent_index.values_mut() {
                    people.remove(&actor);
                }
                self.invalidate_pregnancies(actor, true);
            } else if previously_injured && vitals.health == rules.max_health {
                self.emit(EventKind::Recovered { actor });
            }
        }
    }

    pub(crate) fn possible_work_outputs(
        &self,
        outcomes: &[Outcome],
    ) -> Vec<(SiteId, Good, Quantity)> {
        let Some(world) = &self.state.physical else {
            return Vec::new();
        };
        outcomes
            .iter()
            .filter_map(|out| {
                let actor = &world.actors[&out.id];
                if out.remaining != Some(0) || !matches!(actor.activity, Activity::Work { .. }) {
                    return None;
                }
                let site = &world.sites[&actor.job.as_ref()?.site];
                Some((
                    site.id,
                    self.state.inventories[&site.output].good,
                    site.output_quantity,
                ))
            })
            .collect()
    }

    pub fn debug_add_inventory(
        &mut self,
        good: Good,
        location: InventoryLocation,
        quantity: Quantity,
        capacity: Quantity,
        access: BTreeSet<PersonId>,
    ) -> Result<InventoryId, Error> {
        let id = self.add_inventory(good, location, quantity, capacity, access)?;
        self.update_property_events();
        Ok(id)
    }

    pub(crate) fn add_inventory(
        &mut self,
        good: Good,
        location: InventoryLocation,
        quantity: Quantity,
        capacity: Quantity,
        access: BTreeSet<PersonId>,
    ) -> Result<InventoryId, Error> {
        if quantity > capacity || access.iter().any(|p| !self.person_index.contains_key(p)) {
            return Err(Error::InvalidFixture("invalid inventory".into()));
        }
        match location {
            InventoryLocation::Area(a) if !self.state.areas.iter().any(|area| area.id == a) => {
                return Err(Error::InvalidFixture("unknown area".into()));
            }
            InventoryLocation::Carried(p) if !self.person_index.contains_key(&p) => {
                return Err(Error::InvalidFixture("unknown carrier".into()));
            }
            _ => {}
        }
        if let InventoryLocation::Carried(actor) = location {
            let held: u128 = self
                .inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .map(|id| u128::from(self.state.inventories[id].quantity.0))
                .sum();
            if self.state.physical.as_ref().is_some_and(|_| {
                held + u128::from(quantity.0)
                    > u128::from(self.body_rules(actor).carrying_capacity.0)
            }) {
                return Err(Error::InvalidFixture("carrying capacity exceeded".into()));
            }
        }
        let id = InventoryId(allocate(&mut self.state)?);
        self.inventory_index
            .entry(location.clone())
            .or_default()
            .push(id);
        self.state.inventories.insert(
            id,
            Inventory {
                id,
                location,
                quantity,
                capacity,
                access,
                last_granted: None,
                good,
                spoils_at: None,
            },
        );
        Ok(id)
    }

    pub fn debug_spoil_at(&mut self, inventory: InventoryId, at: SimTime) -> Result<(), Error> {
        let stock = self
            .state
            .inventories
            .get(&inventory)
            .ok_or_else(|| Error::InvalidFixture("unknown stock".into()))?;
        if stock.good != Good::Food || at <= self.state.time {
            return Err(Error::InvalidFixture("invalid spoilage".into()));
        }
        for due in self.state.due.values_mut() {
            due.retain(|d| !matches!(d, Due::Spoil(id) if *id == inventory));
        }
        self.state.due.retain(|_, due| !due.is_empty());
        self.state
            .inventories
            .get_mut(&inventory)
            .unwrap()
            .spoils_at = Some(at);
        self.state
            .due
            .entry(at)
            .or_default()
            .push(Due::Spoil(inventory));
        Ok(())
    }

    pub fn debug_injure(&mut self, actor: PersonId, damage: u64) -> Result<(), Error> {
        if !self
            .person_index
            .get(&actor)
            .is_some_and(|i| matches!(self.state.people[*i].location, Location::AtArea(_)))
        {
            return Err(Error::InvalidFixture(
                "fixture injuries require a stationary actor".into(),
            ));
        }
        self.ensure_event_capacity(1)?;
        let world = self.state.physical.as_mut().ok_or(Error::Unavailable)?;
        let a = world.actors.get_mut(&actor).ok_or(Error::Unavailable)?;
        a.vitals.health = a.vitals.health.saturating_sub(damage);
        if a.vitals.health == 0 && !self.state.people[self.person_index[&actor]].hunger_exempt {
            a.vitals.alive = false;
            a.job = None;
            self.emit(EventKind::Died { actor });
            for people in self.dependent_index.values_mut() {
                people.remove(&actor);
            }
            for due in self.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Age(id) if *id == actor));
            }
            self.state.due.retain(|_, due| !due.is_empty());
            self.invalidate_pregnancies(actor, true);
        }
        self.interrupt_actor_activity(actor);
        self.observe_lineage_deaths();
        self.observe_invocation_deaths()?;
        Ok(())
    }
}

pub(crate) fn rules_for(state: &CanonicalState, actor: PersonId) -> Rules {
    let mut rules = state
        .physical
        .as_ref()
        .expect("physical body rules")
        .rules
        .clone();
    if let Some(phenotype) = state
        .genetics
        .as_ref()
        .and_then(|world| world.phenotype(actor).ok())
    {
        let scale = |value: u64, numerator: u64, denominator: u64| {
            (u128::from(value) * u128::from(numerator) / u128::from(denominator))
                .min(u128::from(u64::MAX)) as u64
        };
        rules.max_health = scale(
            rules.max_health,
            u64::from(phenotype.max_health_milli),
            1_000,
        )
        .max(1);
        rules.carrying_capacity.0 = scale(
            rules.carrying_capacity.0,
            u64::from(phenotype.strength_milli),
            1_000,
        );
        rules.hunger_harm = scale(
            rules.hunger_harm,
            1_000,
            u64::from(phenotype.metabolism_milli),
        );
        rules.meal_relief = scale(
            rules.meal_relief,
            1_000,
            u64::from(phenotype.metabolism_milli),
        );
    }
    rules
}
