//! Bounded action templates. Decisions read personal knowledge; physical execution
//! can fail and teach the actor without granting an unseen fact in advance.
use crate::*;

pub use game_characters::decisions::{
    Aspiration, Attachment, ChildSignal, Explanation, Goal, Personality, Relationship,
    SiteKnowledge,
};

pub type Mind = game_characters::decisions::Mind<Command>;
pub(crate) use game_characters::decisions::Control;
pub use game_characters::decisions::Runtime;
pub(crate) use game_characters::decisions::known_path;
use game_characters::decisions::{
    ActionShape, Candidate, Capability, CommitmentRef, Decision, Effort, IntentionKey, Motive,
    NeedRef, PersonalFacts, Prospects, Subject, Uncertainty,
};
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
        if self.host.state.decisions.is_some()
            || minds.len() != self.host.state.people.len()
            || self
                .host
                .state
                .people
                .iter()
                .any(|p| !minds.contains_key(&p.id))
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
            .host
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
        let owner_now = self.host.clock().time;
        if self.host.state.decisions.is_none() {
            return;
        }
        let locations = transport::resolved_locations_for(&self.canonical_state())
            .expect("valid physical observations");
        let mut cohorts: BTreeMap<game_transport::PositionKey, Vec<PersonId>> = BTreeMap::new();
        for person in &self.host.state.people {
            if self.host.state.development.as_ref().is_some_and(|_| {
                matches!(
                    game_lifecycle::life(&self.host.storage, person.id)
                        .unwrap()
                        .stage,
                    LifeStage::Infant | LifeStage::Child
                )
            }) && game_physiology::body(&self.host.storage, person.id)
                .expect("body exists")
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
        for p in &self.host.state.people {
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
            let actor = &game_physiology::body(&self.host.storage, p.id).expect("body exists");
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
                .filter(|id| {
                    self.has_stock_access(
                        p.id,
                        game_cargo::inventory(&self.host.storage, **id).expect("inventory exists"),
                    )
                })
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
                    .position(owner_now)
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
                            .host
                            .state
                            .transport
                            .as_ref()
                            .and_then(|_| game_transport::group(&self.host.storage, **child))
                            .is_some_and(|g| g.bearers.contains(&p.id))
                })
                .filter(|child| self.dependent_signal_visible(p.id, **child))
                .take(64)
                .copied()
                .collect();
            let mut observed_children = Vec::new();
            for child in &visible_children {
                let vitals = &game_physiology::body(&self.host.storage, *child)
                    .expect("body exists")
                    .vitals;
                let rules = &signal_rules[child];
                let mut urgency = if vitals.hunger >= rules.hunger_harm {
                    20_000
                } else if vitals.hunger >= rules.meal_relief {
                    3_000
                } else {
                    0
                };
                if game_lifecycle::life(&self.host.storage, *child)
                    .unwrap()
                    .preference
                    .is_some_and(|(p, until)| p == ChildhoodInterest::Food && until >= owner_now)
                {
                    urgency = urgency.max(3_000);
                }
                observed_children.push((
                    *child,
                    ChildSignal {
                        preference: game_lifecycle::life(&self.host.storage, *child)
                            .unwrap()
                            .preference
                            .filter(|(_, until)| *until >= owner_now)
                            .map(|(p, _)| p),
                        location: locations[child].clone(),
                        urgency,
                        hydration_urgency: hydration_signals[child],
                        observed_at: owner_now,
                    },
                ));
            }
            let mut observed_sites = Vec::new();
            for site in game_workshop::sites(&self.host.storage)
                .map(|(_, site)| site)
                .filter(|s| at_area && s.area == area)
            {
                observed_sites.push(SiteKnowledge {
                    site: site.id,
                    area,
                    input: site.input,
                    tool: site.tool,
                    input_quantity: self
                        .host
                        .state
                        .workshops
                        .recipe(&site.recipe)
                        .expect("validated recipe")
                        .input_quantity,
                    output: site.output,
                    access: site.access.contains(&p.id),
                });
            }
            let observed_stocks: Vec<_> = visible_stocks
                .iter()
                .map(|id| {
                    let inventory =
                        game_cargo::inventory(&self.host.storage, *id).expect("inventory exists");
                    StockKnowledge {
                        inventory: *id,
                        area,
                        good: inventory.good,
                        quantity: inventory.quantity,
                        accessible: accessible_stocks.contains(id),
                        observed_at: owner_now,
                        learned_at: owner_now,
                        source: KnowledgeSource::Observed(p.id),
                    }
                })
                .collect();
            for fact in observed_stocks {
                game_characters::decisions::observe_stock::<Command>(
                    &mut self.host.storage,
                    p.id,
                    owner_now,
                    area,
                    self.host.state.physical.as_ref().unwrap().rules.meal_size,
                    fact,
                )
                .expect("known observation actor");
            }
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                p.id,
            )
            .unwrap();
            if at_area {
                let actual_routes = self
                    .host
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
            mind.child_signals
                .retain(|_, signal| signal.location != *location);
            for (child, signal) in observed_children {
                mind.child_signals.insert(child, signal);
            }
            mind.reconcile_care_observation(p.id);
            for site in observed_sites {
                mind.sites.insert(site.site, site);
            }
        }
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
            let jobs: Vec<_> =
                game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
                    .map(|(id, mind)| (id, (id, mind)))
                    .collect();
            self.engine
                .executor()
                .map_ordered(&jobs, |mind| {
                    Ok(self.decide(mind.0, mind.1, world.switching_margin, Some(&resources)))
                })?
                .into_iter()
                .map(|(_, intention)| intention)
                .collect()
        };
        self.profile.record_phase("review.rank", phase_started);
        let phase_started = Instant::now();
        // Equal claims use the same persistent rotating cursor as physical transfers.
        intentions.sort_by_key(|i| {
            let (resource, cursor) = match i.decision.plan.as_ref().map(|plan| &plan.action) {
                Some(Command::Eat { source }) | Some(Command::Care { source, .. }) => (
                    source.0,
                    game_cargo::inventory(&self.host.storage, *(source))
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
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                i.actor,
            )
            .unwrap();
            mind.record_attempt(owner_now, plan, i.decision.explanation, accepted.is_ok())?;
        }
        self.profile.record_phase("review.apply", phase_started);
        Ok(())
    }

    fn decide(
        &self,
        id: PersonId,
        mind: &Mind,
        margin: i64,
        resources: Option<&equipment::RankingResources>,
    ) -> Intention {
        let p = &self.host.state.people[self.person_index[&id]];
        let actor = &game_physiology::body(&self.host.storage, id).expect("body exists");
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
                signal.observed_at == self.host.clock().time
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
            control: if id == self.host.state.ruler {
                Control::Manual
            } else if self.in_combat(id)
                || self.lineage_handles(id)
                || self.mission_actor_busy(id)
                || self.health_actor_busy(id)
                || self
                    .host
                    .state
                    .transport
                    .as_ref()
                    .is_some_and(|world| world.busy(&self.host.storage, id))
            {
                Control::Delegated
            } else {
                Control::Autonomous
            },
            in_dialogue: self.army_actor_talking(id)
                || self.realm_actor_busy(id)
                || self.property_actor_talking(id),
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
                .is_some_and(|world| world.captivity.contains_key(&id)),
        };
        if !mind.needs_candidates(&facts) {
            return Intention {
                actor: id,
                decision: mind.decide(
                    &facts,
                    game_characters::decisions::stocks(&self.host.storage, id)
                        .expect("stock knowledge exists"),
                    &[],
                    self.host.state.decisions.as_ref().unwrap().work_concept,
                    margin,
                ),
            };
        }
        let care_goals = mind.care_deliveries(&facts);
        let mut candidates = Vec::new();
        let mut add = |action: Command,
                       key: IntentionKey,
                       motives: Vec<Motive>,
                       delivery: Option<game_logistics::Delivery>| {
            if let Some(candidate) =
                self.decision_evidence(id, mind, action, key, motives, delivery)
            {
                candidates.push(candidate);
            }
        };
        for goal in &care_goals {
            let game_logistics::Destination::Contact(child) = goal.delivery.destination else {
                continue;
            };
            let location = match transport::location_position(&goal.location, goal.observed_at) {
                Ok(game_transport::Position::Area(area)) => Location::AtArea(area),
                Ok(game_transport::Position::Road(point)) => Location::StoppedOnRoad {
                    from: point.from,
                    to: point.to,
                    progress: point.progress,
                    duration: point.duration,
                },
                Err(_) => continue,
            };
            let destination = game_logistics::KnownDestination {
                destination: goal.delivery.destination,
                location,
                observed_at: goal.observed_at,
                accessible: true,
                room: None,
                direct_transfer: true,
            };
            let step = self.plan_delivery_for_planning(
                id,
                &goal.delivery,
                &destination,
                goal.delivery.requirement.quantity,
                &[],
                resources,
            );
            let action = match step {
                game_logistics::Step::Arrived { source, .. } => {
                    Some(Command::Care { child, source })
                }
                step => self.delivery_command(id, step),
            };
            if let Some(action) = action {
                let good = goal.delivery.requirement.good;
                let key = IntentionKey::Provision {
                    good,
                    beneficiary: child,
                    source: goal.delivery.requirement.source,
                };
                add(
                    action,
                    key,
                    vec![Motive::Provide {
                        requirement: goal.delivery.requirement.clone(),
                        beneficiary: child,
                        need: Some(if good == Good::Food {
                            NeedRef::Hunger
                        } else {
                            NeedRef::Hydration
                        }),
                    }],
                    Some(goal.delivery.clone()),
                );
            }
        }
        if let Some(action) = self.force_candidate(id) {
            add(
                action,
                IntentionKey::Relief {
                    need: NeedRef::Freedom,
                    beneficiary: id,
                },
                vec![Motive::Relieve {
                    need: NeedRef::Freedom,
                    beneficiary: id,
                }],
                None,
            );
        }
        if matches!(place, Place::Area(_)) {
            if let Some(action) = self.health_candidate(id, resources) {
                let need = if matches!(
                    action,
                    Command::Health {
                        action: HealthAction::EnterShelter { .. }
                            | HealthAction::BuildShelter { .. }
                            | HealthAction::LeaveShelter
                    }
                ) {
                    NeedRef::Exposure
                } else {
                    NeedRef::Hydration
                };
                add(
                    action,
                    IntentionKey::Relief {
                        need,
                        beneficiary: id,
                    },
                    vec![Motive::Relieve {
                        need,
                        beneficiary: id,
                    }],
                    None,
                );
            }
            for action in self.ecology_opportunities(id, resources) {
                let goods = match &action {
                    Command::Ecology {
                        action:
                            EcologyAction::Gather { resource, .. }
                            | EcologyAction::RequestAccess { resource },
                    } => game_ecology::knowledge(&self.host.storage, id)
                        .map(|row| &row.known_resources)
                        .and_then(|known| known.get(resource))
                        .map(|source| BTreeSet::from([source.kind.good()]))
                        .unwrap_or_default(),
                    Command::Move { destination } => {
                        game_ecology::knowledge(&self.host.storage, id)
                            .map(|row| &row.known_resources)
                            .into_iter()
                            .flat_map(|known| known.values())
                            .filter(|source| source.area == *destination && source.available.0 > 0)
                            .map(|source| source.kind.good())
                            .collect()
                    }
                    _ => BTreeSet::new(),
                };
                for good in goods {
                    let quantity = if good == Good::Timber {
                        Quantity(5_000)
                    } else {
                        rules.meal_size
                    };
                    let need = match good {
                        Good::Food => Some(NeedRef::Hunger),
                        Good::Water => Some(NeedRef::Hydration),
                        _ => None,
                    };
                    add(
                        action.clone(),
                        IntentionKey::Provision {
                            good,
                            beneficiary: id,
                            source: None,
                        },
                        vec![Motive::Provide {
                            requirement: game_logistics::Requirement {
                                good,
                                quantity,
                                source: None,
                            },
                            beneficiary: id,
                            need,
                        }],
                        None,
                    );
                }
            }
            for action in self.project_opportunities(id, resources) {
                let mut motives = vec![Motive::Develop {
                    capability: Capability::Livelihood,
                    beneficiary: id,
                }];
                let mut key = IntentionKey::Development {
                    capability: Capability::Livelihood,
                    beneficiary: id,
                    subject: None,
                };
                if let Command::Project {
                    action: ProjectAction::Work { project },
                } = &action
                    && let Some(plan) = self
                        .host
                        .state
                        .projects
                        .as_ref()
                        .and_then(|_| {
                            game_projects::knowledge(&self.host.storage, id)
                                .map(|row| &row.projects)
                        })
                        .and_then(|known| known.get(project))
                {
                    motives = plan
                        .beneficiaries
                        .iter()
                        .map(|beneficiary| Motive::Develop {
                            capability: Capability::Livelihood,
                            beneficiary: *beneficiary,
                        })
                        .collect();
                    key = IntentionKey::Development {
                        capability: Capability::Livelihood,
                        beneficiary: id,
                        subject: Some(Subject::Project(*project)),
                    };
                    if let Some(obligation) = game_agreements::known_construction_obligation(
                        &self.host.storage,
                        id,
                        *project,
                    ) {
                        motives.push(Motive::Fulfil {
                            commitment: CommitmentRef::Obligation(obligation),
                        });
                        key = IntentionKey::Commitment(CommitmentRef::Obligation(obligation));
                    }
                }
                add(action, key, motives, None);
            }
            for action in self.property_opportunities(id) {
                let inventory = match action {
                    Command::Property {
                        action: PropertyAction::RequestAccess { inventory },
                    } => inventory,
                    Command::Property {
                        action: PropertyAction::Take { source, .. },
                    } => source,
                    _ => continue,
                };
                let Some(good) = game_characters::decisions::stocks(&self.host.storage, id)
                    .expect("stock knowledge exists")
                    .known
                    .get(&inventory)
                    .map(|stock| stock.good)
                else {
                    continue;
                };
                let need = match good {
                    Good::Food => Some(NeedRef::Hunger),
                    Good::Water => Some(NeedRef::Hydration),
                    _ => None,
                };
                add(
                    action,
                    IntentionKey::Provision {
                        good,
                        beneficiary: id,
                        source: None,
                    },
                    vec![Motive::Provide {
                        requirement: game_logistics::Requirement {
                            good,
                            quantity: rules.meal_size,
                            source: None,
                        },
                        beneficiary: id,
                        need,
                    }],
                    None,
                );
            }
        }
        for fact in game_characters::decisions::stocks(&self.host.storage, id)
            .expect("stock knowledge exists")
            .known
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
            let mut candidate = self.decision_evidence(
                id,
                mind,
                action,
                IntentionKey::Relief {
                    need: NeedRef::Hunger,
                    beneficiary: id,
                },
                vec![Motive::Relieve {
                    need: NeedRef::Hunger,
                    beneficiary: id,
                }],
                None,
            );
            if let Some(candidate) = &mut candidate {
                candidate.prospects.source = Some(fact.inventory);
            }
            if let Some(candidate) = candidate {
                candidates.push(candidate);
            }
        }
        for child in &facts.observed_contacts {
            if let Some(candidate) = self.decision_evidence(
                id,
                mind,
                Command::Comfort { child: *child },
                IntentionKey::Relief {
                    need: NeedRef::Company,
                    beneficiary: *child,
                },
                vec![Motive::Relieve {
                    need: NeedRef::Company,
                    beneficiary: *child,
                }],
                None,
            ) {
                candidates.push(candidate);
            }
        }
        if let Some(candidate) = self.decision_evidence(
            id,
            mind,
            Command::Rest,
            IntentionKey::Relief {
                need: NeedRef::Recovery,
                beneficiary: id,
            },
            vec![Motive::Relieve {
                need: NeedRef::Recovery,
                beneficiary: id,
            }],
            None,
        ) {
            candidates.push(candidate);
        }
        let mut add = |action, key, motives| {
            if let Some(candidate) = self.decision_evidence(id, mind, action, key, motives, None) {
                candidates.push(candidate);
            }
        };
        if let Place::Area(_) = place {
            if let Some(action) = self.autonomous_cultivation(id, resources) {
                add(
                    action,
                    IntentionKey::Development {
                        capability: Capability::Skill(Skill::Farming),
                        beneficiary: id,
                        subject: None,
                    },
                    vec![Motive::Develop {
                        capability: Capability::Skill(Skill::Farming),
                        beneficiary: id,
                    }],
                );
            }
            for interest in [
                ChildhoodInterest::Food,
                ChildhoodInterest::Company,
                ChildhoodInterest::Movement,
                ChildhoodInterest::Craft,
            ] {
                add(
                    Command::Play { interest },
                    IntentionKey::Development {
                        capability: Capability::Interest(interest),
                        beneficiary: id,
                        subject: None,
                    },
                    vec![Motive::Develop {
                        capability: Capability::Interest(interest),
                        beneficiary: id,
                    }],
                );
            }
            for site in mind.sites.values() {
                add(
                    Command::Work { site: site.site },
                    IntentionKey::Development {
                        capability: Capability::Skill(Skill::Craft),
                        beneficiary: id,
                        subject: Some(Subject::Site(site.site)),
                    },
                    vec![Motive::Develop {
                        capability: Capability::Skill(Skill::Craft),
                        beneficiary: id,
                    }],
                );
            }
        } else {
            for action in [TransportAction::Stop, TransportAction::Resume] {
                add(
                    Command::Transport { action },
                    IntentionKey::Development {
                        capability: Capability::Movement,
                        beneficiary: id,
                        subject: None,
                    },
                    vec![Motive::Develop {
                        capability: Capability::Movement,
                        beneficiary: id,
                    }],
                );
            }
        }
        Intention {
            actor: id,
            decision: mind.decide(
                &facts,
                game_characters::decisions::stocks(&self.host.storage, id)
                    .expect("stock knowledge exists"),
                &candidates,
                self.host.state.decisions.as_ref().unwrap().work_concept,
                margin,
            ),
        }
    }
    /// Translate physical action facts into evidence; no final preference or
    /// provider identity is attached to these opportunities.
    fn decision_evidence(
        &self,
        actor: PersonId,
        mind: &Mind,
        action: Command,
        key: IntentionKey,
        motives: Vec<Motive>,
        delivery: Option<game_logistics::Delivery>,
    ) -> Option<Candidate<Command>> {
        let mut prospects = Prospects {
            risk: crate::logistics::acquisition_risk(&action),
            ..Default::default()
        };
        match &action {
            Command::Move { destination } => {
                if let Location::AtArea(area) = self.resolved_location(actor) {
                    prospects.travel = known_path(&mind.routes, area, *destination).ok()?.1;
                }
                prospects.uncertainty = Uncertainty::Remembered;
            }
            Command::Eat { source } => {
                prospects.source = Some(*source);
                prospects.duration = self.body_rules(actor).eating_time;
            }
            Command::Care { source, .. } => {
                prospects.source = Some(*source);
            }
            Command::Rest => prospects.effort = Effort::Restoration,
            Command::Play { .. } => prospects.effort = Effort::Practice,
            Command::Work { site } => {
                prospects.site = Some(*site);
                prospects.effort = Effort::Practice;
            }
            Command::Health {
                action: HealthAction::Drink { source } | HealthAction::Boil { source, .. },
            } => {
                prospects.source = Some(*source);
            }
            Command::Project { .. } => {
                prospects.effort = Effort::Practice;
                prospects.uncertainty = Uncertainty::Conditional;
            }
            Command::Ecology { .. }
            | Command::Property { .. }
            | Command::Carry { .. }
            | Command::Load { .. } => prospects.uncertainty = Uncertainty::Conditional,
            _ => {}
        }
        let shape = action_shape(&action);
        Some(Candidate {
            action,
            key,
            motives,
            prospects,
            shape,
            delivery,
        })
    }

    pub(crate) fn on_delivery_event(&mut self, kind: &EventKind, sequence: EventId) {
        let EventKind::Cared {
            caregiver,
            child,
            source,
            quantity,
        } = kind
        else {
            return;
        };
        let Some(good) =
            game_cargo::inventory(&self.host.storage, *(source)).map(|stock| stock.good)
        else {
            return;
        };
        if let Some(mind) = self.host.state.decisions.as_mut().and_then(|_| {
            game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                *caregiver,
            )
        }) {
            let _ = mind.record_care_receipt(*child, good, *quantity, sequence.0);
        }
        if let Some(lineage) = &mut self.host.state.lineage {
            let _ = lineage.record_care_receipt(*caregiver, *child, good, *quantity, sequence.0);
        }
    }

    pub(crate) fn learn_from_work(&mut self) {
        let parts = self.host.parts_mut();
        let Some(world) = &mut parts.state.decisions else {
            return;
        };
        for event in parts.events.iter().skip(world.last_learning_event as usize) {
            if let EventKind::Produced { actor, .. } | EventKind::CropHarvested { actor, .. } =
                event.event
            {
                world.learn_production::<Command>(parts.store, actor);
            }
        }
        world.last_learning_event = parts.next_event - 1;
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

    pub(crate) fn receive_report(
        &mut self,
        sender: PersonId,
        recipient: PersonId,
        fact: StockKnowledge,
    ) -> bool {
        let owner_now = self.host.clock().time;
        let Some(_) = &mut self.host.state.decisions else {
            return false;
        };
        let inventory = fact.inventory;
        if !game_characters::decisions::receive_social_report::<Command>(
            &mut self.host.storage,
            recipient,
            sender,
            owner_now,
            fact,
        ) {
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
