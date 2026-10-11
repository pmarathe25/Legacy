//! Control follows the first actual eligible birth, without transferring an estate
//! or political office. Guardians sense that person's location and travel normally.
use crate::*;

pub use game_lineage::*;

impl Simulation {
    pub fn lineage_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.host.state.scenario = scenarios()
            .into_iter()
            .find(|s| s.id == "lineage-fixture")
            .ok_or_else(|| Error::InvalidFixture("lineage fixture is not registered".into()))?;
        let ruler = sim.host.state.ruler;
        let location = game_spatial::location(&sim.host.storage, game_ecs::StableId((ruler).0))
            .expect("location exists")
            .clone();
        sim.host.state.lineage = Some(World {
            rules: MentorshipRules::default(),
            invocation_learning: BTreeMap::new(),
            incarnations: vec![Incarnation {
                person: ruler,
                bound_at: sim.host.clock().time,
                died_at: None,
            }],
            waiting: None,
            guardians: sim
                .host
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
                            provision: None,
                            requested_delivery: None,
                            knows_invocation: true,
                            target: Some(ruler),
                            sensed_location: Some(location.clone()),
                            sensed_at: sim.host.clock().time,
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
        let Some(current) = self.host.state.lineage.as_ref().and_then(World::current) else {
            return;
        };
        if game_physiology::body(&self.host.storage, current)
            .unwrap()
            .vitals
            .alive
        {
            return;
        }
        let constraints: BTreeMap<_, _> = self
            .host
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .map(|id| (*id, self.guardian_constraint(*id)))
            .collect();
        let world = game_lineage::observe_death(
            self.host.state.lineage.as_ref().unwrap(),
            false,
            self.host.clock().time,
            &constraints,
        );
        self.host.state.lineage = Some(world);
    }

    /// Called only after an actual birth has created the person and body. Birth
    /// deadlines are processed in persistent pregnancy order, so the first
    /// simultaneous eligible birth is deterministic across workers and saves.
    pub(crate) fn record_lineage_birth(&mut self, child: PersonId) {
        let Some(waiting) = self
            .host
            .state
            .lineage
            .as_ref()
            .and_then(|world| world.waiting.clone())
        else {
            return;
        };
        let Some(life) = self
            .host
            .state
            .development
            .as_ref()
            .and_then(|_| game_lifecycle::life(&self.host.storage, child))
        else {
            return;
        };
        let birth = ActualBirth {
            person: child,
            born: life.born,
            initial_age: life.initial_age,
            parent_count: life.parents.len(),
            alive: game_physiology::body(&self.host.storage, child)
                .unwrap()
                .vitals
                .alive,
            location: game_spatial::location(&self.host.storage, game_ecs::StableId((child).0))
                .expect("location exists")
                .clone(),
        };
        let constraints = self
            .host
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .map(|id| (*id, self.guardian_constraint(*id)))
            .collect();
        let Some(binding) = game_lineage::bind_actual_birth(
            self.host.state.lineage.as_ref().unwrap(),
            &[birth],
            self.host.clock().time,
            &constraints,
        ) else {
            return;
        };
        self.host.state.people[self.person_index[&waiting.predecessor]].role = Role::Citizen;
        self.host.state.people[self.person_index[&binding.successor]].role = Role::Ruler;
        self.host.state.ruler = binding.successor;
        self.host.state.lineage = Some(binding.lineage);
    }

    pub(crate) fn lineage_handles(&self, person: PersonId) -> bool {
        self.host
            .state
            .lineage
            .as_ref()
            .is_some_and(|world| world.guardians.contains_key(&person))
    }

    fn guardian_constraint(&self, id: PersonId) -> Option<GuardianGoal> {
        game_lineage::guardian_constraint(
            self.host
                .state
                .combat
                .as_ref()
                .is_some_and(|w| w.captivity.contains_key(&id)),
            game_physiology::body(&self.host.storage, id)
                .unwrap()
                .vitals
                .health
                == 0,
            self.combat_restricts(id),
        )
    }

    /// Called at the event's physical commit, not from a later query. A guardian
    /// cannot acquire evidence merely because they eventually visit the area.
    pub(crate) fn witness_lineage_event(&mut self, kind: &EventKind) {
        let owner_now = self.host.clock().time;
        let Some(current) = self.host.state.lineage.as_ref().and_then(World::current) else {
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
        let area = departure.or_else(|| {
            match *game_spatial::location(&self.host.storage, game_ecs::StableId((actor).0))
                .expect("location exists")
            {
                Location::AtArea(area) => Some(area),
                Location::InTransit { .. }
                | Location::StoppedOnRoad { .. }
                | Location::Carried { .. } => None,
            }
        });
        let Some(area) = area else { return };
        let observers: Vec<_> = self
            .host
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .copied()
            .filter(|id| {
                *game_spatial::location(&self.host.storage, game_ecs::StableId(id.0))
                    .expect("location exists")
                    == Location::AtArea(area)
                    && game_physiology::body(&self.host.storage, *id)
                        .unwrap()
                        .vitals
                        .health
                        > 0
            })
            .collect();
        for guardian in observers {
            let known_distress =
                game_characters::decisions::mind::<game_api::Command>(&self.host.storage, guardian)
                    .expect("mind exists")
                    .child_signals
                    .get(&current)
                    .is_some_and(|signal| signal.urgency > 0);
            let conduct = match kind {
                EventKind::Practiced { gain, .. } => Conduct::Practice { gain: *gain },
                EventKind::Ate { .. } => Conduct::Meal,
                EventKind::Produced { .. } => Conduct::Production,
                EventKind::Departed { .. } => Conduct::Departure { known_distress },
                EventKind::Arrived { .. } => Conduct::Arrival,
                EventKind::Cared { .. } => Conduct::Care,
                EventKind::Expressed { .. } => Conduct::Expression,
                EventKind::LessonCompleted { gain, .. } => Conduct::Lesson { gain: *gain },
                _ => continue,
            };
            let stage = game_lifecycle::life(&self.host.storage, current)
                .expect("life exists")
                .stage;
            let personality =
                game_characters::decisions::mind::<game_api::Command>(&self.host.storage, guardian)
                    .expect("mind exists")
                    .personality
                    .clone();
            let assessment = self
                .host
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
            *assessment = game_lineage::observe_conduct(
                assessment,
                guardian_traits(&personality),
                Observation {
                    conduct,
                    stage,
                    at: owner_now,
                    witnessed: true,
                },
            );
        }
    }

    pub(crate) fn start_lesson(
        &mut self,
        student: PersonId,
        teacher: PersonId,
        topic: LessonTopic,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let world = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
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
        let student_location =
            game_spatial::location(&self.host.storage, game_ecs::StableId((student).0))
                .expect("location exists");
        let teacher_location =
            game_spatial::location(&self.host.storage, game_ecs::StableId((teacher).0))
                .expect("location exists");
        if student_location != teacher_location || !matches!(student_location, Location::AtArea(_))
        {
            return Err(Error::Unavailable);
        }
        if [student, teacher].into_iter().any(|id| {
            let body = game_physiology::body(&self.host.storage, id).unwrap();
            !body.vitals.alive
                || body.vitals.health == 0
                || !matches!(
                    self.host.storage.activity(id).unwrap(),
                    Activity::Idle | Activity::Rest
                )
        }) {
            return Err(Error::Unavailable);
        }
        let teacher_mind =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, teacher)
                .expect("mind exists");
        let knows = match topic {
            LessonTopic::Invocation => world.guardians[&teacher].knows_invocation,
            LessonTopic::Craft => {
                self.own_skill_level(teacher, Skill::Craft) > 0
                    || teacher_mind
                        .interests
                        .get(&ChildhoodInterest::Craft)
                        .is_some_and(|skill| *skill > 0)
            }
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
            self.host
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&teacher)
                .unwrap()
                .explanation = reason.into();
            self.host.bump_revision(1)?;
            return Ok(());
        }
        let completes = SimTime(
            self.host
                .clock()
                .time
                .0
                .checked_add(world.rules.lesson_duration.0)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_actor_activity(student);
        self.interrupt_actor_activity(teacher);
        self.host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&teacher)
            .unwrap()
            .assignment = None;
        self.host.state.lineage.as_mut().unwrap().guardians.get_mut(&teacher).unwrap().explanation =
            "The guided lesson is underway; both pupil and teacher are occupied until it completes or is interrupted".into();
        for actor in [student, teacher] {
            *self.host.storage.activity_mut(actor).unwrap() = Activity::Lesson {
                teacher,
                student,
                topic,
                completes,
            };
            self.host
                .state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(actor));
        }
        self.host.bump_revision(1)?;
        Ok(())
    }

    pub(crate) fn available_lesson_commands(&self, student: PersonId) -> Vec<Command> {
        let Some(world) = &self.host.state.lineage else {
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
        let body = game_physiology::body(&self.host.storage, student).unwrap();
        let activity = self.host.storage.activity(student).unwrap();
        let location = game_spatial::location(&self.host.storage, game_ecs::StableId((student).0))
            .expect("location exists");
        if !body.vitals.alive
            || body.vitals.health == 0
            || !matches!(*activity, Activity::Idle | Activity::Rest)
            || !matches!(location, Location::AtArea(_))
        {
            return Vec::new();
        }
        let mut commands = Vec::new();
        for (teacher, guard) in &world.guardians {
            let teacher_activity = self.host.storage.activity(*teacher).unwrap();
            if game_spatial::location(&self.host.storage, game_ecs::StableId(teacher.0))
                .expect("location exists")
                != location
                || self.in_combat(*teacher)
                || self.combat_restricts(*teacher)
                || self.care_index.contains_key(teacher)
                || !matches!(*teacher_activity, Activity::Idle | Activity::Rest)
            {
                continue;
            }
            let mind = &game_characters::decisions::mind::<game_api::Command>(
                &self.host.storage,
                *teacher,
            )
            .expect("mind exists");
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
                        LessonTopic::Craft => {
                            self.own_skill_level(*teacher, Skill::Craft) > 0
                                || mind
                                    .interests
                                    .get(&ChildhoodInterest::Craft)
                                    .is_some_and(|skill| *skill > 0)
                        }
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
            for food in mind.knowledge.values().filter(|k| {
                k.good == Good::Food
                    && k.accessible
                    && k.quantity.0 >= 1_000
                    && game_characters::decisions::mind::<game_api::Command>(
                        &self.host.storage,
                        student,
                    )
                    .expect("mind exists")
                    .knowledge
                    .contains_key(&k.inventory)
            }) {
                commands.push(Command::RequestGuardian {
                    guardian: *teacher,
                    request: GuardianRequest::Supply {
                        good: Good::Food,
                        source: food.inventory,
                        quantity: Quantity(1_000),
                    },
                });
            }
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
                        self.host
                            .state
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
        }) = self.host.storage.activity(actor).cloned()
        else {
            return;
        };
        for id in [teacher, student] {
            if let Some(activity) = self.host.storage.activity_mut(id)
                && matches!(*activity, Activity::Lesson { .. })
            {
                *activity = Activity::Idle;
            }
            for due in self.host.state.due.values_mut() {
                due.retain(|event| !matches!(event, Due::Physical(person) if *person == id));
            }
        }
        self.host.state.due.retain(|_, events| !events.is_empty());
    }

    pub(crate) fn complete_lesson(&mut self, actor: PersonId) {
        let Activity::Lesson {
            teacher,
            student,
            topic,
            completes,
        } = *self.host.storage.activity(actor).unwrap()
        else {
            return;
        };
        let paired = *self.host.storage.activity(teacher).unwrap()
            == *self.host.storage.activity(student).unwrap();
        if !paired
            || completes != self.host.clock().time
            || *game_spatial::location(&self.host.storage, game_ecs::StableId((teacher).0))
                .expect("location exists")
                != *game_spatial::location(&self.host.storage, game_ecs::StableId((student).0))
                    .expect("location exists")
            || [teacher, student].into_iter().any(|id| {
                !game_physiology::body(&self.host.storage, id)
                    .unwrap()
                    .vitals
                    .alive
            })
        {
            self.interrupt_lesson(actor);
            return;
        }
        let lesson_gain = self.host.state.lineage.as_ref().unwrap().rules.lesson_gain;
        let gain =
            match topic {
                LessonTopic::Invocation => {
                    let learning = self
                        .host
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
                    let knowledge = game_characters::decisions::mind::<game_api::Command>(
                        &self.host.storage,
                        teacher,
                    )
                    .expect("mind exists")
                    .interests
                    .get(&ChildhoodInterest::Craft)
                    .copied()
                    .unwrap_or(0)
                    .max(u64::from(self.own_skill_level(teacher, Skill::Craft)) * 10);
                    let learning = game_characters::decisions::mind_mut::<game_api::Command>(
                        &mut self.host.storage,
                        student,
                    )
                    .unwrap()
                    .interests
                    .entry(ChildhoodInterest::Craft)
                    .or_default();
                    let gain = lesson_gain.min(knowledge.saturating_sub(*learning));
                    *learning += gain;
                    gain
                }
                LessonTopic::Travel => {
                    let known = game_characters::decisions::mind::<game_api::Command>(
                        &self.host.storage,
                        teacher,
                    )
                    .expect("mind exists")
                    .routes
                    .clone();
                    let student_routes = &mut game_characters::decisions::mind_mut::<
                        game_api::Command,
                    >(&mut self.host.storage, student)
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
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        if let GuardianRequest::Supply {
            good,
            source,
            quantity,
        } = request
        {
            return self.request_guardian_supply(guardian, requester, good, source, quantity);
        }
        let world = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        if world.current() != Some(requester)
            || !world.guardians.contains_key(&guardian)
            || !self.independent(requester)
            || self.combat_restricts(requester)
            || self.in_combat(requester)
            || self.care_index.contains_key(&requester)
            || !matches!(
                *self.host.storage.activity(requester).unwrap(),
                Activity::Idle | Activity::Rest
            )
            || !game_physiology::body(&self.host.storage, requester)
                .unwrap()
                .vitals
                .alive
            || *game_spatial::location(&self.host.storage, game_ecs::StableId((requester).0))
                .expect("location exists")
                != *game_spatial::location(&self.host.storage, game_ecs::StableId((guardian).0))
                    .expect("location exists")
            || !matches!(
                *game_spatial::location(&self.host.storage, game_ecs::StableId((requester).0))
                    .expect("location exists"),
                Location::AtArea(_)
            )
        {
            return Err(Error::Unavailable);
        }
        let guard = &world.guardians[&guardian];
        let ready = guard.assessments.get(&requester).is_some_and(|a| a.ready);
        let activity = self.host.storage.activity(guardian).unwrap();
        let command = match request {
            GuardianRequest::Move { destination } => Command::Move { destination },
            GuardianRequest::Rest => Command::Rest,
            GuardianRequest::Supply { .. } => unreachable!("handled above"),
            GuardianRequest::Work { site } => Command::Work { site },
        };
        let reason = game_lineage::judge_request(
            &request,
            RequestInput {
                constrained: self.guardian_constraint(guardian).is_some(),
                in_combat: self.in_combat(guardian),
                physical_duty_pending: !matches!(*activity, Activity::Idle | Activity::Rest),
                ready,
                known_distress: game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    guardian,
                )
                .expect("mind exists")
                .child_signals
                .get(&requester)
                .is_some_and(|s| s.urgency > 0),
            },
        )
        .err()
        .map(Refusal::reason);
        self.ensure_event_capacity(1)?;
        if let Some(reason) = reason {
            self.emit(EventKind::GuardianRefused {
                guardian,
                requester,
                reason: reason.into(),
            });
            self.host
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&guardian)
                .unwrap()
                .explanation = reason.into();
            self.host.bump_revision(1)?;
            return Ok(());
        }
        let accepted = if let Command::Move { destination } = command {
            self.start_movement(guardian, destination)
        } else {
            self.debug_actor_command(guardian, command)
        };
        match accepted {
            Ok(()) => {
                self.cancel_guardian_delivery(guardian);
                self.emit(EventKind::GuardianAccepted {
                    guardian,
                    requester,
                });
                let guard = self
                    .host
                    .state
                    .lineage
                    .as_mut()
                    .unwrap()
                    .guardians
                    .get_mut(&guardian)
                    .unwrap();
                guard.assignment = Some(Assignment {
                    request,
                    accepted_at: owner_now,
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
                self.host.state.lineage.as_mut().unwrap().guardians.get_mut(&guardian).unwrap().explanation =
                    "The task exceeds my known routes, permissions, or physical resources; I remain committed to the lineage".into();
                self.host.bump_revision(1)?;
                Ok(())
            }
        }
    }

    pub(crate) fn review_lineage(&mut self) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        let Some(world) = &self.host.state.lineage else {
            return Ok(());
        };
        let current = world.current();
        let guardians: Vec<_> = world.guardians.keys().copied().collect();
        for id in guardians {
            if let Some(person) = current {
                let same_place =
                    *game_spatial::location(&self.host.storage, game_ecs::StableId((id).0))
                        .expect("location exists")
                        == *game_spatial::location(
                            &self.host.storage,
                            game_ecs::StableId((person).0),
                        )
                        .expect("location exists")
                        && matches!(
                            *game_spatial::location(&self.host.storage, game_ecs::StableId((id).0))
                                .expect("location exists"),
                            Location::AtArea(_)
                        )
                        && game_physiology::body(&self.host.storage, id)
                            .unwrap()
                            .vitals
                            .health
                            > 0;
                if same_place {
                    let personality = game_characters::decisions::mind::<game_api::Command>(
                        &self.host.storage,
                        id,
                    )
                    .expect("mind exists")
                    .personality
                    .clone();
                    let stage = game_lifecycle::life(&self.host.storage, person)
                        .expect("life exists")
                        .stage;
                    let assessment = self
                        .host
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
                    *assessment = game_lineage::observe_stage(
                        assessment,
                        guardian_traits(&personality),
                        stage,
                        owner_now,
                        true,
                    );
                }
            }
            let force_command = self.force_candidate(id);
            if force_command.is_none() && self.drive_guardian_delivery(id)? {
                continue;
            }
            let sensed = current.map(|person| self.resolved_location(person));
            let body = game_physiology::body(&self.host.storage, id).unwrap();
            let activity = self.host.storage.activity(id).unwrap();
            let own_location = self.resolved_location(id);
            let pending = self.host.state.lineage.as_ref().unwrap().guardians[&id]
                .assignment
                .as_ref()
                .is_some_and(|assignment| match assignment.request {
                    GuardianRequest::Move { destination } => {
                        matches!(own_location, Location::InTransit { .. })
                            || matches!(own_location, Location::AtArea(area) if area != destination)
                    }
                    GuardianRequest::Work { site } => {
                        matches!(*activity, Activity::Work { site: active } if active == site)
                    }
                    GuardianRequest::Rest => owner_now <= assignment.accepted_at,
                    GuardianRequest::Supply { .. } => false,
                });
            let (goal, explanation, command) = if let Some(command) = force_command {
                (
                    GuardianGoal::Confined,
                    "Attempting to release the physical bindings with a carried tool".into(),
                    Some(command),
                )
            } else if self
                .host
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
            } else if self.mission_actor_busy(id) {
                (GuardianGoal::Searching, "Following an accepted objective while retaining responsibility for the incarnation".into(), None)
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
                    matches!(*activity, Activity::Idle | Activity::Work { .. })
                        .then_some(Command::Rest),
                )
            };
            let guardian = self
                .host
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&id)
                .unwrap();
            guardian.target = current;
            guardian.sensed_location = sensed;
            guardian.sensed_at = owner_now;
            guardian.goal = goal;
            guardian.explanation = explanation;
            if !pending {
                guardian.assignment = None;
            }
            if let Some(command) = command {
                let accepted = if let Command::Move { destination } = command {
                    self.start_movement(id, destination)
                } else {
                    self.debug_actor_command(id, command)
                };
                if accepted.is_err() {
                    self.host.state
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
        &mut self,
        guardian: PersonId,
        target: PersonId,
        own_location: &Location,
        target_location: &Location,
    ) -> (GuardianGoal, String, Option<Command>) {
        let activity = self.host.storage.activity(guardian).unwrap();
        if let Some(command) = self.transport_guardian_rescue(guardian, target) {
            return (
                GuardianGoal::Searching,
                "Providing physical support to the incarnation on the road".into(),
                Some(command),
            );
        }
        if let Some(command) = self.force_guardian_rescue(guardian, target) {
            return (
                GuardianGoal::Searching,
                "Reaching the confined incarnation with a physical rescue tool".into(),
                Some(command),
            );
        }
        let mind =
            &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, guardian)
                .expect("mind exists");
        let mut need = mind
            .child_signals
            .get(&target)
            .map(|signal| NeedObservation {
                location: signal.location.clone(),
                observed_at: signal.observed_at,
                urgency: signal.urgency,
                hydration_urgency: signal.hydration_urgency,
                preference: signal.preference,
            });
        let can_reconsider = matches!(
            *activity,
            Activity::Idle | Activity::Rest | Activity::Work { .. }
        );
        let should_rest = matches!(*activity, Activity::Idle | Activity::Work { .. });
        let doses = game_lineage::CareDoses {
            food: self.body_rules(guardian).meal_size,
            water: self.care_dose(Good::Water).unwrap_or(Quantity(0)),
        };
        let together = self.transport_same_place(guardian, target);
        let applicable = self.dependent_signal_visible(guardian, target).then(|| {
            self.host.state.development.as_ref().is_some_and(|_| {
                game_lifecycle::life(&self.host.storage, target)
                    .is_some_and(|life| matches!(life.stage, LifeStage::Infant | LifeStage::Child))
            })
        });
        if applicable == Some(false) {
            need = None;
            self.host
                .state
                .lineage
                .as_mut()
                .unwrap()
                .observe_care_unavailable(guardian);
        }
        let provision = game_lineage::review_care_provision(
            self.host.state.lineage.as_ref().unwrap().guardians[&guardian]
                .provision
                .as_ref(),
            target,
            self.host.clock().time,
            together,
            need.as_ref().filter(|_| applicable != Some(false)),
            doses,
        );
        self.host
            .state
            .lineage
            .as_mut()
            .unwrap()
            .guardians
            .get_mut(&guardian)
            .unwrap()
            .provision = provision.clone();
        if let Some(provision) = provision {
            // This location fact is the guardian's explicit innate connection;
            // it conveys no remote need, inventory balance or permissions.
            let location = self.delivery_observed_location(target);
            let destination = game_logistics::KnownDestination {
                destination: provision.delivery.destination,
                location,
                observed_at: self.host.clock().time,
                accessible: true,
                room: None,
                direct_transfer: true,
            };
            let step = self.plan_delivery(
                guardian,
                &provision.delivery,
                &destination,
                provision.delivery.requirement.quantity,
            );
            let command = match step {
                game_logistics::Step::Arrived { source, .. } => Some(Command::Care {
                    child: target,
                    source,
                }),
                step => self.delivery_command(guardian, step),
            };
            return (GuardianGoal::Mentoring,"Pursuing the observed care requirement through actual acquisition, carrying and handoff".into(),command);
        }
        let intent = game_lineage::guardian_intention(GuardianInput {
            target,
            own_location,
            target_location,
            now: self.host.clock().time,
            can_reconsider,
            should_rest,
            ready: self.host.state.lineage.as_ref().unwrap().guardians[&guardian]
                .assessments
                .get(&target)
                .is_some_and(|a| a.ready),
            need,
        });
        let command = intent.action.map(|action| match action {
            GuardianAction::ApproachRoad {
                from,
                to,
                progress,
                duration,
            } => Command::Transport {
                action: TransportAction::ApproachRoad {
                    from,
                    to,
                    progress,
                    duration,
                },
            },
            GuardianAction::Move { destination } => Command::Move { destination },
            GuardianAction::Comfort { child } => Command::Comfort { child },
            GuardianAction::Rest => Command::Rest,
        });
        (intent.goal, intent.explanation, command)
    }
}

