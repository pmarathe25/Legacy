//! Bounded action templates. Decisions read personal knowledge; physical execution
//! can fail and teach the actor without granting an unseen fact in advance.
use crate::*;
use rayon::prelude::*;

pub use game_characters::decisions::{
    Aspiration, Attachment, ChildSignal, Explanation, Goal, Personality, Relationship,
    SiteKnowledge,
};

pub type Mind = game_characters::decisions::Mind<Command>;
pub type World = game_characters::decisions::World<Command>;
pub(crate) use game_characters::decisions::Control;
pub(crate) use game_characters::decisions::known_path;
use game_characters::decisions::{ActionShape, Candidate, Decision, PersonalFacts, Purpose};
use game_characters::decisions::{Activity as PersonalActivity, NeedLimits, Place};

/// Stored plan geometry is derived from the executable command, never authored
/// separately by a saved belief or supplied by a client.
pub(crate) fn action_shape(command: &Command) -> ActionShape {
    match *command {
        Command::Eat { source } => ActionShape::Eat(source),
        Command::Move { destination } => ActionShape::Move(destination),
        Command::Work { site } => ActionShape::Work(site),
        Command::Care { child, .. } => ActionShape::Care(child),
        Command::Comfort { child } => ActionShape::Comfort(child),
        Command::Rest => ActionShape::Rest,
        _ => ActionShape::Other,
    }
}

#[derive(Clone)]
struct Intention {
    actor: PersonId,
    decision: Decision<Command>,
}

