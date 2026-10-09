//! Training belongs to the actual person performing an action. Continuous work
//! and lessons accrue actual elapsed time; completed social and combat actions
//! carry bounded semantic credits. Merely waiting, traveling, or naming an idea
//! grants no mastery, and borrowing another life never changes these records.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub per_level: SimDuration,
    pub max_level: u16,
    pub work_bonus_per_level: u16,
    pub damage_bonus_per_level: u16,
    pub action_practice_ms: u64,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            // Explicit accelerated fixture rates; full settings can replace them.
            per_level: MINUTE,
            max_level: 100,
            work_bonus_per_level: 10,
            damage_bonus_per_level: 5,
            action_practice_ms: 6_000,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub practice_ms: u64,
    pub level: u16,
    pub progress_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub records: BTreeMap<PersonId, BTreeMap<Skill, Progress>>,
    #[serde(default)]
    pub semantic_ticks: BTreeMap<PersonId, BTreeMap<Skill, u64>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PracticeDelta {
    pub actor: PersonId,
    pub skill: Skill,
    pub milliseconds: u64,
    pub mastery_cap: Option<u64>,
}

impl World {
    pub fn level(&self, actor: PersonId, skill: Skill) -> u16 {
        self.records
            .get(&actor)
            .and_then(|skills| skills.get(&skill))
            .map_or(0, |p| p.level)
    }

    fn practice(&self, actor: PersonId, skill: Skill) -> u64 {
        self.records
            .get(&actor)
            .and_then(|skills| skills.get(&skill))
            .map_or(0, |p| p.practice_ms)
    }

    fn credit(
        &mut self,
        actor: PersonId,
        skill: Skill,
        milliseconds: u64,
        mastery_cap: Option<u64>,
    ) {
        if milliseconds == 0 {
            return;
        }
        let cap = self.rules.per_level.0 * u64::from(self.rules.max_level);
        let existing = self.practice(actor, skill);
        // A guide cannot teach beyond their personal mastery. A pupil with more
        // experience retains it; an inadequate guide never subtracts learning.
        let cap = mastery_cap.map_or(cap, |teacher| cap.min(teacher.max(existing)));
        let credited =
            (u128::from(existing) + u128::from(milliseconds)).min(u128::from(cap)) as u64;
        if credited == existing {
            return;
        }
        self.records
            .entry(actor)
            .or_default()
            .insert(skill, progress(credited, &self.rules));
    }
}

fn progress(practice_ms: u64, rules: &Rules) -> Progress {
    let level = (practice_ms / rules.per_level.0).min(u64::from(rules.max_level)) as u16;
    Progress {
        practice_ms,
        level,
        progress_ms: if level == rules.max_level {
            0
        } else {
            practice_ms % rules.per_level.0
        },
    }
}

pub fn rate_for_level(level: u16, rules: &Rules) -> SkillRate {
    SkillRate {
        numerator: 1_000
            + u64::from(level.min(rules.max_level)) * u64::from(rules.work_bonus_per_level),
        denominator: 1_000,
    }
}

pub fn advance_work(elapsed: u64, rate: SkillRate, remainder: u64) -> Result<(u64, u64), Error> {
    if rate.numerator == 0 || rate.denominator == 0 || remainder >= rate.denominator {
        return Err(Error::InvalidConfiguration(
            "invalid frozen skill work rate".into(),
        ));
    }
    let numerator = u128::from(elapsed) * u128::from(rate.numerator) + u128::from(remainder);
    let work =
        u64::try_from(numerator / u128::from(rate.denominator)).map_err(|_| Error::Overflow)?;
    let remainder = (numerator % u128::from(rate.denominator)) as u64;
    Ok((work, remainder))
}

