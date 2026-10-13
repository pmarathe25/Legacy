//! Learning policy over facts supplied by activity owners.
use crate::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lesson {
    Craft,
    Invocation,
    Travel,
}

/// Activity identity only: the adapter does not choose a trained skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityFacts {
    Gather,
    Cultivate,
    Construct,
    Play {
        interest: ChildhoodInterest,
    },
    Lesson {
        teacher: PersonId,
        student: PersonId,
        topic: Lesson,
    },
    Workshop,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CareWork {
    Treat,
    Boil,
    Shelter,
    Drink,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorFacts {
    pub actor: PersonId,
    pub alive: bool,
    pub health: u64,
    pub at_area: bool,
    pub equipped: bool,
    pub activity: ActivityFacts,
    pub workshop_job: bool,
    pub care_work: Option<CareWork>,
    pub care_can_progress: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionFacts {
    Talk {
        first: PersonId,
        second: PersonId,
    },
    Instruction {
        teacher: PersonId,
        student: PersonId,
        skill: Option<Skill>,
        mastery_cap: u64,
    },
    Practice {
        actor: PersonId,
        skill: Skill,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocialFacts {
    pub interaction: InteractionFacts,
    pub can_progress: bool,
    pub completes: SimTime,
}

/// Receiving Skills owns which achievements earn bounded action credit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningEvent {
    Born { child: PersonId },
    AgreementActivated { accepted: Vec<PersonId> },
    AgreementAmended { proposer: PersonId },
}

/// Evaluated intervals retain the live teacher mastery actually used by policy.
/// Other teacher skills may change before the closing Skills phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluatedLearning {
    pub(crate) deltas: Vec<PracticeDelta>,
    pub(crate) teacher_caps: BTreeMap<PersonId, u64>,
    rules: Rules,
}
/// Holds the evaluated remainder exclusively until the native origin accepts.
/// Dropping a rejected origin's acknowledgement leaves its evaluation intact.
#[must_use = "finish after accepted publication; drop when its origin is rejected"]
pub struct PreparedLearningAcknowledgement<'a> {
    evaluation: &'a mut EvaluatedLearning,
    applied: Vec<(PersonId, u64)>,
}
impl PreparedLearningAcknowledgement<'_> {
    pub fn finish(self) {
        let actors: BTreeSet<_> = self.applied.iter().map(|(actor, _)| *actor).collect();
        self.evaluation
            .deltas
            .retain(|delta| !actors.contains(&delta.actor));
        for (actor, craft) in self.applied {
            if let Some(cap) = self.evaluation.teacher_caps.get_mut(&actor) {
                *cap = craft;
            }
        }
    }
}
impl EvaluatedLearning {
    pub fn deltas(&self) -> &[PracticeDelta] {
        &self.deltas
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if self.rules != runtime.rules {
            return Err(Error::Unavailable);
        }
        for (teacher, practice) in &self.teacher_caps {
            if !store.contains(game_ecs::StableId(teacher.0))
                || runtime.practice(store, *teacher, Skill::Craft) != *practice
            {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub fn prepare_acknowledgement<'a>(
        &'a mut self,
        runtime: &Runtime,
        store: &Store,
        batch: &game_ecs::Batch,
        receipt: &StagedPractice,
    ) -> Result<PreparedLearningAcknowledgement<'a>, Error> {
        self.validate(runtime, store)?;
        let mut applied = Vec::new();
        for actor in receipt.applied_learning_actors()? {
            let expected = self
                .deltas
                .iter()
                .filter(|delta| delta.actor == actor)
                .cloned()
                .collect::<Vec<_>>();
            let craft = receipt.projected_actor_learning(
                runtime,
                store,
                batch,
                actor,
                &expected,
                self.teacher_caps.get(&actor).copied(),
            )?;
            applied.push((actor, craft));
        }
        Ok(PreparedLearningAcknowledgement {
            evaluation: self,
            applied,
        })
    }
}

impl Runtime {
    /// Select actual interval practice, including pupil caps, without an
    /// application simulation or copies of canonical entity components.
    pub fn evaluate_learning(
        &self,
        store: &Store,
        from: SimTime,
        next: SimTime,
        actors: impl IntoIterator<Item = ActorFacts>,
        interactions: impl IntoIterator<Item = SocialFacts>,
    ) -> Result<EvaluatedLearning, Error> {
        self.rules.validate()?;
        let elapsed = next.0.checked_sub(from.0).ok_or(Error::Overflow)?;
        let mut pending = Vec::new();
        let mut teacher_caps = BTreeMap::new();
        if elapsed == 0 {
            return Ok(EvaluatedLearning {
                deltas: pending,
                teacher_caps,
                rules: self.rules.clone(),
            });
        }
        for facts in actors {
            if !facts.alive || facts.health == 0 || !facts.at_area {
                continue;
            }
            let lesson = match facts.activity {
                ActivityFacts::Gather | ActivityFacts::Cultivate if facts.equipped => {
                    Some((Skill::Farming, None))
                }
                ActivityFacts::Construct
                | ActivityFacts::Play {
                    interest: ChildhoodInterest::Craft,
                } => Some((Skill::Craft, None)),
                ActivityFacts::Lesson {
                    teacher,
                    student,
                    topic,
                } if student == facts.actor => match topic {
                    Lesson::Craft => {
                        let cap = self.practice(store, teacher, Skill::Craft);
                        teacher_caps.insert(teacher, cap);
                        Some((Skill::Craft, Some(cap)))
                    }
                    Lesson::Invocation => Some((Skill::Invocation, None)),
                    Lesson::Travel => None,
                },
                ActivityFacts::Lesson { teacher, .. } if teacher == facts.actor => {
                    Some((Skill::Leadership, None))
                }
                _ if facts.care_can_progress => match facts.care_work {
                    Some(CareWork::Treat | CareWork::Boil) => Some((Skill::Medicine, None)),
                    Some(CareWork::Shelter) => Some((Skill::Craft, None)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((skill, mastery_cap)) = lesson {
                pending.push(PracticeDelta {
                    actor: facts.actor,
                    skill,
                    milliseconds: elapsed,
                    mastery_cap,
                });
            }
            if facts.activity == ActivityFacts::Workshop && facts.workshop_job && facts.equipped {
                pending.push(PracticeDelta {
                    actor: facts.actor,
                    skill: Skill::Craft,
                    milliseconds: elapsed,
                    mastery_cap: None,
                });
            }
        }
        for facts in interactions {
            if !facts.can_progress {
                continue;
            }
            let milliseconds = next.0.min(facts.completes.0).saturating_sub(from.0);
            let mut credit = |actor, skill, mastery_cap| {
                pending.push(PracticeDelta {
                    actor,
                    skill,
                    milliseconds,
                    mastery_cap,
                })
            };
            match facts.interaction {
                InteractionFacts::Talk { first, second } => {
                    credit(first, Skill::Negotiation, None);
                    credit(second, Skill::Negotiation, None);
                }
                InteractionFacts::Instruction {
                    teacher,
                    student,
                    skill,
                    mastery_cap,
                } => {
                    if let Some(skill) = skill {
                        credit(student, skill, Some(mastery_cap));
                    }
                    credit(teacher, Skill::Leadership, None);
                }
                InteractionFacts::Practice { actor, skill } => credit(actor, skill, None),
            }
        }
        Ok(EvaluatedLearning {
            deltas: pending,
            teacher_caps,
            rules: self.rules.clone(),
        })
    }

    pub fn work_rate(&self, store: &Store, actor: PersonId, skill: Skill) -> SkillRate {
        rate_for_level(self.level(store, actor, skill), &self.rules)
    }

    pub fn combat_damage(
        &self,
        store: &Store,
        actor: PersonId,
        base: u64,
        strength_milli: u16,
    ) -> Result<u64, Error> {
        self.rules.validate()?;
        let trained = damage_for_level(base, self.level(store, actor, Skill::Combat), &self.rules)?;
        scale_damage(trained, strength_milli)
    }

    pub fn view(&self, store: &Store, actor: PersonId) -> Vec<SkillView> {
        skill_views(skills(store, actor))
    }
    pub fn view_projected(
        &self,
        store: &Store,
        batch: &game_ecs::Batch,
        actor: PersonId,
    ) -> Result<Vec<SkillView>, Error> {
        Ok(skill_views(
            batch.projected::<Skills>(store, game_ecs::StableId(actor.0))?,
        ))
    }
}

pub fn scale_damage(base: u64, strength_milli: u16) -> Result<u64, Error> {
    u64::try_from(u128::from(base) * u128::from(strength_milli) / 1_000)
        .map_err(|_| Error::Overflow)
}

/// The application supplies person membership and physical-runtime presence;
/// Skills owns all validation of its canonical rows and runtime prerequisites.
pub fn validate_world(
    runtime: Option<&Runtime>,
    store: &Store,
    now: SimTime,
    physical_present: bool,
    mut person_exists: impl FnMut(PersonId) -> bool,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent personal skills".into());
    let Some(runtime) = runtime else {
        return if all_skills(store).next().is_some() {
            Err(fail())
        } else {
            Ok(())
        };
    };
    if !physical_present {
        return Err(fail());
    }
    runtime.validate(store, now).map_err(|_| fail())?;
    if all_skills(store).any(|(actor, _)| !person_exists(actor)) {
        return Err(fail());
    }
    Ok(())
}

fn skill_views(row: Option<&Skills>) -> Vec<SkillView> {
    Skill::ALL
        .into_iter()
        .map(|skill| {
            let progress = row
                .and_then(|row| row.records.get(&skill))
                .cloned()
                .unwrap_or_default();
            SkillView {
                skill,
                level: progress.level,
                practice_ms: progress.practice_ms,
                progress_ms: progress.progress_ms,
            }
        })
        .collect()
}
