//! Control follows the first actual eligible birth, without transferring an estate
//! or political office. Guardians sense that person's location and travel normally.
use crate::*;

pub use game_lineage::Runtime as World;
pub use game_lineage::*;

struct WitnessInputs {
    event: game_lineage::WitnessEvent,
    location: Location,
    stage: game_core::LifeStage,
    now: SimTime,
    observers: Vec<game_lineage::WitnessFacts>,
}

/// Binding bookkeeping for a selected projection supplied by guarded origins.
/// Callers retain the originating owner guards through canonical publication.
pub(crate) struct PreparedLineageWitness<Output = ()> {
    plan: game_lineage::PreparedLineage<Output>,
    inputs: Vec<WitnessInputs>,
    reads: game_ecs::ReadSet,
    projected_reads: game_ecs::ReadFootprint,
}
pub(crate) struct StagedLineageWitness<Output = ()> {
    plan: game_lineage::StagedLineage<Output>,
    facts: game_ecs::ReadGuard,
}
impl<Output> PreparedLineageWitness<Output> {
    pub(crate) fn with_death(
        self,
        simulation: &Simulation,
        current_alive: bool,
        now: SimTime,
        constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
    ) -> Result<PreparedLineageWitness<Succession>, Error> {
        let runtime = simulation
            .host
            .state
            .lineage
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let plan = self.plan.with_death(
            runtime,
            &simulation.host.storage,
            current_alive,
            now,
            constraints,
        )?;
        Ok(PreparedLineageWitness {
            plan,
            inputs: self.inputs,
            reads: self.reads,
            projected_reads: self.projected_reads,
        })
    }
    pub(crate) fn stage_into(
        self,
        simulation: &Simulation,
        batch: &mut game_ecs::Batch,
    ) -> Result<StagedLineageWitness<Output>, Error> {
        let runtime = simulation
            .host
            .state
            .lineage
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let plan = self
            .plan
            .stage_into(runtime, &simulation.host.storage, batch)?;
        let facts = self
            .reads
            .bind(&simulation.host.storage, batch, &self.projected_reads)?;
        Ok(StagedLineageWitness { plan, facts })
    }
}
impl PreparedLineageWitness {
    /// Combine this observation with an existing owner-prepared care or lesson
    /// transition, preserving its result and staging each Lineage row once.
    pub(crate) fn fold_into<Output>(
        self,
        mut plan: game_lineage::PreparedLineage<Output>,
        simulation: &Simulation,
    ) -> Result<PreparedLineageWitness<Output>, Error> {
        let runtime = simulation
            .host
            .state
            .lineage
            .as_ref()
            .ok_or(Error::Unavailable)?;
        for input in &self.inputs {
            plan = plan.with_witness(
                runtime,
                &simulation.host.storage,
                input.event,
                &input.location,
                input.stage,
                input.now,
                input.observers.iter().cloned(),
            )?;
        }
        Ok(PreparedLineageWitness {
            plan,
            inputs: self.inputs,
            reads: self.reads,
            projected_reads: self.projected_reads,
        })
    }
    /// Preserve receipt order while publishing one final Lineage write set.
    pub(crate) fn followed_by(
        mut self,
        next: Self,
        simulation: &Simulation,
    ) -> Result<Self, Error> {
        let runtime = simulation
            .host
            .state
            .lineage
            .as_ref()
            .ok_or(Error::Unavailable)?;
        for input in &next.inputs {
            self.plan = self.plan.with_witness(
                runtime,
                &simulation.host.storage,
                input.event,
                &input.location,
                input.stage,
                input.now,
                input.observers.iter().cloned(),
            )?;
        }
        self.inputs.extend(next.inputs);
        self.reads.extend(next.reads);
        self.projected_reads.extend(next.projected_reads);
        Ok(self)
    }
}
impl<Output> StagedLineageWitness<Output> {
    pub(crate) fn validate(
        &self,
        simulation: &Simulation,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        let runtime = simulation
            .host
            .state
            .lineage
            .as_ref()
            .ok_or(Error::Unavailable)?;
        self.plan
            .validate(runtime, &simulation.host.storage, batch)?;
        self.facts.validate(&simulation.host.storage, batch)
    }
    pub(crate) fn row_guards(&self) -> [game_ecs::ReadGuard; 2] {
        [self.plan.guard().clone(), self.facts.clone()]
    }
    pub(crate) fn finish(self) -> Output {
        self.plan.finish()
    }
}

