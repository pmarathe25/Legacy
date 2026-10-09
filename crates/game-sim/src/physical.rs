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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
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
    remaining: Option<u64>,
}

impl Simulation {
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
                    can_access: i.access.contains(&ruler.id),
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

    pub(crate) fn submit_physical(
        &mut self,
        submission: Submission,
    ) -> Result<CommandReceipt, Error> {
        self.state
            .next_command
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.debug_physical_command(self.state.ruler, submission.command.clone())?;
        let receipt = CommandReceipt {
            caller_sequence: submission.caller_sequence,
            authoritative_sequence: self.state.next_command,
            accepted_at: self.state.time,
        };
        self.state.next_command += 1;
        self.state.accepted.insert(
            submission.caller_sequence,
            (submission.command, receipt.clone()),
        );
        Ok(receipt)
    }

    pub fn debug_physical_command(
        &mut self,
        actor: PersonId,
        command: Command,
    ) -> Result<(), Error> {
        if let Command::Conceive { partner } = command {
            return self.start_conception(actor, partner);
        }
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
                    || food.good != Good::Food
                    || !self.can_use(actor, food)
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
                        accessible: i.access.contains(&actor),
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
                    || quantity.0 == 0
                    || source_inventory.good != destination_inventory.good
                    || (source_inventory.location != InventoryLocation::Carried(actor)
                        && destination_inventory.location != InventoryLocation::Carried(actor))
                    || !self.can_use(actor, source_inventory)
                    || !self.can_use(actor, destination_inventory)
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
                let tool = &self.state.inventories[&site_record.tool];
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
                        }),
                        Some((site_record.input, site_record.input_quantity)),
                    )
                }
            }
            _ => return Err(Error::Unavailable),
        };
        self.ensure_event_capacity(2)?;
        self.interrupt_physical(actor);
        if let Some((source, quantity)) = consume {
            self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
            self.emit(EventKind::Consumed {
                actor,
                source,
                quantity,
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
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn interrupt_physical(&mut self, actor: PersonId) {
        self.invalidate_pregnancies(actor, false);
        if let Some(world) = &mut self.state.physical {
            let a = world.actors.get_mut(&actor).unwrap();
            if let Activity::Care { child, .. } | Activity::Comfort { child, .. } = a.activity {
                self.care_index.remove(&child);
            }
            a.activity = Activity::Idle;
            for due in self.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Physical(id) if *id == actor));
            }
            self.state.due.retain(|_, due| !due.is_empty());
        }
    }

    fn can_use(&self, actor: PersonId, inventory: &Inventory) -> bool {
        let p = &self.state.people[self.person_index[&actor]];
        if let Location::AtArea(area) = p.location {
            inventory.access.contains(&actor) && self.inventory_area(inventory) == Some(area)
        } else {
            false
        }
    }

    fn unreserved_stock(&self, source: InventoryId, excluding: PersonId) -> u64 {
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
        u128::from(self.state.inventories[&source].quantity.0).saturating_sub(reserved) as u64
    }

    pub(crate) fn transfer_capacity(&self, destination: InventoryId, amount: u64) -> bool {
        let Some(world) = &self.state.physical else {
            return true;
        };
        let inventory = &self.state.inventories[&destination];
        if let InventoryLocation::Carried(actor) = inventory.location {
            let carried: u128 = self
                .inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .map(|id| u128::from(self.state.inventories[id].quantity.0))
                .sum();
            carried + u128::from(amount) <= u128::from(world.rules.carrying_capacity.0)
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
            let p = &self.state.people[self.person_index[id]];
            if a.vitals.health < world.rules.max_health
                && (p.hunger_exempt
                    || a.activity == Activity::Rest && a.vitals.hunger < world.rules.hunger_harm)
            {
                dt = dt.min(world.rules.max_health - a.vitals.health);
            }
            if !p.hunger_exempt {
                let until_harm = world.rules.hunger_harm.saturating_sub(a.vitals.hunger);
                if until_harm > 0 {
                    dt = dt.min(until_harm);
                }
                dt = dt.min(until_harm.saturating_add(a.vitals.health));
            }
            if matches!(
                a.activity,
                Activity::Work { .. } | Activity::Load { .. } | Activity::Play { .. }
            ) {
                dt = dt.min(world.rules.fatigue_limit.saturating_sub(a.vitals.fatigue));
            }
            if matches!(a.activity, Activity::Work { .. })
                && let Some(job) = &a.job
            {
                dt = dt.min(job.remaining);
            }
        }
        // Inactive/dead records never impose a zero-length scheduling loop.
        SimTime(self.state.time.0 + dt.max(1).min(limit.0 - self.state.time.0))
    }

    pub(crate) fn evaluate_physical(&self, elapsed: u64) -> Result<Vec<Outcome>, Error> {
        let Some(world) = &self.state.physical else {
            return Ok(Vec::new());
        };
        let work = |(id, a): (&PersonId, &Actor)| -> Result<Outcome, Error> {
            let p = &self.state.people[self.person_index[id]];
            let mut vitals = a.vitals.clone();
            if vitals.alive {
                if !p.hunger_exempt {
                    vitals.hunger = vitals.hunger.checked_add(elapsed).ok_or(Error::Overflow)?;
                    let harm = vitals.hunger.saturating_sub(world.rules.hunger_harm)
                        - a.vitals.hunger.saturating_sub(world.rules.hunger_harm);
                    vitals.health = vitals.health.saturating_sub(harm);
                }
                let exerting = matches!(
                    a.activity,
                    Activity::Work { .. }
                        | Activity::Load { .. }
                        | Activity::Combat { .. }
                        | Activity::Play { .. }
                ) || matches!(p.location, Location::InTransit { .. });
                if exerting {
                    vitals.fatigue = vitals
                        .fatigue
                        .saturating_add(elapsed)
                        .min(world.rules.fatigue_limit);
                } else if a.activity == Activity::Rest {
                    vitals.fatigue = vitals.fatigue.saturating_sub(elapsed.saturating_mul(2));
                }
                if p.hunger_exempt {
                    vitals.health = vitals
                        .health
                        .saturating_add(elapsed)
                        .min(world.rules.max_health);
                } else if a.activity == Activity::Rest {
                    let healing =
                        elapsed.min(world.rules.hunger_harm.saturating_sub(a.vitals.hunger));
                    vitals.health = vitals
                        .health
                        .saturating_add(healing)
                        .min(world.rules.max_health);
                }
                if vitals.health == 0 && !p.hunger_exempt {
                    vitals.alive = false;
                }
            }
            let remaining = a.job.as_ref().map(|j| {
                if matches!(a.activity, Activity::Work { .. }) {
                    j.remaining.saturating_sub(elapsed)
                } else {
                    j.remaining
                }
            });
            Ok(Outcome {
                id: *id,
                vitals,
                remaining,
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

    pub(crate) fn commit_physical(&mut self, outcomes: Vec<Outcome>) {
        for out in outcomes {
            let actor = out.id;
            let world = self.state.physical.as_ref().unwrap();
            let previous = &world.actors[&actor];
            let previously_alive = previous.vitals.alive;
            let previously_injured = previous.vitals.health < world.rules.max_health;
            let rules = world.rules.clone();
            let mut vitals = out.vitals;
            let ate = self
                .state
                .events
                .iter()
                .rev()
                .take_while(|e| e.time == self.state.time)
                .any(|e| {
                    matches!(e.kind, EventKind::Ate { actor: id, .. } if id == actor)
                        || matches!(e.kind, EventKind::Cared { child: id, .. } if id == actor)
                });
            if ate {
                vitals.hunger = vitals.hunger.saturating_sub(rules.meal_relief);
            }
            let job_done =
                out.remaining == Some(0) && matches!(previous.activity, Activity::Work { .. });
            let interrupted = vitals.fatigue >= rules.fatigue_limit
                && matches!(
                    previous.activity,
                    Activity::Work { .. } | Activity::Load { .. } | Activity::Play { .. }
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
            if let (Some(job), Some(remaining)) = (&mut a.job, out.remaining) {
                job.remaining = remaining;
            }
            if job_done {
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
                    self.interrupt_physical(actor);
                    self.emit(EventKind::Interrupted { actor });
                }
            } else if interrupted {
                self.interrupt_physical(actor);
                self.emit(EventKind::Interrupted { actor });
            }
            if previously_alive && !vitals.alive {
                self.interrupt_physical(actor);
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

    pub fn debug_add_inventory(
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
            if self.state.physical.as_ref().is_some_and(|w| {
                held + u128::from(quantity.0) > u128::from(w.rules.carrying_capacity.0)
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
        self.interrupt_physical(actor);
        Ok(())
    }
}
