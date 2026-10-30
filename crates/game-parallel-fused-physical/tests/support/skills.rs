use game_parallel::{JobPhase, SkillRule, Skills, StartWork, World};
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/skills.toml")).unwrap();
    world.autowork = None;
    world
}

fn start(kernel: &mut Kernel, id: u64, worker: u64, region: u64) {
    kernel
        .start_work(&[StartWork {
            id,
            worker,
            region,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
}

fn reload(kernel: Kernel) -> Kernel {
    let saved = serde_json::to_string(&kernel.export()).unwrap();
    Kernel::compile(World::from_json(&saved).unwrap()).unwrap()
}

#[test]
fn finished_work_trains_only_its_worker_and_next_job_uses_committed_rate() {
    let mut kernel = Kernel::compile(world()).unwrap();
    start(&mut kernel, 1, 1, 1);
    for remaining in (1..=3).rev() {
        kernel.step().unwrap();
        let state = kernel.export();
        assert_eq!(state.production.unwrap().jobs[0].remaining, remaining);
        assert!(state.skills.unwrap().records.is_empty());
    }
    let draft = kernel.prepare().unwrap();
    assert!(kernel.export().skills.unwrap().records.is_empty());
    kernel.commit(draft).unwrap();
    let state = kernel.export();
    assert_eq!(state.skills.as_ref().unwrap().index().rate(1, 1), 2);
    assert_eq!(state.skills.as_ref().unwrap().index().rate(2, 1), 1);
    assert_eq!(state.skills.as_ref().unwrap().records[0].completions, 1);
    kernel = reload(kernel);
    // External job IDs are reusable; completion time separates new attribution.
    start(&mut kernel, 1, 1, 1);
    kernel.step().unwrap();
    assert_eq!(kernel.export().production.unwrap().jobs[0].remaining, 2);
    kernel.step().unwrap();
    let state = kernel.export();
    state.validate().unwrap();
    let record = &state.skills.as_ref().unwrap().records[0];
    assert_eq!(record.completions, 2);
    assert_eq!(record.credited_jobs[0].finished_at, 4);
    assert_eq!(record.credited_jobs[1].finished_at, 6);
    assert_eq!(record.credited_jobs[0].job, record.credited_jobs[1].job);
}

#[test]
fn waiting_output_and_reload_never_repeat_training() {
    let mut world = world();
    let production = world.production.as_mut().unwrap();
    production.recipes[0].ticks = 1;
    production.inventories[0].stocks[1].quantity = 1;
    production.inventories[0].stocks[1].capacity = 1;
    let mut kernel = Kernel::compile(world).unwrap();
    start(&mut kernel, 7, 1, 1);
    kernel.step().unwrap();
    let state = kernel.export();
    assert_eq!(
        state.production.as_ref().unwrap().jobs[0].phase,
        JobPhase::Waiting
    );
    let records = state.skills.as_ref().unwrap().records.clone();
    kernel = reload(kernel);
    for _ in 0..3 {
        kernel.step().unwrap();
        let state = kernel.export();
        assert_eq!(state.skills.as_ref().unwrap().records, records);
        assert!(state.skills.as_ref().unwrap().events.is_empty());
    }
    let mut state = kernel.export();
    state.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 0;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert!(state.production.unwrap().jobs.is_empty());
    assert_eq!(state.skills.unwrap().records, records);
}

fn family_world() -> World {
    let mut families =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let source = world();
    families.production = source.production;
    families.skills = source.skills;
    families.production.as_mut().unwrap().recipes[0].ticks = 1;
    families
}

#[test]
fn late_lifecycle_failure_rolls_back_finished_work_and_learning() {
    let mut world = family_world();
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(world).unwrap();
    start(&mut kernel, 1, 1, 1);
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
    assert!(kernel.prepare().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn learned_skills_persist_after_death_and_newborns_start_untrained() {
    let mut world = family_world();
    world.families.as_mut().unwrap().members[0].age = 59;
    let mut kernel = Kernel::compile(world).unwrap();
    start(&mut kernel, 1, 1, 1);
    kernel.step().unwrap();
    let state = kernel.export();
    assert!(
        !state
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|m| m.id == 1)
            .unwrap()
            .alive
    );
    assert_eq!(state.skills.as_ref().unwrap().index().rate(1, 1), 2);
    kernel = reload(kernel);
    for _ in 0..7 {
        kernel.step().unwrap();
    }
    let state = kernel.export();
    state.validate().unwrap();
    let newborns: Vec<_> = state
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .filter(|m| m.born.is_some())
        .collect();
    assert!(!newborns.is_empty());
    let rates = state.skills.as_ref().unwrap().index();
    assert!(newborns.iter().all(|m| rates.rate(m.id, 1) == 1));
    assert_eq!(rates.rate(1, 1), 2);
    assert_eq!(reload(kernel).export(), state);
}

#[test]
fn many_autonomous_learners_are_worker_invariant_across_reload() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/skills.toml")).unwrap();
    world.regions[0].people = (1..=5000)
        .map(|id| game_parallel::Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.rules.food_capacity = 100_000;
    world.rules.food_growth = 20_000;
    world.regions[0].food = 100_000;
    for stock in &mut world.production.as_mut().unwrap().inventories[0].stocks {
        stock.capacity = 100_000;
        stock.quantity = if stock.good == 1 { 100_000 } else { 0 };
    }
    world.autowork.as_mut().unwrap().policies = (1..=5000)
        .map(|person| game_parallel::WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                for tick in 1..=12 {
                    kernel.step().unwrap();
                    if tick == 6 {
                        kernel = reload(kernel);
                    }
                }
                kernel.export()
            })
    };
    let serial = run(1);
    let parallel = run(4);
    assert_eq!(serial, parallel);
    parallel.validate().unwrap();
    assert_eq!(parallel.skills.as_ref().unwrap().records.len(), 5000);
    assert!(
        parallel
            .skills
            .unwrap()
            .records
            .iter()
            .all(|r| r.completions >= 3)
    );
}

#[test]
fn malformed_skill_worlds_and_unsupported_reference_hosts_reject_without_mutation() {
    let mut state = world();
    let before = state.clone();
    assert!(state.step().is_err());
    assert_eq!(state, before);
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    state.production = None;
    assert!(state.validate().is_err());
    let mut state = world();
    state.skills = Some(Skills {
        rules: vec![SkillRule {
            recipe: 999,
            completions_per_level: 1,
            max_level: 1,
            work_per_level: 1,
        }],
        records: vec![],
        aliases: vec![],
        catalog_training: None,
        events: vec![],
        lessons: vec![],
        teachers: vec![],
        inherit_teachers: false,
    });
    assert!(Kernel::compile(state).is_err());
}

#[test]
fn invocation_temporarily_borrows_work_skill_without_copying_training() {
    let state = World::from_toml(include_str!(
        "../../../../content/parallel/invoked_skills.toml"
    ))
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut baseline = Kernel::compile(state.clone()).unwrap();
                assert!(
                    game_parallel_observation::ruler(&baseline.export())
                        .unwrap()
                        .borrowed_skills
                        .is_empty()
                );
                start(&mut baseline, 1, 1, 1);
                baseline.step().unwrap();
                assert_eq!(
                    baseline.export().production.as_ref().unwrap().jobs[0].remaining,
                    3
                );

                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.invoke_lives(&[2]).unwrap();
                let visible = game_parallel_observation::ruler(&kernel.export()).unwrap();
                assert_eq!(visible.borrowed_skills.len(), 1);
                assert_eq!(
                    (
                        visible.borrowed_skills[0].life,
                        visible.borrowed_skills[0].recipe,
                        visible.borrowed_skills[0].rate
                    ),
                    (2, 1, 4)
                );
                kernel = reload(kernel);
                start(&mut kernel, 1, 1, 1);
                kernel.step().unwrap();
                let worked = kernel.export();
                assert!(worked.production.as_ref().unwrap().jobs.is_empty());
                let index = worked.skills.as_ref().unwrap().index();
                assert_eq!(index.rate(1, 1), 2);
                assert_eq!(index.rate(2, 1), 4);
                kernel.dismiss_invocation().unwrap();
                assert!(
                    game_parallel_observation::ruler(&kernel.export())
                        .unwrap()
                        .borrowed_skills
                        .is_empty()
                );
                start(&mut kernel, 2, 1, 1);
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.production.as_ref().unwrap().jobs[0].remaining, 2);
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn multiple_invoked_lives_use_the_best_rate_without_stacking_or_copying() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/invoked_skills.toml"
    ))
    .unwrap();
    state.regions[0]
        .people
        .iter_mut()
        .find(|person| person.id == 3)
        .unwrap()
        .health = 0;
    state
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 3)
        .unwrap()
        .alive = false;
    let lineage = state.lineage.as_mut().unwrap();
    lineage.history.push(game_parallel::PastLife {
        person: 3,
        started: 0,
        ended: 0,
    });
    let invocation = lineage.invocation.as_mut().unwrap();
    invocation.rules.strain_threshold = 100;
    invocation.rules.loss_threshold = 200;
    let skills = state.skills.as_mut().unwrap();
    skills.records.push(game_parallel::SkillRecord {
        person: 3,
        recipe: 1,
        completions: 6,
        credited_jobs: vec![],
    });
    skills.records.push(game_parallel::SkillRecord {
        person: 1,
        recipe: 1,
        completions: 1,
        credited_jobs: vec![game_parallel::SkillAttribution {
            finished_at: 0,
            job: 99,
        }],
    });
    state.production.as_mut().unwrap().recipes[0].ticks = 10;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.invoke_lives(&[2, 3]).unwrap();
    start(&mut kernel, 1, 1, 1);
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.production.as_ref().unwrap().jobs[0].remaining, 6);
    assert_eq!(result.skills.as_ref().unwrap().index().rate(1, 1), 2);
    assert_eq!(
        game_parallel_observation::ruler(&result)
            .unwrap()
            .borrowed_skills
            .len(),
        2
    );
}

