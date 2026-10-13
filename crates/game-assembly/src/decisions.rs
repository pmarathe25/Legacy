//! Bounded action templates. Decisions read personal knowledge; physical execution
//! can fail and teach the actor without granting an unseen fact in advance.
use crate::*;

pub use game_characters::decisions::{
    Aspiration, Attachment, ChildSignal, Explanation, Goal, Personality, Relationship,
    SiteKnowledge,
};

pub type Mind = game_characters::decisions::Mind<Command>;
pub use game_characters::decisions::Runtime;
pub(crate) use game_characters::decisions::known_path;
use game_characters::decisions::{ActionShape, PersonalFacts};
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
        Command::Transport {
            action: TransportAction::Stop,
        } => ActionShape::Stop,
        Command::Transport {
            action: TransportAction::Resume,
        } => ActionShape::Resume,
        _ => ActionShape::Other,
    }
}

use game_characters::orchestration::{
    ActionEvidence, ActionKind, ActionRequest, DecisionPort, Opportunities, ProjectOpportunity,
};

struct PersonalPort<'a> {
    sim: &'a Simulation,
    resources: Option<&'a equipment::RankingResources>,
}
impl PersonalPort<'_> {
    fn evidence(&self, action: Command) -> ActionEvidence<Command> {
        let kind = match &action {
            Command::Health {
                action: HealthAction::Drink { source },
            } => ActionKind::Drink(*source),
            Command::Health {
                action: HealthAction::Boil { source, .. },
            } => ActionKind::Boil(*source),
            Command::Health {
                action:
                    HealthAction::EnterShelter { .. }
                    | HealthAction::BuildShelter { .. }
                    | HealthAction::LeaveShelter,
            } => ActionKind::Shelter,
            Command::Play { .. } => ActionKind::Practice,
            Command::Project { .. } => ActionKind::Project,
            Command::Ecology { .. }
            | Command::Property { .. }
            | Command::Carry { .. }
            | Command::Load { .. } => ActionKind::Conditional,
            _ => ActionKind::Shape(action_shape(&action)),
        };
        let source = match &action {
            Command::Care { source, .. } => Some(*source),
            _ => None,
        };
        ActionEvidence {
            risk: crate::logistics::acquisition_risk(&action),
            action,
            kind,
            source,
        }
    }
}
impl DecisionPort<Command> for PersonalPort<'_> {
    fn facts(&self, actor: PersonId) -> PersonalFacts {
        self.sim.personal_facts(actor)
    }
    fn action(&self, request: ActionRequest) -> ActionEvidence<Command> {
        self.evidence(match request {
            ActionRequest::Eat(source) => Command::Eat { source },
            ActionRequest::Move(destination) => Command::Move { destination },
            ActionRequest::Work(site) => Command::Work { site },
            ActionRequest::Care(child, source) => Command::Care { child, source },
            ActionRequest::Comfort(child) => Command::Comfort { child },
            ActionRequest::Rest => Command::Rest,
            ActionRequest::Play(interest) => Command::Play { interest },
            ActionRequest::Stop => Command::Transport {
                action: TransportAction::Stop,
            },
            ActionRequest::Resume => Command::Transport {
                action: TransportAction::Resume,
            },
        })
    }
    fn force_action(&self, actor: PersonId) -> Option<ActionEvidence<Command>> {
        self.sim
            .force_candidate(actor)
            .map(|action| self.evidence(action))
    }
    fn opportunities(&self, actor: PersonId) -> Opportunities<Command> {
        let sim = self.sim;
        Opportunities {
            health: sim
                .health_candidate(actor, self.resources)
                .map(|action| self.evidence(action)),
            provisions: sim
                .ecology_provision_opportunities(actor, self.resources)
                .into_iter()
                .map(|(action, requirement)| (self.evidence(action), requirement))
                .collect(),
            projects: sim
                .project_opportunities(actor, self.resources)
                .into_iter()
                .map(|action| {
                    let project = match &action {
                        Command::Project {
                            action: ProjectAction::Work { project },
                        } => Some(*project),
                        _ => None,
                    };
                    let known = project.and_then(|project| {
                        game_projects::knowledge(&sim.host.storage, actor)
                            .and_then(|row| row.projects.get(&project))
                    });
                    ProjectOpportunity {
                        action: self.evidence(action),
                        project: project.filter(|_| known.is_some()),
                        beneficiaries: known.map_or_else(Vec::new, |row| {
                            row.beneficiaries.iter().copied().collect()
                        }),
                        obligation: project.filter(|_| known.is_some()).and_then(|project| {
                            game_agreements::known_construction_obligation(
                                &sim.host.storage,
                                actor,
                                project,
                            )
                        }),
                    }
                })
                .collect(),
            property: sim
                .property_opportunities(actor)
                .into_iter()
                .filter_map(|action| {
                    let inventory = match &action {
                        Command::Property {
                            action: PropertyAction::RequestAccess { inventory },
                        } => *inventory,
                        Command::Property {
                            action: PropertyAction::Take { source, .. },
                        } => *source,
                        _ => return None,
                    };
                    Some((self.evidence(action), inventory))
                })
                .collect(),
            cultivation: sim
                .autonomous_cultivation(actor, self.resources)
                .map(|action| self.evidence(action)),
        }
    }
    fn delivery_action(
        &self,
        actor: PersonId,
        goal: &game_characters::decisions::CareDelivery,
    ) -> Option<ActionEvidence<Command>> {
        let location = transport::location_position(&goal.location, goal.observed_at).ok()?;
        let destination = game_logistics::KnownDestination {
            destination: goal.delivery.destination,
            location: game_transport::location_from_position(location),
            observed_at: goal.observed_at,
            accessible: true,
            room: None,
            direct_transfer: true,
        };
        let step = self.sim.plan_delivery_for_planning(
            actor,
            &goal.delivery,
            &destination,
            goal.delivery.requirement.quantity,
            &[],
            self.resources,
        );
        let action = match step {
            game_logistics::Step::Arrived { source, .. } => {
                let game_logistics::Destination::Contact(child) = goal.delivery.destination else {
                    return None;
                };
                Command::Care { child, source }
            }
            step => self.sim.delivery_command(actor, step)?,
        };
        Some(self.evidence(action))
    }
}