fn guardian_traits(personality: &decisions::Personality) -> GuardianTraits {
    GuardianTraits {
        discipline: personality.discipline,
        patience: personality.patience,
        risk_tolerance: personality.risk_tolerance,
    }
}
pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.state.lineage else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent lineage state".into());
    let Some(_) = &state.state.physical else {
        return Err(fail());
    };
    let Some(development) = &state.state.development else {
        return Err(fail());
    };
    if state.state.decisions.is_none()
        || world.incarnations.is_empty()
        || world.rules.lesson_duration.0 == 0
        || !(1..=1_000).contains(&world.rules.lesson_gain)
        || !(1..=1_000).contains(&world.rules.invocation_access_threshold)
        || world.invocation_learning.iter().any(|(id, value)| {
            game_physiology::body(state.components, *id).is_none() || *value > 1_000
        })
    {
        return Err(fail());
    }
    for (actor, activity) in state.components.activities() {
        if let Activity::Lesson {
            teacher,
            student,
            topic,
            completes,
        } = *activity
        {
            if ![actor, teacher, student]
                .into_iter()
                .all(|id| game_physiology::body(state.components, id).is_some())
                || ![teacher, student].contains(&actor)
                || teacher == student
                || !world.guardians.contains_key(&teacher)
                || world.current() != Some(student)
                || *state.components.activity(teacher).unwrap()
                    != *state.components.activity(student).unwrap()
                || state
                    .state
                    .people
                    .iter()
                    .find(|p| p.id == teacher)
                    .and_then(|p| {
                        game_spatial::location(state.components, game_ecs::StableId((p.id).0))
                    })
                    != state
                        .state
                        .people
                        .iter()
                        .find(|p| p.id == student)
                        .and_then(|p| {
                            game_spatial::location(state.components, game_ecs::StableId((p.id).0))
                        })
            {
                return Err(fail());
            }
            let started = completes
                .0
                .checked_sub(world.rules.lesson_duration.0)
                .ok_or_else(fail)?;
            let life = game_lifecycle::life(state.components, student).ok_or_else(fail)?;
            let age = started
                .checked_sub(life.born.0)
                .and_then(|v| v.checked_add(life.initial_age.0))
                .ok_or_else(fail)?;
            if started > state.time.0
                || age < development.rules.infancy.0
                || !state.state.due.get(&completes).is_some_and(|due| {
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
        let life = game_lifecycle::life(state.components, incarnation.person).ok_or_else(fail)?;
        let body = game_physiology::body(state.components, incarnation.person).ok_or_else(fail)?;
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
            event.at == died
                && matches!(event.event, EventKind::Died { actor } if actor == incarnation.person)
        }) {
            return Err(fail());
        }
        if i > 0 {
            let previous_death = world.incarnations[i - 1].died_at.ok_or_else(fail)?;
            let first_eligible = game_lifecycle::all_lives(state.components)
                .map(|(_, life)| life)
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
    if last.person != state.state.ruler {
        return Err(fail());
    }
    match (&world.waiting, last.died_at) {
        (Some(waiting), Some(died))
            if waiting.predecessor == last.person && waiting.died_at == died => {}
        (None, None) => {}
        _ => return Err(fail()),
    }
    let expected_guardians: BTreeSet<_> = state
        .state
        .people
        .iter()
        .filter(|p| p.role == Role::Guardian)
        .map(|p| p.id)
        .collect();
    if world.guardians.keys().copied().collect::<BTreeSet<_>>() != expected_guardians {
        return Err(fail());
    }
    for (guardian_id, guardian) in &world.guardians {
        if guardian.target != world.current()
            || guardian.sensed_at > state.time
            || guardian.target.is_some() != guardian.sensed_location.is_some()
        {
            return Err(fail());
        }
        if guardian.provision.as_ref().is_some_and(|plan| {
            !plan.delivery.valid()
                || world.current().is_none_or(|target| {
                    plan.delivery.destination != game_logistics::Destination::Contact(target)
                })
                || plan.delivery.last_receipt >= state.next_event
                || plan
                    .delivery
                    .requirement
                    .source
                    .is_some_and(|source| !state.state.inventories.contains_key(&source))
                || plan.observed_at > state.time
        }) {
            return Err(fail());
        }
        if let Some(requested) = &guardian.requested_delivery {
            let delivery = &requested.delivery;
            if !delivery.valid()
                || requested.accepted_at > state.time
                || requested.last_event >= state.next_event
                || delivery.last_receipt >= state.next_event
                || delivery.requirement.source.is_none_or(|source| {
                    !state
                        .state
                        .inventories
                        .get(&source)
                        .is_some_and(|stock| stock.good == delivery.requirement.good)
                })
                || world.current().is_none_or(|target| {
                    delivery.destination != game_logistics::Destination::Contact(target)
                })
                || requested.cargo.iter().any(|(inventory, quantity)| {
                    *quantity == 0
                        || !state.state.inventories.get(inventory).is_some_and(|stock| {
                            stock.good == delivery.requirement.good
                                && stock.location == InventoryLocation::Carried(*guardian_id)
                                && *quantity <= stock.quantity.0
                        })
                })
            {
                return Err(fail());
            }
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
                        !state.state.areas.iter().any(|a| a.id == destination)
                    }
                    GuardianRequest::Work { site } => {
                        game_workshop::site(state.components, site).is_none()
                    }
                    GuardianRequest::Rest => false,
                    GuardianRequest::Supply {
                        source, quantity, ..
                    } => !state.state.inventories.contains_key(&source) || quantity.0 == 0,
                })
        {
            return Err(fail());
        }
        if let Some(location) = &guardian.sensed_location {
            let known_area = |id| state.state.areas.iter().any(|area| area.id == id);
            match location {
                Location::AtArea(area) if !known_area(*area) => return Err(fail()),
                Location::InTransit {
                    from,
                    to,
                    departed,
                    arrives,
                    ..
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