#[test]
fn consenting_lesson_preserves_invoked_expertise_for_another_person() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/teaching.toml")).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                assert!(kernel.teach(1, 3, 1).is_err());
                assert_eq!(kernel.tick(), 0);
                kernel.invoke_lives(&[2]).unwrap();
                assert!(kernel.teach(1, 3, 1).unwrap());
                let taught = kernel.export();
                assert_eq!(taught.tick, 1);
                assert_eq!(taught.skills.as_ref().unwrap().index().rate(3, 1), 2);
                assert_eq!(taught.skills.as_ref().unwrap().lessons.len(), 1);
                assert_eq!(
                    taught.skills.as_ref().unwrap().lessons[0].source_life,
                    Some(2)
                );
                let personal = game_parallel_observation::ruler(&taught).unwrap();
                assert_eq!(personal.lessons[0].student, 3);
                assert!(
                    taught
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.a == 1 && event.b == 3)
                );
                kernel = reload(kernel);
                kernel.dismiss_invocation().unwrap();
                start(&mut kernel, 1, 3, 1);
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.production.as_ref().unwrap().jobs[0].remaining, 2);
                assert_eq!(result.skills.as_ref().unwrap().index().rate(1, 1), 1);
                assert_eq!(result.skills.as_ref().unwrap().index().rate(3, 1), 2);
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn refusal_and_failed_tick_do_not_publish_a_lesson() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/teaching.toml")).unwrap();
    state
        .social
        .as_mut()
        .unwrap()
        .participants
        .iter_mut()
        .find(|participant| participant.person == 3)
        .unwrap()
        .willing = false;
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    kernel.invoke_lives(&[2]).unwrap();
    assert!(!kernel.teach(1, 3, 1).unwrap());
    assert_eq!(kernel.tick(), 1);
    assert!(kernel.export().skills.unwrap().lessons.is_empty());
    state
        .social
        .as_mut()
        .unwrap()
        .participants
        .iter_mut()
        .find(|participant| participant.person == 3)
        .unwrap()
        .willing = true;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.invoke_lives(&[2]).unwrap();
    let before = kernel.export();
    assert!(kernel.teach(1, 3, 1).is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn teaching_waits_for_invocation_cost_consent_before_learning() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/teaching.toml")).unwrap();
    state
        .lineage
        .as_mut()
        .unwrap()
        .invocation
        .as_mut()
        .unwrap()
        .rules
        .strain_threshold = 2;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.invoke_lives(&[2]).unwrap();
    assert_eq!(
        kernel.teach(1, 3, 1).unwrap_err(),
        "invocation warning needs a choice"
    );
    let paused = kernel.export();
    assert_eq!(paused.tick, 0);
    assert!(paused.skills.as_ref().unwrap().lessons.is_empty());
    let warning = paused
        .lineage
        .as_ref()
        .unwrap()
        .invocation
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap();
    kernel.accept_invocation_warning(warning.revision).unwrap();
    assert!(kernel.teach(1, 3, 1).unwrap());
    assert_eq!(kernel.export().skills.unwrap().lessons.len(), 1);
}