impl Simulation {
    pub(crate) fn initialize_decision_people(
        &mut self,
        minds: BTreeMap<PersonId, Mind>,
    ) -> Result<(), Error> {
        if self.state.decisions.is_some()
            || minds.len() != self.state.people.len()
            || self.state.people.iter().any(|p| !minds.contains_key(&p.id))
        {
            return Err(Error::InvalidConfiguration("invalid initial minds".into()));
        }
        let work_concept = ConceptId(allocate(&mut self.state)?);
        self.state.decisions = Some(World {
            minds,
            concepts: BTreeMap::from([(work_concept, Good::Food)]),
            work_concept,
            last_learning_event: self.state.next_event - 1,
            switching_margin: 50,
        });
        Ok(())
    }
    pub fn autonomy_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::survival_fixture(seed, execution)?;
        sim.state.scenario = scenarios().remove(2);
        let minds = sim
            .state
            .people
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let restful = i % 3 == 1;
                (
                    p.id,
                    Mind {
                        interests: BTreeMap::new(),
                        // These test villagers have an authored map of the three
                        // fixture areas. Newborns learn routes by observation.
                        routes: fixture_routes(&sim.state.areas),
                        last_failure: None,
                        child_signals: BTreeMap::new(),
                        personality: if restful {
                            Personality::restful()
                        } else {
                            Personality::industrious()
                        },
                        dream: if restful {
                            Aspiration::Comfort
                        } else {
                            Aspiration::Independence
                        },
                        knowledge: BTreeMap::new(),
                        sites: BTreeMap::new(),
                        relationships: BTreeMap::new(),
                        values: BTreeMap::new(),
                        goal: Goal::Rest,
                        plan: None,
                        explanation: Explanation {
                            goal: Goal::Rest,
                            need: 0,
                            benefit: 0,
                            value: 0,
                            total: 0,
                            blocker: None,
                        },
                        retry_after: SimTime(0),
                    },
                )
            })
            .collect();
        sim.initialize_decision_people(minds)?;
        sim.observe_local_facts();
        Ok(sim)
    }

    fn observe_local_facts(&mut self) {
        if self.state.decisions.is_none() {
            return;
        }
        let locations =
            transport::resolved_locations_for(&self.state).expect("valid physical observations");
        let mut cohorts: BTreeMap<game_transport::PositionKey, Vec<PersonId>> = BTreeMap::new();
        for person in &self.state.people {
            if self.state.development.as_ref().is_some_and(|w| {
                matches!(
                    w.lives[&person.id].stage,
                    LifeStage::Infant | LifeStage::Child
                )
            }) && self.state.physical.as_ref().unwrap().actors[&person.id]
                .vitals
                .alive
                && let Some(position) = self.transport_position(person.id)
            {
                cohorts.entry(position.key()).or_default().push(person.id);
            }
        }
        let signal_rules: BTreeMap<_, _> = cohorts
            .values()
            .flatten()
            .map(|child| (*child, self.body_rules(*child)))
            .collect();
        let hydration_signals: BTreeMap<_, _> = cohorts
            .values()
            .flatten()
            .map(|child| (*child, self.observed_hydration_urgency(*child)))
            .collect();
        for p in &self.state.people {
            let location = &locations[&p.id];
            let area = match *location {
                Location::AtArea(area) => area,
                Location::StoppedOnRoad { from, .. } | Location::InTransit { from, .. } => from,
                Location::Carried { .. } => continue,
            };
            let stationary = matches!(
                location,
                Location::AtArea(_) | Location::StoppedOnRoad { .. }
            );
            let at_area = matches!(location, Location::AtArea(_));
            let actor = &self.state.physical.as_ref().unwrap().actors[&p.id];
            if !actor.vitals.alive || actor.vitals.health == 0 {
                continue;
            }
            let visible_stocks: Vec<_> = self
                .inventory_index
                .get(&InventoryLocation::Area(area))
                .filter(|_| at_area)
                .into_iter()
                .flatten()
                .chain(
                    self.inventory_index
                        .get(&InventoryLocation::Carried(p.id))
                        .into_iter()
                        .flatten(),
                )
                .copied()
                .collect();
            let accessible_stocks: BTreeSet<_> = visible_stocks
                .iter()
                .filter(|id| self.has_stock_access(p.id, &self.state.inventories[id]))
                .copied()
                .collect();
            let visible_children = cohorts.get(
                &match *location {
                    Location::AtArea(area) => game_transport::Position::Area(area),
                    Location::StoppedOnRoad {
                        from,
                        to,
                        progress,
                        duration,
                    } => game_transport::Position::Road(game_transport::RoadPosition {
                        from,
                        to,
                        progress,
                        duration,
                    }),
                    Location::InTransit {
                        from,
                        to,
                        departed,
                        road_start,
                        arrives,
                    } => game_transport::Leg {
                        from,
                        to,
                        departed,
                        road_start,
                        arrives,
                    }
                    .position(self.state.time)
                    .expect("valid current leg"),
                    Location::Carried { .. } => continue,
                }
                .key(),
            );
            let visible_children: Vec<_> = visible_children
                .into_iter()
                .flatten()
                .filter(|child| {
                    stationary
                        || self
                            .state
                            .transport
                            .as_ref()
                            .and_then(|w| w.groups.get(child))
                            .is_some_and(|g| g.bearers.contains(&p.id))
                })
                .take(64)
                .filter(|child| self.transport_same_place(p.id, **child))
                .copied()
                .collect();
            let mind = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&p.id)
                .unwrap();
            if at_area {
                let actual_routes = self
                    .state
                    .areas
                    .iter()
                    .find(|a| a.id == area)
                    .unwrap()
                    .links
                    .iter()
                    .map(|l| (l.destination, l.duration))
                    .collect();
                mind.routes.insert(area, actual_routes);
            }
            for id in &visible_stocks {
                let inventory = &self.state.inventories[id];
                mind.observe_stock(
                    self.state.time,
                    area,
                    self.state.physical.as_ref().unwrap().rules.meal_size,
                    StockKnowledge {
                        inventory: *id,
                        area,
                        good: inventory.good,
                        quantity: inventory.quantity,
                        accessible: accessible_stocks.contains(id),
                        observed_at: self.state.time,
                        learned_at: self.state.time,
                        source: KnowledgeSource::Observed(p.id),
                    },
                );
            }
            mind.child_signals
                .retain(|_, signal| signal.location != *location);
            for child in &visible_children {
                let vitals = &self.state.physical.as_ref().unwrap().actors[child].vitals;
                let rules = &signal_rules[child];
                let mut urgency = if vitals.hunger >= rules.hunger_harm {
                    20_000
                } else if vitals.hunger >= rules.meal_relief {
                    3_000
                } else {
                    0
                };
                if self.state.development.as_ref().unwrap().lives[child]
                    .preference
                    .is_some_and(|(p, until)| {
                        p == ChildhoodInterest::Food && until >= self.state.time
                    })
                {
                    urgency = urgency.max(3_000);
                }
                mind.child_signals.insert(
                    *child,
                    ChildSignal {
                        preference: self.state.development.as_ref().unwrap().lives[child]
                            .preference
                            .filter(|(_, until)| *until >= self.state.time)
                            .map(|(p, _)| p),
                        location: locations[child].clone(),
                        urgency,
                        hydration_urgency: hydration_signals[child],
                        observed_at: self.state.time,
                    },
                );
            }
            for site in self
                .state
                .physical
                .as_ref()
                .unwrap()
                .sites
                .values()
                .filter(|s| at_area && s.area == area)
            {
                mind.sites.insert(
                    site.id,
                    SiteKnowledge {
                        site: site.id,
                        area,
                        input: site.input,
                        tool: site.tool,
                        input_quantity: site.input_quantity,
                        output: site.output,
                        access: site.access.contains(&p.id),
                    },
                );
            }
        }
    }

    pub(crate) fn review_decisions(&mut self) -> Result<(), Error> {
        let Some(_) = self.state.decisions else {
            return Ok(());
        };
        let phase_started = Instant::now();
        self.observe_local_facts();
        self.profile
            .record_phase("review.observe_local_facts", phase_started);
        let phase_started = Instant::now();
        self.review_transport_choices()?;
        self.profile
            .record_phase("review.review_transport_choices", phase_started);
        let phase_started = Instant::now();
        self.observe_projects();
        self.profile
            .record_phase("review.observe_projects", phase_started);
        let phase_started = Instant::now();
        self.observe_property();
        self.profile
            .record_phase("review.observe_property", phase_started);
        let phase_started = Instant::now();
        self.review_property_sharing();
        self.profile
            .record_phase("review.review_property_sharing", phase_started);
        let phase_started = Instant::now();
        self.review_lineage()?;
        self.profile
            .record_phase("review.review_lineage", phase_started);
        let phase_started = Instant::now();
        self.review_agreements()?;
        self.profile
            .record_phase("review.review_agreements", phase_started);
        let phase_started = Instant::now();
        self.review_institutions()?;
        self.profile
            .record_phase("review.review_institutions", phase_started);
        let phase_started = Instant::now();
        self.review_realm()?;
        self.profile
            .record_phase("review.review_realm", phase_started);
        let phase_started = Instant::now();
        self.review_health()?;
        self.profile
            .record_phase("review.review_health", phase_started);
        let phase_started = Instant::now();
        self.review_enforcement()?;
        self.profile
            .record_phase("review.review_enforcement", phase_started);
        let phase_started = Instant::now();
        self.review_armies()?;
        self.profile
            .record_phase("review.review_armies", phase_started);
        let phase_started = Instant::now();
        self.review_missions()?;
        self.profile
            .record_phase("review.review_missions", phase_started);
        let phase_started = Instant::now();
        self.review_projects()?;
        self.profile
            .record_phase("review.review_projects", phase_started);
        let phase_started = Instant::now();
        self.review_conception()?;
        self.profile
            .record_phase("review.review_conception", phase_started);
        let phase_started = Instant::now();
        let world = self.state.decisions.as_ref().unwrap();
        let evaluate =
            |(id, mind): (&PersonId, &Mind)| self.decide(*id, mind, world.switching_margin);
        let mut intentions: Vec<_> = if self.execution.workers == 1 {
            world.minds.iter().map(evaluate).collect()
        } else {
            self.engine
                .executor()
                .install(|| world.minds.par_iter().map(evaluate).collect())
        };
        self.profile.record_phase("review.rank", phase_started);
        let phase_started = Instant::now();
        // Equal claims use the same persistent rotating cursor as physical transfers.
        intentions.sort_by_key(|i| {
            let (resource, cursor) = match i.decision.plan.as_ref().map(|plan| &plan.action) {
                Some(Command::Eat { source }) | Some(Command::Care { source, .. }) => {
                    (source.0, self.state.inventories[source].last_granted)
                }
                Some(Command::Work { site }) => (
                    site.0,
                    self.state.physical.as_ref().unwrap().sites[site].last_granted,
                ),
                _ => (0, None),
            };
            (
                resource,
                cursor.is_some_and(|last| i.actor <= last),
                i.actor,
            )
        });
        for i in intentions {
            let Some(plan) = i.decision.plan else {
                continue;
            };
            let command = plan.action.clone();
            let phase_started = Instant::now();
            let kind = match &command {
                Command::Rest => "apply.Rest",
                Command::Work { .. } => "apply.Work",
                Command::Eat { .. } => "apply.Eat",
                Command::Load { .. } => "apply.Load",
                Command::Move { .. } => "apply.Move",
                Command::Care { .. } => "apply.Care",
                Command::Play { .. } => "apply.Play",
                _ => "apply.Other",
            };
            *self.profile.command_attempts.entry(kind).or_default() += 1;
            let accepted = if let Command::Move { destination } = command {
                self.start_movement(i.actor, destination)
            } else {
                self.debug_actor_command(i.actor, command.clone())
            };
            self.profile.record_phase(kind, phase_started);
            let mind = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&i.actor)
                .unwrap();
            mind.record_attempt(
                self.state.time,
                plan,
                i.decision.explanation,
                accepted.is_ok(),
            )?;
        }
        self.profile.record_phase("review.apply", phase_started);
        Ok(())
    }

    fn decide(&self, id: PersonId, mind: &Mind, margin: i64) -> Intention {
        let p = &self.state.people[self.person_index[&id]];
        let actor = &self.state.physical.as_ref().unwrap().actors[&id];
        let rules = self.body_rules(id);
        let place = match self.resolved_location(id) {
            Location::AtArea(area) => Place::Area(area),
            Location::StoppedOnRoad { .. } => Place::Stopped,
            Location::InTransit { .. } => Place::Moving,
            Location::Carried { .. } => unreachable!("resolved physical location"),
        };
        let observed_contacts = mind
            .child_signals
            .iter()
            .filter(|(_, signal)| {
                signal.observed_at == self.state.time
                    && match place {
                        Place::Area(area) => signal.location == Location::AtArea(area),
                        _ => transport::location_position(&signal.location, signal.observed_at)
                            .is_ok_and(|point| {
                                self.transport_position(id)
                                    .is_some_and(|own| point.same_point(own))
                            }),
                    }
            })
            .map(|(child, _)| *child)
            .collect();
        let facts = PersonalFacts {
            actor: id,
            now: self.state.time,
            vitals: actor.vitals.clone(),
            limits: NeedLimits {
                meal_relief: rules.meal_relief,
                hunger_harm: rules.hunger_harm,
                meal_size: rules.meal_size,
                eating_time: rules.eating_time,
                fatigue_limit: rules.fatigue_limit,
                max_health: rules.max_health,
                drink_quantity: self
                    .state
                    .health
                    .as_ref()
                    .map_or(Quantity(u64::MAX), |world| world.rules.drink_quantity),
            },
            activity: match actor.activity {
                Activity::Idle => PersonalActivity::Idle,
                Activity::Rest => PersonalActivity::Rest,
                Activity::Work { .. } => PersonalActivity::Working,
                _ => PersonalActivity::Committed,
            },
            place,
            control: if id == self.state.ruler {
                Control::Manual
            } else if self.in_combat(id)
                || self.lineage_handles(id)
                || self.mission_actor_busy(id)
                || self.health_actor_busy(id)
                || self
                    .state
                    .transport
                    .as_ref()
                    .is_some_and(|world| world.busy(id))
            {
                Control::Delegated
            } else {
                Control::Autonomous
            },
            in_dialogue: self.army_actor_talking(id) || self.realm_actor_busy(id),
            independent: self.independent(id),
            child: self
                .state
                .development
                .as_ref()
                .is_some_and(|world| world.lives[&id].stage == LifeStage::Child),
            hunger_exempt: p.hunger_exempt,
            work_units: p.work_units,
            own_cargo: self
                .inventory_index
                .get(&InventoryLocation::Carried(id))
                .into_iter()
                .flatten()
                .copied()
                .collect(),
            observed_contacts,
            can_resume: place == Place::Stopped && self.transport_can_depart(id),
        };
        let mut candidates = Vec::new();
        let mut add = |action, purpose| {
            let shape = action_shape(&action);
            candidates.push(Candidate {
                action,
                purpose,
                shape,
            });
        };
        if let Some(action) = self.force_candidate(id) {
            add(action, Purpose::Force);
        }
        if matches!(place, Place::Area(_)) {
            if let Some(action) = self.health_candidate(id) {
                add(action, Purpose::Health);
            }
            if let Some(action) = self.ecology_candidate(id) {
                add(action, Purpose::Forage);
            }
            if let Some(action) = self.project_candidate(id) {
                add(action, Purpose::Project);
            }
            if let Some(action) = self.property_candidate(id) {
                add(action, Purpose::Property);
            }
        }
        for fact in mind
            .knowledge
            .values()
            .filter(|fact| fact.good == Good::Food)
        {
            let action = match place {
                Place::Area(area) if fact.area != area => Command::Move {
                    destination: fact.area,
                },
                _ => Command::Eat {
                    source: fact.inventory,
                },
            };
            add(action, Purpose::Food(fact.inventory));
        }
        for child in &facts.observed_contacts {
            add(Command::Comfort { child: *child }, Purpose::Comfort(*child));
            for fact in mind
                .knowledge
                .values()
                .filter(|fact| matches!(fact.good, Good::Food | Good::Water))
            {
                add(
                    Command::Care {
                        child: *child,
                        source: fact.inventory,
                    },
                    Purpose::Care {
                        child: *child,
                        source: fact.inventory,
                    },
                );
            }
            if matches!(place, Place::Area(area) if !mind.knowledge.values().any(|fact| fact.good == Good::Water && fact.accessible && fact.area == area && fact.quantity >= facts.limits.drink_quantity))
                && let Some(action) = self
                    .ecology_water_candidate(id, true)
                    .or_else(|| self.health_water_procurement(id))
            {
                add(action, Purpose::CareSupply(*child));
            }
        }
        add(Command::Rest, Purpose::Rest);
        if let Place::Area(_) = place {
            if let Some(action) = self.autonomous_cultivation(id) {
                add(action, Purpose::Cultivate);
            }
            for interest in [
                ChildhoodInterest::Food,
                ChildhoodInterest::Company,
                ChildhoodInterest::Movement,
                ChildhoodInterest::Craft,
            ] {
                add(Command::Play { interest }, Purpose::Play(interest));
            }
            for site in mind.sites.values() {
                add(Command::Work { site: site.site }, Purpose::Work(site.site));
            }
        } else {
            add(
                Command::Transport {
                    action: TransportAction::Stop,
                },
                Purpose::Stop,
            );
            add(
                Command::Transport {
                    action: TransportAction::Resume,
                },
                Purpose::Resume,
            );
        }
        Intention {
            actor: id,
            decision: mind.decide(
                &facts,
                &candidates,
                self.state.decisions.as_ref().unwrap().work_concept,
                margin,
            ),
        }
    }
    pub(crate) fn learn_from_work(&mut self) {
        let Some(world) = &mut self.state.decisions else {
            return;
        };
        for event in self
            .state
            .events
            .iter()
            .skip(world.last_learning_event as usize)
        {
            if let EventKind::Produced { actor, .. } | EventKind::CropHarvested { actor, .. } =
                event.kind
            {
                world.learn_production(actor);
            }
        }
        world.last_learning_event = self.state.next_event - 1;
    }

    pub fn debug_set_personality(
        &mut self,
        id: PersonId,
        personality: Personality,
        dream: Aspiration,
    ) -> Result<(), Error> {
        if !personality.valid() {
            return Err(Error::InvalidFixture("traits must be 0..=1000".into()));
        }
        let mind = self
            .state
            .decisions
            .as_mut()
            .and_then(|w| w.minds.get_mut(&id))
            .ok_or(Error::Unavailable)?;
        mind.personality = personality;
        mind.dream = dream;
        Ok(())
    }

    /// Explicit setup of an authored belief for false/stale-information scenarios.
    pub fn debug_set_knowledge(&mut self, id: PersonId, fact: StockKnowledge) -> Result<(), Error> {
        if !self.state.inventories.contains_key(&fact.inventory)
            || fact.observed_at > self.state.time
            || fact.learned_at > self.state.time
        {
            return Err(Error::InvalidFixture("invalid knowledge".into()));
        }
        let mind = self
            .state
            .decisions
            .as_mut()
            .and_then(|w| w.minds.get_mut(&id))
            .ok_or(Error::Unavailable)?;
        mind.knowledge.insert(fact.inventory, fact);
        Ok(())
    }

    pub(crate) fn receive_report(
        &mut self,
        sender: PersonId,
        recipient: PersonId,
        fact: StockKnowledge,
    ) -> bool {
        let Some(world) = &mut self.state.decisions else {
            return false;
        };
        let Some(receiver) = world.minds.get_mut(&recipient) else {
            return false;
        };
        let inventory = fact.inventory;
        if !receiver.receive_report(sender, self.state.time, fact) {
            return false;
        }
        self.emit(EventKind::Reported {
            sender,
            recipient,
            inventory,
        });
        true
    }
}

pub(crate) fn fixture_routes(areas: &[Area]) -> BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>> {
    areas
        .iter()
        .map(|a| {
            (
                a.id,
                a.links
                    .iter()
                    .map(|l| (l.destination, l.duration))
                    .collect(),
            )
        })
        .collect()
}
