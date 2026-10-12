//! Physical rules with explicit scenario rates. The survival fixture is test data,
//! not the setting's eventual biology, farming cycle, or autonomous social policy.
use crate::*;

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
#[serde(deny_unknown_fields)]
pub struct World {
    pub rules: Rules,
}

#[derive(Clone)]
pub(crate) struct Outcome {
    id: PersonId,
    opening_body: game_physiology::Body,
    opening_activity: Activity,
    opening_job: Option<Job>,
    opening_location: Location,
    opening_development: Option<game_physiology::traits::Development>,
    opening_phenotype: Option<game_physiology::traits::Phenotype>,
    vitals: Vitals,
    fatigue_remainder: u16,
    health_remainder: u16,
    remaining: Option<u64>,
    work_remainder: Option<u64>,
}

impl Simulation {
    pub(crate) fn body_rules(&self, actor: PersonId) -> Rules {
        rules_for(&self.canonical_state(), actor)
    }
    pub(crate) fn care_dose(&self, good: Good) -> Option<Quantity> {
        match good {
            Good::Food => self
                .host
                .state
                .physical
                .as_ref()
                .map(|world| world.rules.meal_size),
            Good::Water => self
                .host
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
            .host
            .state
            .development
            .as_ref()
            .and_then(|_| game_lifecycle::life(&self.host.storage, child))
            .ok_or(Error::Unavailable)?;
        let receiver = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, child))
            .ok_or(Error::Unavailable)?;
        let stock = game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
        let quantity = self.care_dose(stock.good).ok_or(Error::Unavailable)?;
        if !matches!(life.stage, LifeStage::Infant | LifeStage::Child)
            || actor == child
            || !receiver.vitals.alive
            || !matches!(
                self.host.storage.activity(child),
                Some(Activity::Idle | Activity::Rest)
            )
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
            .host
            .state
            .physical
            .as_mut()
            .and_then(|_| game_physiology::body_mut(&mut self.host.storage, actor))
        else {
            return;
        };
        body.rescale_health_factor(old_health_milli, new_health_milli);
    }
    pub(crate) fn initialize_physical_people(&mut self, rules: Rules) -> Result<(), Error> {
        if self.host.state.physical.is_some()
            || rules.max_health == 0
            || rules.fatigue_limit == 0
            || rules.eating_time.0 == 0
            || rules.loading_time.0 == 0
            || rules.carrying_capacity.0 == 0
        {
            return Err(Error::InvalidConfiguration("invalid initial bodies".into()));
        }
        let people: Vec<_> = self
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect();
        for person in people {
            game_physiology::insert_body(
                &mut self.host.storage,
                person,
                game_physiology::Body {
                    fatigue_remainder: 0,
                    health_remainder: 0,
                    vitals: Vitals {
                        hunger: 0,
                        fatigue: 0,
                        health: rules.max_health,
                        alive: true,
                    },
                },
            )?;
            self.host.storage.set_activity(person, Activity::Idle)?;
        }
        self.host.state.physical = Some(World { rules });
        Ok(())
    }
    pub fn survival_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::kernel_fixture(seed, 15, execution)?;
        sim.host.state.scenario = scenarios().remove(1);
        let village = sim.host.state.areas[0].id;
        let forest = sim.host.state.areas[2].id;
        let people: BTreeSet<_> = sim.host.state.people.iter().map(|p| p.id).collect();
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
        let food = game_cargo::inventories(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        game_cargo::inventory_mut(&mut sim.host.storage, food)
            .unwrap()
            .quantity = Quantity(100_000);
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
        for (area, input, tool, output) in [
            (village, timber, tools, food),
            (forest, deposit, forest_tools, output),
        ] {
            let id = SiteId(sim.host.allocate_id()?.0);
            game_workshop::insert_site(
                &mut sim.host.storage,
                Site {
                    recipe: game_workshop::RecipeId(
                        if area == village {
                            "timber-food"
                        } else {
                            "timber-timber"
                        }
                        .into(),
                    ),
                    id,
                    area,
                    input,
                    tool,
                    output,
                    access: people.clone(),
                    last_granted: None,
                },
            )
            .expect("validated site insertion");
        }
        // Each good has a separate physical stack. Carrying capacity is shared
        // across every stack carried by the same person.
        for p in sim.host.state.people.clone() {
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
        self.host.state.physical.as_ref()?;
        let actor =
            &game_physiology::body(&self.host.storage, self.host.state.ruler).expect("body exists");
        let ruler = &self.host.state.people[self.person_index[&self.host.state.ruler]];
        if !actor.vitals.alive {
            return Some(PhysicalView {
                vitals: actor.vitals.clone(),
                activity: self
                    .host
                    .storage
                    .activity(self.host.state.ruler)
                    .expect("activity exists")
                    .clone(),
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
            activity: self
                .host
                .storage
                .activity(self.host.state.ruler)
                .expect("activity exists")
                .clone(),
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
                .map(|id| {
                    game_cargo::inventory(&self.host.storage, *(id)).expect("inventory exists")
                })
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
            local_sites: game_workshop::sites(&self.host.storage)
                .map(|(_, site)| site)
                .filter(|s| Some(s.area) == area && s.access.contains(&ruler.id))
                .map(|s| s.id)
                .collect(),
        })
    }

    pub(crate) fn physically_working(&self, actor: PersonId) -> bool {
        self.host.state.physical.as_ref().is_none_or(|_| {
            let a = &game_physiology::body(&self.host.storage, actor).expect("body exists");
            a.vitals.alive
                && a.vitals.health > 0
                && matches!(
                    *self.host.storage.activity(actor).expect("activity exists"),
                    Activity::Work { .. }
                )
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
        let world = self
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let a = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
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
                    .host
                    .state
                    .development
                    .as_ref()
                    .and_then(|_| game_lifecycle::life(&self.host.storage, actor))
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
                    self.host
                        .clock()
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
                    .host
                    .state
                    .development
                    .as_ref()
                    .and_then(|_| game_lifecycle::life(&self.host.storage, actor))
                    .ok_or(Error::Unavailable)?;
                if life.stage != LifeStage::Child || a.vitals.fatigue >= rules.fatigue_limit {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.host
                        .clock()
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
                    self.host
                        .clock()
                        .time
                        .0
                        .checked_add(rules.loading_time.0)
                        .ok_or(Error::Overflow)?,
                );
                (Activity::Unbind { target, completes }, None, None)
            }
            Command::Comfort { child } => {
                let life = self
                    .host
                    .state
                    .development
                    .as_ref()
                    .and_then(|_| game_lifecycle::life(&self.host.storage, child))
                    .ok_or(Error::Unavailable)?;
                if !matches!(life.stage, LifeStage::Infant | LifeStage::Child)
                    || child == actor
                    || !game_physiology::body(&self.host.storage, child)
                        .expect("body exists")
                        .vitals
                        .alive
                    || !self.transport_same_place(actor, child)
                    || self.care_index.contains_key(&child)
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.host
                        .clock()
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
                    self.host
                        .clock()
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
                    .host
                    .state
                    .decisions
                    .as_ref()
                    .and_then(|_| {
                        game_characters::decisions::mind::<game_api::Command>(
                            &self.host.storage,
                            actor,
                        )
                    })
                    .and_then(|m| m.knowledge.get(&inventory))
                    .cloned();
                let visible = game_cargo::inventory(&self.host.storage, inventory).filter(|i| {
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
                        observed_at: self.host.clock().time,
                        learned_at: self.host.clock().time,
                        source: KnowledgeSource::Observed(actor),
                    })
                    .or(remembered)
                    .ok_or(Error::Unavailable)?;
                if self.host.state.decisions.is_none() {
                    return Err(Error::Unavailable);
                }
                let receiver = self
                    .person_index
                    .get(&recipient)
                    .map(|i| &self.host.state.people[*i])
                    .ok_or(Error::Unavailable)?;
                if recipient == actor
                    || *game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId((receiver.id).0),
                    )
                    .expect("location exists")
                        != Location::AtArea(area)
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.host
                        .clock()
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
                let inventory =
                    game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
                if inventory.good != Good::Food
                    || !self.can_use(actor, inventory)
                    || self.unreserved_stock(source, actor) < rules.meal_size.0
                {
                    return Err(Error::Unavailable);
                }
                let completes = SimTime(
                    self.host
                        .clock()
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
                let site_record =
                    game_workshop::site(&self.host.storage, site).ok_or(Error::Unavailable)?;
                let tool_id = self
                    .select_work_tool(actor, site)
                    .ok_or(Error::Unavailable)?;
                let input = game_cargo::inventory(&self.host.storage, site_record.input)
                    .expect("inventory exists");
                let start = self.host.state.workshops.resolve_site(site_record)?.prepare_start(
                    game_workshop::job(&self.host.storage,actor),
                    input,
                    game_cargo::inventory(&self.host.storage, tool_id).expect("inventory exists"),
                    game_cargo::inventory(&self.host.storage, site_record.output).expect("inventory exists"),
                    game_workshop::StartFacts {
                        actor,
                        area,
                        capable: a.vitals.alive && a.vitals.health > 0,
                        adult: self.host.state.development.as_ref().is_none_or(|_| {
                            matches!(game_lifecycle::life(&self.host.storage,actor).unwrap().stage, LifeStage::Adult | LifeStage::Elder)
                        }),
                        fatigue_allows: a.vitals.fatigue < rules.fatigue_limit,
                        site_access: site_record.access.contains(&actor),
                        occupied_by_other: self.host.storage.activities().any(|(id, activity)| {
                            id != actor && matches!(*activity, Activity::Work { site: working } if working == site)
                        }),
                        input_usable: self.can_use(actor, input),
                        input_unreserved: Quantity(self.unreserved_stock(site_record.input, actor)),
                        tool_usable: self.can_use(actor, game_cargo::inventory(&self.host.storage, tool_id).expect("inventory exists")),
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
        self.host.preflight_records(0, 0, 1)?;
        let phase_started = Instant::now();
        self.interrupt_actor_activity(actor);
        self.profile
            .record_phase("physical.interrupt_actor_activity", phase_started);
        if let Some(consumption) = consume {
            consumption.commit(
                game_cargo::inventory_mut(&mut self.host.storage, consumption.source).unwrap(),
            )?;
            self.emit(EventKind::Consumed {
                actor,
                source: consumption.source,
                quantity: consumption.quantity,
            });
        }
        *self
            .host
            .storage
            .activity_mut(actor)
            .expect("activity exists") = activity.clone();
        if let Activity::Tell { ref fact, .. } = activity {
            game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                actor,
            )
            .unwrap()
            .knowledge
            .insert(fact.inventory, fact.clone());
        }
        if let Activity::Care { child, .. } | Activity::Comfort { child, .. } = activity {
            self.care_index.insert(child, actor);
        }
        if let Some(job) = new_job {
            game_workshop::insert_job(&mut self.host.storage, actor, job).expect("actor exists");
        }
        if matches!(command, Command::Cancel) {
            game_workshop::remove_job(&mut self.host.storage, actor)
                .map(|_| ())
                .expect("actor exists");
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
            self.host
                .state
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
        self.host.bump_revision(1)?;
        Ok(())
    }

    pub(crate) fn cargo_actor_facts(
        &self,
        actor: PersonId,
    ) -> Result<game_cargo::ActorFacts, Error> {
        let body = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
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
                    .map(|id| {
                        u128::from(
                            game_cargo::inventory(&self.host.storage, *(id))
                                .expect("inventory exists")
                                .quantity
                                .0,
                        )
                    })
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
        let from = game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
        let to =
            game_cargo::inventory(&self.host.storage, destination).ok_or(Error::Unavailable)?;
        game_cargo::prepare_load(
            self.cargo_actor_facts(actor)?,
            from,
            to,
            quantity,
            self.cargo_source_facts(actor, from, quantity),
            self.cargo_receiver_facts(actor, source, to, quantity),
            self.host.clock().time,
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
        let physical = self
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let claims = self
            .host
            .storage
            .activities()
            .filter_map(|(actor, activity)| {
                let (inventory, quantity) = match *activity {
                    Activity::Eat { source, .. } => (source, physical.rules.meal_size),
                    Activity::Care { source, .. } => (
                        source,
                        self.care_dose(
                            game_cargo::inventory(&self.host.storage, source)
                                .expect("inventory exists")
                                .good,
                        )
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
                        actor,
                        source: inventory,
                        quantity,
                    })
            });
        let health = self
            .host
            .state
            .health
            .as_ref()
            .into_iter()
            .flat_map(|_| game_health::jobs(&self.host.storage).map(|(_, job)| job))
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
        let quantity = game_cargo::inventory(&self.host.storage, source)
            .expect("inventory exists")
            .quantity;
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
        if self.host.state.physical.is_none() {
            return true;
        }
        let inventory =
            game_cargo::inventory(&self.host.storage, destination).expect("inventory exists");
        if let InventoryLocation::Carried(actor) = inventory.location {
            let carried: u128 = self
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
                .sum();
            carried + u128::from(amount) <= u128::from(self.body_rules(actor).carrying_capacity.0)
                && self.transport_transfer_capacity(actor, amount)
        } else {
            true
        }
    }

    pub(crate) fn physical_boundary(&self, limit: SimTime) -> SimTime {
        if self.host.state.physical.is_none() {
            return limit;
        }
        let mut dt = limit.0 - self.host.clock().time.0;
        for (id, body) in game_physiology::bodies(&self.host.storage) {
            if !body.vitals.alive {
                continue;
            }
            if matches!(self.host.storage.activity(id), Some(Activity::Work { .. }))
                && let Some(job) = game_workshop::job(&self.host.storage, id)
            {
                dt = dt.min(
                    job.completion_after(self.invocation_work_rate(id, job.rate, job.body_rate))
                        .expect("validated frozen skill rate"),
                );
            }
        }
        // Inactive/dead records never impose a zero-length scheduling loop.
        SimTime(self.host.clock().time.0 + dt.max(1).min(limit.0 - self.host.clock().time.0))
    }

    fn body_context<'a>(
        &self,
        actor: PersonId,
        body: &'a game_physiology::Body,
        activity: &Activity,
    ) -> Result<
        (
            &'a game_physiology::Body,
            game_physiology::BodyLimits,
            game_physiology::ActivityFlags,
            bool,
        ),
        Error,
    > {
        self.host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let rules = self.body_rules(actor);
        let person = &self.host.state.people[self.person_index[&actor]];
        let exerting = matches!(
            *activity,
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
        let resting = *activity == Activity::Rest || self.health_patient_resting(actor);
        Ok((
            body,
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
        exposure: game_health::ExposureInput,
    ) -> Result<(Vitals, u16, u16), Error> {
        let body = game_physiology::body(&self.host.storage, actor).ok_or(Error::Unavailable)?;
        let activity = self
            .host
            .storage
            .activity(actor)
            .ok_or(Error::Unavailable)?;
        self.integrated_body(actor, body, activity, elapsed, exposure)
    }

    fn integrated_body(
        &self,
        actor: PersonId,
        body: &game_physiology::Body,
        current_activity: &Activity,
        elapsed: u64,
        exposure: game_health::ExposureInput,
    ) -> Result<(Vitals, u16, u16), Error> {
        let (snapshot, limits, activity, exempt) =
            self.body_context(actor, body, current_activity)?;
        let health = self.health_body_delta(actor, elapsed, exposure)?;
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
            snapshot,
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
        let Some(world) = &self.host.state.physical else {
            return Ok(limit);
        };
        let exposures = self.health_exposures()?;
        let mut next = limit;
        // Hunger changes the healing/harm slope. Clip that transition before
        // searching crossings so a recovery followed by starvation cannot hide
        // an earlier boundary at the interval endpoint.
        for (actor, body) in game_physiology::bodies(&self.host.storage) {
            let exempt = self.host.state.people[self.person_index[&actor]].hunger_exempt;
            let rules = self.body_rules(actor);
            if body.vitals.alive && !exempt && body.vitals.hunger < rules.hunger_harm {
                let remaining = rules.hunger_harm - body.vitals.hunger;
                if remaining < next.0 - self.host.clock().time.0 {
                    next = SimTime(self.host.clock().time.0 + remaining);
                }
            }
        }
        for (actor, body) in game_physiology::bodies(&self.host.storage) {
            if !body.vitals.alive {
                continue;
            }
            let elapsed = next.0 - self.host.clock().time.0;
            let before = &body.vitals;
            let rules = self.body_rules(actor);
            let incapacitation = self
                .host
                .state
                .combat
                .as_ref()
                .map(|w| w.rules.incapacitation);
            let crossed = |dt| {
                let (after, _, _) = self.integrated_vitals(
                    actor,
                    dt,
                    exposures.get(&actor).copied().unwrap_or_default(),
                )?;
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
                next = SimTime(self.host.clock().time.0 + dt);
            }
        }
        Ok(next)
    }

    pub(crate) fn evaluate_physical(
        &self,
        elapsed: u64,
        exposures: &BTreeMap<PersonId, game_health::ExposureInput>,
    ) -> Result<Vec<Outcome>, Error> {
        if self.host.state.physical.is_none() {
            return Ok(Vec::new());
        }
        let equipment = self.equipment_availability();
        let mut input: Vec<_> = self
            .physical_query
            .iter(&self.host.storage)?
            .map(|(id, body, activity, job)| {
                (PersonId(id.0), (PersonId(id.0), body, activity, job))
            })
            .collect();
        input.sort_unstable_by_key(|(id, _)| *id);
        let outputs = self
            .engine
            .executor()
            .map_ordered(&input, |(id, body, activity, job)| {
                let id = *id;
                let (vitals, fatigue_remainder, health_remainder) = self.integrated_body(
                    id,
                    body,
                    activity,
                    elapsed,
                    exposures.get(&id).copied().unwrap_or_default(),
                )?;
                let (remaining, work_remainder) = if let Some(job) = job {
                    let progress = job.evaluate(
                        elapsed,
                        self.invocation_work_rate(id, job.rate, job.body_rate),
                        matches!(**activity, Activity::Work { .. }),
                        equipment.get(&id).copied().unwrap_or(false),
                    )?;
                    (Some(progress.remaining), Some(progress.remainder))
                } else {
                    (None, None)
                };
                Ok(Outcome {
                    id,
                    opening_body: (*body).clone(),
                    opening_activity: (*activity).clone(),
                    opening_job: job.cloned(),
                    opening_location: game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId((id).0),
                    )
                    .ok_or(Error::Unavailable)?
                    .clone(),
                    opening_development: game_physiology::traits::development(
                        &self.host.storage,
                        id,
                    )
                    .copied(),
                    opening_phenotype: self.genetic_phenotype(id),
                    vitals,
                    fatigue_remainder,
                    health_remainder,
                    remaining,
                    work_remainder,
                })
            })?;
        Ok(outputs.into_iter().map(|(_, outcome)| outcome).collect())
    }

    pub(crate) fn validate_physical_output(&self, outcomes: &[Outcome]) -> Result<(), Error> {
        if self.host.state.physical.is_some() && outcomes.len() != self.host.state.people.len()
            || outcomes.iter().any(|outcome| {
                game_physiology::body(&self.host.storage, outcome.id) != Some(&outcome.opening_body)
                    || self.host.storage.activity(outcome.id) != Some(&outcome.opening_activity)
                    || game_workshop::job(&self.host.storage, outcome.id)
                        != outcome.opening_job.as_ref()
                    || game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId((outcome.id).0),
                    ) != Some(&outcome.opening_location)
                    || game_physiology::traits::development(&self.host.storage, outcome.id).copied()
                        != outcome.opening_development
                    || self.genetic_phenotype(outcome.id) != outcome.opening_phenotype
            })
        {
            return Err(Error::InvalidSave("stale physical component batch".into()));
        }
        Ok(())
    }

    pub(crate) fn workshop_practice_deltas(
        &self,
        elapsed: u64,
        equipment: &BTreeMap<PersonId, bool>,
    ) -> Vec<game_skills::PracticeDelta> {
        let Some(_) = &self.host.state.physical else {
            return Vec::new();
        };
        equipment
            .iter()
            .filter_map(|(actor, equipped)| {
                let body = &game_physiology::body(&self.host.storage, *actor).expect("body exists");
                game_workshop::job(&self.host.storage, *actor)?.practice(
                    *actor,
                    elapsed,
                    body.vitals.alive
                        && body.vitals.health > 0
                        && matches!(
                            *self.host.storage.activity(*actor).expect("activity exists"),
                            Activity::Work { .. }
                        )
                        && matches!(
                            *game_spatial::location(
                                &self.host.storage,
                                game_ecs::StableId(
                                    (self.host.state.people[self.person_index[actor]].id).0
                                )
                            )
                            .expect("location exists"),
                            Location::AtArea(_)
                        ),
                    *equipped,
                )
            })
            .collect()
    }

    pub(crate) fn complete_physical(&mut self, actor: PersonId) -> Result<(), Error> {
        let activity = self
            .host
            .storage
            .activity(actor)
            .expect("body exists")
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
            return self.complete_gathering(actor);
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
                if game_physiology::body(&self.host.storage, child)
                    .expect("body exists")
                    .vitals
                    .alive
                    && self.transport_same_place(child, actor)
                {
                    let life = game_lifecycle::life_mut(&mut self.host.storage, child).unwrap();
                    if life
                        .preference
                        .is_some_and(|(p, _)| p == ChildhoodInterest::Company)
                    {
                        life.preference = None;
                    }
                    let relationship = game_characters::decisions::mind_mut::<game_api::Command>(
                        &mut self.host.storage,
                        child,
                    )
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
                game_lifecycle::life_mut(&mut self.host.storage, actor)
                    .unwrap()
                    .preference = Some((preference, SimTime(self.host.clock().time.0 + 300_000)));
                self.emit(EventKind::Expressed { actor, preference });
                success = true;
            }
            Activity::Play { interest, .. } => {
                let experience = game_characters::decisions::mind_mut::<game_api::Command>(
                    &mut self.host.storage,
                    actor,
                )
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
                let quantity = self.host.state.physical.as_ref().unwrap().rules.meal_size;
                if self.can_use(
                    actor,
                    game_cargo::inventory(&self.host.storage, source).expect("inventory exists"),
                ) && game_cargo::inventory(&self.host.storage, source)
                    .expect("inventory exists")
                    .quantity
                    >= quantity
                {
                    let stock = game_cargo::inventory_mut(&mut self.host.storage, source).unwrap();
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
                    job.closing_request(self.host.clock().time, self.cargo_closing_facts(&intent))?;
                let resolved = self
                    .prepare_cargo_requests(vec![request], None)?
                    .commit(&mut self.host.storage)?;
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
                let sender = &self.host.state.people[self.person_index[&actor]];
                let receiver = &self.host.state.people[self.person_index[&recipient]];
                let receiver_body =
                    &game_physiology::body(&self.host.storage, recipient).expect("body exists");
                if *game_spatial::location(&self.host.storage, game_ecs::StableId((sender.id).0))
                    .expect("location exists")
                    == *game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId((receiver.id).0),
                    )
                    .expect("location exists")
                    && matches!(
                        *game_spatial::location(
                            &self.host.storage,
                            game_ecs::StableId((sender.id).0)
                        )
                        .expect("location exists"),
                        Location::AtArea(_)
                    )
                    && receiver_body.vitals.alive
                    && receiver_body.vitals.health > 0
                    && matches!(
                        *self
                            .host
                            .storage
                            .activity(recipient)
                            .expect("activity exists"),
                        Activity::Idle | Activity::Rest
                    )
                {
                    success = self.receive_report(actor, recipient, fact);
                }
            }
            Activity::Care { child, source, .. } => {
                self.care_index.remove(&child);
                let recipient =
                    &game_physiology::body(&self.host.storage, child).expect("body exists");
                let quantity = self
                    .care_dose(
                        game_cargo::inventory(&self.host.storage, source)
                            .expect("inventory exists")
                            .good,
                    )
                    .ok_or(Error::Unavailable)?;
                if recipient.vitals.alive
                    && self.transport_same_place(child, actor)
                    && self.can_use(
                        actor,
                        game_cargo::inventory(&self.host.storage, source)
                            .expect("inventory exists"),
                    )
                    && game_cargo::inventory(&self.host.storage, source)
                        .expect("inventory exists")
                        .quantity
                        >= quantity
                {
                    if game_cargo::inventory(&self.host.storage, source)
                        .expect("inventory exists")
                        .good
                        == Good::Water
                        && self.health_water_given(child, source, quantity).is_err()
                    {
                        self.emit(EventKind::Interrupted { actor });
                        *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
                        return Ok(());
                    }
                    let food = game_cargo::inventory_mut(&mut self.host.storage, source).unwrap();
                    food.quantity.0 -= quantity.0;
                    food.last_granted = Some(actor);
                    game_lifecycle::life_mut(&mut self.host.storage, child)
                        .unwrap()
                        .caregivers
                        .insert(actor);
                    let relationship = game_characters::decisions::mind_mut::<game_api::Command>(
                        &mut self.host.storage,
                        child,
                    )
                    .unwrap()
                    .relationships
                    .entry(actor)
                    .or_insert(decisions::Relationship {
                        trust: 0,
                        affection: 0,
                    });
                    relationship.trust = (relationship.trust + 50).min(1_000);
                    relationship.affection = (relationship.affection + 50).min(1_000);
                    let life = game_lifecycle::life_mut(&mut self.host.storage, child).unwrap();
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
        *self.host.storage.activity_mut(actor).unwrap() = Activity::Idle;
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
            let previous = &game_physiology::body(&self.host.storage, actor).expect("body exists");
            let previously_alive = previous.vitals.alive;
            let rules = self.body_rules(actor);
            let previously_injured = previous.vitals.health < rules.max_health;
            let mut vitals = out.vitals;
            let ate = self.host.events()
                .iter()
                .rev()
                .take_while(|e| e.at == self.host.clock().time)
                .any(|e| {
                    matches!(e.event, EventKind::Ate { actor: id, .. } if id == actor)
                        || matches!(e.event, EventKind::Cared { child: id, source, .. } if id == actor && game_cargo::inventory(&self.host.storage, source).expect("inventory exists").good == Good::Food)
                });
            if ate {
                vitals.hunger = vitals.hunger.saturating_sub(rules.meal_relief);
            }
            let job_done = out.remaining == Some(0)
                && matches!(
                    *self.host.storage.activity(actor).expect("activity exists"),
                    Activity::Work { .. }
                );
            let interrupted = (self.social_activity_is_practical(actor)
                && (vitals.fatigue >= rules.fatigue_limit
                    || self.social_selected_tool(actor).is_some()
                        && !equipment.get(&actor).copied().unwrap_or(false)))
                || (self.health_work_exerting(actor) && vitals.fatigue >= rules.fatigue_limit)
                || (self.health_selected_tool(actor).is_some()
                    && !equipment.get(&actor).copied().unwrap_or(false))
                || (matches!(
                    *self.host.storage.activity(actor).expect("activity exists"),
                    Activity::Work { .. }
                ) && game_workshop::interrupted(
                    vitals.fatigue < rules.fatigue_limit,
                    equipment.get(&actor).copied().unwrap_or(false),
                ))
                || vitals.fatigue >= rules.fatigue_limit
                    && matches!(
                        *self.host.storage.activity(actor).expect("activity exists"),
                        Activity::Gather { .. }
                            | Activity::Load { .. }
                            | Activity::Take { .. }
                            | Activity::Construct { .. }
                            | Activity::Play { .. }
                            | Activity::Cultivate { .. }
                    );
            let a = game_physiology::body_mut(&mut self.host.storage, actor).unwrap();
            a.vitals = vitals.clone();
            a.fatigue_remainder = out.fatigue_remainder;
            a.health_remainder = out.health_remainder;
            if let (Some(job), Some(remaining)) = (
                game_workshop::job_mut(&mut self.host.storage, actor),
                out.remaining,
            ) {
                job.apply_progress(game_workshop::Progress {
                    remaining,
                    remainder: out.work_remainder.unwrap(),
                });
            }
            if job_done && equipment.get(&actor).copied().unwrap_or(false) {
                let site_id = game_workshop::job(&self.host.storage, actor).unwrap().site;
                self.host.state.physical.as_ref().unwrap();
                let site = &game_workshop::site(&self.host.storage, site_id).expect("site exists");
                let production = game_workshop::job(&self.host.storage, actor)
                    .as_ref()
                    .unwrap()
                    .prepare_completion(
                        self.host.state.workshops.resolve_site(site)?,
                        game_cargo::inventory(&self.host.storage, site.output)
                            .expect("inventory exists"),
                        game_workshop::CompletionFacts {
                            actor,
                            active: true,
                            alive_at_start: previously_alive,
                            equipped: equipment.get(&actor).copied().unwrap_or(false),
                        },
                    )?;
                if let Some(production) = production {
                    let receipt = EventKind::Produced {
                        actor: production.actor,
                        site: production.site,
                        output: production.output,
                        quantity: production.quantity,
                    };
                    production.commit_stocks(&self.host.state.workshops, &mut self.host.storage)?;
                    *self
                        .host
                        .storage
                        .activity_mut(actor)
                        .expect("activity exists") = Activity::Idle;
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
                game_workshop::remove_job(&mut self.host.storage, actor)
                    .map(|_| ())
                    .expect("actor exists");
                game_transport::replace_route(&mut self.host.storage, actor, Vec::new())
                    .expect("actor route");
                for due in self.host.state.due.values_mut() {
                    due.retain(|d| !matches!(d, Due::Arrive(id) | Due::Age(id) if *id == actor));
                }
                self.host.state.due.retain(|_, due| !due.is_empty());
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
        let Some(_) = &self.host.state.physical else {
            return Vec::new();
        };
        outcomes
            .iter()
            .filter_map(|out| {
                if !matches!(
                    *self.host.storage.activity(out.id).expect("activity exists"),
                    Activity::Work { .. }
                ) {
                    return None;
                }
                let job = game_workshop::job(&self.host.storage, out.id)?;
                let site = &game_workshop::site(&self.host.storage, job.site).expect("site exists");
                job.output_forecast(
                    game_workshop::Progress {
                        remaining: out.remaining?,
                        remainder: out.work_remainder?,
                    },
                    self.host.state.workshops.resolve_site(site).ok()?,
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
            InventoryLocation::Area(a)
                if !self.host.state.areas.iter().any(|area| area.id == a) =>
            {
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
                .map(|id| {
                    u128::from(
                        game_cargo::inventory(&self.host.storage, *(id))
                            .expect("inventory exists")
                            .quantity
                            .0,
                    )
                })
                .sum();
            if self.host.state.physical.as_ref().is_some_and(|_| {
                quantity.0 > 0
                    && (held + u128::from(quantity.0)
                        > u128::from(self.body_rules(actor).carrying_capacity.0)
                        || !self.transport_transfer_capacity(actor, quantity.0))
            }) {
                return Err(Error::InvalidFixture("carrying capacity exceeded".into()));
            }
        }
        let id = InventoryId(self.host.allocate_id()?.0);
        self.inventory_index
            .entry(location.clone())
            .or_default()
            .push(id);
        game_cargo::insert_inventory(
            &mut self.host.storage,
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
        )?;
        Ok(id)
    }

    pub fn debug_spoil_at(&mut self, inventory: InventoryId, at: SimTime) -> Result<(), Error> {
        let stock = game_cargo::inventory(&self.host.storage, inventory)
            .ok_or_else(|| Error::InvalidFixture("unknown stock".into()))?;
        if stock.good != Good::Food || at <= self.host.clock().time {
            return Err(Error::InvalidFixture("invalid spoilage".into()));
        }
        for due in self.host.state.due.values_mut() {
            due.retain(|d| !matches!(d, Due::Spoil(id) if *id == inventory));
        }
        self.host.state.due.retain(|_, due| !due.is_empty());
        game_cargo::inventory_mut(&mut self.host.storage, inventory)
            .unwrap()
            .spoils_at = Some(at);
        self.host
            .state
            .due
            .entry(at)
            .or_default()
            .push(Due::Spoil(inventory));
        Ok(())
    }

    pub fn debug_injure(&mut self, actor: PersonId, damage: u64) -> Result<(), Error> {
        if !self.person_index.get(&actor).is_some_and(|i| {
            matches!(
                *game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId((self.host.state.people[*i].id).0)
                )
                .expect("location exists"),
                Location::AtArea(_)
            )
        }) {
            return Err(Error::InvalidFixture(
                "fixture injuries require a stationary actor".into(),
            ));
        }
        self.ensure_event_capacity(1)?;
        self.host
            .state
            .physical
            .as_mut()
            .ok_or(Error::Unavailable)?;
        let a =
            game_physiology::body_mut(&mut self.host.storage, actor).ok_or(Error::Unavailable)?;
        a.vitals.health = a.vitals.health.saturating_sub(damage);
        if a.vitals.health == 0 && !self.host.state.people[self.person_index[&actor]].hunger_exempt
        {
            a.vitals.alive = false;
            game_workshop::remove_job(&mut self.host.storage, actor)
                .map(|_| ())
                .expect("actor exists");
            self.emit(EventKind::Died { actor });
            for people in self.dependent_index.values_mut() {
                people.remove(&actor);
            }
            for due in self.host.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Age(id) if *id == actor));
            }
            self.host.state.due.retain(|_, due| !due.is_empty());
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
        .state
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
