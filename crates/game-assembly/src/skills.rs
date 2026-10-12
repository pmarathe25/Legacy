//! Training belongs to the actual person performing an action. Continuous work
//! and lessons accrue actual elapsed time; completed social and combat actions
//! carry bounded semantic credits. Merely waiting, traveling, or naming an idea
//! grants no mastery, and borrowing another life never changes these records.
use crate::*;

pub(crate) use game_skills::PracticeDelta;
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
        for person in &sim.host.state.people {
            game_skills::insert_skills(&mut sim.host.storage, person.id, Skills::default())?;
            if person.role == Role::Guardian {
                // These authored immortal mentors have practiced the lineage
                // method before the fixture begins. This is not a newborn gift.
                let prior_practice = world.rules.per_level.0 * u64::from(world.rules.max_level);
                world.credit(
                    &mut sim.host.storage,
                    person.id,
                    Skill::Invocation,
                    prior_practice,
                    None,
                )?;
            }
        }
        sim.host.state.skills = Some(world);
        Ok(sim)
    }

    pub(crate) fn evaluate_skills(&self, next: SimTime) -> Result<Vec<PracticeDelta>, Error> {
        let elapsed = next
            .0
            .checked_sub(self.host.clock().time.0)
            .ok_or(Error::Overflow)?;
        let (Some(skills), Some(_)) = (&self.host.state.skills, &self.host.state.physical) else {
            return Ok(Vec::new());
        };
        let equipment = self.equipment_availability();
        let mut pending = Vec::new();
        for (actor, body) in game_physiology::bodies(&self.host.storage) {
            let actor = &actor;
            let activity = self.host.storage.activity(*actor).expect("activity exists");
            if elapsed == 0
                || !body.vitals.alive
                || body.vitals.health == 0
                || !matches!(
                    *game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId((self.host.state.people[self.person_index[actor]].id).0)
                    )
                    .expect("location exists"),
                    Location::AtArea(_)
                )
            {
                continue;
            }
            let (skill, cap) = match *activity {
                Activity::Gather { .. } | Activity::Cultivate { .. }
                    if equipment.get(actor).copied().unwrap_or(false) =>
                {
                    (Skill::Farming, None)
                }
                Activity::Construct { .. } => (Skill::Craft, None),
                Activity::Play {
                    interest: ChildhoodInterest::Craft,
                    ..
                } => (Skill::Craft, None),
                Activity::Lesson {
                    teacher,
                    student,
                    topic,
                    ..
                } if student == *actor => {
                    match topic {
                        LessonTopic::Craft => (
                            Skill::Craft,
                            Some(skills.practice(&self.host.storage, teacher, Skill::Craft)),
                        ),
                        LessonTopic::Invocation => (Skill::Invocation, None),
                        // Route instruction supplies actual information, but does
                        // not pretend that sitting through it is farming/combat.
                        LessonTopic::Travel => continue,
                    }
                }
                Activity::Lesson { teacher, .. } if teacher == *actor => (Skill::Leadership, None),
                _ => {
                    let Some(skill) = self.health_practice_skill(*actor) else {
                        continue;
                    };
                    (skill, None)
                }
            };
            pending.push(PracticeDelta {
                actor: *actor,
                skill,
                milliseconds: elapsed,
                mastery_cap: cap,
            });
        }
        pending.extend(self.workshop_practice_deltas(elapsed, &equipment));
        pending.extend(self.social_practice_deltas(next));
        Ok(pending)
    }

    pub(crate) fn commit_skills(&mut self, pending: Vec<PracticeDelta>) {
        let Some(world) = &mut self.host.state.skills else {
            return;
        };
        world
            .prepare_practice(&self.host.storage, pending)
            .expect("validated practice effects")
            .commit(world, &mut self.host.storage)
            .expect("validated practice batch");
    }

    pub(crate) fn skill_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        let Some(world) = &self.host.state.skills else {
            return Ok(limit);
        };
        world.practice_boundary(
            &self.host.storage,
            self.host.clock().time,
            limit,
            &self.evaluate_skills(limit)?,
        )
    }

    pub(crate) fn credit_skill(&mut self, actor: PersonId, skill: Skill, milliseconds: u64) {
        let owner_now = self.host.clock().time;
        if milliseconds == 0 {
            return;
        }
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

    pub(crate) fn on_skill_event(&mut self, kind: &EventKind) {
        if let EventKind::Born { child, .. } = kind {
            if self.host.state.skills.is_some() {
                game_skills::insert_skills(&mut self.host.storage, *child, Skills::default())
                    .expect("validated newborn skills");
            }
            return;
        }
        let Some(world) = &self.host.state.skills else {
            return;
        };
        let credit = world.rules.action_practice_ms;
        let actors: Vec<_> = match kind {
            EventKind::AgreementActivated { agreement } => self
                .host
                .state
                .agreements
                .as_ref()
                .and_then(|_| game_agreements::agreement(&self.host.storage, *(agreement)))
                .map_or_else(Vec::new, |agreement| {
                    agreement
                        .accepted
                        .iter()
                        .map(|actor| (*actor, Skill::Negotiation))
                        .collect()
                }),
            EventKind::AgreementAmended { amendment, .. } => self
                .host
                .state
                .agreements
                .as_ref()
                .and_then(|_| game_agreements::amendment(&self.host.storage, *(amendment)))
                .map_or_else(Vec::new, |amendment| {
                    vec![(amendment.proposer, Skill::Negotiation)]
                }),
            // Produced/Cultivated/Practiced/LessonCompleted already have their
            // actual elapsed practice; completion must not add a second minute.
            _ => Vec::new(),
        };
        for (actor, skill) in actors {
            self.credit_skill(actor, skill, credit);
        }
    }

    pub(crate) fn own_skill_level(&self, actor: PersonId, skill: Skill) -> u16 {
        self.host
            .state
            .skills
            .as_ref()
            .map_or(0, |world| world.level(&self.host.storage, actor, skill))
    }

    pub(crate) fn own_skill_view(&self, actor: PersonId) -> Vec<SkillView> {
        let Some(_) = &self.host.state.skills else {
            return Vec::new();
        };
        Skill::ALL
            .into_iter()
            .map(|skill| {
                let owned = game_skills::skills(&self.host.storage, actor)
                    .and_then(|skills| skills.records.get(&skill))
                    .cloned()
                    .unwrap_or_default();
                SkillView {
                    skill,
                    level: owned.level,
                    practice_ms: owned.practice_ms,
                    progress_ms: owned.progress_ms,
                }
            })
            .collect()
    }

    pub(crate) fn effective_work_rate(&self, actor: PersonId, skill: Skill) -> SkillRate {
        let rate = self
            .host
            .state
            .skills
            .as_ref()
            .map_or_else(SkillRate::default, |world| {
                rate_for_level(self.own_skill_level(actor, skill), &world.rules)
            });
        self.genetic_work_rate(actor, rate)
            .expect("validated skill and phenotype bounds")
    }

    pub(crate) fn effective_combat_damage(&self, actor: PersonId, base: u64) -> Result<u64, Error> {
        let trained = match &self.host.state.skills {
            Some(world) => damage_for_level(
                base,
                self.own_skill_level(actor, Skill::Combat),
                &world.rules,
            )?,
            None => base,
        };
        u64::try_from(u128::from(trained) * u128::from(self.genetic_strength_milli(actor)) / 1_000)
            .map_err(|_| Error::Overflow)
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent personal skills".into());
    let Some(runtime) = &state.state.skills else {
        if game_skills::all_skills(state.components).next().is_some() {
            return Err(fail());
        }
        return Ok(());
    };
    if state.state.physical.is_none() {
        return Err(fail());
    }
    runtime
        .validate(state.components, state.time)
        .map_err(|_| fail())?;
    if game_skills::all_skills(state.components)
        .any(|(actor, _)| !state.state.people.iter().any(|person| person.id == actor))
    {
        return Err(fail());
    }
    Ok(())
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
            .host
            .state
            .people
            .iter()
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
        let target = fighter.host.state.people[3].id;
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
        let teacher = *sim
            .host
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .next()
            .unwrap();
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
            sim.host
                .state
                .lineage
                .as_ref()
                .unwrap()
                .invocation_learning
                .get(&student)
                .copied()
                .unwrap_or(0),
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
            sim.host.state.lineage.as_ref().unwrap().invocation_learning[&student],
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
            .host
            .state
            .people
            .iter()
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
