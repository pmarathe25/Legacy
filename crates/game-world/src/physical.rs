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

pub use game_workshop::{Job, Site};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub fatigue_remainder: u16,
    pub health_remainder: u16,
    pub vitals: Vitals,
    pub activity: Activity,
    pub job: Option<Job>,
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
    pub(crate) fn care_dose(&self, good: Good) -> Option<Quantity> {
        match good {
            Good::Food => self
                .state
                .physical
                .as_ref()
                .map(|world| world.rules.meal_size),
            Good::Water => self
                .state
                .health
                .as_ref()
                .map(|world| world.rules.drink_quantity),
            _ => None,
        }
    }

    pub(crate) fn prepare_care_source(
        &self,
        actor: PersonId,
        child: PersonId,
        source: InventoryId,
    ) -> Result<Quantity, Error> {
        let life = self
            .state
            .development
            .as_ref()
            .and_then(|world| world.lives.get(&child))
            .ok_or(Error::Unavailable)?;
        let receiver = self
            .state
            .physical
            .as_ref()
            .and_then(|world| world.actors.get(&child))
            .ok_or(Error::Unavailable)?;
        let stock = self
            .state
            .inventories
            .get(&source)
            .ok_or(Error::Unavailable)?;
        let quantity = self.care_dose(stock.good).ok_or(Error::Unavailable)?;
        if !matches!(life.stage, LifeStage::Infant | LifeStage::Child)
            || actor == child
            || !receiver.vitals.alive
            || !matches!(receiver.activity, Activity::Idle | Activity::Rest)
            || self.care_index.get(&child).is_some_and(|id| *id != actor)
            || !self.transport_same_place(actor, child)
            || !self.can_use(actor, stock)
            || self.unreserved_stock(source, actor) < quantity.0
        {
            return Err(Error::Unavailable);
        }
        if stock.good == Good::Water {
            self.preflight_water_care(child, source, quantity)?;
        }
        Ok(quantity)
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
        (body.vitals.health, body.health_remainder) = game_physiology::traits::rescale_health(
            body.vitals.health,
            body.health_remainder,
            body.vitals.alive,
            old_health_milli,
            new_health_milli,
        );
    }
    pub(crate) fn initialize_physical_people(&mut self, rules: Rules) -> Result<(), Error> {
        if self.state.physical.is_some()
            || rules.max_health == 0
            || rules.fatigue_limit == 0
            || rules.eating_time.0 == 0
            || rules.loading_time.0 == 0
            || rules.carrying_capacity.0 == 0
        {
            return Err(Error::InvalidConfiguration("invalid initial bodies".into()));
        }
        let actors = self
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
        self.state.physical = Some(World {
            rules,
            actors,
            sites: BTreeMap::new(),
        });
        Ok(())
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
        sim.initialize_physical_people(rules)?;
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
        let area = if let Location::AtArea(a) = self.resolved_location(ruler.id) {
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
        if !a.vitals.alive || a.vitals.health == 0 {
            return Err(Error::Unavailable);
        }
        let area = match self.resolved_location(actor) {
            Location::AtArea(area) => area,
            Location::StoppedOnRoad { from, .. }
                if matches!(
                    command,
                    Command::Cancel
                        | Command::Rest
                        | Command::Eat { .. }
                        | Command::Load { .. }
                        | Command::Care { .. }
                        | Command::Comfort { .. }
                        | Command::FreeCaptive { .. }
                        | Command::Signal { .. }
                ) =>
            {
                from
            }
            Location::InTransit { from, .. }
                if matches!(command, Command::Signal { .. } | Command::Cancel) =>
            {
                from
            }
            _ => return Err(Error::Unavailable),
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
                    || !self.transport_same_place(actor, child)
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
                self.prepare_care_source(actor, child, source)?;
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
                let job = self.prepare_cargo_load(actor, source, destination, quantity)?;
                (Self::cargo_load_activity(&job), None, None)
            }
            Command::Work { site } => {
                let site_record = world.sites.get(&site).ok_or(Error::Unavailable)?;
                let tool_id = self
                    .select_work_tool(actor, site)
                    .ok_or(Error::Unavailable)?;
                let input = &self.state.inventories[&site_record.input];
                let start = site_record.prepare_start(
                    a.job.as_ref(),
                    input,
                    &self.state.inventories[&tool_id],
                    &self.state.inventories[&site_record.output],
                    game_workshop::StartFacts {
                        actor,
                        area,
                        capable: a.vitals.alive && a.vitals.health > 0,
                        adult: self.state.development.as_ref().is_none_or(|world| {
                            matches!(world.lives[&actor].stage, LifeStage::Adult | LifeStage::Elder)
                        }),
                        fatigue_allows: a.vitals.fatigue < rules.fatigue_limit,
                        site_access: site_record.access.contains(&actor),
                        occupied_by_other: world.actors.iter().any(|(id, other)| {
                            *id != actor && matches!(other.activity, Activity::Work { site: working } if working == site)
                        }),
                        input_usable: self.can_use(actor, input),
                        input_unreserved: Quantity(self.unreserved_stock(site_record.input, actor)),
                        tool_usable: self.can_use(actor, &self.state.inventories[&tool_id]),
                        rate: self.effective_work_rate(actor, Skill::Craft),
                        body_rate: SkillRate {
                            numerator: u64::from(self.genetic_strength_milli(actor)),
                            denominator: 1_000,
                        },
                    },
                )?;
                (Activity::Work { site }, Some(start.job), start.consumption)
            }
            _ => return Err(Error::Unavailable),
        };
        let project_input = if let Command::Work { site } = command
            && let Some(consumption) = &consume
        {
            self.prepare_enterprise_input(site, consumption.quantity)?
        } else {
            None
        };
        self.ensure_event_capacity(3)?;
        self.commit_project_input(project_input)?;
        self.apply_prepared_physical_action(actor, command, activity, new_job, consume)
    }

    /// Mutation bridge for an already admitted action. Cargo allocation invokes
    /// this only after its owner checks every proposed-slot load requirement.
    fn apply_prepared_physical_action(
        &mut self,
        actor: PersonId,
        command: Command,
        activity: Activity,
        new_job: Option<Job>,
        consume: Option<game_workshop::Consumption>,
    ) -> Result<(), Error> {
        let phase_started = Instant::now();
        self.interrupt_actor_activity(actor);
        self.profile
            .record_phase("physical.interrupt_actor_activity", phase_started);
        if let Some(consumption) = consume {
            consumption.commit(self.state.inventories.get_mut(&consumption.source).unwrap())?;
            self.emit(EventKind::Consumed {
                actor,
                source: consumption.source,
                quantity: consumption.quantity,
            });
        }
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
        if matches!(command, Command::Cancel) {
            game_workshop::cancel(&mut a.job);
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
        let phase_started = Instant::now();
        self.update_property_events()?;
        self.profile
            .record_phase("physical.update_property_events", phase_started);
        let phase_started = Instant::now();
        self.reconcile_agreements()?;
        self.profile
            .record_phase("physical.reconcile_agreements", phase_started);
        let phase_started = Instant::now();
        self.observe_agreement_events();
        self.profile
            .record_phase("physical.observe_agreement_events", phase_started);
        let phase_started = Instant::now();
        self.reconcile_institutions()?;
        self.profile
            .record_phase("physical.reconcile_institutions", phase_started);
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn cargo_actor_facts(
        &self,
        actor: PersonId,
    ) -> Result<game_cargo::ActorFacts, Error> {
        let body = self
            .state
            .physical
            .as_ref()
            .and_then(|world| world.actors.get(&actor))
            .ok_or(Error::Unavailable)?;
        Ok(game_cargo::ActorFacts {
            actor,
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: self.independent(actor),
            confined: self.combat_restricts(actor),
            in_combat: self.in_combat(actor),
            receiving_care: self.care_index.contains_key(&actor),
            stationary: matches!(
                self.resolved_location(actor),
                Location::AtArea(_) | Location::StoppedOnRoad { .. }
            ),
            fatigue: body.vitals.fatigue,
            fatigue_limit: self.body_rules(actor).fatigue_limit,
        })
    }
    pub(crate) fn cargo_source_facts(
        &self,
        actor: PersonId,
        stock: &Inventory,
        quantity: Quantity,
    ) -> game_cargo::SourceFacts {
        game_cargo::SourceFacts {
            contact: self.stock_here(actor, stock),
            accessible: self.has_stock_access(actor, stock),
            unreserved: Quantity(self.unreserved_stock(stock.id, actor)),
            record_permission: self.record_transfer_allowed(stock.id, quantity),
        }
    }
    pub(crate) fn cargo_receiver_facts(
        &self,
        actor: PersonId,
        source: InventoryId,
        stock: &Inventory,
        quantity: Quantity,
    ) -> game_cargo::ReceiverFacts {
        let (body_room, transport_allows) = match stock.location {
            InventoryLocation::Carried(holder) => {
                let held: u128 = self
                    .inventory_index
                    .get(&InventoryLocation::Carried(holder))
                    .into_iter()
                    .flatten()
                    .map(|id| u128::from(self.state.inventories[id].quantity.0))
                    .sum();
                (
                    Quantity(
                        u128::from(self.body_rules(holder).carrying_capacity.0).saturating_sub(held)
                            as u64,
                    ),
                    self.transport_transfer_capacity(holder, quantity.0),
                )
            }
            InventoryLocation::Area(_) => (Quantity(u64::MAX), true),
        };
        game_cargo::ReceiverFacts {
            contact: self.stock_here(actor, stock),
            accessible: self.has_stock_access(actor, stock),
            accepted: self.agreement_accepts_delivery(actor, stock.id, quantity)
                || self.guardian_accepts_cargo(actor, source, stock, quantity),
            body_room,
            transport_allows,
        }
    }
    pub(crate) fn prepare_cargo_load(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> Result<game_cargo::LoadJob, Error> {
        let from = self
            .state
            .inventories
            .get(&source)
            .ok_or(Error::Unavailable)?;
        let to = self
            .state
            .inventories
            .get(&destination)
            .ok_or(Error::Unavailable)?;
        game_cargo::prepare_load(
            self.cargo_actor_facts(actor)?,
            from,
            to,
            quantity,
            self.cargo_source_facts(actor, from, quantity),
            self.cargo_receiver_facts(actor, source, to, quantity),
            self.state.time,
            self.body_rules(actor).loading_time,
        )
    }
    fn cargo_load_activity(job: &game_cargo::LoadJob) -> Activity {
        Activity::Load {
            source: job.source,
            destination: job.destination,
            quantity: job.quantity,
            completes: job.completes,
        }
    }
    pub(crate) fn apply_cargo_load(&mut self, job: game_cargo::LoadJob) -> Result<(), Error> {
        let command = Command::Load {
            source: job.source,
            destination: job.destination,
            quantity: job.quantity,
        };
        self.apply_prepared_physical_action(
            job.actor,
            command,
            Self::cargo_load_activity(&job),
            None,
            None,
        )
    }

    pub(crate) fn can_use(&self, actor: PersonId, inventory: &Inventory) -> bool {
        self.has_stock_access(actor, inventory) && self.stock_here(actor, inventory)
    }

    pub(crate) fn stock_here(&self, actor: PersonId, inventory: &Inventory) -> bool {
        match inventory.location {
            InventoryLocation::Area(area) => {
                self.resolved_location(actor) == Location::AtArea(area)
            }
            InventoryLocation::Carried(holder) => self.transport_same_place(actor, holder),
        }
    }

    pub(crate) fn reservation_totals(
        &self,
        source: Option<InventoryId>,
    ) -> Result<game_cargo::ReservationTotals, Error> {
        let physical = self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let claims = physical.actors.iter().filter_map(|(actor, body)| {
            let (inventory, quantity) = match body.activity {
                Activity::Eat { source, .. } => (source, physical.rules.meal_size),
                Activity::Care { source, .. } => (
                    source,
                    self.care_dose(self.state.inventories[&source].good)
                        .unwrap_or(Quantity(0)),
                ),
                Activity::Load {
                    source, quantity, ..
                } => (source, quantity),
                _ => return None,
            };
            source
                .is_none_or(|wanted| inventory == wanted)
                .then_some(game_cargo::Reservation {
                    actor: *actor,
                    source: inventory,
                    quantity,
                })
        });
        let health = self
            .state
            .health
            .as_ref()
            .into_iter()
            .flat_map(|world| world.jobs.values())
            .filter_map(|job| {
                let (inventory, quantity) = match job.work {
                    game_health::Work::Drink { source, quantity }
                    | game_health::Work::Boil {
                        source, quantity, ..
                    } => (source, quantity),
                    _ => return None,
                };
                source
                    .is_none_or(|wanted| inventory == wanted)
                    .then_some(game_cargo::Reservation {
                        actor: job.actor,
                        source: inventory,
                        quantity,
                    })
            });
        game_cargo::ReservationTotals::prepare(claims.chain(health))
    }

    pub(crate) fn unreserved_stock(&self, source: InventoryId, excluding: PersonId) -> u64 {
        self.unreserved_stock_for_planning(source, excluding, None)
    }

    pub(crate) fn unreserved_stock_for_planning(
        &self,
        source: InventoryId,
        excluding: PersonId,
        resources: Option<&equipment::RankingResources>,
    ) -> u64 {
        let quantity = self.state.inventories[&source].quantity;
        if let Some(resources) = resources {
            return resources
                .reservations
                .available(source, quantity, excluding)
                .0;
        }
        self.reservation_totals(Some(source))
            .map_or(0, |totals| totals.available(source, quantity, excluding).0)
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
                && self.transport_transfer_capacity(actor, amount)
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
                    job.completion_after(self.invocation_work_rate(*id, job.rate, job.body_rate))
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
                | Activity::ArmyApproach { .. }
                | Activity::Play { .. }
                | Activity::Cultivate { .. }
        ) || self.transport_exerting(actor)
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
                let progress = job.evaluate(
                    elapsed,
                    self.invocation_work_rate(*id, job.rate, job.body_rate),
                    matches!(a.activity, Activity::Work { .. }),
                    equipment.get(id).copied().unwrap_or(false),
                )?;
                (Some(progress.remaining), Some(progress.remainder))
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
        self.engine.executor().install(|| {
            if world.actors.len() < 256 || self.execution.workers == 1 {
                world.actors.iter().map(work).collect()
            } else {
                world.actors.par_iter().map(work).collect()
            }
        })
    }

    pub(crate) fn workshop_practice_deltas(
        &self,
        elapsed: u64,
        equipment: &BTreeMap<PersonId, bool>,
    ) -> Vec<game_skills::PracticeDelta> {
        let Some(world) = &self.state.physical else {
            return Vec::new();
        };
        equipment
            .iter()
            .filter_map(|(actor, equipped)| {
                let body = &world.actors[actor];
                body.job.as_ref()?.practice(
                    *actor,
                    elapsed,
                    body.vitals.alive
                        && body.vitals.health > 0
                        && matches!(body.activity, Activity::Work { .. })
                        && matches!(
                            self.state.people[self.person_index[actor]].location,
                            Location::AtArea(_)
                        ),
                    *equipped,
                )
            })
            .collect()
    }

    pub(crate) fn complete_physical(&mut self, actor: PersonId) -> Result<(), Error> {
        let activity = self.state.physical.as_ref().unwrap().actors[&actor]
            .activity
            .clone();
        if matches!(activity, Activity::RealmTalk { .. }) {
            self.complete_realm_talk(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Lift { .. }) {
            self.complete_transport_lift(actor);
            return Ok(());
        }
        let mut success = false;
        if matches!(
            activity,
            Activity::Social { .. } | Activity::Instruction { .. } | Activity::Practice { .. }
        ) {
            self.complete_social(actor);
            return Ok(());
        }
        if matches!(activity, Activity::EnforcementTalk { .. }) {
            self.complete_enforcement_talk(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Gather { .. }) {
            self.complete_gathering(actor);
            return Ok(());
        }
        if matches!(
            activity,
            Activity::Escape { .. } | Activity::Restrain { .. }
        ) {
            self.complete_force(actor);
            return Ok(());
        }
        if matches!(activity, Activity::ReportAgreement { .. }) {
            self.complete_agreement_report(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Construct { .. }) {
            self.complete_construction(actor);
            return Ok(());
        }
        if matches!(activity, Activity::Lesson { .. }) {
            self.complete_lesson(actor);
            return Ok(());
        }
        if matches!(
            activity,
            Activity::Take { .. } | Activity::ReportIncident { .. }
        ) {
            self.complete_property_action(actor)?;
            return Ok(());
        }
        if matches!(activity, Activity::Cultivate { .. }) {
            self.complete_cultivation(actor);
            return Ok(());
        }
        if let Activity::Conceive { pregnancy, .. } = activity {
            self.complete_conception(pregnancy);
            return Ok(());
        }
        if activity == Activity::Idle {
            return Ok(());
        }
        match activity {
            Activity::Comfort { child, .. } => {
                self.care_index.remove(&child);
                if self.state.physical.as_ref().unwrap().actors[&child]
                    .vitals
                    .alive
                    && self.transport_same_place(child, actor)
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
                completes,
            } => {
                let job = game_cargo::LoadJob {
                    actor,
                    source,
                    destination,
                    quantity,
                    completes,
                };
                let intent = TransferIntent {
                    actor,
                    source,
                    destination,
                    quantity,
                };
                let request =
                    job.closing_request(self.state.time, self.cargo_closing_facts(&intent))?;
                let resolved = self
                    .prepare_cargo_requests(vec![request], None)?
                    .commit(&mut self.state.inventories)?;
                if resolved[0].1 {
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
                let quantity = self
                    .care_dose(self.state.inventories[&source].good)
                    .ok_or(Error::Unavailable)?;
                if recipient.vitals.alive
                    && self.transport_same_place(child, actor)
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
                        return Ok(());
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
        Ok(())
    }

    pub(crate) fn commit_physical(
        &mut self,
        outcomes: Vec<Outcome>,
        equipment: &BTreeMap<PersonId, bool>,
    ) -> Result<(), Error> {
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
                    && game_workshop::interrupted(
                        vitals.fatigue < rules.fatigue_limit,
                        equipment.get(&actor).copied().unwrap_or(false),
                    ))
                || vitals.fatigue >= rules.fatigue_limit
                    && matches!(
                        previous.activity,
                        Activity::Gather { .. }
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
                job.apply_progress(game_workshop::Progress {
                    remaining,
                    remainder: out.work_remainder.unwrap(),
                });
            }
            if job_done && equipment.get(&actor).copied().unwrap_or(false) {
                let site_id = a.job.as_ref().unwrap().site;
                let world = self.state.physical.as_ref().unwrap();
                let site = &world.sites[&site_id];
                let production = world.actors[&actor]
                    .job
                    .as_ref()
                    .unwrap()
                    .prepare_completion(
                        site,
                        &self.state.inventories[&site.output],
                        game_workshop::CompletionFacts {
                            actor,
                            active: true,
                            alive_at_start: previously_alive,
                            equipped: equipment.get(&actor).copied().unwrap_or(false),
                        },
                    )?;
                let output_id = site.output;
                if let Some(production) = production {
                    let receipt = EventKind::Produced {
                        actor: production.actor,
                        site: production.site,
                        output: production.output,
                        quantity: production.quantity,
                    };
                    let world = self.state.physical.as_mut().unwrap();
                    let body = world.actors.get_mut(&actor).unwrap();
                    production.commit(
                        &mut body.job,
                        world.sites.get_mut(&site_id).unwrap(),
                        self.state.inventories.get_mut(&output_id).unwrap(),
                    )?;
                    body.activity = Activity::Idle;
                    self.emit(receipt);
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
        Ok(())
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
                if !matches!(actor.activity, Activity::Work { .. }) {
                    return None;
                }
                let job = actor.job.as_ref()?;
                let site = &world.sites[&job.site];
                job.output_forecast(
                    game_workshop::Progress {
                        remaining: out.remaining?,
                        remainder: out.work_remainder?,
                    },
                    site,
                    self.state.inventories[&site.output].good,
                )
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
        self.update_property_events()?;
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
                quantity.0 > 0
                    && (held + u128::from(quantity.0)
                        > u128::from(self.body_rules(actor).carrying_capacity.0)
                        || !self.transport_transfer_capacity(actor, quantity.0))
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
    if let Some(phenotype) = genetics::phenotype(state, actor) {
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