fn projected_witness_row<'a, C: game_ecs::Component>(
    reads: &mut game_ecs::ReadSet,
    projected_reads: &mut game_ecs::ReadFootprint,
    store: &'a game_ecs::Store,
    batch: &'a game_ecs::Batch,
    id: game_ecs::StableId,
) -> Result<Option<&'a C>, Error> {
    reads.read::<C>(store, id);
    if batch.touches::<C>(id) {
        projected_reads.protect::<C>(id);
    }
    batch.projected::<C>(store, id)
}

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
        let founders: Vec<_> =
            sim.people()
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
                .collect();
        let history_id = game_ecs::StableId(ruler.0);
        let bound_at = sim.host.clock().time;
        sim.host.state.lineage = Some(World::initialize(
            &mut sim.host.storage,
            history_id,
            MentorshipRules::default(),
            History {
                incarnations: vec![Incarnation {
                    person: ruler,
                    bound_at,
                    died_at: None,
                }],
            },
            founders,
        )?);
        Ok(sim)
    }

    pub(crate) fn observe_lineage_deaths(&mut self) {
        let Some(world) = self.host.state.lineage.as_ref() else {
            return;
        };
        let Some(current) = world.current(&self.host.storage) else {
            return;
        };
        let alive = game_physiology::body(&self.host.storage, current)
            .expect("incarnation body")
            .vitals
            .alive;
        let constraints = game_lineage::guardians(&self.host.storage)
            .map(|(id, _)| (id, self.guardian_constraint(id)))
            .collect();
        let prepared = world
            .prepare_death(
                &self.host.storage,
                alive,
                self.host.clock().time,
                &constraints,
            )
            .expect("validated lineage death");
        self.commit_lineage(prepared)
            .expect("registered lineage death");
    }

    pub(crate) fn lineage_handles(&self, person: PersonId) -> bool {
        self.host.state.lineage.is_some()
            && game_lineage::guardian(&self.host.storage, person).is_some()
    }
    pub(crate) fn commit_lineage<T>(
        &mut self,
        prepared: game_lineage::PreparedLineage<T>,
    ) -> Result<T, Error> {
        prepared.commit(
            self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?,
            &mut self.host.storage,
        )
    }
    fn lineage_factual_reads(&self, actors: &[PersonId]) -> game_ecs::ReadSet {
        let mut reads = game_ecs::ReadSet::new();
        for actor in actors {
            let id = game_ecs::StableId(actor.0);
            reads.read::<game_physiology::Body>(&self.host.storage, id);
            reads.read::<game_lifecycle::Life>(&self.host.storage, id);
            reads.read::<Location>(&self.host.storage, id);
            reads.read::<game_characters::decisions::Mind<Command>>(&self.host.storage, id);
            reads.read::<game_characters::decisions::Stocks>(&self.host.storage, id);
            reads.read::<game_skills::Skills>(&self.host.storage, id);
        }
        reads
    }
    pub(crate) fn lineage_participant(&self, person: PersonId) -> game_lineage::ParticipantFacts {
        let body =
            game_physiology::body(&self.host.storage, person).expect("lineage participant body");
        game_lineage::ParticipantFacts {
            person,
            location: game_spatial::location(&self.host.storage, game_ecs::StableId(person.0))
                .expect("participant location")
                .clone(),
            alive: body.vitals.alive,
            health: body.vitals.health,
            idle: matches!(
                self.host.storage.activity(person),
                Some(Activity::Idle | Activity::Rest)
            ),
            independent: self.independent(person),
            in_combat: self.in_combat(person),
            restricted: self.combat_restricts(person),
            in_care: self.care_index.contains_key(&person),
        }
    }
    fn lineage_teacher(&self, teacher: PersonId, student: PersonId) -> game_lineage::TeacherFacts {
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, teacher)
            .expect("guardian mind");
        let location = game_spatial::location(&self.host.storage, game_ecs::StableId(student.0))
            .expect("pupil location");
        let destinations = self
            .host
            .state
            .areas
            .iter()
            .map(|area| {
                (
                    area.id,
                    match location {
                        Location::AtArea(from) => {
                            decisions::known_path(&mind.routes, *from, area.id).is_ok()
                        }
                        _ => false,
                    },
                )
            })
            .collect();
        game_lineage::TeacherFacts {
            participant: self.lineage_participant(teacher),
            craft_level: self.own_skill_level(teacher, Skill::Craft),
            craft_interest: mind
                .interests
                .get(&ChildhoodInterest::Craft)
                .copied()
                .unwrap_or(0),
            knows_routes: !mind.routes.is_empty(),
            signal_urgency: mind
                .child_signals
                .get(&student)
                .map_or(0, |signal| signal.urgency),
            stocks: game_characters::decisions::stocks(&self.host.storage, teacher)
                .expect("guardian stocks")
                .known
                .values()
                .cloned()
                .collect(),
            destinations,
            sites: mind
                .sites
                .values()
                .map(|site| game_lineage::SiteObservation {
                    site: site.site,
                    area: site.area,
                    access: site.access,
                })
                .collect(),
        }
    }

    pub(crate) fn guardian_constraint(&self, id: PersonId) -> Option<GuardianGoal> {
        game_lineage::guardian_constraint(
            game_conflict::captivity(&self.host.storage, id).is_some(),
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
        let mut batch = game_ecs::Batch::new();
        let Some(prepared) = self
            .prepare_lineage_witness(kind, &batch)
            .expect("lineage observation facts")
        else {
            return;
        };
        let staged = prepared
            .stage_into(self, &mut batch)
            .expect("lineage observation staging");
        staged
            .validate(self, &batch)
            .expect("lineage observation admission");
        let mut transaction = game_world::Transaction::new(self.host.transaction_opening(), batch);
        transaction.guards.extend(staged.row_guards());
        self.host
            .commit_transaction(transaction)
            .expect("lineage observation publication");
        staged.finish();
    }

    /// Prepare from the guarded origin's final factual rows before publication.
    /// The origin retains its owner guards; this wrapper binds only the selected
    /// projected inputs, and captures untouched canonical dependencies normally.
    pub(crate) fn prepare_lineage_witness(
        &self,
        kind: &EventKind,
        batch: &game_ecs::Batch,
    ) -> Result<Option<PreparedLineageWitness>, Error> {
        let Some(world) = self.host.state.lineage.as_ref() else {
            return Ok(None);
        };
        let Some(current) = batch
            .projected::<game_lineage::History>(&self.host.storage, world.history)?
            .and_then(game_lineage::History::current)
        else {
            return Ok(None);
        };
        use game_lineage::WitnessConduct as W;
        let (actor, conduct, departure) = match *kind {
            EventKind::Practiced { actor, gain, .. } => (actor, W::Practice { gain }, None),
            EventKind::Ate { actor, .. } => (actor, W::Meal, None),
            EventKind::Produced { actor, .. } => (actor, W::Production, None),
            EventKind::Departed { actor, from, .. } => (actor, W::Departure, Some(from)),
            EventKind::Arrived { actor, .. } => (actor, W::Arrival, None),
            EventKind::Cared { caregiver, .. } => (caregiver, W::Care, None),
            EventKind::Expressed { actor, .. } => (actor, W::Expression, None),
            EventKind::LessonCompleted { student, gain, .. } => (student, W::Lesson { gain }, None),
            _ => return Ok(None),
        };
        let mut reads = game_ecs::ReadSet::new();
        let mut projected_reads = game_ecs::ReadFootprint::new();
        let current_id = game_ecs::StableId(current.0);
        let location = projected_witness_row::<Location>(
            &mut reads,
            &mut projected_reads,
            &self.host.storage,
            batch,
            current_id,
        )?
        .ok_or(Error::Unavailable)?
        .clone();
        let stage = projected_witness_row::<game_lifecycle::Life>(
            &mut reads,
            &mut projected_reads,
            &self.host.storage,
            batch,
            current_id,
        )?
        .ok_or(Error::Unavailable)?
        .stage;
        let mut observers = Vec::new();
        let event = game_lineage::WitnessEvent {
            actor,
            conduct,
            departure,
        };
        let observer_ids = world
            .witness_target(&self.host.storage, event)?
            .map(|_| {
                game_lineage::guardians(&self.host.storage)
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in observer_ids {
            let stable_id = game_ecs::StableId(id.0);
            let mind = projected_witness_row::<game_characters::decisions::Mind<Command>>(
                &mut reads,
                &mut projected_reads,
                &self.host.storage,
                batch,
                stable_id,
            )?
            .ok_or(Error::Unavailable)?;
            let location = projected_witness_row::<Location>(
                &mut reads,
                &mut projected_reads,
                &self.host.storage,
                batch,
                stable_id,
            )?
            .ok_or(Error::Unavailable)?
            .clone();
            let body = projected_witness_row::<game_physiology::Body>(
                &mut reads,
                &mut projected_reads,
                &self.host.storage,
                batch,
                stable_id,
            )?
            .ok_or(Error::Unavailable)?;
            observers.push(game_lineage::WitnessFacts {
                actor: id,
                location,
                health: body.vitals.health,
                traits: guardian_traits(&mind.personality),
                signal_urgency: mind
                    .child_signals
                    .get(&current)
                    .map_or(0, |signal| signal.urgency),
            });
        }
        let inputs = WitnessInputs {
            event,
            location,
            stage,
            now: self.host.clock().time,
            observers,
        };
        let prepared = world.prepare_witness(
            &self.host.storage,
            inputs.event,
            &inputs.location,
            inputs.stage,
            inputs.now,
            inputs.observers.iter().cloned(),
        )?;
        Ok(Some(PreparedLineageWitness {
            plan: prepared,
            inputs: vec![inputs],
            reads,
            projected_reads,
        }))
    }

    pub(crate) fn start_lesson(
        &mut self,
        student: PersonId,
        teacher: PersonId,
        topic: LessonTopic,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let world = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        let prepared = world.prepare_lesson_start(
            &self.host.storage,
            &self.lineage_participant(student),
            &self.lineage_teacher(teacher, student),
            topic,
            self.host.clock().time,
        )?;
        match prepared.output().clone() {
            game_lineage::LessonDecision::Refused(reason) => {
                self.ensure_event_capacity(1)?;
                self.commit_lineage(prepared)?;
                self.emit(EventKind::GuardianRefused {
                    guardian: teacher,
                    requester: student,
                    reason: reason.into(),
                });
            }
            game_lineage::LessonDecision::Begin {
                teacher,
                student,
                topic,
                completes,
            } => {
                let opening = self.host.transaction_opening();
                let interruptions = self.prepare_interruptions(
                    &[student, teacher],
                    crate::commands::ActivityOwner::Physical,
                )?;
                self.host
                    .preflight_records(0, interruptions.event_count()?, 1)?;
                let activities = [student, teacher]
                    .into_iter()
                    .map(|actor| {
                        (
                            actor,
                            Activity::Lesson {
                                teacher,
                                student,
                                topic,
                                completes,
                            },
                        )
                    })
                    .collect();
                let mut activity_reads = game_ecs::ReadSet::new();
                let mut admitted = game_ecs::ReadFootprint::new();
                for actor in [student, teacher] {
                    let id = game_ecs::StableId(actor.0);
                    activity_reads.read::<Activity>(&self.host.storage, id);
                    admitted.protect::<Activity>(id);
                }
                let mut batch = game_ecs::Batch::new();
                let owner = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
                let lineage = prepared
                    .with_factual_reads(self.lineage_factual_reads(&[student, teacher]))
                    .stage_into(owner, &self.host.storage, &mut batch)?;
                let mut interrupted = interruptions.stage_into(self, &mut batch, &activities)?;
                let activity_guard = activity_reads.bind(&self.host.storage, &batch, &admitted)?;
                lineage.validate(owner, &self.host.storage, &batch)?;
                interrupted.validate_batch(self, &batch)?;
                let mut transaction = game_world::Transaction::new(opening, batch);
                transaction.guards = vec![lineage.guard().clone(), activity_guard];
                transaction.revisions = 1;
                self.host.commit_transaction(transaction)?;
                lineage.finish();
                for actor in [student, teacher] {
                    interrupted.finish_actor(self, actor);
                }
                for actor in [student, teacher] {
                    self.host
                        .state
                        .due
                        .entry(completes)
                        .or_default()
                        .push(Due::Physical(actor));
                }
                return Ok(());
            }
        }
        self.host.bump_revision(1)?;
        Ok(())
    }

    pub(crate) fn available_lesson_commands(&self, student: PersonId) -> Vec<Command> {
        let Some(world) = self.host.state.lineage.as_ref() else {
            return Vec::new();
        };
        let known = game_characters::decisions::stocks(&self.host.storage, student)
            .expect("pupil stocks")
            .known
            .keys()
            .copied()
            .collect();
        let teachers = game_lineage::guardians(&self.host.storage)
            .map(|(teacher, _)| self.lineage_teacher(teacher, student));
        world
            .available_commands(
                &self.host.storage,
                &self.lineage_participant(student),
                &known,
                teachers,
            )
            .into_iter()
            .map(|command| match command {
                game_lineage::AvailableCommand::Learn { teacher, topic } => {
                    Command::Learn { teacher, topic }
                }
                game_lineage::AvailableCommand::Request { guardian, request } => {
                    Command::RequestGuardian { guardian, request }
                }
            })
            .collect()
    }

    pub(crate) fn interrupt_lesson(&mut self, actor: PersonId) {
        fn progress(activity: Option<&Activity>) -> Option<game_lineage::LessonProgress> {
            let Activity::Lesson {
                teacher,
                student,
                topic,
                completes,
            } = activity?
            else {
                return None;
            };
            Some(game_lineage::LessonProgress {
                teacher: *teacher,
                student: *student,
                topic: *topic,
                completes: *completes,
            })
        }
        let Some(lesson) = progress(self.host.storage.activity(actor)) else {
            return;
        };
        let interruption = game_lineage::lesson_interruption(
            lesson,
            [lesson.teacher, lesson.student].map(|id| progress(self.host.storage.activity(id))),
        );
        let mut batch = game_ecs::Batch::new();
        for (id, clear) in interruption
            .participants
            .into_iter()
            .zip(interruption.clear)
        {
            if clear {
                batch.insert(game_ecs::StableId(id.0), Activity::Idle);
            }
            for due in self.host.state.due.values_mut() {
                due.retain(|event| !matches!(event, Due::Physical(person) if *person == id));
            }
        }
        self.host
            .storage
            .apply_batch(batch)
            .expect("lesson activity cleanup");
        self.host.state.due.retain(|_, events| !events.is_empty());
    }

    pub(crate) fn complete_lesson(&mut self, actor: PersonId) -> Result<(), Error> {
        let Activity::Lesson {
            teacher,
            student,
            topic,
            completes,
        } = *self
            .host
            .storage
            .activity(actor)
            .ok_or(Error::Unavailable)?
        else {
            return Ok(());
        };
        let world = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        let paired = self.host.storage.activity(teacher) == self.host.storage.activity(student);
        if !world.lesson_can_finish(
            game_lineage::LessonProgress {
                teacher,
                student,
                topic,
                completes,
            },
            &self.lineage_participant(teacher),
            &self.lineage_participant(student),
            paired,
            self.host.clock().time,
        ) {
            self.interrupt_lesson(actor);
            return Ok(());
        }
        let opening = self.host.transaction_opening();
        self.host.preflight_records(0, 1, 0)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut writes = game_ecs::ReadFootprint::new();
        for person in [teacher, student] {
            let id = game_ecs::StableId(person.0);
            reads.read::<Activity>(&self.host.storage, id);
            reads.read::<Location>(&self.host.storage, id);
            reads.read::<game_physiology::Body>(&self.host.storage, id);
            reads.read::<game_lifecycle::Life>(&self.host.storage, id);
            writes.protect::<Activity>(id);
        }
        let lesson_gain = world.rules.lesson_gain;
        let (gain, learning, minds) = match topic {
            LessonTopic::Invocation => {
                let learning = world.prepare_learning(&self.host.storage, student, lesson_gain)?;
                (
                    *learning.output(),
                    Some(learning),
                    game_characters::receivers::PreparedMindEffects::default(),
                )
            }
            LessonTopic::Craft | LessonTopic::Travel => {
                let knowledge = match topic {
                    LessonTopic::Craft => {
                        reads.read::<game_skills::Skills>(
                            &self.host.storage,
                            game_ecs::StableId(teacher.0),
                        );
                        game_characters::receivers::LessonKnowledge::Craft {
                            skill_level: self.own_skill_level(teacher, Skill::Craft),
                            lesson_gain,
                        }
                    }
                    _ => game_characters::receivers::LessonKnowledge::Travel { lesson_gain },
                };
                let learning = game_characters::receivers::prepare_lineage_lesson::<Command>(
                    &self.host.storage,
                    teacher,
                    student,
                    knowledge,
                )?;
                (learning.gain(), None, learning.into_mind_effects())
            }
        };
        let event = EventKind::LessonCompleted {
            teacher,
            student,
            topic,
            gain,
        };
        let mut batch = game_ecs::Batch::new();
        for person in [teacher, student] {
            batch.insert(game_ecs::StableId(person.0), Activity::Idle);
        }
        let mut guards = vec![reads.bind(&self.host.storage, &batch, &writes)?];
        let traits =
            if let Some(prepared) = self.prepare_trait_events(std::slice::from_ref(&event))? {
                Some(
                    prepared.fold_minds(&self.host.storage, minds)?.stage_into(
                        self.host
                            .state
                            .temperament
                            .as_ref()
                            .ok_or(Error::Unavailable)?,
                        &self.host.storage,
                        &mut batch,
                        &self
                            .host
                            .state
                            .genetics
                            .as_ref()
                            .ok_or(Error::Unavailable)?
                            .allele_source(&self.host.storage),
                    )?,
                )
            } else {
                guards.push(minds.stage_into(&self.host.storage, &mut batch)?);
                None
            };
        if let Some(traits) = &traits {
            self.validate_trait_events(traits, &batch)?;
            guards.extend(traits.row_guards().iter().cloned());
        }
        let witness = self.prepare_lineage_witness(&event, &batch)?;
        enum StagedLesson {
            Invocation(game_lineage::StagedLineage<u64>),
            ObservedInvocation(StagedLineageWitness<u64>),
            Observation(StagedLineageWitness),
        }
        let lineage = if let Some(learning) = learning {
            if let Some(witness) = witness {
                let staged = witness
                    .fold_into(learning, self)?
                    .stage_into(self, &mut batch)?;
                staged.validate(self, &batch)?;
                guards.extend(staged.row_guards());
                Some(StagedLesson::ObservedInvocation(staged))
            } else {
                let staged = learning.stage_into(world, &self.host.storage, &mut batch)?;
                staged.validate(world, &self.host.storage, &batch)?;
                guards.push(staged.guard().clone());
                Some(StagedLesson::Invocation(staged))
            }
        } else {
            if let Some(witness) = witness {
                let staged = witness.stage_into(self, &mut batch)?;
                staged.validate(self, &batch)?;
                guards.extend(staged.row_guards());
                Some(StagedLesson::Observation(staged))
            } else {
                None
            }
        };
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards = guards;
        transaction.events = vec![event];
        self.host.commit_transaction(transaction)?;
        if let Some(lineage) = lineage {
            match lineage {
                StagedLesson::Invocation(staged) => {
                    staged.finish();
                }
                StagedLesson::ObservedInvocation(staged) => {
                    staged.finish();
                }
                StagedLesson::Observation(staged) => {
                    staged.finish();
                }
            }
        }
        for person in [teacher, student] {
            for due in self.host.state.due.values_mut() {
                due.retain(|event| !matches!(event, Due::Physical(id) if *id == person));
            }
        }
        self.host.state.due.retain(|_, due| !due.is_empty());
        Ok(())
    }

    pub(crate) fn request_guardian(
        &mut self,
        requester: PersonId,
        actor: PersonId,
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
            return self.request_guardian_supply(actor, requester, good, source, quantity);
        }
        let world = self.host.state.lineage.as_ref().ok_or(Error::Unavailable)?;
        let requester_facts = self.lineage_participant(requester);
        let mind = game_characters::decisions::mind::<Command>(&self.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        let location = game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
            .ok_or(Error::Unavailable)?;
        let facts = game_lineage::RequestFacts {
            now: owner_now,
            requester_independent: requester_facts.independent,
            requester_alive: requester_facts.alive,
            requester_idle: requester_facts.idle,
            requester_in_combat: requester_facts.in_combat,
            requester_restricted: requester_facts.restricted,
            requester_in_care: requester_facts.in_care,
            requester_location: requester_facts.location,
            guardian_location: location.clone(),
            guardian_constrained: self.guardian_constraint(actor).is_some(),
            guardian_in_combat: self.in_combat(actor),
            guardian_duty_pending: !matches!(
                self.host.storage.activity(actor),
                Some(Activity::Idle | Activity::Rest)
            ),
            signal_urgency: mind
                .child_signals
                .get(&requester)
                .map_or(0, |signal| signal.urgency),
        };
        let prepared =
            world.prepare_request(&self.host.storage, actor, requester, request.clone(), facts)?;
        self.ensure_event_capacity(1)?;
        if let game_lineage::RequestDecision::Refused(reason) = prepared.output().clone() {
            self.commit_lineage(prepared)?;
            self.emit(EventKind::GuardianRefused {
                guardian: actor,
                requester,
                reason: reason.into(),
            });
            self.host.bump_revision(1)?;
            return Ok(());
        }
        let command = match request {
            GuardianRequest::Move { destination } => Command::Move { destination },
            GuardianRequest::Rest => Command::Rest,
            GuardianRequest::Work { site } => Command::Work { site },
            GuardianRequest::Supply { .. } => unreachable!("supply bound separately"),
        };
        let result = match command {
            Command::Move { destination } => self.start_movement(actor, destination),
            command => self.debug_actor_command(actor, command),
        };
        match result {
            Ok(()) => {
                let change = self
                    .host
                    .state
                    .lineage
                    .as_ref()
                    .unwrap()
                    .prepare_request_success(&self.host.storage, actor, request, owner_now)?;
                self.commit_lineage(change)?;
                self.emit(EventKind::GuardianAccepted {
                    guardian: actor,
                    requester,
                });
                Ok(())
            }
            Err(Error::Overflow) => Err(Error::Overflow),
            Err(_) => {
                let change = self
                    .host
                    .state
                    .lineage
                    .as_ref()
                    .unwrap()
                    .prepare_request_failure(&self.host.storage, actor)?;
                let game_lineage::RequestDecision::Refused(reason) = self.commit_lineage(change)?
                else {
                    unreachable!()
                };
                self.emit(EventKind::GuardianRefused {
                    guardian: actor,
                    requester,
                    reason: reason.into(),
                });
                self.host.bump_revision(1)?;
                Ok(())
            }
        }
    }

    pub(crate) fn review_lineage(&mut self) -> Result<(), Error> {
        let Some(world) = self.host.state.lineage.as_ref() else {
            return Ok(());
        };
        let current = world.current(&self.host.storage);
        let ids: Vec<_> = game_lineage::guardians(&self.host.storage)
            .map(|(id, _)| id)
            .collect();
        for actor in ids {
            let now = self.host.clock().time;
            if let Some(target) = current {
                let mind =
                    game_characters::decisions::mind::<Command>(&self.host.storage, actor).unwrap();
                let prepared = self
                    .host
                    .state
                    .lineage
                    .as_ref()
                    .unwrap()
                    .prepare_stage_sighting(
                        &self.host.storage,
                        actor,
                        guardian_traits(&mind.personality),
                        game_lifecycle::life(&self.host.storage, target)
                            .unwrap()
                            .stage,
                        now,
                        game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
                            .unwrap(),
                        game_spatial::location(&self.host.storage, game_ecs::StableId(target.0))
                            .unwrap(),
                        game_physiology::body(&self.host.storage, actor)
                            .unwrap()
                            .vitals
                            .health,
                    )?;
                self.commit_lineage(prepared)?;
            }
            let escape = self.force_candidate(actor);
            let delivery = match game_lineage::review_disposition(
                escape.is_some(),
                game_lineage::DeliveryAttempt::NotAttempted,
            ) {
                game_lineage::ReviewDisposition::AttemptDelivery => {
                    if self.drive_guardian_delivery(actor)? {
                        game_lineage::DeliveryAttempt::Occupied
                    } else {
                        game_lineage::DeliveryAttempt::Unavailable
                    }
                }
                _ => game_lineage::DeliveryAttempt::NotAttempted,
            };
            if game_lineage::review_disposition(escape.is_some(), delivery)
                == game_lineage::ReviewDisposition::Complete
            {
                continue;
            }
            let sensed = current.map(|target| self.resolved_location(target));
            let own = self.resolved_location(actor);
            let transport_rescue =
                current.and_then(|target| self.transport_guardian_rescue(actor, target));
            let force_rescue = current.and_then(|target| self.force_guardian_rescue(actor, target));
            let mind =
                game_characters::decisions::mind::<Command>(&self.host.storage, actor).unwrap();
            let need = current
                .and_then(|target| mind.child_signals.get(&target))
                .map(|signal| NeedObservation {
                    location: signal.location.clone(),
                    observed_at: signal.observed_at,
                    urgency: signal.urgency,
                    hydration_urgency: signal.hydration_urgency,
                    preference: signal.preference,
                });
            let applicable = current.and_then(|target| {
                self.host
                    .state
                    .lineage
                    .as_ref()
                    .unwrap()
                    .care_applicability(
                        self.dependent_signal_visible(actor, target),
                        self.host.state.development.as_ref().and_then(|_| {
                            game_lifecycle::life(&self.host.storage, target).map(|life| life.stage)
                        }),
                    )
            });
            let activity = self.host.storage.activity(actor).unwrap();
            let facts = ReviewFacts {
                now,
                location: &own,
                activity: match activity {
                    Activity::Idle => GuardianActivity::Idle,
                    Activity::Rest => GuardianActivity::Rest,
                    Activity::Work { site } => GuardianActivity::Work(*site),
                    _ => GuardianActivity::Other,
                },
                confined: game_conflict::captivity(&self.host.storage, actor).is_some(),
                incapacitated: game_physiology::body(&self.host.storage, actor)
                    .unwrap()
                    .vitals
                    .health
                    == 0,
                combat_restricted: self.combat_restricts(actor),
                in_combat: self.in_combat(actor),
                serving_objective: self.mission_actor_busy(actor),
            };
            let assistance = current.map(|target| game_lineage::AssistanceFacts {
                target_location: sensed.as_ref().unwrap(),
                need: need.clone(),
                transport_rescue: transport_rescue.is_some(),
                force_rescue: force_rescue.is_some(),
                provision: Some(ProvisionFacts {
                    now,
                    together: self.transport_same_place(actor, target),
                    applicable,
                    need: need.as_ref(),
                    doses: CareDoses {
                        food: self.body_rules(actor).meal_size,
                        water: self.care_dose(Good::Water).unwrap_or(Quantity(0)),
                    },
                }),
            });
            let prepared = self.host.state.lineage.as_ref().unwrap().prepare_review(
                &self.host.storage,
                actor,
                facts,
                sensed.clone(),
                escape.is_some(),
                assistance,
            )?;
            let decision = self.commit_lineage(prepared)?;
            let command = match decision.action {
                Some(ReviewAction::Escape) => escape,
                Some(ReviewAction::TransportRescue) => transport_rescue,
                Some(ReviewAction::ForceRescue) => force_rescue,
                Some(ReviewAction::Physical(action)) => Some(match action {
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
                }),
                Some(ReviewAction::Provision(provision)) => {
                    let target = current.expect("living care destination");
                    let destination = game_logistics::KnownDestination {
                        destination: provision.delivery.destination,
                        location: self.delivery_observed_location(target),
                        observed_at: now,
                        accessible: true,
                        room: None,
                        direct_transfer: true,
                    };
                    let step = self.plan_delivery(
                        actor,
                        &provision.delivery,
                        &destination,
                        provision.delivery.requirement.quantity,
                    );
                    match step {
                        game_logistics::Step::Arrived { source, .. } => Some(Command::Care {
                            child: target,
                            source,
                        }),
                        step => self.delivery_command(actor, step),
                    }
                }
                None => None,
            };
            if let Some(command) = command {
                let result = match command {
                    Command::Move { destination } => self.start_movement(actor, destination),
                    command => self.debug_actor_command(actor, command),
                };
                if result.is_err() {
                    let prepared = self
                        .host
                        .state
                        .lineage
                        .as_ref()
                        .unwrap()
                        .prepare_failed_attempt(&self.host.storage, actor)?;
                    self.commit_lineage(prepared)?;
                }
            }
        }
        Ok(())
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
    let fail = || Error::InvalidSave("inconsistent composed lineage".into());
    let Some(world) = state.state.lineage.as_ref() else {
        return if state
            .components
            .iter::<game_lineage::History>()
            .next()
            .is_none()
            && game_lineage::guardians(state.components).next().is_none()
            && state
                .components
                .iter::<game_lineage::InvocationLearning>()
                .next()
                .is_none()
        {
            Ok(())
        } else {
            Err(fail())
        };
    };
    if state.state.physical.is_none() || state.state.decisions.is_none() {
        return Err(fail());
    }
    let development = state.state.development.as_ref().ok_or_else(fail)?;
    world.validate_references(
        state.components,
        state.time,
        state.next_event,
        state.state.ruler,
        &LineageReferencesAdapter(state),
    )?;
    fn progress(activity: Option<&Activity>) -> Option<game_lineage::LessonProgress> {
        let Activity::Lesson {
            teacher,
            student,
            topic,
            completes,
        } = activity?
        else {
            return None;
        };
        Some(game_lineage::LessonProgress {
            teacher: *teacher,
            student: *student,
            topic: *topic,
            completes: *completes,
        })
    }
    for (actor, activity) in state.components.activities() {
        let Some(lesson) = progress(Some(activity)) else {
            continue;
        };
        let life = game_lifecycle::life(state.components, lesson.student).ok_or_else(fail)?;
        let partner = if actor == lesson.teacher {
            lesson.student
        } else {
            lesson.teacher
        };
        let teacher_location =
            game_spatial::location(state.components, game_ecs::StableId(lesson.teacher.0))
                .ok_or_else(fail)?;
        let student_location =
            game_spatial::location(state.components, game_ecs::StableId(lesson.student.0))
                .ok_or_else(fail)?;
        let scheduled = [lesson.teacher, lesson.student].map(|id| {
            state
                .state
                .due
                .get(&lesson.completes)
                .is_some_and(|events| {
                    events
                        .iter()
                        .any(|event| matches!(event, Due::Physical(actor) if *actor == id))
                })
        });
        world.validate_lesson(
            state.components,
            state.time,
            game_lineage::LessonSaveFacts {
                actor,
                progress: lesson,
                partner: progress(state.components.activity(partner)),
                teacher_location,
                student_location,
                born: life.born,
                initial_age: life.initial_age,
                infancy: development.rules.infancy,
                scheduled,
            },
        )?;
    }
    Ok(())
}
struct LineageReferencesAdapter<'a>(&'a CanonicalState<'a>);
impl game_lineage::LineageReferences for LineageReferencesAdapter<'_> {
    fn people(&self) -> impl Iterator<Item = game_lineage::PersonReference> {
        self.0.people().filter_map(|person| {
            let life = game_lifecycle::life(self.0.components, person.id)?;
            let body = game_physiology::body(self.0.components, person.id)?;
            Some(game_lineage::PersonReference {
                person: person.id,
                born: life.born,
                initial_age: life.initial_age,
                parent_count: life.parents.len(),
                alive: body.vitals.alive,
            })
        })
    }
    fn guardian_people(&self) -> impl Iterator<Item = PersonId> {
        self.0
            .people()
            .filter(|person| person.role == Role::Guardian)
            .map(|person| person.id)
    }
    fn recorded_death(&self, person: PersonId, at: SimTime) -> bool {
        self.0.events.iter().any(|event| {
            event.at == at && matches!(event.event, EventKind::Died { actor } if actor == person)
        })
    }
    fn stock(&self, id: InventoryId) -> Option<game_lineage::StockReference> {
        let stock = game_cargo::inventory(self.0.components, id)?;
        Some(game_lineage::StockReference {
            good: stock.good,
            quantity: stock.quantity,
            holder: match stock.location {
                InventoryLocation::Carried(actor) => Some(actor),
                _ => None,
            },
        })
    }
    fn area_exists(&self, id: AreaId) -> bool {
        self.0.state.areas.iter().any(|area| area.id == id)
    }
    fn site_exists(&self, id: SiteId) -> bool {
        game_workshop::site(self.0.components, id).is_some()
    }
}

#[cfg(test)]
mod lesson_publication_tests {
    use super::*;

    fn ready(topic: LessonTopic) -> (Simulation, PersonId, PersonId) {
        let mut sim = Simulation::lineage_fixture(9, Execution::default()).unwrap();
        sim.initialize_genetics(genetics::Config::default())
            .unwrap();
        let student = sim.host.state.ruler;
        let teacher = game_lineage::guardians(&sim.host.storage).next().unwrap().0;
        game_characters::decisions::mind_mut::<Command>(&mut sim.host.storage, teacher)
            .unwrap()
            .interests
            .insert(ChildhoodInterest::Craft, 100);
        let completes = sim.host.clock().time;
        for actor in [teacher, student] {
            sim.host
                .storage
                .set_activity(
                    actor,
                    Activity::Lesson {
                        teacher,
                        student,
                        topic,
                        completes,
                    },
                )
                .unwrap();
            sim.host
                .state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(actor));
        }
        (sim, teacher, student)
    }

    #[test]
    fn invocation_craft_and_travel_complete_with_one_receipt_and_one_guidance_credit() {
        for topic in [
            LessonTopic::Invocation,
            LessonTopic::Craft,
            LessonTopic::Travel,
        ] {
            let (mut sim, teacher, student) = ready(topic);
            let before_events = sim.host.events().len();
            let before_teacher =
                game_characters::temperament::person_temperament(&sim.host.storage, teacher)
                    .unwrap()
                    .learned_offsets;
            let before_student =
                game_characters::temperament::person_temperament(&sim.host.storage, student)
                    .unwrap()
                    .learned_offsets;
            sim.complete_lesson(student).unwrap();
            assert_eq!(sim.host.events().len(), before_events + 1);
            assert!(
                matches!(sim.host.events().last().unwrap().event, EventKind::LessonCompleted { topic: actual, .. } if actual == topic)
            );
            for (actor, before) in [(teacher, before_teacher), (student, before_student)] {
                let after =
                    game_characters::temperament::person_temperament(&sim.host.storage, actor)
                        .unwrap();
                assert_eq!(
                    after.learned_offsets,
                    [before[0] + 1, before[1] + 1, before[2], before[3]]
                );
                assert_eq!(sim.host.storage.activity(actor), Some(&Activity::Idle));
                assert!(
                    sim.host
                        .state
                        .due
                        .values()
                        .flatten()
                        .all(|due| !matches!(due, Due::Physical(id) if *id == actor))
                );
            }
            match topic {
                LessonTopic::Invocation => assert_eq!(
                    game_lineage::invocation_learning(&sim.host.storage, student),
                    100
                ),
                LessonTopic::Craft => assert_eq!(
                    game_characters::decisions::mind::<Command>(&sim.host.storage, student)
                        .unwrap()
                        .interests[&ChildhoodInterest::Craft],
                    100
                ),
                LessonTopic::Travel => {
                    let teacher =
                        game_characters::decisions::mind::<Command>(&sim.host.storage, teacher)
                            .unwrap();
                    let student =
                        game_characters::decisions::mind::<Command>(&sim.host.storage, student)
                            .unwrap();
                    for (from, links) in &teacher.routes {
                        for (to, duration) in links {
                            assert_eq!(student.routes[from][to], *duration);
                        }
                    }
                }
            }
            sim.complete_lesson(teacher).unwrap();
            assert_eq!(sim.host.events().len(), before_events + 1);
        }
    }

    #[test]
    fn invalid_lesson_source_leaves_learning_activities_due_and_receipt_unpublished() {
        let (mut sim, teacher, student) = ready(LessonTopic::Craft);
        game_characters::decisions::mind_mut::<Command>(&mut sim.host.storage, teacher)
            .unwrap()
            .interests
            .insert(ChildhoodInterest::Craft, 1001);
        let before_rows = sim.host.storage.clone();
        let before_due = sim.host.state.due.clone();
        let before_events = sim.host.events().to_vec();
        assert!(sim.complete_lesson(student).is_err());
        assert_eq!(sim.host.storage, before_rows);
        assert_eq!(sim.host.state.due, before_due);
        assert_eq!(sim.host.events(), before_events);
    }
}
