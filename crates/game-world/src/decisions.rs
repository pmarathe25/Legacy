//! Bounded action templates. Decisions read personal knowledge; physical execution
//! can fail and teach the actor without granting an unseen fact in advance.
use crate::*;
use rayon::prelude::*;

pub use game_characters::decisions::{
    Attachment, ChildSignal, Dream, Explanation, Goal, Personality, Relationship, SiteKnowledge,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind {
    pub interests: BTreeMap<ChildhoodInterest, u64>,
    pub routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    pub last_failure: Option<(SimTime, InventoryId)>,
    pub child_signals: BTreeMap<PersonId, ChildSignal>,
    pub personality: Personality,
    pub dream: Dream,
    pub knowledge: BTreeMap<InventoryId, StockKnowledge>,
    pub sites: BTreeMap<SiteId, SiteKnowledge>,
    pub relationships: BTreeMap<PersonId, Relationship>,
    pub values: BTreeMap<ConceptId, Attachment>,
    pub goal: Goal,
    pub plan: Option<Command>,
    pub explanation: Explanation,
    pub retry_after: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub minds: BTreeMap<PersonId, Mind>,
    pub concepts: BTreeMap<ConceptId, Good>,
    pub work_concept: ConceptId,
    pub last_learning_event: u64,
    pub switching_margin: i64,
}
#[derive(Clone)]
struct Intention {
    actor: PersonId,
    command: Option<Command>,
    explanation: Explanation,
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
                            Dream::Comfort
                        } else {
                            Dream::Independence
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
                if mind.goal == Goal::Survival
                    && matches!(mind.plan, Some(Command::Move { destination }) if destination == area)
                    && mind.knowledge.get(id).is_some_and(|old| {
                        old.good == Good::Food
                            && old.quantity.0
                                >= self.state.physical.as_ref().unwrap().rules.meal_size.0
                    })
                    && inventory.quantity.0
                        < self.state.physical.as_ref().unwrap().rules.meal_size.0
                {
                    mind.last_failure = Some((self.state.time, *id));
                }
                mind.knowledge.insert(
                    *id,
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
            let (resource, cursor) = match i.command {
                Some(Command::Eat { source }) | Some(Command::Care { source, .. }) => {
                    (source.0, self.state.inventories[&source].last_granted)
                }
                Some(Command::Work { site }) => (
                    site.0,
                    self.state.physical.as_ref().unwrap().sites[&site].last_granted,
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
            let Some(command) = i.command else {
                continue;
            };
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
            mind.goal = i.explanation.goal;
            mind.plan = Some(command);
            mind.explanation = i.explanation;
            if accepted.is_err() {
                mind.explanation.blocker = Some("physical attempt failed".into());
                mind.retry_after = SimTime(
                    self.state
                        .time
                        .0
                        .checked_add(120_000)
                        .ok_or(Error::Overflow)?,
                );
            }
        }
        self.profile.record_phase("review.apply", phase_started);
        Ok(())
    }

    fn decide(&self, id: PersonId, mind: &Mind, margin: i64) -> Intention {
        let p = &self.state.people[self.person_index[&id]];
        let physical = self.state.physical.as_ref().unwrap();
        let actor = &physical.actors[&id];
        let rules = self.body_rules(id);
        let none = || Intention {
            actor: id,
            command: None,
            explanation: mind.explanation.clone(),
        };
        if (self.army_actor_talking(id) || self.realm_actor_busy(id))
            && actor.vitals.hunger < rules.meal_relief
            && actor.vitals.fatigue < rules.fatigue_limit
            && actor.vitals.health >= rules.max_health / 2
            && !mind
                .child_signals
                .values()
                .any(|signal| signal.urgency >= 20_000)
        {
            return none();
        }
        if self.in_combat(id)
            || self.lineage_handles(id)
            || self.mission_actor_busy(id)
            || self.health_actor_busy(id)
            || self
                .state
                .transport
                .as_ref()
                .is_some_and(|world| world.busy(id))
            || id == self.state.ruler
            || !actor.vitals.alive
            || actor.vitals.health == 0
        {
            return none();
        }
        if let Some(command) = self.force_candidate(id) {
            return Intention {
                actor: id,
                command: Some(command),
                explanation: Explanation {
                    goal: Goal::Practice,
                    need: 2_000,
                    benefit: 0,
                    value: 0,
                    total: 2_000,
                    blocker: None,
                },
            };
        }
        if !self.independent(id) {
            return none();
        }
        let area = match self.resolved_location(id) {
            Location::AtArea(area) => area,
            Location::StoppedOnRoad { .. } | Location::InTransit { .. } => {
                return self.decide_on_road(id, mind);
            }
            Location::Carried { .. } => return none(),
        };
        let urgent = actor.vitals.hunger >= rules.meal_relief;
        if matches!(
            actor.activity,
            Activity::Eat { .. }
                | Activity::Load { .. }
                | Activity::Tell { .. }
                | Activity::Care { .. }
                | Activity::Conceive { .. }
                | Activity::Unbind { .. }
                | Activity::Comfort { .. }
                | Activity::Express { .. }
                | Activity::Play { .. }
                | Activity::Cultivate { .. }
                | Activity::Take { .. }
                | Activity::ReportIncident { .. }
                | Activity::Lesson { .. }
                | Activity::Construct { .. }
                | Activity::Social { .. }
                | Activity::Instruction { .. }
                | Activity::Practice { .. }
                | Activity::EnforcementTalk { .. }
                | Activity::HealthWork { .. }
                | Activity::Gather { .. }
                | Activity::MissionTalk { .. }
                | Activity::ReportAgreement { .. }
                | Activity::Escape { .. }
                | Activity::Restrain { .. }
                | Activity::WritingMemory { .. }
                | Activity::ReadingMemory { .. }
                | Activity::TeachingMemory { .. }
                | Activity::MovingRecord { .. }
        ) || matches!(actor.activity, Activity::Work { .. }) && !urgent
        {
            return none();
        }
        let mut candidates = Vec::<(Command, Explanation)>::new();
        let mut add = |command, goal, need, benefit, value| {
            candidates.push((
                command,
                Explanation {
                    goal,
                    need,
                    benefit,
                    value,
                    total: need + benefit + value,
                    blocker: None,
                },
            ))
        };
        if let Some(command) = self.health_candidate(id) {
            add(command, Goal::Recovery, 2_000, 200, 0);
        }
        if let Some(command) = self.ecology_candidate(id) {
            add(
                command,
                Goal::Survival,
                if urgent { 2_000 } else { 0 },
                200,
                0,
            );
        }
        if let Some(command) = self.project_candidate(id) {
            add(
                command,
                Goal::Practice,
                0,
                i64::from(mind.personality.ambition) + 200,
                0,
            );
        }
        if let Some(command) = self.property_candidate(id) {
            add(command, Goal::Survival, 9_000, 0, 0);
        }
        if !p.hunger_exempt && urgent {
            for fact in mind
                .knowledge
                .values()
                .filter(|f| f.good == Good::Food && f.accessible && f.quantity >= rules.meal_size)
            {
                let (command, travel) = if fact.area == area {
                    (
                        Command::Eat {
                            source: fact.inventory,
                        },
                        0,
                    )
                } else if let Ok((_, duration)) = known_path(&mind.routes, area, fact.area) {
                    (
                        Command::Move {
                            destination: fact.area,
                        },
                        duration.0,
                    )
                } else {
                    continue;
                };
                let survival_time = rules
                    .hunger_harm
                    .saturating_sub(actor.vitals.hunger)
                    .saturating_add(actor.vitals.health);
                if travel.saturating_add(rules.eating_time.0) > survival_time {
                    continue;
                }
                add(
                    command,
                    Goal::Survival,
                    actor.vitals.hunger as i64 / 100 + 10_000,
                    -((travel / 1_000).min(i64::MAX as u64) as i64),
                    0,
                );
            }
        }
        for (child, signal) in mind.child_signals.iter().filter(|(_, signal)| {
            signal.location == Location::AtArea(area)
                && (signal.urgency > 0 || signal.preference == Some(ChildhoodInterest::Company))
                && signal.observed_at == self.state.time
        }) {
            if *child == id {
                continue;
            }
            let attachment = mind
                .relationships
                .get(child)
                .map_or(0, |r| r.affection.max(0));
            let motivation = i64::from(mind.personality.empathy) / 2 + i64::from(attachment);
            if motivation < 300 {
                continue;
            }
            if signal.preference == Some(ChildhoodInterest::Company) {
                add(
                    Command::Comfort { child: *child },
                    Goal::Care,
                    500,
                    motivation,
                    0,
                );
            }
            if signal.urgency == 0 {
                continue;
            }
            for fact in mind.knowledge.values().filter(|f| {
                f.area == area
                    && f.good == Good::Food
                    && f.accessible
                    && f.quantity >= rules.meal_size
            }) {
                add(
                    Command::Care {
                        child: *child,
                        source: fact.inventory,
                    },
                    Goal::Care,
                    signal.urgency,
                    motivation,
                    0,
                );
            }
        }
        let recovery = actor.vitals.fatigue as i64 / 500
            + (rules.max_health - actor.vitals.health) as i64 / 100;
        add(
            Command::Rest,
            Goal::Recovery,
            recovery,
            i64::from(mind.personality.hedonism) / 3,
            0,
        );
        if self.state.time >= mind.retry_after
            && !urgent
            && actor.vitals.fatigue < rules.fatigue_limit
        {
            if let Some(command) = self.autonomous_cultivation(id) {
                add(
                    command,
                    Goal::Practice,
                    0,
                    i64::from(mind.personality.discipline + mind.personality.ambition) / 4,
                    100,
                );
            }
            let work_value = mind
                .values
                .get(&self.state.decisions.as_ref().unwrap().work_concept)
                .map_or(0, |v| i64::from(v.weight));
            let child = self
                .state
                .development
                .as_ref()
                .is_some_and(|w| w.lives[&id].stage == LifeStage::Child);
            if child {
                for interest in [
                    ChildhoodInterest::Food,
                    ChildhoodInterest::Company,
                    ChildhoodInterest::Movement,
                    ChildhoodInterest::Craft,
                ] {
                    let inclination = match interest {
                        ChildhoodInterest::Company => mind.personality.sociability,
                        ChildhoodInterest::Craft => mind.personality.discipline,
                        ChildhoodInterest::Movement => mind.personality.risk_tolerance,
                        ChildhoodInterest::Food => mind.personality.hedonism,
                    };
                    add(
                        Command::Play { interest },
                        Goal::Practice,
                        0,
                        i64::from(inclination) / 2,
                        mind.interests.get(&interest).copied().unwrap_or(0) as i64 / 4,
                    );
                }
            }
            for site in mind
                .sites
                .values()
                .filter(|s| !child && s.area == area && s.access)
            {
                let feasible = mind
                    .knowledge
                    .get(&site.input)
                    .is_some_and(|f| f.accessible && f.quantity >= site.input_quantity)
                    && mind
                        .knowledge
                        .get(&site.tool)
                        .is_some_and(|f| f.accessible && f.quantity.0 >= 1_000);
                if feasible {
                    let dream = if mind.dream == Dream::Independence {
                        100
                    } else {
                        0
                    };
                    let benefit =
                        i64::from(mind.personality.discipline + mind.personality.ambition) / 4
                            + dream
                            + (p.work_units / 10_000).min(100) as i64;
                    add(
                        Command::Work { site: site.site },
                        Goal::Practice,
                        0,
                        benefit,
                        work_value
                            + mind
                                .interests
                                .get(&ChildhoodInterest::Craft)
                                .copied()
                                .unwrap_or(0) as i64
                                / 4,
                    );
                }
            }
        }
        let scores: Vec<_> = candidates
            .iter()
            .map(
                |(command, explanation)| game_characters::decisions::CandidateScore {
                    total: explanation.total,
                    tie: match command {
                        Command::Eat { source } => (0, source.0),
                        Command::Move { destination } => (1, destination.0),
                        Command::Work { site } => (2, site.0),
                        _ => (3, 0),
                    },
                    resting: *command == Command::Rest,
                },
            )
            .collect();
        let Some(index) = game_characters::decisions::choose(
            &scores,
            game_characters::decisions::ChoiceContext {
                working: matches!(actor.activity, Activity::Work { .. }),
                resting: actor.activity == Activity::Rest,
                urgent,
                previous_score: mind.explanation.total,
                switching_margin: margin,
            },
        ) else {
            return none();
        };
        let (command, explanation) = candidates.swap_remove(index);
        Intention {
            actor: id,
            command: Some(command),
            explanation,
        }
    }

    fn decide_on_road(&self, id: PersonId, mind: &Mind) -> Intention {
        let body = &self.state.physical.as_ref().unwrap().actors[&id];
        let none = || Intention {
            actor: id,
            command: None,
            explanation: mind.explanation.clone(),
        };
        if !matches!(body.activity, Activity::Idle | Activity::Rest) {
            return none();
        }
        let location = self.resolved_location(id);
        let current_signals: Vec<_> =
            mind.child_signals
                .iter()
                .filter(|(child, signal)| {
                    **child != id
                        && transport::location_position(&signal.location, signal.observed_at)
                            .is_ok_and(|point| {
                                self.transport_position(id)
                                    .is_some_and(|own| point.same_point(own))
                            })
                        && signal.observed_at == self.state.time
                })
                .collect();
        let motivation = |child: &PersonId| {
            i64::from(mind.personality.empathy) / 2
                + i64::from(
                    mind.relationships
                        .get(child)
                        .map_or(0, |r| r.affection.max(0)),
                )
        };
        if matches!(location, Location::InTransit { .. }) {
            if current_signals.iter().any(|(child, signal)| {
                (signal.urgency > 0 || signal.preference == Some(ChildhoodInterest::Company))
                    && motivation(child) >= 300
            }) {
                return Intention {
                    actor: id,
                    command: Some(Command::Transport {
                        action: TransportAction::Stop,
                    }),
                    explanation: Explanation {
                        goal: Goal::Care,
                        need: 3_000,
                        benefit: 0,
                        value: 0,
                        total: 3_000,
                        blocker: None,
                    },
                };
            }
            return none();
        }
        let rules = self.body_rules(id);
        let own_stocks: Vec<_> = mind
            .knowledge
            .values()
            .filter(|fact| {
                fact.source == KnowledgeSource::Observed(id)
                    && fact.accessible
                    && self
                        .inventory_index
                        .get(&InventoryLocation::Carried(id))
                        .is_some_and(|ids| ids.contains(&fact.inventory))
            })
            .collect();
        let mut candidates = Vec::new();
        let mut add = |command, goal, need, benefit| {
            candidates.push((
                command,
                Explanation {
                    goal,
                    need,
                    benefit,
                    value: 0,
                    total: need + benefit,
                    blocker: None,
                },
            ))
        };
        if body.vitals.hunger >= rules.meal_relief {
            for fact in &own_stocks {
                if fact.good == Good::Food && fact.quantity >= rules.meal_size {
                    add(
                        Command::Eat {
                            source: fact.inventory,
                        },
                        Goal::Survival,
                        10_000 + body.vitals.hunger as i64 / 100,
                        0,
                    );
                }
            }
        }
        for (child, signal) in &current_signals {
            let motive = motivation(child);
            if motive < 300 {
                continue;
            }
            if signal.urgency > 0 {
                for fact in &own_stocks {
                    if fact.good == Good::Food && fact.quantity >= rules.meal_size {
                        add(
                            Command::Care {
                                child: **child,
                                source: fact.inventory,
                            },
                            Goal::Care,
                            signal.urgency,
                            motive,
                        );
                    }
                }
            }
            if signal.preference == Some(ChildhoodInterest::Company) {
                add(Command::Comfort { child: **child }, Goal::Care, 500, motive);
            }
        }
        let recovery = (body.vitals.fatigue / 500
            + rules.max_health.saturating_sub(body.vitals.health) / 100)
            .min(i64::MAX as u64) as i64;
        add(
            Command::Rest,
            Goal::Recovery,
            recovery,
            i64::from(mind.personality.hedonism) / 3,
        );
        if recovery == 0
            && current_signals
                .iter()
                .all(|(_, s)| s.urgency == 0 && s.preference != Some(ChildhoodInterest::Company))
            && self.transport_can_depart(id)
        {
            add(
                Command::Transport {
                    action: TransportAction::Resume,
                },
                Goal::Survival,
                0,
                100,
            );
        }
        let scores: Vec<_> = candidates
            .iter()
            .map(
                |(command, explanation)| game_characters::decisions::CandidateScore {
                    total: explanation.total,
                    tie: match command {
                        Command::Care { child, .. } | Command::Comfort { child } => (0, child.0),
                        Command::Eat { source } => (1, source.0),
                        Command::Rest => (2, 0),
                        _ => (3, 0),
                    },
                    resting: *command == Command::Rest,
                },
            )
            .collect();
        let Some(index) = game_characters::decisions::choose(
            &scores,
            game_characters::decisions::ChoiceContext {
                working: false,
                resting: body.activity == Activity::Rest,
                urgent: body.vitals.hunger >= rules.meal_relief
                    || current_signals.iter().any(|(_, s)| s.urgency > 0),
                previous_score: mind.explanation.total,
                switching_margin: self.state.decisions.as_ref().unwrap().switching_margin,
            },
        ) else {
            return none();
        };
        let (command, explanation) = candidates.swap_remove(index);
        Intention {
            actor: id,
            command: Some(command),
            explanation,
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
                let attachment = world
                    .minds
                    .get_mut(&actor)
                    .unwrap()
                    .values
                    .entry(world.work_concept)
                    .or_insert(Attachment {
                        weight: 0,
                        experiences: 0,
                    });
                attachment.weight = (attachment.weight + 100).min(1_000);
                attachment.experiences = attachment.experiences.saturating_add(1);
            }
        }
        world.last_learning_event = self.state.next_event - 1;
    }

    pub fn debug_set_personality(
        &mut self,
        id: PersonId,
        personality: Personality,
        dream: Dream,
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
        mut fact: StockKnowledge,
    ) -> bool {
        let Some(world) = &mut self.state.decisions else {
            return false;
        };
        let Some(receiver) = world.minds.get_mut(&recipient) else {
            return false;
        };
        fact.source = KnowledgeSource::Reported(sender);
        fact.learned_at = self.state.time;
        // Reports describe a permission belonging to the sender, not a transferable
        // entitlement. A report cannot grant the recipient access to the goods.
        fact.accessible = receiver
            .knowledge
            .get(&fact.inventory)
            .is_some_and(|known| known.accessible);
        let relationship = receiver
            .relationships
            .entry(sender)
            .or_insert(Relationship {
                trust: 0,
                affection: 0,
            });
        if relationship.trust < -500 {
            return false;
        }
        relationship.trust = (relationship.trust + 10).min(1_000);
        if receiver
            .knowledge
            .get(&fact.inventory)
            .is_none_or(|old| old.observed_at <= fact.observed_at)
        {
            receiver.knowledge.insert(fact.inventory, fact.clone());
        }
        self.emit(EventKind::Reported {
            sender,
            recipient,
            inventory: fact.inventory,
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

pub(crate) fn known_path(
    graph: &BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    origin: AreaId,
    destination: AreaId,
) -> Result<(Vec<AreaId>, SimDuration), Error> {
    let mut frontier = BTreeSet::from([(0_u64, Vec::<AreaId>::new(), origin)]);
    let mut visited = BTreeSet::new();
    while let Some((cost, path, area)) = frontier.pop_first() {
        if !visited.insert(area) {
            continue;
        }
        if area == destination {
            return Ok((path, SimDuration(cost)));
        }
        for (to, duration) in graph.get(&area).into_iter().flatten() {
            let mut next = path.clone();
            next.push(*to);
            frontier.insert((
                cost.checked_add(duration.0).ok_or(Error::Overflow)?,
                next,
                *to,
            ));
        }
    }
    Err(Error::NoRoute)
}
