//! Control follows the first actual eligible birth, without transferring an estate
//! or political office. Guardians sense that person's location and travel normally.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incarnation {
    pub person: PersonId,
    pub bound_at: SimTime,
    pub died_at: Option<SimTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub predecessor: PersonId,
    pub died_at: SimTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianGoal {
    Waiting,
    Searching,
    Mentoring,
    Confined,
    Recovering,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentorshipRules {
    pub lesson_duration: SimDuration,
    pub lesson_gain: u64,
    pub invocation_access_threshold: u64,
}
impl Default for MentorshipRules {
    fn default() -> Self {
        Self {
            lesson_duration: MINUTE,
            lesson_gain: 100,
            invocation_access_threshold: 200,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assessment {
    pub independence: u64,
    pub competence: u64,
    pub judgment: u64,
    pub risks: u64,
    pub observed_stage: Option<LifeStage>,
    pub observed_at: SimTime,
    pub ready: bool,
    pub explanation: String,
}
impl Default for Assessment {
    fn default() -> Self {
        Self {
            independence: 0,
            competence: 0,
            judgment: 0,
            risks: 0,
            observed_stage: None,
            observed_at: SimTime(0),
            ready: false,
            explanation: "No independent conduct has been observed".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub request: GuardianRequest,
    pub accepted_at: SimTime,
}

fn knows_invocation() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guardian {
    pub target: Option<PersonId>,
    /// This connection conveys location only. It grants no stock, permissions,
    /// relationships, private intentions, or knowledge to other people.
    pub sensed_location: Option<Location>,
    pub sensed_at: SimTime,
    pub goal: GuardianGoal,
    pub explanation: String,
    #[serde(default)]
    pub assessments: BTreeMap<PersonId, Assessment>,
    #[serde(default)]
    pub assignment: Option<Assignment>,
    #[serde(default = "knows_invocation")]
    pub knows_invocation: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub incarnations: Vec<Incarnation>,
    pub waiting: Option<Waiting>,
    pub guardians: BTreeMap<PersonId, Guardian>,
    #[serde(default)]
    pub rules: MentorshipRules,
    #[serde(default)]
    pub invocation_learning: BTreeMap<PersonId, u64>,
}

impl World {
    pub fn current(&self) -> Option<PersonId> {
        self.incarnations
            .last()
            .filter(|life| life.died_at.is_none())
            .map(|life| life.person)
    }
}

impl Simulation {
    pub fn lineage_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.state.scenario = scenarios()
            .into_iter()
            .find(|s| s.id == "lineage-fixture")
            .ok_or_else(|| Error::InvalidFixture("lineage fixture is not registered".into()))?;
        let ruler = sim.state.ruler;
        let location = sim.state.people[sim.person_index[&ruler]].location.clone();
        sim.state.lineage = Some(World {
            rules: MentorshipRules::default(),
            invocation_learning: BTreeMap::new(),
            incarnations: vec![Incarnation {
                person: ruler,
                bound_at: sim.state.time,
                died_at: None,
            }],
            waiting: None,
            guardians: sim
                .state
                .people
                .iter()
                .filter(|p| p.role == Role::Guardian)
                .map(|p| {
                    (
                        p.id,
                        Guardian {
                            assessments: BTreeMap::new(),
                            assignment: None,
                            knows_invocation: true,
                            target: Some(ruler),
                            sensed_location: Some(location.clone()),
                            sensed_at: sim.state.time,
                            goal: GuardianGoal::Mentoring,
                            explanation:
                                "Assisting the current incarnation; readiness remains unassessed"
                                    .into(),
                        },
                    )
                })
                .collect(),
        });
        Ok(sim)
    }

    pub(crate) fn observe_lineage_deaths(&mut self) {
        let Some(current) = self.state.lineage.as_ref().and_then(World::current) else {
            return;
        };
        if self.state.physical.as_ref().unwrap().actors[&current]
            .vitals
            .alive
        {
            return;
        }
        let constraints: BTreeMap<_, _> = self
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .map(|id| (*id, self.guardian_constraint(*id)))
            .collect();
        let world = self.state.lineage.as_mut().unwrap();
        world.incarnations.last_mut().unwrap().died_at = Some(self.state.time);
        world.waiting = Some(Waiting {
            predecessor: current,
            died_at: self.state.time,
        });
        for (id, guardian) in &mut world.guardians {
            guardian.assignment = None;
            guardian.target = None;
            guardian.sensed_location = None;
            guardian.sensed_at = self.state.time;
            guardian.goal = constraints[id].unwrap_or(GuardianGoal::Waiting);
            guardian.explanation =
                "No living incarnation; waiting for an actual post-death birth".into();
        }
    }

    /// Called only after an actual birth has created the person and body. Birth
    /// deadlines are processed in persistent pregnancy order, so the first
    /// simultaneous eligible birth is deterministic across workers and saves.
    pub(crate) fn record_lineage_birth(&mut self, child: PersonId) {
        let Some(waiting) = self
            .state
            .lineage
            .as_ref()
            .and_then(|world| world.waiting.clone())
        else {
            return;
        };
        let Some(life) = self
            .state
            .development
            .as_ref()
            .and_then(|world| world.lives.get(&child))
        else {
            return;
        };
        if life.born != self.state.time
            || life.born <= waiting.died_at
            || life.initial_age != SimDuration(0)
            || life.parents.len() != 2
            || !self.state.physical.as_ref().unwrap().actors[&child]
                .vitals
                .alive
        {
            return;
        }
        let location = self.state.people[self.person_index[&child]]
            .location
            .clone();
        let constraints: BTreeMap<_, _> = self
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .map(|id| (*id, self.guardian_constraint(*id)))
            .collect();
        // Role::Ruler is the existing character-control tag. Neither this change
        // nor the lineage record grants claims, permissions, goods, or an office.
        self.state.people[self.person_index[&waiting.predecessor]].role = Role::Citizen;
        self.state.people[self.person_index[&child]].role = Role::Ruler;
        self.state.ruler = child;
        let world = self.state.lineage.as_mut().unwrap();
        world.incarnations.push(Incarnation {
            person: child,
            bound_at: self.state.time,
            died_at: None,
        });
        world.waiting = None;
        for (id, guardian) in &mut world.guardians {
            guardian.target = Some(child);
            guardian.sensed_location = Some(location.clone());
            guardian.sensed_at = self.state.time;
            guardian.goal = constraints[id].unwrap_or(GuardianGoal::Searching);
            guardian.explanation =
                "The innate connection locates the newborn; reaching them requires travel".into();
        }
    }

    pub(crate) fn lineage_handles(&self, person: PersonId) -> bool {
        self.state
            .lineage
            .as_ref()
            .is_some_and(|world| world.guardians.contains_key(&person))
    }

    fn guardian_constraint(&self, id: PersonId) -> Option<GuardianGoal> {
        if self
            .state
            .combat
            .as_ref()
            .is_some_and(|world| world.captivity.contains_key(&id))
        {
            Some(GuardianGoal::Confined)
        } else if self.state.physical.as_ref().unwrap().actors[&id]
            .vitals
            .health
            == 0
            || self.combat_restricts(id)
        {
            Some(GuardianGoal::Recovering)
        } else {
            None
        }
    }

    /// Called at the event's physical commit, not from a later query. A guardian
    /// cannot acquire evidence merely because they eventually visit the area.
    pub(crate) fn witness_lineage_event(&mut self, kind: &EventKind) {
        let Some(current) = self.state.lineage.as_ref().and_then(World::current) else {
            return;
        };
        let (actor, departure) = match kind {
            EventKind::Practiced { actor, .. }
            | EventKind::Ate { actor, .. }
            | EventKind::Produced { actor, .. }
            | EventKind::Expressed { actor, .. }
            | EventKind::Arrived { actor, .. } => (*actor, None),
            EventKind::Departed { actor, from, .. } => (*actor, Some(*from)),
            EventKind::Cared { caregiver, .. } => (*caregiver, None),
            EventKind::LessonCompleted { student, .. } => (*student, None),
            _ => return,
        };
        if actor != current {
            return;
        }
        let area =
            departure.or_else(
                || match self.state.people[self.person_index[&actor]].location {
                    Location::AtArea(area) => Some(area),
                    Location::InTransit { .. } => None,
                },
            );
        let Some(area) = area else { return };
        let observers: Vec<_> = self
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .copied()
            .filter(|id| {
                self.state.people[self.person_index[id]].location == Location::AtArea(area)
                    && self.state.physical.as_ref().unwrap().actors[id]
                        .vitals
                        .health
                        > 0
            })
            .collect();
        for guardian in observers {
            let known_distress = self.state.decisions.as_ref().unwrap().minds[&guardian]
                .child_signals
                .get(&current)
                .is_some_and(|signal| signal.urgency > 0);
            let (independence, competence, judgment, risks) = match kind {
                EventKind::Practiced { gain, .. } if *gain > 0 => (0, (*gain).min(100), 25, 0),
                EventKind::Ate { .. } => (100, 0, 50, 0),
                EventKind::Produced { .. } => (100, 100, 50, 0),
                EventKind::Departed { .. } if known_distress => (100, 0, 0, 100),
                EventKind::Departed { .. } => (100, 0, 50, 0),
                EventKind::Arrived { .. } => (50, 0, 25, 0),
                EventKind::Cared { .. } => (50, 50, 100, 0),
                EventKind::Expressed { .. } => (0, 0, 10, 0),
                EventKind::LessonCompleted { gain, .. } if *gain > 0 => {
                    (0, (*gain).min(100), 50, 0)
                }
                _ => continue,
            };
            let stage = self.state.development.as_ref().unwrap().lives[&current].stage;
            let personality = self.state.decisions.as_ref().unwrap().minds[&guardian]
                .personality
                .clone();
            let assessment = self
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&guardian)
                .unwrap()
                .assessments
                .entry(current)
                .or_default();
            assessment.independence = (assessment.independence + independence).min(1_000);
            assessment.competence = (assessment.competence + competence).min(1_000);
            assessment.judgment = (assessment.judgment + judgment).min(1_000);
            assessment.risks = (assessment.risks + risks).min(1_000);
            assessment.observed_at = self.state.time;
            assessment.observed_stage = Some(stage);
            judge_readiness(assessment, &personality);
        }
    }

    pub(crate) fn start_lesson(
        &mut self,
        student: PersonId,
        teacher: PersonId,
        topic: LessonTopic,
    ) -> Result<(), Error> {
        let world = self.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        if world.current() != Some(student)
            || !world.guardians.contains_key(&teacher)
            || student == teacher
            || !self.independent(student)
            || self.in_combat(student)
            || self.in_combat(teacher)
            || self.combat_restricts(student)
            || self.combat_restricts(teacher)
            || self.care_index.contains_key(&student)
            || self.care_index.contains_key(&teacher)
        {
            return Err(Error::Unavailable);
        }
        let student_location = &self.state.people[self.person_index[&student]].location;
        let teacher_location = &self.state.people[self.person_index[&teacher]].location;
        if student_location != teacher_location || !matches!(student_location, Location::AtArea(_))
        {
            return Err(Error::Unavailable);
        }
        let bodies = &self.state.physical.as_ref().unwrap().actors;
        if [student, teacher].into_iter().any(|id| {
            !bodies[&id].vitals.alive
                || bodies[&id].vitals.health == 0
                || !matches!(bodies[&id].activity, Activity::Idle | Activity::Rest)
        }) {
            return Err(Error::Unavailable);
        }
        let teacher_mind = &self.state.decisions.as_ref().unwrap().minds[&teacher];
        let knows = match topic {
            LessonTopic::Invocation => world.guardians[&teacher].knows_invocation,
            LessonTopic::Craft => teacher_mind
                .interests
                .get(&ChildhoodInterest::Craft)
                .is_some_and(|skill| *skill > 0),
            LessonTopic::Travel => !teacher_mind.routes.is_empty(),
        };
        let unsafe_now = teacher_mind
            .child_signals
            .get(&student)
            .is_some_and(|s| s.urgency >= 20_000);
        if !knows || unsafe_now {
            self.ensure_event_capacity(1)?;
            let reason = if !knows {
                "I cannot teach knowledge I do not have"
            } else {
                "The pupil needs immediate physical care before a lesson"
            };
            self.emit(EventKind::GuardianRefused {
                guardian: teacher,
                requester: student,
                reason: reason.into(),
            });
            self.state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&teacher)
                .unwrap()
                .explanation = reason.into();
            self.revision += 1;
            return Ok(());
        }
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(world.rules.lesson_duration.0)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_physical(student);
        self.interrupt_physical(teacher);
        self.state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&teacher)
            .unwrap()
            .assignment = None;
        self.state.lineage.as_mut().unwrap().guardians.get_mut(&teacher).unwrap().explanation =
            "The guided lesson is underway; both pupil and teacher are occupied until it completes or is interrupted".into();
        for actor in [student, teacher] {
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .activity = Activity::Lesson {
                teacher,
                student,
                topic,
                completes,
            };
            self.state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(actor));
        }
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn available_lesson_commands(&self, student: PersonId) -> Vec<Command> {
        let Some(world) = &self.state.lineage else {
            return Vec::new();
        };
        if world.current() != Some(student)
            || !self.independent(student)
            || self.in_combat(student)
            || self.combat_restricts(student)
            || self.care_index.contains_key(&student)
        {
            return Vec::new();
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&student];
        let location = &self.state.people[self.person_index[&student]].location;
        if !body.vitals.alive
            || body.vitals.health == 0
            || !matches!(body.activity, Activity::Idle | Activity::Rest)
            || !matches!(location, Location::AtArea(_))
        {
            return Vec::new();
        }
        let mut commands = Vec::new();
        for (teacher, guard) in &world.guardians {
            let teacher_body = &self.state.physical.as_ref().unwrap().actors[teacher];
            if &self.state.people[self.person_index[teacher]].location != location
                || self.in_combat(*teacher)
                || self.combat_restricts(*teacher)
                || self.care_index.contains_key(teacher)
                || !matches!(teacher_body.activity, Activity::Idle | Activity::Rest)
            {
                continue;
            }
            let mind = &self.state.decisions.as_ref().unwrap().minds[teacher];
            if !mind
                .child_signals
                .get(&student)
                .is_some_and(|signal| signal.urgency >= 20_000)
            {
                for topic in [
                    LessonTopic::Invocation,
                    LessonTopic::Craft,
                    LessonTopic::Travel,
                ] {
                    let can_teach = match topic {
                        LessonTopic::Invocation => guard.knows_invocation,
                        LessonTopic::Craft => mind
                            .interests
                            .get(&ChildhoodInterest::Craft)
                            .is_some_and(|skill| *skill > 0),
                        LessonTopic::Travel => !mind.routes.is_empty(),
                    };
                    if can_teach {
                        commands.push(Command::Learn {
                            teacher: *teacher,
                            topic,
                        });
                    }
                }
            }
            commands.push(Command::RequestGuardian {
                guardian: *teacher,
                request: GuardianRequest::Rest,
            });
            if guard
                .assessments
                .get(&student)
                .is_some_and(|assessment| assessment.ready)
            {
                let Location::AtArea(area) = location else {
                    unreachable!()
                };
                if !mind
                    .child_signals
                    .get(&student)
                    .is_some_and(|signal| signal.urgency > 0)
                {
                    for destination in
                        self.state
                            .areas
                            .iter()
                            .map(|area| area.id)
                            .filter(|destination| {
                                destination != area
                                    && decisions::known_path(&mind.routes, *area, *destination)
                                        .is_ok()
                            })
                    {
                        commands.push(Command::RequestGuardian {
                            guardian: *teacher,
                            request: GuardianRequest::Move { destination },
                        });
                    }
                }
                for site in mind
                    .sites
                    .values()
                    .filter(|site| site.area == *area && site.access)
                {
                    commands.push(Command::RequestGuardian {
                        guardian: *teacher,
                        request: GuardianRequest::Work { site: site.site },
                    });
                }
            }
        }
        commands
    }

    pub(crate) fn interrupt_lesson(&mut self, actor: PersonId) {
        let Some(Activity::Lesson {
            teacher, student, ..
        }) = self
            .state
            .physical
            .as_ref()
            .and_then(|w| w.actors.get(&actor))
            .map(|a| a.activity.clone())
        else {
            return;
        };
        for id in [teacher, student] {
            if let Some(body) = self.state.physical.as_mut().unwrap().actors.get_mut(&id)
                && matches!(body.activity, Activity::Lesson { .. })
            {
                body.activity = Activity::Idle;
            }
            for due in self.state.due.values_mut() {
                due.retain(|event| !matches!(event, Due::Physical(person) if *person == id));
            }
        }
        self.state.due.retain(|_, events| !events.is_empty());
    }

    pub(crate) fn complete_lesson(&mut self, actor: PersonId) {
        let Activity::Lesson {
            teacher,
            student,
            topic,
            completes,
        } = self.state.physical.as_ref().unwrap().actors[&actor].activity
        else {
            return;
        };
        let paired = self.state.physical.as_ref().unwrap().actors[&teacher].activity
            == self.state.physical.as_ref().unwrap().actors[&student].activity;
        if !paired
            || completes != self.state.time
            || self.state.people[self.person_index[&teacher]].location
                != self.state.people[self.person_index[&student]].location
            || [teacher, student].into_iter().any(|id| {
                !self.state.physical.as_ref().unwrap().actors[&id]
                    .vitals
                    .alive
            })
        {
            self.interrupt_lesson(actor);
            return;
        }
        let lesson_gain = self.state.lineage.as_ref().unwrap().rules.lesson_gain;
        let gain = match topic {
            LessonTopic::Invocation => {
                let learning = self
                    .state
                    .lineage
                    .as_mut()
                    .unwrap()
                    .invocation_learning
                    .entry(student)
                    .or_default();
                let gain = lesson_gain.min(1_000 - *learning);
                *learning += gain;
                gain
            }
            LessonTopic::Craft => {
                let knowledge = self.state.decisions.as_ref().unwrap().minds[&teacher]
                    .interests
                    .get(&ChildhoodInterest::Craft)
                    .copied()
                    .unwrap_or(0);
                let learning = self
                    .state
                    .decisions
                    .as_mut()
                    .unwrap()
                    .minds
                    .get_mut(&student)
                    .unwrap()
                    .interests
                    .entry(ChildhoodInterest::Craft)
                    .or_default();
                let gain = lesson_gain.min(knowledge.saturating_sub(*learning));
                *learning += gain;
                gain
            }
            LessonTopic::Travel => {
                let known = self.state.decisions.as_ref().unwrap().minds[&teacher]
                    .routes
                    .clone();
                let student_routes = &mut self
                    .state
                    .decisions
                    .as_mut()
                    .unwrap()
                    .minds
                    .get_mut(&student)
                    .unwrap()
                    .routes;
                let mut new_links = 0_u64;
                for (area, links) in known {
                    let own = student_routes.entry(area).or_default();
                    for (destination, duration) in links {
                        if own.insert(destination, duration).is_none() {
                            new_links += 1;
                        }
                    }
                }
                new_links.min(lesson_gain)
            }
        };
        self.interrupt_lesson(actor);
        self.emit(EventKind::LessonCompleted {
            teacher,
            student,
            topic,
            gain,
        });
    }

    pub(crate) fn request_guardian(
        &mut self,
        requester: PersonId,
        guardian: PersonId,
        request: GuardianRequest,
    ) -> Result<(), Error> {
        let world = self.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        if world.current() != Some(requester)
            || !world.guardians.contains_key(&guardian)
            || !self.independent(requester)
            || self.combat_restricts(requester)
            || self.in_combat(requester)
            || self.care_index.contains_key(&requester)
            || !matches!(
                self.state.physical.as_ref().unwrap().actors[&requester].activity,
                Activity::Idle | Activity::Rest
            )
            || !self.state.physical.as_ref().unwrap().actors[&requester]
                .vitals
                .alive
            || self.state.people[self.person_index[&requester]].location
                != self.state.people[self.person_index[&guardian]].location
            || !matches!(
                self.state.people[self.person_index[&requester]].location,
                Location::AtArea(_)
            )
        {
            return Err(Error::Unavailable);
        }
        let guard = &world.guardians[&guardian];
        let ready = guard.assessments.get(&requester).is_some_and(|a| a.ready);
        let body = &self.state.physical.as_ref().unwrap().actors[&guardian];
        let command = match request {
            GuardianRequest::Move { destination } => Command::Move { destination },
            GuardianRequest::Rest => Command::Rest,
            GuardianRequest::Work { site } => Command::Work { site },
        };
        let reason = if self.guardian_constraint(guardian).is_some() || self.in_combat(guardian) {
            Some("My body or restraints prevent that assistance")
        } else if !matches!(body.activity, Activity::Idle | Activity::Rest) {
            Some("I must finish my current physical duty first")
        } else if !ready && !matches!(request, GuardianRequest::Rest) {
            Some(
                "I have not yet observed enough independence, competence, and judgment to follow that order",
            )
        } else if matches!(request, GuardianRequest::Move { .. })
            && self.state.decisions.as_ref().unwrap().minds[&guardian]
                .child_signals
                .get(&requester)
                .is_some_and(|s| s.urgency > 0)
        {
            Some("I will not leave while I can see that you need immediate care")
        } else {
            None
        };
        self.ensure_event_capacity(1)?;
        if let Some(reason) = reason {
            self.emit(EventKind::GuardianRefused {
                guardian,
                requester,
                reason: reason.into(),
            });
            self.state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&guardian)
                .unwrap()
                .explanation = reason.into();
            self.revision += 1;
            return Ok(());
        }
        let accepted = if let Command::Move { destination } = command {
            self.start_movement(guardian, destination)
        } else {
            self.debug_physical_command(guardian, command)
        };
        match accepted {
            Ok(()) => {
                let guard = self
                    .state
                    .lineage
                    .as_mut()
                    .unwrap()
                    .guardians
                    .get_mut(&guardian)
                    .unwrap();
                guard.assignment = Some(Assignment {
                    request,
                    accepted_at: self.state.time,
                });
                guard.explanation =
                    "I accept this physical task while retaining my judgment and lineage duty"
                        .into();
                Ok(())
            }
            Err(Error::Overflow) => Err(Error::Overflow),
            Err(_) => {
                self.emit(EventKind::GuardianRefused { guardian, requester,
                    reason: "I cannot carry this out with my knowledge, permissions, or physical resources".into() });
                self.state.lineage.as_mut().unwrap().guardians.get_mut(&guardian).unwrap().explanation =
                    "The task exceeds my known routes, permissions, or physical resources; I remain committed to the lineage".into();
                self.revision += 1;
                Ok(())
            }
        }
    }

    pub(crate) fn review_lineage(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.lineage else {
            return Ok(());
        };
        let current = world.current();
        let guardians: Vec<_> = world.guardians.keys().copied().collect();
        for id in guardians {
            if let Some(person) = current {
                let same_place = self.state.people[self.person_index[&id]].location
                    == self.state.people[self.person_index[&person]].location
                    && matches!(
                        self.state.people[self.person_index[&id]].location,
                        Location::AtArea(_)
                    )
                    && self.state.physical.as_ref().unwrap().actors[&id]
                        .vitals
                        .health
                        > 0;
                if same_place {
                    let personality = self.state.decisions.as_ref().unwrap().minds[&id]
                        .personality
                        .clone();
                    let stage = self.state.development.as_ref().unwrap().lives[&person].stage;
                    let assessment = self
                        .state
                        .lineage
                        .as_mut()
                        .unwrap()
                        .guardians
                        .get_mut(&id)
                        .unwrap()
                        .assessments
                        .entry(person)
                        .or_default();
                    assessment.observed_stage = Some(stage);
                    assessment.observed_at = self.state.time;
                    judge_readiness(assessment, &personality);
                }
            }
            let sensed = current.map(|person| {
                self.state.people[self.person_index[&person]]
                    .location
                    .clone()
            });
            let body = &self.state.physical.as_ref().unwrap().actors[&id];
            let own_location = self.state.people[self.person_index[&id]].location.clone();
            let pending = self.state.lineage.as_ref().unwrap().guardians[&id]
                .assignment
                .as_ref()
                .is_some_and(|assignment| match assignment.request {
                    GuardianRequest::Move { destination } => {
                        matches!(own_location, Location::InTransit { .. })
                            || matches!(own_location, Location::AtArea(area) if area != destination)
                    }
                    GuardianRequest::Work { site } => {
                        matches!(body.activity, Activity::Work { site: active } if active == site)
                    }
                    GuardianRequest::Rest => self.state.time <= assignment.accepted_at,
                });
            let (goal, explanation, command) = if self
                .state
                .combat
                .as_ref()
                .is_some_and(|w| w.captivity.contains_key(&id))
            {
                (
                    GuardianGoal::Confined,
                    "The body is confined; the connection does not release its restraints".into(),
                    None,
                )
            } else if body.vitals.health == 0 || self.combat_restricts(id) {
                (
                    GuardianGoal::Recovering,
                    "Recovering in place before resuming the lineage duty".into(),
                    None,
                )
            } else if self.in_combat(id) {
                (
                    GuardianGoal::Searching,
                    "The encounter must be resolved before the guardian can continue".into(),
                    None,
                )
            } else if pending {
                (
                    GuardianGoal::Mentoring,
                    "Carrying out the accepted physical task before reconsidering lineage assistance".into(),
                    None,
                )
            } else if let (Some(target), Some(target_location)) = (current, &sensed) {
                self.guardian_intention(id, target, &own_location, target_location)
            } else {
                (
                    GuardianGoal::Waiting,
                    "No living incarnation; other people and births continue normally".into(),
                    matches!(body.activity, Activity::Idle | Activity::Work { .. })
                        .then_some(Command::Rest),
                )
            };
            let guardian = self
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&id)
                .unwrap();
            guardian.target = current;
            guardian.sensed_location = sensed;
            guardian.sensed_at = self.state.time;
            guardian.goal = goal;
            guardian.explanation = explanation;
            if !pending {
                guardian.assignment = None;
            }
            if let Some(command) = command {
                let accepted = if let Command::Move { destination } = command {
                    self.start_movement(id, destination)
                } else {
                    self.debug_physical_command(id, command)
                };
                if accepted.is_err() {
                    self.state
                        .lineage
                        .as_mut()
                        .unwrap()
                        .guardians
                        .get_mut(&id)
                        .unwrap()
                        .explanation = "A physical attempt failed; the guardian will reconsider without abandoning the lineage".into();
                }
            }
        }
        Ok(())
    }

    fn guardian_intention(
        &self,
        guardian: PersonId,
        target: PersonId,
        own_location: &Location,
        target_location: &Location,
    ) -> (GuardianGoal, String, Option<Command>) {
        let body = &self.state.physical.as_ref().unwrap().actors[&guardian];
        if matches!(own_location, Location::InTransit { .. }) {
            return (
                GuardianGoal::Searching,
                "Traveling along the current physical route toward the sensed incarnation".into(),
                None,
            );
        }
        if own_location != target_location {
            let destination = match target_location {
                Location::AtArea(area) => *area,
                Location::InTransit { to, .. } => *to,
            };
            return (
                GuardianGoal::Searching,
                "The innate connection gives a destination; a known traversable route is still needed".into(),
                Some(Command::Move { destination }),
            );
        }
        if !matches!(
            body.activity,
            Activity::Idle | Activity::Rest | Activity::Work { .. }
        ) {
            return (
                GuardianGoal::Mentoring,
                "Completing the current physical assistance before reconsidering".into(),
                None,
            );
        }
        let Location::AtArea(area) = own_location else {
            unreachable!()
        };
        let mind = &self.state.decisions.as_ref().unwrap().minds[&guardian];
        // Needs and food access come from ordinary local observations. The
        // lineage connection does not reveal private body needs or distant food.
        if let Some(signal) = mind
            .child_signals
            .get(&target)
            .filter(|s| s.area == *area && s.observed_at == self.state.time)
        {
            if signal.urgency > 0 {
                if let Some(food) = mind.knowledge.values().find(|fact| {
                    fact.good == Good::Food
                        && fact.area == *area
                        && fact.accessible
                        && fact.quantity >= self.state.physical.as_ref().unwrap().rules.meal_size
                }) {
                    return (
                        GuardianGoal::Mentoring,
                        "Observed distress motivates care using food the guardian knows and can access".into(),
                        Some(Command::Care { child: target, source: food.inventory }),
                    );
                }
                return (
                    GuardianGoal::Mentoring,
                    "The child needs food, but no usable known supply is available here".into(),
                    None,
                );
            }
            if signal.preference == Some(ChildhoodInterest::Company) {
                return (
                    GuardianGoal::Mentoring,
                    "Responding to the child's observed request for company".into(),
                    Some(Command::Comfort { child: target }),
                );
            }
        }
        (
            GuardianGoal::Mentoring,
            if self.state.lineage.as_ref().unwrap().guardians[&guardian]
                .assessments
                .get(&target)
                .is_some_and(|assessment| assessment.ready)
            {
                "Present with the incarnation; observed readiness permits requests, which I still judge individually".into()
            } else {
                "Present with the incarnation; further observed competence and independence are needed before I follow orders".into()
            },
            matches!(body.activity, Activity::Idle | Activity::Work { .. })
                .then_some(Command::Rest),
        )
    }
}

