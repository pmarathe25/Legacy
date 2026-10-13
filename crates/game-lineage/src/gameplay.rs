//! Lineage gameplay policy from factual participants and personal observations.
use crate::*;
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct ParticipantFacts {
    pub person: PersonId,
    pub location: Location,
    pub alive: bool,
    pub health: u64,
    pub idle: bool,
    pub independent: bool,
    pub in_combat: bool,
    pub restricted: bool,
    pub in_care: bool,
}

impl ParticipantFacts {
    fn can_offer(&self) -> bool {
        self.idle && !self.in_combat && !self.restricted && !self.in_care
    }
    fn can_learn(&self) -> bool {
        self.independent
            && self.alive
            && self.health > 0
            && self.idle
            && !self.in_combat
            && !self.restricted
            && !self.in_care
            && matches!(self.location, Location::AtArea(_))
    }
    fn can_teach(&self) -> bool {
        self.alive
            && self.health > 0
            && self.idle
            && !self.in_combat
            && !self.restricted
            && !self.in_care
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SiteObservation {
    pub site: SiteId,
    pub area: AreaId,
    pub access: bool,
}
pub struct TeacherFacts {
    pub participant: ParticipantFacts,
    pub craft_level: u16,
    pub craft_interest: u64,
    pub knows_routes: bool,
    pub signal_urgency: i64,
    pub stocks: Vec<StockKnowledge>,
    pub destinations: Vec<(AreaId, bool)>,
    pub sites: Vec<SiteObservation>,
}
fn knows_topic(guardian: &Guardian, facts: &TeacherFacts, topic: LessonTopic) -> bool {
    match topic {
        LessonTopic::Invocation => guardian.knows_invocation,
        LessonTopic::Craft => facts.craft_level > 0 || facts.craft_interest > 0,
        LessonTopic::Travel => facts.knows_routes,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LessonDecision {
    Begin {
        teacher: PersonId,
        student: PersonId,
        topic: LessonTopic,
        completes: SimTime,
    },
    Refused(&'static str),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvailableCommand {
    Learn {
        teacher: PersonId,
        topic: LessonTopic,
    },
    Request {
        guardian: PersonId,
        request: GuardianRequest,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LessonProgress {
    pub teacher: PersonId,
    pub student: PersonId,
    pub topic: LessonTopic,
    pub completes: SimTime,
}
pub struct LessonInterruption {
    pub participants: [PersonId; 2],
    pub clear: [bool; 2],
}
pub fn lesson_interruption(
    lesson: LessonProgress,
    participating_lessons: [Option<LessonProgress>; 2],
) -> LessonInterruption {
    LessonInterruption {
        participants: [lesson.teacher, lesson.student],
        clear: participating_lessons.map(|lesson| lesson.is_some()),
    }
}
impl Runtime {
    pub fn care_applicability(&self, observed: bool, stage: Option<LifeStage>) -> Option<bool> {
        observed.then_some(matches!(stage, Some(LifeStage::Infant | LifeStage::Child)))
    }
    pub fn available_commands(
        &self,
        store: &Store,
        student: &ParticipantFacts,
        known_stocks: &BTreeSet<InventoryId>,
        teachers: impl IntoIterator<Item = TeacherFacts>,
    ) -> Vec<AvailableCommand> {
        if self.current(store) != Some(student.person) || !student.can_learn() {
            return Vec::new();
        }
        let mut teachers: Vec<_> = teachers.into_iter().collect();
        teachers.sort_by_key(|facts| facts.participant.person);
        let mut commands = Vec::new();
        for mut facts in teachers {
            facts.stocks.sort_by_key(|stock| stock.inventory);
            facts.destinations.sort_by_key(|(area, _)| *area);
            facts.sites.sort_by_key(|site| site.site);
            let actor = facts.participant.person;
            let Some(guardian) = guardian(store, actor) else {
                continue;
            };
            if !facts.participant.can_offer() || facts.participant.location != student.location {
                continue;
            }
            for topic in [
                LessonTopic::Invocation,
                LessonTopic::Craft,
                LessonTopic::Travel,
            ] {
                if judge_lesson(LessonJudgmentFacts {
                    teacher_knows_topic: knows_topic(guardian, &facts, topic),
                    immediate_care: facts.signal_urgency >= 20_000,
                })
                .is_ok()
                {
                    commands.push(AvailableCommand::Learn {
                        teacher: actor,
                        topic,
                    });
                }
            }
            commands.push(AvailableCommand::Request {
                guardian: actor,
                request: GuardianRequest::Rest,
            });
            for stock in &facts.stocks {
                if stock.good == Good::Food
                    && stock.accessible
                    && stock.quantity.0 >= 1_000
                    && known_stocks.contains(&stock.inventory)
                {
                    commands.push(AvailableCommand::Request {
                        guardian: actor,
                        request: GuardianRequest::Supply {
                            good: Good::Food,
                            source: stock.inventory,
                            quantity: Quantity(1_000),
                        },
                    });
                }
            }
            if guardian
                .assessments
                .get(&student.person)
                .is_some_and(|assessment| assessment.ready)
            {
                let Location::AtArea(area) = student.location else {
                    continue;
                };
                if facts.signal_urgency <= 0 {
                    for (destination, reachable) in &facts.destinations {
                        if *reachable && *destination != area {
                            commands.push(AvailableCommand::Request {
                                guardian: actor,
                                request: GuardianRequest::Move {
                                    destination: *destination,
                                },
                            });
                        }
                    }
                }
                for site in &facts.sites {
                    if site.area == area && site.access {
                        commands.push(AvailableCommand::Request {
                            guardian: actor,
                            request: GuardianRequest::Work { site: site.site },
                        });
                    }
                }
            }
        }
        commands
    }
    pub fn prepare_lesson_start(
        &self,
        store: &Store,
        student: &ParticipantFacts,
        teacher: &TeacherFacts,
        topic: LessonTopic,
        now: SimTime,
    ) -> Result<PreparedLineage<LessonDecision>, Error> {
        let actor = teacher.participant.person;
        let before = self.project(store, &[actor])?;
        if before.world.current() != Some(student.person)
            || actor == student.person
            || !student.can_learn()
            || !teacher.participant.can_teach()
            || teacher.participant.location != student.location
        {
            return Err(Error::Unavailable);
        }
        let mut after = before.world.clone();
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        let decision = match judge_lesson(LessonJudgmentFacts {
            teacher_knows_topic: knows_topic(guardian, teacher, topic),
            immediate_care: teacher.signal_urgency >= 20_000,
        }) {
            Err(reason) => {
                guardian.explanation = reason.into();
                LessonDecision::Refused(reason)
            }
            Ok(()) => {
                let completes = SimTime(
                    now.0
                        .checked_add(self.rules.lesson_duration.0)
                        .ok_or(Error::Overflow)?,
                );
                guardian.assignment = None;
                guardian.explanation = "The guided lesson is underway; both pupil and teacher are occupied until it completes or is interrupted".into();
                LessonDecision::Begin {
                    teacher: actor,
                    student: student.person,
                    topic,
                    completes,
                }
            }
        };
        self.change(before, after, decision)
    }
    pub fn lesson_can_finish(
        &self,
        lesson: LessonProgress,
        teacher: &ParticipantFacts,
        student: &ParticipantFacts,
        paired: bool,
        now: SimTime,
    ) -> bool {
        teacher.person == lesson.teacher
            && student.person == lesson.student
            && paired
            && lesson.completes == now
            && teacher.location == student.location
            && teacher.alive
            && student.alive
    }
    pub fn validate_lesson(
        &self,
        store: &Store,
        now: SimTime,
        facts: LessonSaveFacts<'_>,
    ) -> Result<(), Error> {
        let fail = || Error::InvalidSave("inconsistent lineage lesson".into());
        let lesson = facts.progress;
        if ![lesson.teacher, lesson.student].contains(&facts.actor)
            || lesson.teacher == lesson.student
            || guardian(store, lesson.teacher).is_none()
            || self.current(store) != Some(lesson.student)
            || facts.partner != Some(lesson)
            || facts.teacher_location != facts.student_location
        {
            return Err(fail());
        }
        let started = lesson
            .completes
            .0
            .checked_sub(self.rules.lesson_duration.0)
            .ok_or_else(fail)?;
        let age = started
            .checked_sub(facts.born.0)
            .and_then(|age| age.checked_add(facts.initial_age.0))
            .ok_or_else(fail)?;
        if started > now.0
            || age < facts.infancy.0
            || !facts.scheduled[0]
            || !facts.scheduled[1]
            || lesson.topic == LessonTopic::Invocation
                && !guardian(store, lesson.teacher)
                    .ok_or_else(fail)?
                    .knows_invocation
        {
            return Err(fail());
        }
        Ok(())
    }
}
pub struct LessonSaveFacts<'a> {
    pub actor: PersonId,
    pub progress: LessonProgress,
    pub partner: Option<LessonProgress>,
    pub teacher_location: &'a Location,
    pub student_location: &'a Location,
    pub born: SimTime,
    pub initial_age: SimDuration,
    pub infancy: SimDuration,
    pub scheduled: [bool; 2],
}

#[derive(Clone, Copy, Debug)]
pub enum WitnessConduct {
    Practice { gain: u64 },
    Meal,
    Production,
    Departure,
    Arrival,
    Care,
    Expression,
    Lesson { gain: u64 },
}
#[derive(Clone, Copy, Debug)]
pub struct WitnessEvent {
    pub actor: PersonId,
    pub conduct: WitnessConduct,
    pub departure: Option<AreaId>,
}
#[derive(Clone, Debug)]
pub struct WitnessFacts {
    pub actor: PersonId,
    pub location: Location,
    pub health: u64,
    pub traits: GuardianTraits,
    pub signal_urgency: i64,
}

pub(crate) fn apply_witness(
    after: &mut World,
    event: WitnessEvent,
    current_location: &Location,
    stage: LifeStage,
    now: SimTime,
    observers: impl IntoIterator<Item = WitnessFacts>,
) -> Result<(), Error> {
    if after.current() == Some(event.actor) {
        let area = event.departure.or(match current_location {
            Location::AtArea(area) => Some(*area),
            _ => None,
        });
        if let Some(area) = area {
            for facts in observers {
                crate::canonical_transitions::valid_traits(facts.traits)?;
                if facts.health == 0 || facts.location != Location::AtArea(area) {
                    continue;
                }
                let Some(guardian) = after.guardians.get_mut(&facts.actor) else {
                    continue;
                };
                let conduct = match event.conduct {
                    WitnessConduct::Practice { gain } => Conduct::Practice { gain },
                    WitnessConduct::Meal => Conduct::Meal,
                    WitnessConduct::Production => Conduct::Production,
                    WitnessConduct::Departure => Conduct::Departure {
                        known_distress: facts.signal_urgency > 0,
                    },
                    WitnessConduct::Arrival => Conduct::Arrival,
                    WitnessConduct::Care => Conduct::Care,
                    WitnessConduct::Expression => Conduct::Expression,
                    WitnessConduct::Lesson { gain } => Conduct::Lesson { gain },
                };
                let assessment = guardian.assessments.entry(event.actor).or_default();
                *assessment = observe_conduct(
                    assessment,
                    facts.traits,
                    Observation {
                        conduct,
                        stage,
                        at: now,
                        witnessed: true,
                    },
                );
            }
        }
    }
    Ok(())
}
impl Runtime {
    /// Ask which incarnation requires observer facts for this conduct. Other
    /// actors' receipts cannot change lineage assessments.
    pub fn witness_target(
        &self,
        store: &Store,
        event: WitnessEvent,
    ) -> Result<Option<PersonId>, Error> {
        self.rules.validate()?;
        let history = self.history(store)?;
        history.validate()?;
        Ok(history.current().filter(|actor| *actor == event.actor))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "event identity and independent factual observations"
    )]
    pub fn prepare_witness(
        &self,
        store: &Store,
        event: WitnessEvent,
        current_location: &Location,
        stage: LifeStage,
        now: SimTime,
        observers: impl IntoIterator<Item = WitnessFacts>,
    ) -> Result<PreparedLineage<()>, Error> {
        let history = self.project(store, &[])?;
        if history.world.current() != Some(event.actor) {
            let after = history.world.clone();
            return self.change(history, after, ());
        }
        let area = event.departure.or(match current_location {
            Location::AtArea(area) => Some(*area),
            _ => None,
        });
        let observers = observers.into_iter().collect::<Vec<_>>();
        let actors = observers
            .iter()
            .filter(|facts| {
                facts.health > 0
                    && area.is_some_and(|area| facts.location == Location::AtArea(area))
                    && guardian(store, facts.actor).is_some()
            })
            .map(|facts| facts.actor)
            .collect::<Vec<_>>();
        let mut before = self.project(store, &actors)?;
        if area.is_some() {
            before.reads.membership::<Guardian>(store, |_, _| true);
        }
        let mut after = before.world.clone();
        apply_witness(&mut after, event, current_location, stage, now, observers)?;
        self.change(before, after, ())
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "stage and local body and position facts are separate"
    )]
    pub fn prepare_stage_sighting(
        &self,
        store: &Store,
        actor: PersonId,
        traits: GuardianTraits,
        stage: LifeStage,
        now: SimTime,
        own: &Location,
        target: &Location,
        health: u64,
    ) -> Result<PreparedLineage<()>, Error> {
        self.prepare_stage_observation(
            store,
            actor,
            traits,
            stage,
            now,
            health > 0 && own == target && matches!(own, Location::AtArea(_)),
        )
    }
}

#[cfg(test)]
mod witness_composition_tests {
    use super::*;
    use game_ecs::{Batch, Schema, StableId};
    use std::collections::BTreeMap;

    fn fixture() -> (Runtime, Store) {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        schema
            .register::<u64>("test/guard", |value| {
                if *value > 1 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        store.spawn(StableId(1)).unwrap();
        store.spawn(StableId(2)).unwrap();
        let runtime = Runtime::initialize(
            &mut store,
            StableId(1),
            MentorshipRules::default(),
            History {
                incarnations: vec![Incarnation {
                    person: PersonId(1),
                    bound_at: SimTime(0),
                    died_at: None,
                }],
            },
            [(
                PersonId(2),
                Guardian {
                    target: Some(PersonId(1)),
                    sensed_location: Some(Location::AtArea(AreaId(7))),
                    sensed_at: SimTime(0),
                    goal: GuardianGoal::Mentoring,
                    explanation: "present".into(),
                    assessments: BTreeMap::new(),
                    assignment: None,
                    requested_delivery: None,
                    provision: None,
                    knows_invocation: true,
                },
            )],
        )
        .unwrap();
        let need = NeedObservation {
            location: Location::AtArea(AreaId(7)),
            observed_at: SimTime(1),
            urgency: 100,
            hydration_urgency: 0,
            preference: None,
        };
        runtime
            .prepare_care_provision(
                &store,
                PersonId(2),
                ProvisionFacts {
                    now: SimTime(1),
                    together: true,
                    applicable: Some(true),
                    need: Some(&need),
                    doses: CareDoses {
                        food: Quantity(1_000),
                        water: Quantity(250),
                    },
                },
            )
            .unwrap()
            .commit(&runtime, &mut store)
            .unwrap();
        (runtime, store)
    }
    fn observers() -> [WitnessFacts; 1] {
        [WitnessFacts {
            actor: PersonId(2),
            location: Location::AtArea(AreaId(7)),
            health: 100,
            traits: GuardianTraits {
                discipline: 0,
                patience: 0,
                risk_tolerance: 1_000,
            },
            signal_urgency: 0,
        }]
    }
    fn event(actor: u64) -> WitnessEvent {
        WitnessEvent {
            actor: PersonId(actor),
            conduct: WitnessConduct::Care,
            departure: None,
        }
    }

    #[test]
    fn pending_care_and_conduct_share_one_guardian_write_and_preserve_output() {
        let (runtime, mut store) = fixture();
        let combined = runtime
            .prepare_care_receipt(
                &store,
                PersonId(2),
                PersonId(1),
                Good::Food,
                Quantity(500),
                1,
            )
            .unwrap()
            .with_witness(
                &runtime,
                &store,
                event(1),
                &Location::AtArea(AreaId(7)),
                LifeStage::Adult,
                SimTime(2),
                observers(),
            )
            .unwrap();
        let mut batch = Batch::new();
        let staged = combined.stage_into(&runtime, &store, &mut batch).unwrap();
        assert_eq!(batch.edit_count(StableId(2)), 1);
        staged.validate(&runtime, &store, &batch).unwrap();
        let before = store.snapshot().unwrap();
        batch.insert(StableId(1), 2_u64);
        assert_eq!(store.apply_batch(batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);

        let combined = runtime
            .prepare_care_receipt(
                &store,
                PersonId(2),
                PersonId(1),
                Good::Food,
                Quantity(500),
                1,
            )
            .unwrap()
            .with_witness(
                &runtime,
                &store,
                event(1),
                &Location::AtArea(AreaId(7)),
                LifeStage::Adult,
                SimTime(2),
                observers(),
            )
            .unwrap();
        assert!(combined.commit(&runtime, &mut store).unwrap());
        let guardian = guardian(&store, PersonId(2)).unwrap();
        assert_eq!(
            guardian.provision.as_ref().unwrap().delivery.remaining,
            Quantity(500)
        );
        assert_eq!(guardian.assessments[&PersonId(1)].observed_at, SimTime(2));
    }

    #[test]
    fn irrelevant_conduct_does_not_lock_pending_guardian_effects() {
        let (runtime, store) = fixture();
        assert_eq!(runtime.witness_target(&store, event(2)).unwrap(), None);
        let witness = runtime
            .prepare_witness(
                &store,
                event(2),
                &Location::AtArea(AreaId(7)),
                LifeStage::Adult,
                SimTime(2),
                observers(),
            )
            .unwrap();
        let care = runtime
            .prepare_care_receipt(
                &store,
                PersonId(2),
                PersonId(1),
                Good::Food,
                Quantity(500),
                1,
            )
            .unwrap();
        let mut batch = Batch::new();
        let care = care.stage_into(&runtime, &store, &mut batch).unwrap();
        let witness = witness.stage_into(&runtime, &store, &mut batch).unwrap();
        witness.validate(&runtime, &store, &batch).unwrap();
        care.validate(&runtime, &store, &batch).unwrap();
        assert_eq!(batch.edit_count(StableId(2)), 1);
    }

    #[test]
    fn ordered_witness_fold_matches_separate_owner_publications() {
        let (runtime, store) = fixture();
        let mut expected = store.clone();
        let mut actual = store;
        let first = WitnessEvent {
            actor: PersonId(1),
            conduct: WitnessConduct::Departure,
            departure: Some(AreaId(7)),
        };
        let second = WitnessEvent {
            actor: PersonId(1),
            conduct: WitnessConduct::Arrival,
            departure: None,
        };
        for event in [first, second] {
            runtime
                .prepare_witness(
                    &expected,
                    event,
                    &Location::AtArea(AreaId(7)),
                    LifeStage::Adult,
                    SimTime(2),
                    observers(),
                )
                .unwrap()
                .commit(&runtime, &mut expected)
                .unwrap();
        }
        let combined = runtime
            .prepare_witness(
                &actual,
                first,
                &Location::AtArea(AreaId(7)),
                LifeStage::Adult,
                SimTime(2),
                observers(),
            )
            .unwrap()
            .with_witness(
                &runtime,
                &actual,
                second,
                &Location::AtArea(AreaId(7)),
                LifeStage::Adult,
                SimTime(2),
                observers(),
            )
            .unwrap();
        let mut batch = Batch::new();
        let staged = combined.stage_into(&runtime, &actual, &mut batch).unwrap();
        assert_eq!(batch.edit_count(StableId(2)), 1);
        staged.validate(&runtime, &actual, &batch).unwrap();
        actual.apply_batch(batch).unwrap();
        staged.finish();
        assert_eq!(actual.snapshot().unwrap(), expected.snapshot().unwrap());
    }
}