#[test]
fn an_invoked_life_can_share_skills_it_learned_from_a_lesson() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/teaching.toml")).unwrap();
    let skills = state.skills.as_mut().unwrap();
    skills.records.clear();
    skills.lessons.push(game_parallel::SkillLesson {
        teacher: 3,
        student: 2,
        recipe: 1,
        tick: 0,
        source_life: None,
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.invoke_lives(&[2]).unwrap();
    let view = game_parallel_observation::ruler(&kernel.export()).unwrap();
    assert_eq!(view.borrowed_skills[0].rate, 2);
    assert!(kernel.teach(1, 3, 1).unwrap());
    let result = kernel.export();
    let skills = result.skills.as_ref().unwrap();
    assert_eq!(skills.index().rate(3, 1), 2);
    assert_eq!(skills.lessons.last().unwrap().source_life, Some(2));
}

#[test]
fn willing_npcs_teach_each_other_during_real_social_encounters() {
    let state = World::from_toml(include_str!(
        "../../../../content/parallel/auto_teaching.toml"
    ))
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let first = kernel.export();
                let skills = first.skills.as_ref().unwrap();
                assert_eq!(skills.lessons.len(), 1);
                assert_eq!(
                    (skills.lessons[0].teacher, skills.lessons[0].student),
                    (3, 4)
                );
                assert_eq!(skills.index().rate(4, 1), 2);
                assert!(
                    first
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.a == 3 && event.b == 4)
                );
                kernel = reload(kernel);
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.skills.as_ref().unwrap().index().rate(4, 1), 2);
                assert_eq!(result.skills.as_ref().unwrap().lessons.len(), 2);
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn unwilling_or_failed_social_ticks_do_not_create_automatic_lessons() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/auto_teaching.toml"
    ))
    .unwrap();
    state
        .social
        .as_mut()
        .unwrap()
        .participants
        .iter_mut()
        .find(|row| row.person == 4)
        .unwrap()
        .willing = false;
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    kernel.step().unwrap();
    assert!(kernel.export().skills.as_ref().unwrap().lessons.is_empty());
    state
        .social
        .as_mut()
        .unwrap()
        .participants
        .iter_mut()
        .find(|row| row.person == 4)
        .unwrap()
        .willing = true;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn newborns_inherit_a_teaching_role_without_inheriting_skill() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/campaign.toml")).unwrap();
    state.families.as_mut().unwrap().members[0].partner = Some(2);
    state.families.as_mut().unwrap().members[1].partner = Some(1);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for _ in 0..6 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let family = result.families.as_ref().unwrap();
                let child = family
                    .members
                    .iter()
                    .find(|member| member.born.is_some() && member.parents.contains(&2))
                    .unwrap();
                let skills = result.skills.as_ref().unwrap();
                let roles: Vec<_> = skills
                    .teachers
                    .iter()
                    .filter(|policy| policy.person == child.id)
                    .collect();
                assert_eq!(roles.len(), 2);
                assert!(roles.iter().all(
                    |policy| policy.eligible_at == child.born.unwrap() + family.rules.adult_age
                ));
                assert_eq!(skills.index().rate(child.id, 1), 1);
                result.validate().unwrap();
                let reloaded = reload(kernel);
                assert_eq!(reloaded.export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn ruler_can_learn_from_a_willing_local_teacher_and_use_the_skill() {
    let state = World::from_toml(include_str!(
        "../../../../content/parallel/auto_teaching.toml"
    ))
    .unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                assert!(kernel.learn(1, 3, 1).unwrap());
                let taught = kernel.export();
                assert_eq!(taught.tick, 1);
                assert_eq!(taught.skills.as_ref().unwrap().index().rate(1, 1), 2);
                assert_eq!(
                    (
                        taught.skills.as_ref().unwrap().lessons[0].teacher,
                        taught.skills.as_ref().unwrap().lessons[0].student
                    ),
                    (3, 1)
                );
                assert!(
                    taught
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.a == 1 && event.b == 3)
                );
                kernel = reload(kernel);
                start(&mut kernel, 1, 1, 1);
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.production.as_ref().unwrap().jobs[0].remaining, 2);
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn npc_teacher_may_decline_and_failed_ticks_do_not_teach() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/auto_teaching.toml"
    ))
    .unwrap();
    state.skills.as_mut().unwrap().teachers[0].enabled = false;
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    assert!(!kernel.learn(1, 3, 1).unwrap());
    assert_eq!(kernel.tick(), 1);
    assert!(kernel.export().skills.as_ref().unwrap().lessons.is_empty());
    assert!(
        kernel
            .export()
            .social
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.a == 1 && event.b == 3)
    );
    state.skills.as_mut().unwrap().teachers[0].enabled = true;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.learn(1, 3, 1).is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn a_teacher_can_choose_to_help_because_of_fear() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/auto_teaching.toml"
    ))
    .unwrap();
    state.skills.as_mut().unwrap().teachers[0].enabled = false;
    state
        .social
        .as_mut()
        .unwrap()
        .participants
        .iter_mut()
        .find(|p| p.person == 3)
        .unwrap()
        .willing = false;
    state.social.as_mut().unwrap().pressures = vec![game_parallel::SocialPressurePolicy {
        person: 3,
        fear_at_least: Some(2),
        obligation_at_least: None,
        enabled: true,
    }];
    let mut opinions = World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
        .unwrap()
        .opinions
        .unwrap();
    opinions.records = vec![game_parallel::Opinion {
        observer: 3,
        target: 1,
        trust: 0,
        fear: 2,
        obligation: 0,
        affinity: 0,
    }];
    state.opinions = Some(opinions);
    let mut kernel = Kernel::compile(state).unwrap();
    assert!(kernel.learn(1, 3, 1).unwrap());
    let result = kernel.export();
    assert_eq!(result.skills.as_ref().unwrap().lessons[0].teacher, 3);
    assert_eq!(
        result
            .social
            .as_ref()
            .unwrap()
            .events
            .iter()
            .find(|event| (event.a, event.b) == (1, 3))
            .unwrap()
            .pressured,
        Some(3)
    );
    assert_eq!(result.opinions.as_ref().unwrap().index().trust(3, 1), 0);
}