fn judge_readiness(assessment: &mut Assessment, personality: &decisions::Personality) {
    let threshold =
        250 + u64::from(personality.discipline) / 5 + u64::from(personality.patience) / 10
            - u64::from(personality.risk_tolerance) / 5;
    let risk_weight = 1_000 - u64::from(personality.risk_tolerance);
    let score = (assessment.independence + assessment.competence + assessment.judgment)
        .saturating_sub(assessment.risks * risk_weight / 1_000);
    assessment.ready = assessment
        .observed_stage
        .is_some_and(|stage| stage != LifeStage::Infant)
        && assessment.independence >= 100
        && assessment.competence >= 100
        && assessment.judgment >= 75
        && score >= threshold;
    assessment.explanation = format!(
        "{}; witnessed independence {}, competence {}, judgment {}, risk {}. My required evidence score is {}, observed {}",
        if assessment.ready {
            "Ready for requests that still use my own judgment"
        } else {
            "More observed independence, competence, or judgment is needed"
        },
        assessment.independence,
        assessment.competence,
        assessment.judgment,
        assessment.risks,
        threshold,
        score,
    );
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.lineage else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent lineage state".into());
    let Some(physical) = &state.physical else {
        return Err(fail());
    };
    let Some(development) = &state.development else {
        return Err(fail());
    };
    if state.decisions.is_none()
        || world.incarnations.is_empty()
        || world.rules.lesson_duration.0 == 0
        || !(1..=1_000).contains(&world.rules.lesson_gain)
        || !(1..=1_000).contains(&world.rules.invocation_access_threshold)
        || world
            .invocation_learning
            .iter()
            .any(|(id, value)| !physical.actors.contains_key(id) || *value > 1_000)
    {
        return Err(fail());
    }
    for (actor, body) in &physical.actors {
        if let Activity::Lesson {
            teacher,
            student,
            topic,
            completes,
        } = body.activity
        {
            if ![*actor, teacher, student]
                .into_iter()
                .all(|id| physical.actors.contains_key(&id))
                || ![teacher, student].contains(actor)
                || teacher == student
                || !world.guardians.contains_key(&teacher)
                || world.current() != Some(student)
                || physical.actors[&teacher].activity != physical.actors[&student].activity
                || state
                    .people
                    .iter()
                    .find(|p| p.id == teacher)
                    .map(|p| &p.location)
                    != state
                        .people
                        .iter()
                        .find(|p| p.id == student)
                        .map(|p| &p.location)
            {
                return Err(fail());
            }
            let started = completes
                .0
                .checked_sub(world.rules.lesson_duration.0)
                .ok_or_else(fail)?;
            let life = &development.lives[&student];
            let age = started
                .checked_sub(life.born.0)
                .and_then(|v| v.checked_add(life.initial_age.0))
                .ok_or_else(fail)?;
            if started > state.time.0
                || age < development.rules.infancy.0
                || !state.due.get(&completes).is_some_and(|due| {
                    [teacher, student].into_iter().all(|person| {
                        due.iter()
                            .any(|event| matches!(event, Due::Physical(id) if *id == person))
                    })
                })
                || matches!(topic, LessonTopic::Invocation)
                    && !world.guardians[&teacher].knows_invocation
            {
                return Err(fail());
            }
        }
    }
    let mut seen = BTreeSet::new();
    for (i, incarnation) in world.incarnations.iter().enumerate() {
        let life = development
            .lives
            .get(&incarnation.person)
            .ok_or_else(fail)?;
        let body = physical.actors.get(&incarnation.person).ok_or_else(fail)?;
        if !seen.insert(incarnation.person)
            || incarnation.bound_at > state.time
            || incarnation
                .died_at
                .is_some_and(|d| d < incarnation.bound_at || d > state.time)
            || incarnation.died_at.is_some() == body.vitals.alive
            || i + 1 < world.incarnations.len() && incarnation.died_at.is_none()
        {
            return Err(fail());
        }
        if let Some(died) = incarnation.died_at && !state.events.iter().any(|event| {
            event.time == died
                && matches!(event.kind, EventKind::Died { actor } if actor == incarnation.person)
        }) {
            return Err(fail());
        }
        if i > 0 {
            let previous_death = world.incarnations[i - 1].died_at.ok_or_else(fail)?;
            let first_eligible = development
                .lives
                .values()
                .filter(|candidate| {
                    candidate.born > previous_death
                        && candidate.initial_age == SimDuration(0)
                        && candidate.parents.len() == 2
                })
                .min_by_key(|candidate| (candidate.born, candidate.id))
                .map(|candidate| candidate.id);
            if life.born != incarnation.bound_at
                || life.born <= previous_death
                || life.initial_age != SimDuration(0)
                || life.parents.len() != 2
                || first_eligible != Some(incarnation.person)
            {
                return Err(fail());
            }
        }
    }
    let last = world.incarnations.last().unwrap();
    if last.person != state.ruler {
        return Err(fail());
    }
    match (&world.waiting, last.died_at) {
        (Some(waiting), Some(died))
            if waiting.predecessor == last.person && waiting.died_at == died => {}
        (None, None) => {}
        _ => return Err(fail()),
    }
    let expected_guardians: BTreeSet<_> = state
        .people
        .iter()
        .filter(|p| p.role == Role::Guardian)
        .map(|p| p.id)
        .collect();
    if world.guardians.keys().copied().collect::<BTreeSet<_>>() != expected_guardians {
        return Err(fail());
    }
    for guardian in world.guardians.values() {
        if guardian.target != world.current()
            || guardian.sensed_at > state.time
            || guardian.target.is_some() != guardian.sensed_location.is_some()
        {
            return Err(fail());
        }
        for (person, assessment) in &guardian.assessments {
            if !seen.contains(person)
                || assessment.observed_at > state.time
                || [
                    assessment.independence,
                    assessment.competence,
                    assessment.judgment,
                    assessment.risks,
                ]
                .into_iter()
                .any(|value| value > 1_000)
                || assessment.ready
                    && (assessment.independence < 100
                        || assessment.competence < 100
                        || assessment.judgment < 75
                        || assessment.observed_stage == Some(LifeStage::Infant))
            {
                return Err(fail());
            }
        }
        if let Some(assignment) = &guardian.assignment
            && (assignment.accepted_at > state.time
                || world.current().is_none()
                || match assignment.request {
                    GuardianRequest::Move { destination } => {
                        !state.areas.iter().any(|a| a.id == destination)
                    }
                    GuardianRequest::Work { site } => !physical.sites.contains_key(&site),
                    GuardianRequest::Rest => false,
                })
        {
            return Err(fail());
        }
        if let Some(location) = &guardian.sensed_location {
            let known_area = |id| state.areas.iter().any(|area| area.id == id);
            match location {
                Location::AtArea(area) if !known_area(*area) => return Err(fail()),
                Location::InTransit {
                    from,
                    to,
                    departed,
                    arrives,
                } if !known_area(*from)
                    || !known_area(*to)
                    || departed >= arrives
                    || *departed > guardian.sensed_at =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
    }
    Ok(())
}