impl game_characters::orchestration::ObservationPort for PersonalPort<'_> {
    fn actors(&self) -> Vec<PersonId> {
        self.sim.people().map(|row| row.id).collect()
    }
    fn observer(&self, actor: PersonId) -> game_characters::orchestration::ObserverFacts {
        let body = game_physiology::body(&self.sim.host.storage, actor).expect("body exists");
        let life = self
            .sim
            .host
            .state
            .development
            .as_ref()
            .and_then(|_| game_lifecycle::life(&self.sim.host.storage, actor));
        game_characters::orchestration::ObserverFacts {
            actor,
            location: self.sim.resolved_location(actor),
            point: self
                .sim
                .transport_position(actor)
                .map(|point| match point.key() {
                    game_transport::PositionKey::Area(area) => {
                        game_characters::orchestration::ObservationPoint::Area(area)
                    }
                    game_transport::PositionKey::Road {
                        from,
                        to,
                        numerator,
                        denominator,
                    } => game_characters::orchestration::ObservationPoint::Road {
                        from,
                        to,
                        numerator,
                        denominator,
                    },
                }),
            alive: body.vitals.alive,
            health: body.vitals.health,
            dependent: life
                .is_some_and(|row| matches!(row.stage, LifeStage::Infant | LifeStage::Child)),
            meal_size: self
                .sim
                .host
                .state
                .physical
                .as_ref()
                .expect("physical rules")
                .rules
                .meal_size,
        }
    }
    fn co_present(&self, first: &Location, second: &Location, now: SimTime) -> bool {
        transport::location_position(first, now)
            .ok()
            .zip(transport::location_position(second, now).ok())
            .is_some_and(|(a, b)| a.same_point(b))
    }
    fn bears(&self, actor: PersonId, child: PersonId) -> bool {
        self.sim
            .host
            .state
            .transport
            .as_ref()
            .and_then(|_| game_transport::group(&self.sim.host.storage, child))
            .is_some_and(|row| row.bearers.contains(&actor))
    }
    fn signal_visible(&self, actor: PersonId, child: PersonId) -> bool {
        self.sim.dependent_signal_visible(actor, child)
    }
    fn dependent(&self, child: PersonId) -> game_characters::orchestration::DependentFacts {
        let body = game_physiology::body(&self.sim.host.storage, child).expect("body exists");
        let rules = self.sim.body_rules(child);
        game_characters::orchestration::DependentFacts {
            hunger: body.vitals.hunger,
            meal_relief: rules.meal_relief,
            hunger_harm: rules.hunger_harm,
            preference: game_lifecycle::life(&self.sim.host.storage, child)
                .expect("dependent lifecycle exists")
                .preference,
            hydration_urgency: self.sim.observed_hydration_urgency(child),
        }
    }
    fn stocks(
        &self,
        actor: PersonId,
        area: AreaId,
        at_area: bool,
        now: SimTime,
    ) -> Vec<StockKnowledge> {
        let facts = |inventory: &InventoryId| {
            let row = game_cargo::inventory(&self.sim.host.storage, *inventory)
                .expect("inventory exists");
            game_characters::perception::StockObservationFacts {
                inventory: *inventory,
                good: row.good,
                quantity: row.quantity,
                accessible: self.sim.has_stock_access(actor, row),
            }
        };
        let ground = self
            .sim
            .inventory_index
            .get(&InventoryLocation::Area(area))
            .into_iter()
            .flatten()
            .map(facts);
        let carried = self
            .sim
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(facts);
        game_characters::perception::observe_stock_facts(actor, area, at_area, now, ground, carried)
    }
    fn sites(&self) -> Vec<game_characters::orchestration::ObservedSite> {
        game_workshop::sites(&self.sim.host.storage)
            .map(|(_, row)| game_characters::orchestration::ObservedSite {
                site: row.id,
                area: row.area,
                input: row.input,
                tool: row.tool,
                input_quantity: self
                    .sim
                    .host
                    .state
                    .workshops
                    .recipe(&row.recipe)
                    .expect("validated recipe")
                    .input_quantity,
                output: row.output,
                access: row.access.clone(),
            })
            .collect()
    }
    fn routes(&self, area: AreaId) -> BTreeMap<AreaId, SimDuration> {
        self.sim
            .host
            .state
            .areas
            .iter()
            .find(|row| row.id == area)
            .expect("known area")
            .links
            .iter()
            .map(|link| (link.destination, link.duration))
            .collect()
    }
}