pub fn work_duration(remaining: u64, rate: SkillRate, remainder: u64) -> Result<u64, Error> {
    if rate.numerator == 0 || rate.denominator == 0 || remainder >= rate.denominator {
        return Err(Error::InvalidConfiguration(
            "invalid frozen skill work rate".into(),
        ));
    }
    if remaining == 0 {
        return Ok(0);
    }
    let needed = (u128::from(remaining) * u128::from(rate.denominator))
        .saturating_sub(u128::from(remainder));
    u64::try_from(needed.div_ceil(u128::from(rate.numerator))).map_err(|_| Error::Overflow)
}

impl Simulation {
    pub fn skills_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::lineage_fixture(seed, execution)?;
        sim.state.scenario = scenarios()
            .into_iter()
            .find(|scenario| scenario.id == "skills-fixture")
            .ok_or_else(|| Error::InvalidFixture("skills fixture is not registered".into()))?;
        let mut world = World::default();
        for person in &sim.state.people {
            world.records.insert(person.id, BTreeMap::new());
            if person.role == Role::Guardian {
                // These authored immortal mentors have practiced the lineage
                // method before the fixture begins. This is not a newborn gift.
                let prior_practice = world.rules.per_level.0 * u64::from(world.rules.max_level);
                world.credit(person.id, Skill::Invocation, prior_practice, None);
            }
        }
        sim.state.skills = Some(world);
        Ok(sim)
    }

    pub(crate) fn evaluate_skills(&self, next: SimTime) -> Result<Vec<PracticeDelta>, Error> {
        let elapsed = next
            .0
            .checked_sub(self.state.time.0)
            .ok_or(Error::Overflow)?;
        let (Some(skills), Some(physical)) = (&self.state.skills, &self.state.physical) else {
            return Ok(Vec::new());
        };
        let equipment = self.equipment_availability();
        let mut pending = Vec::new();
        for (actor, body) in &physical.actors {
            if elapsed == 0
                || !body.vitals.alive
                || body.vitals.health == 0
                || !matches!(
                    self.state.people[self.person_index[actor]].location,
                    Location::AtArea(_)
                )
            {
                continue;
            }
            let (skill, cap) = match body.activity {
                Activity::Work { .. } if equipment.get(actor).copied().unwrap_or(false) => {
                    (Skill::Craft, None)
                }
                Activity::Cultivate { .. } if equipment.get(actor).copied().unwrap_or(false) => {
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
                        LessonTopic::Craft => {
                            (Skill::Craft, Some(skills.practice(teacher, Skill::Craft)))
                        }
                        LessonTopic::Invocation => (Skill::Invocation, None),
                        // Route instruction supplies actual information, but does
                        // not pretend that sitting through it is farming/combat.
                        LessonTopic::Travel => continue,
                    }
                }
                Activity::Lesson { teacher, .. } if teacher == *actor => (Skill::Leadership, None),
                _ => continue,
            };
            pending.push(PracticeDelta {
                actor: *actor,
                skill,
                milliseconds: elapsed,
                mastery_cap: cap,
            });
        }
        Ok(pending)
    }

    pub(crate) fn commit_skills(&mut self, pending: Vec<PracticeDelta>) {
        let Some(world) = &mut self.state.skills else {
            return;
        };
        for delta in pending {
            world.credit(
                delta.actor,
                delta.skill,
                delta.milliseconds,
                delta.mastery_cap,
            );
        }
    }

    pub(crate) fn credit_skill(&mut self, actor: PersonId, skill: Skill, milliseconds: u64) {
        if milliseconds == 0 {
            return;
        }
        if self.person_index.contains_key(&actor)
            && let Some(world) = &mut self.state.skills
        {
            if matches!(skill, Skill::Negotiation | Skill::Leadership) {
                let tick = self.state.time.0 / MINUTE.0;
                let last = world.semantic_ticks.entry(actor).or_default();
                if last.get(&skill) == Some(&tick) {
                    return;
                }
                last.insert(skill, tick);
            }
            world.credit(actor, skill, milliseconds, None);
        }
    }

    pub(crate) fn on_skill_event(&mut self, kind: &EventKind) {
        if let EventKind::Born { child, .. } = kind {
            if let Some(world) = &mut self.state.skills {
                world.records.entry(*child).or_default();
            }
            return;
        }
        let Some(world) = &self.state.skills else {
            return;
        };
        let credit = world.rules.action_practice_ms;
        let actors: Vec<_> = match kind {
            EventKind::Struck { actor, .. } => vec![(*actor, Skill::Combat)],
            EventKind::AgreementActivated { agreement } => self
                .state
                .agreements
                .as_ref()
                .and_then(|world| world.agreements.get(agreement))
                .map_or_else(Vec::new, |agreement| {
                    agreement
                        .accepted
                        .iter()
                        .map(|actor| (*actor, Skill::Negotiation))
                        .collect()
                }),
            EventKind::AgreementAmended { amendment, .. } => self
                .state
                .agreements
                .as_ref()
                .and_then(|world| world.amendments.get(amendment))
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
        self.state
            .skills
            .as_ref()
            .map_or(0, |world| world.level(actor, skill))
    }

    pub(crate) fn own_skill_view(&self, actor: PersonId) -> Vec<SkillView> {
        let Some(world) = &self.state.skills else {
            return Vec::new();
        };
        Skill::ALL
            .into_iter()
            .map(|skill| {
                let owned = world
                    .records
                    .get(&actor)
                    .and_then(|skills| skills.get(&skill))
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
        self.state
            .skills
            .as_ref()
            .map_or_else(SkillRate::default, |world| {
                rate_for_level(self.own_skill_level(actor, skill), &world.rules)
            })
    }

    pub(crate) fn effective_combat_damage(&self, actor: PersonId, base: u64) -> Result<u64, Error> {
        let Some(world) = &self.state.skills else {
            return Ok(base);
        };
        damage_for_level(
            base,
            self.own_skill_level(actor, Skill::Combat),
            &world.rules,
        )
    }
}

pub fn damage_for_level(base: u64, level: u16, rules: &Rules) -> Result<u64, Error> {
    let numerator =
        1_000 + u64::from(level.min(rules.max_level)) * u64::from(rules.damage_bonus_per_level);
    u64::try_from(u128::from(base) * u128::from(numerator) / 1_000).map_err(|_| Error::Overflow)
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.skills else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent personal skills".into());
    let rules = &world.rules;
    let cap = rules
        .per_level
        .0
        .checked_mul(u64::from(rules.max_level))
        .ok_or_else(fail)?;
    if state.physical.is_none()
        || rules.per_level.0 == 0
        || !(1..=1_000).contains(&rules.max_level)
        || u64::from(rules.max_level) * u64::from(rules.work_bonus_per_level) > 1_000
        || u64::from(rules.max_level) * u64::from(rules.damage_bonus_per_level) > 1_000
        || rules.action_practice_ms == 0
        || rules.action_practice_ms > cap
    {
        return Err(fail());
    }
    for (actor, skills) in &world.records {
        if !state.people.iter().any(|person| person.id == *actor) {
            return Err(fail());
        }
        for record in skills.values() {
            if record.practice_ms > cap || record != &progress(record.practice_ms, rules) {
                return Err(fail());
            }
        }
    }
    for (actor, ticks) in &world.semantic_ticks {
        if !state.people.iter().any(|person| person.id == *actor)
            || ticks.iter().any(|(skill, tick)| {
                !matches!(skill, Skill::Negotiation | Skill::Leadership)
                    || *tick > state.time.0 / MINUTE.0
            })
        {
            return Err(fail());
        }
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
                decisions::Dream::Comfort,
            )
            .unwrap();
        }
        sim
    }

    fn local_site(sim: &Simulation) -> SiteId {
        sim.state
            .physical
            .as_ref()
            .unwrap()
            .sites
            .values()
            .find(|site| site.area == AreaId(1))
            .unwrap()
            .id
    }

    #[test]
    fn completed_work_improves_actual_output_time_and_a_jobs_rate_stays_frozen() {
        let mut sim = sim(1);
        let actor = sim.state.ruler;
        let site = local_site(&sim);
        sim.debug_physical_command(actor, Command::Work { site })
            .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.own_skill_level(actor, Skill::Craft), 1);
        let rate = sim.effective_work_rate(actor, Skill::Craft);
        let duration = work_duration(60_000, rate, 0).unwrap();
        assert!(duration < MINUTE.0);
        sim.debug_physical_command(actor, Command::Work { site })
            .unwrap();
        sim.advance(SimDuration(duration - 1)).unwrap();
        assert!(matches!(
            sim.state.physical.as_ref().unwrap().actors[&actor].activity,
            Activity::Work { .. }
        ));
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(sim.state.events.iter().filter(|event| matches!(event.kind, EventKind::Produced { actor: worker, .. } if worker == actor)).count(), 2);
        sim.debug_physical_command(actor, Command::Work { site })
            .unwrap();
        let remaining_to_level = 120_000
            - sim
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Craft);
        sim.advance(SimDuration(remaining_to_level)).unwrap();
        assert_eq!(sim.own_skill_level(actor, Skill::Craft), 2);
        let now_faster =
            work_duration(60_000, sim.effective_work_rate(actor, Skill::Craft), 0).unwrap();
        sim.advance(SimDuration(now_faster - remaining_to_level))
            .unwrap();
        assert!(matches!(
            sim.state.physical.as_ref().unwrap().actors[&actor].activity,
            Activity::Work { .. }
        ));
        sim.advance(SimDuration(duration - now_faster)).unwrap();
        assert_eq!(sim.state.events.iter().filter(|event| matches!(event.kind, EventKind::Produced { actor: worker, .. } if worker == actor)).count(), 3);
    }

    #[test]
    fn canceled_work_credits_only_elapsed_training_and_idle_or_travel_add_nothing() {
        let mut sim = sim(1);
        let actor = sim.state.ruler;
        sim.debug_physical_command(
            actor,
            Command::Work {
                site: local_site(&sim),
            },
        )
        .unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        sim.debug_physical_command(actor, Command::Cancel).unwrap();
        let practice = sim
            .state
            .skills
            .as_ref()
            .unwrap()
            .practice(actor, Skill::Craft);
        assert_eq!(practice, 30_000);
        sim.advance(MINUTE).unwrap();
        sim.debug_move_actor(actor, AreaId(2)).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        assert_eq!(
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Craft),
            practice
        );
        assert_eq!(sim.own_skill_level(actor, Skill::Craft), 0);
        assert_eq!(sim.own_skill_level(actor, Skill::Farming), 0);
    }

    #[test]
    fn pending_training_is_passive_and_restores_exact_progress_with_different_workers() {
        let mut a = sim(1);
        let actor = a.state.ruler;
        a.debug_physical_command(
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
        let before = a.state.clone();
        for _ in 0..100 {
            a.own_skill_view(actor);
            a.observe();
        }
        assert_eq!(a.state, before);
        a.advance(SimDuration(45_000)).unwrap();
        for duration in [1, 14_999, 20_000, 10_000] {
            b.advance(SimDuration(duration)).unwrap();
        }
        assert_eq!(a.state, b.state);
        assert_eq!(
            a.state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Craft),
            60_000
        );
    }

    #[test]
    fn an_actual_newborn_does_not_receive_the_predecessors_practical_mastery() {
        let mut sim = Simulation::skills_fixture(1, Execution::default()).unwrap();
        let predecessor = sim.state.ruler;
        sim.debug_physical_command(
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
        let child = sim.state.ruler;
        assert_ne!(child, predecessor);
        assert!(
            sim.own_skill_view(child)
                .iter()
                .all(|skill| skill.practice_ms == 0 && skill.level == 0)
        );
        assert_eq!(sim.own_skill_level(predecessor, Skill::Craft), 1);
        let before = sim.state.skills.clone();
        let _borrowed = rate_for_level(
            sim.own_skill_level(predecessor, Skill::Craft),
            &sim.state.skills.as_ref().unwrap().rules,
        );
        assert_eq!(sim.state.skills, before);
    }

    #[test]
    fn practical_farming_and_real_combat_actions_train_distinct_skills() {
        let mut farmer = Simulation::farming_fixture(1, Execution::default()).unwrap();
        farmer.state.skills = Some(World::default());
        let actor = farmer.state.ruler;
        let field = farmer
            .state
            .environment
            .as_ref()
            .unwrap()
            .fields
            .values()
            .find(|field| field.access == BTreeSet::from([actor]))
            .unwrap()
            .id;
        let area = farmer.state.environment.as_ref().unwrap().fields[&field].area;
        farmer.debug_move_actor(actor, area).unwrap();
        farmer.advance(SimDuration(120_000)).unwrap();
        farmer
            .debug_physical_command(
                actor,
                Command::Cultivate {
                    field,
                    operation: FarmOperation::Prepare,
                },
            )
            .unwrap();
        farmer.advance(SimDuration(30_000)).unwrap();
        farmer
            .debug_physical_command(actor, Command::Cancel)
            .unwrap();
        assert_eq!(
            farmer
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Farming),
            30_000
        );
        assert_eq!(
            farmer
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Craft),
            0
        );

        let mut fighter = sim(1);
        let actor = fighter.state.ruler;
        let target = fighter.state.people[3].id;
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
            fighter
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Combat),
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
            fighter
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Combat),
            6_000
        );
        assert_eq!(
            fighter
                .state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Farming),
            0
        );
    }

    #[test]
    fn coupled_guidance_trains_actual_pupil_and_teacher_without_copying_the_experts_level() {
        let mut sim = sim(1);
        let student = sim.state.ruler;
        let teacher = *sim
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
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(student, Skill::Invocation),
            30_000
        );
        assert_eq!(
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(teacher, Skill::Leadership),
            30_000
        );
        assert_eq!(sim.own_skill_level(student, Skill::Invocation), 0);
        sim.debug_physical_command(teacher, Command::Cancel)
            .unwrap();
        assert_eq!(
            sim.state
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
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(student, Skill::Invocation),
            90_000
        );
        assert_eq!(sim.own_skill_level(student, Skill::Invocation), 1);
        assert_eq!(sim.own_skill_level(teacher, Skill::Invocation), 100);
        assert_eq!(
            sim.state.lineage.as_ref().unwrap().invocation_learning[&student],
            100
        );
        assert!(!sim.state.events.iter().any(|event| matches!(event.kind, EventKind::Produced { actor, .. } if actor == student || actor == teacher)));
    }

    #[test]
    fn only_an_adopted_agreement_trains_negotiation_and_zero_time_churn_is_bounded() {
        let mut sim = Simulation::agreement_fixture(1, Execution::default()).unwrap();
        sim.state.skills = Some(World::default());
        let actor = sim.state.ruler;
        let citizen = sim
            .state
            .people
            .iter()
            .find(|person| person.role == Role::Citizen)
            .unwrap()
            .id;
        let stock = sim
            .state
            .inventories
            .values()
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
        let agreement = *sim
            .state
            .agreements
            .as_ref()
            .unwrap()
            .agreements
            .keys()
            .next_back()
            .unwrap();
        assert_eq!(
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Negotiation),
            0
        );
        sim.debug_agreement_action(citizen, AgreementAction::Accept { agreement })
            .unwrap();
        assert!(sim.state.inventories[&stock].access.contains(&citizen));
        assert_eq!(
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Negotiation),
            6_000
        );
        assert_eq!(
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(citizen, Skill::Negotiation),
            6_000
        );
        for _ in 0..100 {
            sim.credit_skill(actor, Skill::Negotiation, 6_000);
        }
        assert_eq!(
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Negotiation),
            6_000
        );
        sim.advance(MINUTE).unwrap();
        sim.credit_skill(actor, Skill::Negotiation, 6_000);
        assert_eq!(
            sim.state
                .skills
                .as_ref()
                .unwrap()
                .practice(actor, Skill::Negotiation),
            12_000
        );
    }

    #[test]
    fn partial_training_is_fractional_bounded_and_slice_invariant() {
        let mut whole = World::default();
        let mut sliced = whole.clone();
        whole.credit(PersonId(1), Skill::Craft, 170_003, None);
        for elapsed in [1, 59_999, 10_003, 40_000, 60_000] {
            sliced.credit(PersonId(1), Skill::Craft, elapsed, None);
        }
        assert_eq!(whole, sliced);
        assert_eq!(whole.level(PersonId(1), Skill::Craft), 2);
        assert_eq!(
            whole.records[&PersonId(1)][&Skill::Craft].progress_ms,
            50_003
        );
        assert_eq!(whole.level(PersonId(1), Skill::Farming), 0);
        whole.credit(PersonId(1), Skill::Craft, u64::MAX, None);
        assert_eq!(whole.level(PersonId(1), Skill::Craft), 100);
        assert_eq!(whole.records[&PersonId(1)][&Skill::Craft].progress_ms, 0);
        assert_eq!(whole.level(PersonId(2), Skill::Craft), 0);
    }

    #[test]
    fn rational_work_progress_and_completion_share_the_exact_remainder() {
        let rate = SkillRate {
            numerator: 1_370,
            denominator: 1_000,
        };
        let (whole, remainder) = advance_work(70_013, rate, 19).unwrap();
        let (mut total, mut carry) = (0, 19);
        for elapsed in [1, 9, 60_003, 10_000] {
            let (work, next) = advance_work(elapsed, rate, carry).unwrap();
            total += work;
            carry = next;
        }
        assert_eq!((whole, remainder), (total, carry));
        let duration = work_duration(60_000, rate, 19).unwrap();
        assert!(advance_work(duration - 1, rate, 19).unwrap().0 < 60_000);
        assert!(advance_work(duration, rate, 19).unwrap().0 >= 60_000);
        assert_eq!(
            work_duration(60_000, SkillRate::default(), 0).unwrap(),
            60_000
        );
        assert!(advance_work(u64::MAX, rate, 0).is_err());
    }

    #[test]
    fn guidance_cannot_copy_a_teachers_full_mastery_and_borrowing_is_read_only() {
        let mut world = World::default();
        world.credit(PersonId(1), Skill::Craft, 300_000, None);
        let before = world.clone();
        let borrowed = rate_for_level(world.level(PersonId(1), Skill::Craft), &world.rules);
        assert!(borrowed.numerator > 1_000);
        assert_eq!(world, before);
        let teacher = world.practice(PersonId(1), Skill::Craft);
        world.credit(PersonId(2), Skill::Craft, 30_000, Some(teacher));
        assert_eq!(world.level(PersonId(2), Skill::Craft), 0);
        world.credit(PersonId(2), Skill::Craft, 600_000, Some(teacher));
        assert_eq!(world.level(PersonId(2), Skill::Craft), 5);
        world.credit(PersonId(2), Skill::Craft, 60_000, None);
        assert_eq!(world.level(PersonId(2), Skill::Craft), 6);
        world.credit(PersonId(2), Skill::Craft, 60_000, Some(teacher));
        assert_eq!(world.level(PersonId(2), Skill::Craft), 6);
        assert_eq!(damage_for_level(45_000, 100, &world.rules).unwrap(), 67_500);
    }
}
