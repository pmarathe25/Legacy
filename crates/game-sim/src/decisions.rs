//! Bounded action templates. Decisions read personal knowledge; physical execution
//! can fail and teach the actor without granting an unseen fact in advance.
use crate::*;
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Personality {
    pub discipline: u16,
    pub ambition: u16,
    pub sociability: u16,
    pub risk_tolerance: u16,
    pub hedonism: u16,
    pub empathy: u16,
    pub aggression: u16,
    pub honesty: u16,
    pub patience: u16,
}

impl Personality {
    pub fn industrious() -> Self {
        Self {
            discipline: 900,
            ambition: 900,
            sociability: 500,
            risk_tolerance: 500,
            hedonism: 100,
            empathy: 500,
            aggression: 100,
            honesty: 800,
            patience: 600,
        }
    }
    pub fn restful() -> Self {
        Self {
            discipline: 100,
            ambition: 100,
            hedonism: 900,
            ..Self::industrious()
        }
    }
    fn valid(&self) -> bool {
        [
            self.discipline,
            self.ambition,
            self.sociability,
            self.risk_tolerance,
            self.hedonism,
            self.empathy,
            self.aggression,
            self.honesty,
            self.patience,
        ]
        .iter()
        .all(|v| *v <= 1_000)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dream {
    Independence,
    Comfort,
    SupportOthers,
    Parenthood,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Goal {
    Survival,
    Recovery,
    Practice,
    Rest,
    Care,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub trust: i32,
    pub affection: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub weight: i32,
    pub experiences: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    pub goal: Goal,
    pub need: i64,
    pub benefit: i64,
    pub value: i64,
    pub total: i64,
    pub blocker: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteKnowledge {
    pub site: SiteId,
    pub area: AreaId,
    pub input: InventoryId,
    pub tool: InventoryId,
    pub input_quantity: Quantity,
    pub output: InventoryId,
    pub access: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildSignal {
    #[serde(default)]
    pub preference: Option<ChildhoodInterest>,
    pub area: AreaId,
    pub urgency: i64,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind {
    #[serde(default)]
    pub interests: BTreeMap<ChildhoodInterest, u64>,
    #[serde(default)]
    pub routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    #[serde(default)]
    pub last_failure: Option<(SimTime, InventoryId)>,
    #[serde(default)]
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
    pub fn autonomy_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::survival_fixture(seed, execution)?;
        sim.state.scenario = scenarios().remove(2);
        let work_concept = ConceptId(allocate(&mut sim.state)?);
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
        sim.state.decisions = Some(World {
            minds,
            concepts: BTreeMap::from([(work_concept, Good::Food)]),
            work_concept,
            last_learning_event: 0,
            switching_margin: 50,
        });
        sim.observe_local_facts();
        Ok(sim)
    }

    fn observe_local_facts(&mut self) {
        if self.state.decisions.is_none() {
            return;
        }
        for p in &self.state.people {
            let Location::AtArea(area) = p.location else {
                continue;
            };
            let actor = &self.state.physical.as_ref().unwrap().actors[&p.id];
            if !actor.vitals.alive || actor.vitals.health == 0 {
                continue;
            }
            let visible_stocks: Vec<_> = self
                .inventory_index
                .get(&InventoryLocation::Area(area))
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
            let mind = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&p.id)
                .unwrap();
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
            mind.child_signals.retain(|_, signal| signal.area != area);
            for child in self.dependent_index.get(&area).into_iter().flatten() {
                let vitals = &self.state.physical.as_ref().unwrap().actors[child].vitals;
                let rules = &self.state.physical.as_ref().unwrap().rules;
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
                        area,
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
                .filter(|s| s.area == area)
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
        self.observe_local_facts();
        self.observe_projects();
        self.observe_property();
        self.review_property_sharing();
        self.review_lineage()?;
        self.review_agreements()?;
        self.review_institutions()?;
        self.review_projects()?;
        self.review_conception()?;
        let world = self.state.decisions.as_ref().unwrap();
        let evaluate =
            |(id, mind): (&PersonId, &Mind)| self.decide(*id, mind, world.switching_margin);
        let mut intentions: Vec<_> = if self.execution.workers == 1 {
            world.minds.iter().map(evaluate).collect()
        } else {
            self.pool
                .install(|| world.minds.par_iter().map(evaluate).collect())
        };
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
            let accepted = if let Command::Move { destination } = command {
                self.start_movement(i.actor, destination)
            } else {
                self.debug_physical_command(i.actor, command.clone())
            };
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
        Ok(())
    }

    fn decide(&self, id: PersonId, mind: &Mind, margin: i64) -> Intention {
        let p = &self.state.people[self.person_index[&id]];
        let physical = self.state.physical.as_ref().unwrap();
        let actor = &physical.actors[&id];
        let none = || Intention {
            actor: id,
            command: None,
            explanation: mind.explanation.clone(),
        };
        if self.in_combat(id)
            || self.lineage_handles(id)
            || id == self.state.ruler
            || !actor.vitals.alive
            || actor.vitals.health == 0
        {
            return none();
        }
        if !self.independent(id) {
            return none();
        }
        let Location::AtArea(area) = p.location else {
            return none();
        };
        let urgent = actor.vitals.hunger >= physical.rules.meal_relief;
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
            for fact in mind.knowledge.values().filter(|f| {
                f.good == Good::Food && f.accessible && f.quantity >= physical.rules.meal_size
            }) {
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
                let survival_time = physical
                    .rules
                    .hunger_harm
                    .saturating_sub(actor.vitals.hunger)
                    .saturating_add(actor.vitals.health);
                if travel.saturating_add(physical.rules.eating_time.0) > survival_time {
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
            signal.area == area
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
                    && f.quantity >= physical.rules.meal_size
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
            + (physical.rules.max_health - actor.vitals.health) as i64 / 100;
        add(
            Command::Rest,
            Goal::Recovery,
            recovery,
            i64::from(mind.personality.hedonism) / 3,
            0,
        );
        if self.state.time >= mind.retry_after
            && !urgent
            && actor.vitals.fatigue < physical.rules.fatigue_limit
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
        candidates.sort_by_key(|(command, explanation)| {
            (
                std::cmp::Reverse(explanation.total),
                match command {
                    Command::Eat { source } => (0, source.0),
                    Command::Move { destination } => (1, destination.0),
                    Command::Work { site } => (2, site.0),
                    _ => (3, 0),
                },
            )
        });
        let Some((command, explanation)) = candidates.into_iter().next() else {
            return none();
        };
        if command == Command::Rest && actor.activity == Activity::Rest {
            return none();
        }
        if matches!(actor.activity, Activity::Work { .. })
            && explanation.total < mind.explanation.total + margin
            && !urgent
        {
            return none();
        }
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