impl Simulation {
    pub(crate) fn initialize_decision_people(
        &mut self,
        minds: BTreeMap<PersonId, Mind>,
    ) -> Result<(), Error> {
        if self.host.state.decisions.is_some()
            || minds.len() != self.people().len()
            || self.people().any(|p| !minds.contains_key(&p.id))
        {
            return Err(Error::InvalidConfiguration("invalid initial minds".into()));
        }
        if game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
            .next()
            .is_some()
        {
            return Err(Error::InvalidConfiguration(
                "initial minds already exist".into(),
            ));
        }
        self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
        let mut batch = game_ecs::Batch::new();
        for (actor, mind) in minds {
            batch.insert(game_ecs::StableId(actor.0), mind);
            batch.insert(
                game_ecs::StableId(actor.0),
                game_characters::decisions::Stocks::default(),
            );
        }
        self.host.storage.apply_batch(batch)?;
        let work_concept = ConceptId(self.host.allocate_id()?.0);
        self.host.state.decisions = Some(Runtime {
            concepts: BTreeMap::from([(work_concept, Good::Food)]),
            work_concept,
            last_learning_event: self.host.next_event() - 1,
            switching_margin: 50,
        });
        Ok(())
    }
    pub fn autonomy_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::survival_fixture(seed, execution)?;
        sim.host.state.scenario = scenarios().remove(2);
        let minds = sim
            .people()
            .enumerate()
            .map(|(i, p)| {
                let restful = i % 3 == 1;
                (
                    p.id,
                    Mind {
                        interests: BTreeMap::new(),
                        // These test villagers have an authored map of the three
                        // fixture areas. Newborns learn routes by observation.
                        routes: fixture_routes(&sim.host.state.areas),
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
        if self.host.state.decisions.is_none() {
            return;
        }
        let now = self.host.clock().time;
        let observations = game_characters::orchestration::prepare_observations(
            now,
            &PersonalPort {
                sim: self,
                resources: None,
            },
        );
        game_characters::orchestration::apply_observations::<Command>(
            &mut self.host.storage,
            now,
            observations,
        )
        .expect("known observation actor");
    }

    pub(crate) fn review_decisions(&mut self) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let Some(_) = self.host.state.decisions else {
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
        self.observe_property()?;
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
        let mut intentions: Vec<_> = {
            let resources = self.prepare_ranking_resources()?;
            let world = self.host.state.decisions.as_ref().unwrap();
            let jobs = world.review_jobs::<Command>(&self.host.storage);
            let port = PersonalPort {
                sim: self,
                resources: Some(&resources),
            };
            self.engine
                .executor()
                .map_ordered(&jobs, |mind| {
                    Ok(world.decide_from_port(
                        mind.0,
                        mind.1,
                        game_characters::decisions::stocks(&self.host.storage, mind.0)
                            .expect("stocks exist"),
                        &port,
                    ))
                })?
                .into_iter()
                .map(|(_, intention)| intention)
                .collect()
        };
        self.profile.record_phase("review.rank", phase_started);
        let phase_started = Instant::now();
        game_characters::orchestration::order_intentions(&mut intentions, |intention| {
            let (resource, last_granted) =
                match intention.decision.plan.as_ref().map(|plan| &plan.action) {
                    Some(Command::Eat { source }) | Some(Command::Care { source, .. }) => (
                        source.0,
                        game_cargo::inventory(&self.host.storage, *source)
                            .expect("inventory exists")
                            .last_granted,
                    ),
                    Some(Command::Work { site }) => (
                        site.0,
                        game_workshop::site(&self.host.storage, *site)
                            .expect("site exists")
                            .last_granted,
                    ),
                    _ => (0, None),
                };
            game_characters::orchestration::ClaimFacts {
                resource,
                last_granted,
            }
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
            game_characters::orchestration::record_attempt::<Command>(
                &mut self.host.storage,
                i.actor,
                owner_now,
                plan,
                i.decision.explanation,
                accepted.is_ok(),
            )?;
        }
        self.profile.record_phase("review.apply", phase_started);
        Ok(())
    }

    fn personal_facts(&self, id: PersonId) -> PersonalFacts {
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, id)
            .expect("mind exists");
        let p = self.person_at(self.person_index[&id]);
        let actor = &game_physiology::body(&self.host.storage, id).expect("body exists");
        let rules = self.body_rules(id);
        let place = match self.resolved_location(id) {
            Location::AtArea(area) => Place::Area(area),
            Location::StoppedOnRoad { .. } => Place::Stopped,
            Location::InTransit { .. } => Place::Moving,
            Location::Carried { .. } => unreachable!("resolved physical location"),
        };
        let observed_contacts = game_characters::orchestration::observed_contacts(
            mind,
            self.host.clock().time,
            |location, now| {
                transport::location_position(location, now).is_ok_and(|point| {
                    self.transport_position(id)
                        .is_some_and(|own| point.same_point(own))
                })
            },
        );
        let agency = game_characters::orchestration::AgencyFacts {
            manual: id == self.host.state.ruler,
            in_combat: self.in_combat(id),
            family_busy: self.lineage_handles(id),
            mission_busy: self.mission_actor_busy(id),
            health_busy: self.health_actor_busy(id),
            transport_busy: self
                .host
                .state
                .transport
                .as_ref()
                .is_some_and(|world| world.busy(&self.host.storage, id)),
            army_talking: self.army_actor_talking(id),
            realm_talking: self.realm_actor_busy(id),
            property_talking: self.property_actor_talking(id),
        };
        PersonalFacts {
            actor: id,
            now: self.host.clock().time,
            vitals: actor.vitals.clone(),
            limits: NeedLimits {
                meal_relief: rules.meal_relief,
                hunger_harm: rules.hunger_harm,
                meal_size: rules.meal_size,
                eating_time: rules.eating_time,
                fatigue_limit: rules.fatigue_limit,
                max_health: rules.max_health,
                drink_quantity: self
                    .host
                    .state
                    .health
                    .as_ref()
                    .map_or(Quantity(u64::MAX), |world| world.rules.drink_quantity),
            },
            activity: match *self.host.storage.activity(id).expect("activity exists") {
                Activity::Idle => PersonalActivity::Idle,
                Activity::Rest => PersonalActivity::Rest,
                Activity::Work { .. } => PersonalActivity::Working,
                _ => PersonalActivity::Committed,
            },
            place,
            control: agency.control(),
            in_dialogue: agency.in_dialogue(),
            independent: self.independent(id),
            child: self.host.state.development.as_ref().is_some_and(|_| {
                game_lifecycle::life(&self.host.storage, id).unwrap().stage == LifeStage::Child
            }),
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
            hydration_urgency: self.observed_hydration_urgency(id),
            exposure_urgency: self.observed_exposure_urgency(id),
            freedom_restricted: self
                .host
                .state
                .combat
                .as_ref()
                .is_some_and(|_| game_conflict::captivity(&self.host.storage, id).is_some()),
        }
    }

    pub(crate) fn learn_from_work(&mut self) {
        let parts = self.host.parts_mut();
        let Some(world) = &mut parts.state.decisions else {
            return;
        };
        world.learn_production_events::<Command>(
            parts.store,
            parts.events.iter().filter_map(|event| match event.event {
                EventKind::Produced { actor, .. } | EventKind::CropHarvested { actor, .. } => {
                    Some((event.sequence, actor))
                }
                _ => None,
            }),
            parts.next_event - 1,
        );
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
            .host
            .state
            .decisions
            .as_mut()
            .and_then(|_| {
                game_characters::decisions::mind_mut::<Command>(&mut self.host.storage, id)
            })
            .ok_or(Error::Unavailable)?;
        mind.personality = personality;
        mind.dream = dream;
        Ok(())
    }

    /// Explicit setup of an authored belief for false/stale-information scenarios.
    pub fn debug_set_knowledge(&mut self, id: PersonId, fact: StockKnowledge) -> Result<(), Error> {
        if !game_cargo::inventory(&self.host.storage, fact.inventory).is_some()
            || fact.observed_at > self.host.clock().time
            || fact.learned_at > self.host.clock().time
        {
            return Err(Error::InvalidFixture("invalid knowledge".into()));
        }
        let mind = self
            .host
            .state
            .decisions
            .as_mut()
            .and_then(|_| game_characters::decisions::stocks_mut(&mut self.host.storage, id))
            .ok_or(Error::Unavailable)?;
        mind.known.insert(fact.inventory, fact);
        Ok(())
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
