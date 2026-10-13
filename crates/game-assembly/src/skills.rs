//! Training belongs to the actual person performing an action. Continuous work
//! and lessons accrue actual elapsed time; completed social and combat actions
//! carry bounded semantic credits. Merely waiting, traveling, or naming an idea
//! grants no mastery, and borrowing another life never changes these records.
use crate::*;

pub use game_skills::{
    Progress, Rules, Runtime, Skills, advance_work, damage_for_level, rate_for_level, work_duration,
};

impl Simulation {
    pub fn skills_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::lineage_fixture(seed, execution)?;
        sim.host.state.scenario = scenarios()
            .into_iter()
            .find(|scenario| scenario.id == "skills-fixture")
            .ok_or_else(|| Error::InvalidFixture("skills fixture is not registered".into()))?;
        let world = Runtime::default();
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            let person_role = sim.person(person_id).role;
            game_skills::insert_skills(&mut sim.host.storage, person_id, Skills::default())?;
            if person_role == Role::Guardian {
                // These authored immortal mentors have practiced the lineage
                // method before the fixture begins. This is not a newborn gift.
                let prior_practice = world.rules.per_level.0 * u64::from(world.rules.max_level);
                world.credit(
                    &mut sim.host.storage,
                    person_id,
                    Skill::Invocation,
                    prior_practice,
                    None,
                )?;
            }
        }
        sim.host.state.skills = Some(world);
        Ok(sim)
    }

    pub(crate) fn evaluate_skills(
        &self,
        next: SimTime,
    ) -> Result<Option<game_skills::EvaluatedLearning>, Error> {
        let (Some(runtime), Some(_)) = (&self.host.state.skills, &self.host.state.physical) else {
            return Ok(None);
        };
        let equipment = self.equipment_availability();
        let actors = game_physiology::bodies(&self.host.storage).map(|(actor, body)| {
            let activity = match *self.host.storage.activity(actor).expect("activity exists") {
                Activity::Gather { .. } => game_skills::ActivityFacts::Gather,
                Activity::Cultivate { .. } => game_skills::ActivityFacts::Cultivate,
                Activity::Construct { .. } => game_skills::ActivityFacts::Construct,
                Activity::Play { interest, .. } => game_skills::ActivityFacts::Play { interest },
                Activity::Lesson {
                    teacher,
                    student,
                    topic,
                    ..
                } => game_skills::ActivityFacts::Lesson {
                    teacher,
                    student,
                    topic: match topic {
                        LessonTopic::Craft => game_skills::Lesson::Craft,
                        LessonTopic::Invocation => game_skills::Lesson::Invocation,
                        LessonTopic::Travel => game_skills::Lesson::Travel,
                    },
                },
                Activity::Work { .. } => game_skills::ActivityFacts::Workshop,
                _ => game_skills::ActivityFacts::Other,
            };
            let care_job = self
                .host
                .state
                .health
                .as_ref()
                .and_then(|_| game_health::job(&self.host.storage, actor));
            game_skills::ActorFacts {
                actor,
                alive: body.vitals.alive,
                health: body.vitals.health,
                at_area: matches!(
                    game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)),
                    Some(Location::AtArea(_))
                ),
                equipped: equipment.get(&actor).copied().unwrap_or(false),
                activity,
                workshop_job: game_workshop::job(&self.host.storage, actor).is_some(),
                care_work: care_job.map(|job| match job.work {
                    game_health::Work::Treat { .. } => game_skills::CareWork::Treat,
                    game_health::Work::Boil { .. } => game_skills::CareWork::Boil,
                    game_health::Work::Shelter { .. } => game_skills::CareWork::Shelter,
                    game_health::Work::Drink { .. } => game_skills::CareWork::Drink,
                }),
                care_can_progress: care_job.is_some_and(|job| self.health_job_can_progress(job)),
            }
        });
        let interactions = self
            .host
            .state
            .social
            .iter()
            .flat_map(|_| {
                game_characters::social::interactions(&self.host.storage).map(|(_, row)| row)
            })
            .map(|interaction| {
                let kind = match *interaction {
                    social::Interaction::Talk { first, second, .. } => {
                        game_skills::InteractionFacts::Talk { first, second }
                    }
                    social::Interaction::Instruction {
                        teacher,
                        student,
                        topic,
                        mastery_cap,
                        ..
                    } => game_skills::InteractionFacts::Instruction {
                        teacher,
                        student,
                        skill: match topic {
                            InstructionTopic::Skill(skill) => Some(skill),
                            InstructionTopic::Routes => None,
                        },
                        mastery_cap,
                    },
                    social::Interaction::Practice { actor, skill, .. } => {
                        game_skills::InteractionFacts::Practice { actor, skill }
                    }
                };
                game_skills::SocialFacts {
                    interaction: kind,
                    can_progress: self.social_interaction_valid(interaction),
                    completes: interaction.completes(),
                }
            });
        runtime
            .evaluate_learning(
                &self.host.storage,
                self.host.clock().time,
                next,
                actors,
                interactions,
            )
            .map(Some)
    }

    pub(crate) fn validate_skill_output(
        &self,
        pending: &Option<game_skills::EvaluatedLearning>,
    ) -> Result<(), Error> {
        match (self.host.state.skills.as_ref(), pending.as_ref()) {
            (Some(runtime), Some(evaluation)) => evaluation.validate(runtime, &self.host.storage),
            (_, None) => Ok(()),
            (None, Some(_)) => Err(Error::Unavailable),
        }
    }

    pub(crate) fn commit_skills(
        &mut self,
        pending: Option<game_skills::EvaluatedLearning>,
    ) -> Result<(), Error> {
        let Some(evaluation) = pending else {
            return Ok(());
        };
        let runtime = self.host.state.skills.as_ref().ok_or(Error::Unavailable)?;
        runtime
            .prepare_learning(&self.host.storage, evaluation)?
            .commit(runtime, &mut self.host.storage)
    }

    pub(crate) fn skill_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        let Some(world) = &self.host.state.skills else {
            return Ok(limit);
        };
        let Some(evaluation) = self.evaluate_skills(limit)? else {
            return Ok(limit);
        };
        world.practice_boundary(
            &self.host.storage,
            self.host.clock().time,
            limit,
            evaluation.deltas(),
        )
    }

    #[cfg(test)]
    pub(crate) fn credit_skill(&mut self, actor: PersonId, skill: Skill, milliseconds: u64) {
        let owner_now = self.host.clock().time;
        if self.person_index.contains_key(&actor)
            && let Some(world) = &mut self.host.state.skills
        {
            world
                .credit_action(
                    &mut self.host.storage,
                    actor,
                    skill,
                    milliseconds,
                    owner_now,
                )
                .expect("validated semantic skill credit");
        }
    }

    pub(crate) fn stage_agreement_skill_events(
        &self,
        prepared: &game_agreements::PreparedTransition,
        batch: &mut game_ecs::Batch,
    ) -> Result<Option<game_skills::StagedPractice>, Error> {
        let Some(runtime) = self.host.state.skills.as_ref() else {
            return Ok(None);
        };
        let mut events = Vec::new();
        for event in &prepared.events {
            match event {
                game_agreements::TransitionEvent::Activated { agreement } => {
                    let closing = batch
                        .projected::<game_agreements::Agreement>(
                            &self.host.storage,
                            game_ecs::StableId(agreement.0),
                        )?
                        .ok_or(Error::Unavailable)?;
                    events.push(game_skills::LearningEvent::AgreementActivated {
                        accepted: closing
                            .accepted
                            .iter()
                            .copied()
                            .filter(|actor| self.person_index.contains_key(actor))
                            .collect(),
                    });
                }
                game_agreements::TransitionEvent::Amended { amendment, .. } => {
                    let closing = batch
                        .projected::<game_agreements::Amendment>(
                            &self.host.storage,
                            game_ecs::StableId(amendment.0),
                        )?
                        .ok_or(Error::Unavailable)?;
                    if self.person_index.contains_key(&closing.proposer) {
                        events.push(game_skills::LearningEvent::AgreementAmended {
                            proposer: closing.proposer,
                        });
                    }
                }
                _ => {}
            }
        }
        if events.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            runtime
                .prepare_events(&self.host.storage, events, self.host.clock().time)?
                .stage_into(runtime, &self.host.storage, batch)?,
        ))
    }

    pub(crate) fn own_skill_level(&self, actor: PersonId, skill: Skill) -> u16 {
        self.host
            .state
            .skills
            .as_ref()
            .map_or(0, |world| world.level(&self.host.storage, actor, skill))
    }

    pub(crate) fn own_skill_view(&self, actor: PersonId) -> Vec<SkillView> {
        self.host
            .state
            .skills
            .as_ref()
            .map_or_else(Vec::new, |runtime| runtime.view(&self.host.storage, actor))
    }

    pub(crate) fn effective_work_rate(&self, actor: PersonId, skill: Skill) -> SkillRate {
        let rate = self
            .host
            .state
            .skills
            .as_ref()
            .map_or_else(SkillRate::default, |world| {
                world.work_rate(&self.host.storage, actor, skill)
            });
        self.genetic_work_rate(actor, rate)
            .expect("validated skill and phenotype bounds")
    }

    pub(crate) fn effective_combat_damage(&self, actor: PersonId, base: u64) -> Result<u64, Error> {
        match &self.host.state.skills {
            Some(runtime) => runtime.combat_damage(
                &self.host.storage,
                actor,
                base,
                self.genetic_strength_milli(actor),
            ),
            None => game_skills::scale_damage(base, self.genetic_strength_milli(actor)),
        }
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let people: BTreeSet<_> = state.people().map(|person| person.id).collect();
    game_skills::validate_world(
        state.state.skills.as_ref(),
        state.components,
        state.time,
        state.state.physical.is_some(),
        |actor| people.contains(&actor),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim(workers: usize) -> Simulation {
        let mut sim = Simulation::skills_fixture(
            1,
            Execution {
                workers,
                reverse_jobs: true,
            },
        )
        .unwrap();
        let citizens: Vec<_> = sim
            .people()
            .filter(|p| p.role == Role::Citizen)
            .map(|p| p.id)
            .collect();
        for actor in citizens {
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        sim
    }

    fn local_site(sim: &Simulation) -> SiteId {
        game_workshop::sites(&sim.host.storage)
            .map(|(_, site)| site)
            .find(|site| site.area == AreaId(1))
            .unwrap()
            .id
    }

    #[test]
    fn completed_work_improves_actual_output_time_and_a_jobs_rate_stays_frozen() {
        let mut sim = sim(1);
        let actor = sim.host.state.ruler;
        let site = local_site(&sim);
        sim.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.own_skill_level(actor, Skill::Craft), 1);
        let rate = sim.effective_work_rate(actor, Skill::Craft);
        let duration = work_duration(60_000, rate, 0).unwrap();
        assert!(duration < MINUTE.0);
        sim.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        sim.advance(SimDuration(duration - 1)).unwrap();
        assert!(matches!(
            *sim.host.storage.activity(actor).expect("body exists"),
            Activity::Work { .. }
        ));
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(sim.host.events().iter().filter(|event| matches!(event.event, EventKind::Produced { actor: worker, .. } if worker == actor)).count(), 2);
        sim.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        let remaining_to_level = 120_000
            - sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                actor,
                Skill::Craft,
            );
        sim.advance(SimDuration(remaining_to_level)).unwrap();
        assert_eq!(sim.own_skill_level(actor, Skill::Craft), 2);
        let now_faster =
            work_duration(60_000, sim.effective_work_rate(actor, Skill::Craft), 0).unwrap();
        sim.advance(SimDuration(now_faster - remaining_to_level))
            .unwrap();
        assert!(matches!(
            *sim.host.storage.activity(actor).expect("body exists"),
            Activity::Work { .. }
        ));
        sim.advance(SimDuration(duration - now_faster)).unwrap();
        assert_eq!(sim.host.events().iter().filter(|event| matches!(event.event, EventKind::Produced { actor: worker, .. } if worker == actor)).count(), 3);
    }

    #[test]
    fn canceled_work_credits_only_elapsed_training_and_idle_or_travel_add_nothing() {
        let mut sim = sim(1);
        let actor = sim.host.state.ruler;
        sim.debug_actor_command(
            actor,
            Command::Work {
                site: local_site(&sim),
            },
        )
        .unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        sim.debug_actor_command(actor, Command::Cancel).unwrap();
        let practice = sim.host.state.skills.as_ref().unwrap().practice(
            &sim.host.storage,
            actor,
            Skill::Craft,
        );
        assert_eq!(practice, 30_000);
        sim.advance(MINUTE).unwrap();
        sim.debug_move_actor(actor, AreaId(2)).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                actor,
                Skill::Craft
            ),
            practice
        );
        assert_eq!(sim.own_skill_level(actor, Skill::Craft), 0);
        assert_eq!(sim.own_skill_level(actor, Skill::Farming), 0);
    }

    #[test]
    fn pending_training_is_passive_and_restores_exact_progress_with_different_workers() {
        let mut a = sim(1);
        let actor = a.host.state.ruler;
        a.debug_actor_command(
            actor,
            Command::Work {
                site: local_site(&a),
            },
        )
        .unwrap();
        a.advance(SimDuration(15_000)).unwrap();
        let mut b = Simulation::load(
            &a.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: false,
            },
        )
        .unwrap();
        let before = a.canonical_snapshot().unwrap();
        for _ in 0..100 {
            a.own_skill_view(actor);
            a.observe();
        }
        assert_eq!(a.canonical_snapshot().unwrap(), before);
        a.advance(SimDuration(45_000)).unwrap();
        for duration in [1, 14_999, 20_000, 10_000] {
            b.advance(SimDuration(duration)).unwrap();
        }
        assert_eq!(a.canonical_state(), b.canonical_state());
        assert_eq!(
            a.host
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(&a.host.storage, actor, Skill::Craft),
            60_000
        );
    }

    #[test]
    fn an_actual_newborn_does_not_receive_the_predecessors_practical_mastery() {
        let mut sim = Simulation::skills_fixture(1, Execution::default()).unwrap();
        let predecessor = sim.host.state.ruler;
        sim.debug_actor_command(
            predecessor,
            Command::Work {
                site: local_site(&sim),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.own_skill_level(predecessor, Skill::Craft), 1);
        sim.debug_injure(predecessor, 180_000).unwrap();
        sim.advance(SimDuration(900_000)).unwrap();
        let child = sim.host.state.ruler;
        assert_ne!(child, predecessor);
        assert!(
            sim.own_skill_view(child)
                .iter()
                .all(|skill| skill.practice_ms == 0 && skill.level == 0)
        );
        assert_eq!(sim.own_skill_level(predecessor, Skill::Craft), 1);
        let before = sim.host.storage.snapshot().unwrap();
        let _borrowed = rate_for_level(
            sim.own_skill_level(predecessor, Skill::Craft),
            &sim.host.state.skills.as_ref().unwrap().rules,
        );
        assert_eq!(sim.host.storage.snapshot().unwrap(), before);
    }

    #[test]
    fn practical_farming_and_real_combat_actions_train_distinct_skills() {
        let mut farmer = Simulation::farming_fixture(1, Execution::default()).unwrap();
        farmer.host.state.skills = Some(Runtime::default());
        let actor = farmer.host.state.ruler;
        let field = farmer
            .host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .fields(&farmer.host.storage)
            .find(|(_, field)| field.access == BTreeSet::from([actor]))
            .unwrap()
            .0;
        let area = farmer
            .host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .field(&farmer.host.storage, field)
            .unwrap()
            .area;
        farmer.debug_move_actor(actor, area).unwrap();
        farmer.advance(SimDuration(120_000)).unwrap();
        farmer
            .debug_actor_command(
                actor,
                Command::Cultivate {
                    field,
                    operation: FarmOperation::Prepare,
                },
            )
            .unwrap();
        farmer.advance(SimDuration(30_000)).unwrap();
        farmer.debug_actor_command(actor, Command::Cancel).unwrap();
        assert_eq!(
            farmer.host.state.skills.as_ref().unwrap().practice(
                &farmer.host.storage,
                actor,
                Skill::Farming
            ),
            30_000
        );
        assert_eq!(
            farmer.host.state.skills.as_ref().unwrap().practice(
                &farmer.host.storage,
                actor,
                Skill::Craft
            ),
            0
        );

        let mut fighter = sim(1);
        let actor = fighter.host.state.ruler;
        let target = fighter.person_at(3).id;
        fighter
            .submit(Submission {
                caller_sequence: 1,
                command: Command::Engage { target },
            })
            .unwrap();
        let encounter = fighter.observe().combat.unwrap().encounter;
        fighter
            .submit(Submission {
                caller_sequence: 2,
                command: Command::CombatTurn {
                    encounter,
                    movement: None,
                    action: CombatAction::Attack { target },
                },
            })
            .unwrap();
        assert_eq!(
            fighter.host.state.skills.as_ref().unwrap().practice(
                &fighter.host.storage,
                actor,
                Skill::Combat
            ),
            6_000
        );
        fighter.advance(SimDuration(6_000)).unwrap();
        fighter
            .submit(Submission {
                caller_sequence: 3,
                command: Command::CombatTurn {
                    encounter,
                    movement: None,
                    action: CombatAction::Wait,
                },
            })
            .unwrap();
        fighter.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            fighter.host.state.skills.as_ref().unwrap().practice(
                &fighter.host.storage,
                actor,
                Skill::Combat
            ),
            6_000
        );
        assert_eq!(
            fighter.host.state.skills.as_ref().unwrap().practice(
                &fighter.host.storage,
                actor,
                Skill::Farming
            ),
            0
        );
    }

    #[test]
    fn coupled_guidance_trains_actual_pupil_and_teacher_without_copying_the_experts_level() {
        let mut sim = sim(1);
        let student = sim.host.state.ruler;
        let teacher = game_lineage::guardians(&sim.host.storage).next().unwrap().0;
        assert_eq!(sim.own_skill_level(teacher, Skill::Invocation), 100);
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Learn {
                teacher,
                topic: LessonTopic::Invocation,
            },
        })
        .unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                student,
                Skill::Invocation
            ),
            30_000
        );
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                teacher,
                Skill::Leadership
            ),
            30_000
        );
        assert_eq!(sim.own_skill_level(student, Skill::Invocation), 0);
        sim.debug_actor_command(teacher, Command::Cancel).unwrap();
        assert_eq!(
            game_lineage::invocation_learning(&sim.host.storage, student),
            0
        );
        sim.submit(Submission {
            caller_sequence: 2,
            command: Command::Learn {
                teacher,
                topic: LessonTopic::Invocation,
            },
        })
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                student,
                Skill::Invocation
            ),
            90_000
        );
        assert_eq!(sim.own_skill_level(student, Skill::Invocation), 1);
        assert_eq!(sim.own_skill_level(teacher, Skill::Invocation), 100);
        assert_eq!(
            game_lineage::invocation_learning(&sim.host.storage, student),
            100
        );
        assert!(!sim.host.events().iter().any(|event| matches!(event.event, EventKind::Produced { actor, .. } if actor == student || actor == teacher)));
    }

    #[test]
    fn only_an_adopted_agreement_trains_negotiation_and_zero_time_churn_is_bounded() {
        let mut sim = Simulation::agreement_fixture(1, Execution::default()).unwrap();
        sim.host.state.skills = Some(Runtime::default());
        let actor = sim.host.state.ruler;
        let citizen = sim
            .people()
            .find(|person| person.role == Role::Citizen)
            .unwrap()
            .id;
        let stock = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.location == InventoryLocation::Area(AreaId(1)) && stock.good == Good::Food
            })
            .unwrap()
            .id;
        sim.debug_agreement_action(
            actor,
            AgreementAction::Propose {
                parties: BTreeSet::from([actor, citizen]),
                terms: vec![AgreementTerm::Access {
                    grantor: actor,
                    grantee: citizen,
                    inventory: stock,
                    allowed: true,
                }],
                procedure: None,
            },
        )
        .unwrap();
        let agreement = game_agreements::agreements(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap();
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                actor,
                Skill::Negotiation
            ),
            0
        );
        sim.debug_agreement_action(citizen, AgreementAction::Accept { agreement })
            .unwrap();
        assert!(
            game_cargo::inventory(&sim.host.storage, stock)
                .expect("inventory exists")
                .access
                .contains(&citizen)
        );
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                actor,
                Skill::Negotiation
            ),
            6_000
        );
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                citizen,
                Skill::Negotiation
            ),
            6_000
        );
        for _ in 0..100 {
            sim.credit_skill(actor, Skill::Negotiation, 6_000);
        }
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                actor,
                Skill::Negotiation
            ),
            6_000
        );
        sim.advance(MINUTE).unwrap();
        sim.credit_skill(actor, Skill::Negotiation, 6_000);
        assert_eq!(
            sim.host.state.skills.as_ref().unwrap().practice(
                &sim.host.storage,
                actor,
                Skill::Negotiation
            ),
            12_000
        );
    }
}
