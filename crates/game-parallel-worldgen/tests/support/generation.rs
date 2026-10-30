use crate::{Settings, generate};
use game_parallel::{Sex, World};
use game_parallel_fused_physical::Kernel;
fn inputs() -> (World, Settings) {
    let template =
        World::from_toml(include_str!("../../../../content/parallel/campaign.toml")).unwrap();
    let settings = Settings::from_toml(include_str!(
        "../../../../content/parallel/legacy_food_worldgen.toml"
    ))
    .unwrap();
    (template, settings)
}
#[test]
fn seeded_generation_builds_a_connected_playable_world_and_varies_by_seed() {
    let (template, settings) = inputs();
    let one = generate(&template, &settings).unwrap();
    let again = generate(&template, &settings).unwrap();
    assert_eq!(one, again);
    assert_eq!(one.regions.len(), settings.regions as usize);
    assert_eq!(
        one.regions
            .iter()
            .map(|region| region.people.len())
            .sum::<usize>(),
        settings.population as usize
    );
    assert_eq!(one.routes.len(), settings.regions as usize * 2);
    assert_eq!(one.lineage.as_ref().unwrap().current, Some(1));
    assert!(
        one.families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .any(|member| member.genome.sex == Sex::Female)
    );
    assert!(
        one.families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .any(|member| member.genome.sex == Sex::Male)
    );
    assert!(one.institutions.is_none());
    assert!(one.civic_auto.is_none());
    let mut different = settings.clone();
    different.seed += 1;
    assert_ne!(
        one.personality,
        generate(&template, &different).unwrap().personality
    );
    one.validate().unwrap();
}

#[test]
fn generated_people_can_ask_and_help_without_predefined_institutions() {
    let (template, mut settings) = inputs();
    settings.population = 200;
    settings.regions = 4;
    settings.trading_percent = 50;
    settings.requesting_percent = 100;
    settings.helpful_percent = 100;
    settings.working_percent = 0;
    settings.farming_percent = 0;
    let world = generate(&template, &settings).unwrap();
    assert!(world.institutions.is_none());
    assert!(
        world
            .market
            .as_ref()
            .unwrap()
            .policies
            .iter()
            .any(|policy| policy.request_help)
    );
    assert!(
        !world
            .market
            .as_ref()
            .unwrap()
            .policies
            .iter()
            .any(|policy| policy.person == 1)
    );
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(world.clone()).unwrap();
                engine.step().unwrap();
                assert!(!engine.export().market.as_ref().unwrap().offers.is_empty());
                engine.step().unwrap();
                let result = engine.export();
                assert!(
                    result
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .any(|record| record.obligation > 0)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_dispositions_can_produce_a_real_asked_work_action() {
    use game_parallel::{Goal, work_requests::Outcome};
    let (template, mut settings) = inputs();
    settings.population = 8;
    settings.regions = 4;
    settings.working_percent = 0;
    settings.farming_percent = 0;
    settings.trading_percent = 0;
    settings.requesting_percent = 0;
    settings.work_requesting_percent = 100;
    settings.work_helpful_percent = 100;
    settings.work_request_interval = 1;
    let mut world = generate(&template, &settings).unwrap();
    world.market = None;
    world.personality = None;
    world.dreams = None;
    let goals = world.goals.as_mut().unwrap();
    goals.plan_inputs = false;
    goals.people.push(Goal {
        person: 2,
        recipe: 1,
        priority: 10,
        target: 1,
        progress: 0,
        next_review: 0,
        enabled: true,
    });
    let inventory = world
        .production
        .as_mut()
        .unwrap()
        .inventories
        .iter_mut()
        .find(|row| row.region == 2)
        .unwrap();
    inventory
        .stocks
        .iter_mut()
        .find(|stock| stock.good == 1)
        .unwrap()
        .quantity = 6;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(world.clone()).unwrap();
                engine.step().unwrap();
                let pending = engine.export();
                assert!(
                    pending
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .pending
                        .iter()
                        .any(|row| row.requester == 2 && row.worker == 6)
                );
                engine = Kernel::compile(
                    World::from_json(&serde_json::to_string(&pending).unwrap()).unwrap(),
                )
                .unwrap();
                engine.step().unwrap();
                let result = engine.export();
                assert!(
                    result
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(
                            |row| row.worker == 6 && matches!(row.outcome, Outcome::Started { .. })
                        ),
                    "{:?}",
                    result.autowork.as_ref().unwrap().requests
                );
                assert!(
                    result
                        .production
                        .as_ref()
                        .unwrap()
                        .jobs
                        .iter()
                        .any(|job| job.worker == 6)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_individuals_can_interpret_claimed_goods_differently() {
    let (template, mut settings) = inputs();
    settings.claim_tolerance_percent = 100;
    let tolerant = generate(&template, &settings).unwrap();
    let mut opinions = tolerant.opinions.unwrap();
    opinions.witness_theft(2, 3, 0);
    assert_eq!(opinions.index().trust(2, 3), 0);
    settings.claim_tolerance_percent = 0;
    let intolerant = generate(&template, &settings).unwrap();
    let mut opinions = intolerant.opinions.unwrap();
    opinions.witness_theft(2, 3, 0);
    assert_eq!(opinions.index().trust(2, 3), -1);
}
#[test]
fn generated_world_runs_and_survives_reload_with_worker_invariant_results() {
    let (template, settings) = inputs();
    let state = generate(&template, &settings).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=12 {
                    kernel.step().unwrap();
                    if tick == 6 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert!(result.families.as_ref().unwrap().totals.births > 0);
    result.validate().unwrap();
}
#[test]
fn generated_ten_thousand_person_world_advances_on_multiple_workers() {
    let (template, mut settings) = inputs();
    settings.population = 10_000;
    settings.regions = 100;
    let state = generate(&template, &settings).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                kernel.step().unwrap();
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    result.validate().unwrap();
}
#[test]
fn invalid_generation_settings_fail_before_creating_a_world() {
    let (template, mut settings) = inputs();
    settings.population = 1;
    assert!(generate(&template, &settings).is_err());
    settings.population = 10;
    settings.regions = 2;
    settings.initial_food = template.rules.food_capacity + 1;
    assert!(generate(&template, &settings).is_err());
    settings.initial_food = 1;
    settings.working_percent = 101;
    assert!(generate(&template, &settings).is_err());
}

#[test]
fn generation_itself_is_independent_of_worker_count() {
    let (template, mut settings) = inputs();
    settings.population = 10_000;
    settings.regions = 100;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| generate(&template, &settings).unwrap())
    };
    assert_eq!(run(1), run(4));
}

/// Reproducible full-owner throughput measurement; disabled during ordinary tests.
#[test]
#[ignore = "performance measurement"]
fn generated_world_benchmark() {
    let value = |name: &str, default: usize| {
        std::env::var(name)
            .ok()
            .map_or(default, |s| s.parse().unwrap())
    };
    let workers = value("LEGACY_WORKERS", 1);
    let count = value("LEGACY_PEOPLE", 10_000);
    let ticks = value("LEGACY_TICKS", 8);
    let (mut template, mut settings) = inputs();
    if std::env::var_os("LEGACY_NUTRITION").is_some() {
        template.nutrition = World::from_toml(include_str!(
            "../../../../content/parallel/generated_campaign.toml"
        ))
        .unwrap()
        .nutrition;
    }
    if std::env::var_os("LEGACY_CATALOG_SKILLS").is_some() {
        template.skills.as_mut().unwrap().catalog_training = World::from_toml(include_str!(
            "../../../../content/parallel/generated_campaign.toml"
        ))
        .unwrap()
        .skills
        .unwrap()
        .catalog_training;
    }
    if std::env::var_os("LEGACY_PERSONAL_FOOD").is_some() {
        template.rules.food_growth = 60;
        template.rules.food_capacity = 300;
        let family = template.families.as_mut().unwrap();
        family.rules.minimum_health = 70;
        family.rules.maximum_hunger = 5;
        family.rules.birth_spacing = 7;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        game_parallel_content_import::augment(
            template.production.as_mut().unwrap(),
            &root.join("objects"),
            &root.join("workshop"),
            600_000,
            1000,
            10,
        )
        .unwrap();
        settings.personal_food_access = true;
        settings.dependent_food_care = true;
        settings.food_asking_percent = 25;
        settings.food_helpful_percent = 50;
        settings.food_sharing_percent = 50;
        settings.reconsider_partnership_percent = 50;
        settings.food_stores_per_region = 2;
        settings.food_store_users_percent = 25;
        settings.food_depositors_percent = 10;
        settings.deposit_claim_percent = 30;
        settings.store_builders_percent = 30;
        settings.store_build_interval = 1;
        settings.store_builder_blueprint = Some(game_parallel::FoodStoreBlueprint {
            id: 7,
            good: 1,
            quantity: 2,
            capacity: 20,
        });
        settings.food_store_claim_percent = 50;
        settings.food_store_objection_percent = 50;
        settings.confrontation_percent = 10;
        settings.confrontation_threshold = -2;
        settings.confrontation_fear_limit = 2;
        settings.confrontation_interval = 5;
        settings.confrontation_damage = 5;
        settings.work_memory_limit = 4;
        settings.care_pressure_threshold = Some(3);
        settings.social_pressure_percent = 50;
        settings.social_fear_threshold = Some(2);
        settings.social_obligation_threshold = Some(3);
    }
    let farming = std::env::var_os("LEGACY_FARMING").is_some();
    if farming {
        template.agriculture.as_mut().unwrap().volunteers = Some(Vec::new());
        settings.farming_percent = 50;
    }
    settings.population = u32::try_from(count).unwrap();
    settings.regions = u32::try_from((count / 100).clamp(2, 1000)).unwrap();
    let departures = std::env::var_os("LEGACY_DEPARTURES").is_some();
    let held = departures && std::env::var_os("LEGACY_HELD").is_some();
    let hot = std::env::var_os("LEGACY_HOT").is_some() || departures;
    if hot {
        settings.initial_institutions = false;
    }
    if departures {
        settings.working_percent = 0;
        settings.trading_percent = 0;
        settings.organizing_percent = 0;
        settings.joining_percent = 0;
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let mut state = pool.install(|| generate(&template, &settings)).unwrap();
    match std::env::var("LEGACY_PROFILE_OFF").ok().as_deref() {
        None => {}
        Some("social") => state.social = None,
        Some("market") => state.market = None,
        Some("farming") => state.agriculture = None,
        Some("disease") => state.disease = None,
        Some("knowledge") => {
            state.knowledge = None;
            state.autonomy = None;
        }
        Some("goals") => {
            state.dreams = None;
            state.purpose = None;
            state.goals = None;
        }
        Some(value) => panic!("unknown profiling category: {value}"),
    }
    if hot {
        let (first, remaining) = state.regions.split_at_mut(1);
        for region in remaining {
            first[0].people.append(&mut region.people);
        }
    }
    if departures {
        use game_parallel::KnowledgeRecord;
        state.rules.food_growth = 0;
        state.rules.food_capacity = u32::try_from(count * 2 + 100).unwrap();
        state.regions[0].food = state.rules.food_capacity;
        state.regions[0]
            .people
            .iter_mut()
            .for_each(|person| person.hunger = 7);
        state
            .autonomy
            .as_mut()
            .unwrap()
            .actors
            .iter_mut()
            .for_each(|actor| actor.next_review = 0);
        state
            .knowledge
            .as_mut()
            .unwrap()
            .minds
            .iter_mut()
            .for_each(|mind| {
                mind.records = vec![
                    KnowledgeRecord {
                        region: 1,
                        food: 0,
                        observed_at: 0,
                        witness: mind.person,
                        learned_from: None,
                    },
                    KnowledgeRecord {
                        region: 2,
                        food: 1_000,
                        observed_at: 0,
                        witness: 2,
                        learned_from: None,
                    },
                ];
            });
        if held {
            state.cargo.as_mut().unwrap().holdings = (2..=count as u64)
                .map(|person| game_parallel::Holding { person, food: 1 })
                .collect();
        }
        state.social = None;
        state.agriculture = None;
        state.civic_auto = None;
    }
    let mut kernel = pool.install(|| Kernel::compile(state)).unwrap();
    let started = std::time::Instant::now();
    let (prepare, commit) = pool.install(|| {
        let mut prepare = std::time::Duration::ZERO;
        let mut commit = std::time::Duration::ZERO;
        for _ in 0..ticks {
            let at = std::time::Instant::now();
            let draft = kernel.prepare().unwrap();
            prepare += at.elapsed();
            let at = std::time::Instant::now();
            kernel.commit(draft).unwrap();
            commit += at.elapsed();
        }
        (prepare, commit)
    });
    let elapsed = started.elapsed();
    let state = kernel.export();
    state.validate().unwrap();
    if departures && ticks == 1 {
        assert_eq!(state.travelers.len(), count - 1);
        assert_eq!(
            state
                .cargo
                .as_ref()
                .unwrap()
                .holdings
                .iter()
                .map(|holding| u64::from(holding.food))
                .sum::<u64>(),
            100 + if held { count as u64 - 1 } else { 0 }
        );
    }
    println!(
        "workers={workers} initial_people={count} regions={} hot={hot} departures={departures} held={held} farming_override={farming} ticks={ticks} final_people={} travelers={} seconds={:.6} prepare_seconds={:.6} commit_seconds={:.6}",
        settings.regions,
        state.families.as_ref().unwrap().members.len(),
        state.travelers.len(),
        elapsed.as_secs_f64(),
        prepare.as_secs_f64(),
        commit.as_secs_f64()
    );
}

#[test]
fn concentrated_births_cross_physical_and_knowledge_blocks_across_workers() {
    let (template, mut settings) = inputs();
    settings.population = 10_000;
    settings.regions = 2;
    settings.initial_food = template.rules.food_capacity;
    settings.initial_institutions = false;
    settings.working_percent = 0;
    let mut state = generate(&template, &settings).unwrap();
    let people = std::mem::take(&mut state.regions[1].people);
    state.regions[0].people.extend(people);
    state.validate().unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=6 {
                    kernel.step().unwrap();
                    if tick == 4 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert!(result.families.as_ref().unwrap().totals.births > 4096);
    assert_eq!(
        result.knowledge.as_ref().unwrap().minds.len(),
        result.families.as_ref().unwrap().members.len()
    );
    result.validate().unwrap();
}

#[test]
fn generated_citizens_trade_their_real_initial_goods_by_mutual_valuation() {
    use game_parallel::TradeOutcome;
    let (template, mut settings) = inputs();
    settings.population = 40;
    settings.regions = 2;
    settings.initial_institutions = false;
    settings.working_percent = 0;
    settings.trading_percent = 100;
    settings.trader_items = 1;
    let state = generate(&template, &settings).unwrap();
    let total = |world: &World, good| {
        world
            .goods_cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .flat_map(|holding| holding.items.iter())
            .filter(|item| item.good == good)
            .map(|item| item.quantity)
            .sum::<u64>()
    };
    let before = (total(&state, 1), total(&state, 2));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                assert!(!kernel.export().market.as_ref().unwrap().offers.is_empty());
                kernel.step().unwrap();
                kernel.export()
            })
    };
    let after = run(1);
    assert_eq!(after, run(4));
    assert!(
        after
            .market
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.outcome == TradeOutcome::Accepted)
    );
    assert_eq!((total(&after, 1), total(&after, 2)), before);
    after.validate().unwrap();
}

#[test]
fn generation_resets_template_learning_and_prior_invocation_state() {
    let (mut template, settings) = inputs();
    let mut history =
        World::from_toml(include_str!("../../../../content/parallel/teaching.toml")).unwrap();
    history
        .skills
        .as_mut()
        .unwrap()
        .lessons
        .push(game_parallel::SkillLesson {
            teacher: 1,
            student: 3,
            recipe: 1,
            tick: 0,
            source_life: Some(2),
        });
    template.skills = history.skills;
    template.lineage.as_mut().unwrap().invocation = history.lineage.unwrap().invocation;
    let invocation = template
        .lineage
        .as_mut()
        .unwrap()
        .invocation
        .as_mut()
        .unwrap();
    invocation.active.push(2);
    invocation.burden = 10;
    invocation.exposure = 9;
    let world = generate(&template, &settings).unwrap();
    let skills = world.skills.as_ref().unwrap();
    assert!(skills.records.is_empty());
    assert!(skills.lessons.is_empty());
    let invocation = world.lineage.as_ref().unwrap().invocation.as_ref().unwrap();
    assert_eq!(invocation.owner, Some(1));
    assert!(invocation.active.is_empty());
    assert_eq!((invocation.burden, invocation.exposure), (0, 0));
    world.validate().unwrap();
}

#[test]
fn generated_workers_learn_and_can_share_authored_crafts() {
    let (template, mut settings) = inputs();
    settings.working_percent = 100;
    let state = generate(&template, &settings).unwrap();
    let skills = state.skills.as_ref().unwrap();
    assert!(skills.records.is_empty());
    assert!(skills.lessons.is_empty());
    assert_eq!(
        skills.teachers.len(),
        (settings.population as usize - 1) * 2
    );
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for _ in 0..15 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                assert!(!result.skills.as_ref().unwrap().records.is_empty());
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_people_do_not_inherit_template_farm_consent() {
    let (mut template, mut settings) = inputs();
    settings.farming_percent = 0;
    template.agriculture.as_mut().unwrap().volunteers = Some(vec![1]);
    let world = generate(&template, &settings).unwrap();
    assert_eq!(
        world.agriculture.as_ref().unwrap().volunteers.as_deref(),
        Some(&[][..])
    );
    world.validate().unwrap();
}
#[test]
fn generated_farm_volunteers_follow_seeded_authored_fraction() {
    let (mut template, mut settings) = inputs();
    template.agriculture.as_mut().unwrap().volunteers = Some(vec![1]);
    settings.farming_percent = 100;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| generate(&template, &settings).unwrap())
    };
    let result = run(1);
    assert_eq!(result, run(4));
    let volunteers = result
        .agriculture
        .as_ref()
        .unwrap()
        .volunteers
        .as_ref()
        .unwrap();
    assert_eq!(volunteers.len(), settings.population as usize - 1);
    assert!(!volunteers.contains(&1));
    result.validate().unwrap();
    settings.farming_percent = 101;
    assert!(generate(&template, &settings).is_err());
    settings.farming_percent = 50;
    template.agriculture.as_mut().unwrap().volunteers = None;
    assert!(generate(&template, &settings).is_err());
}
#[test]
fn generated_residents_begin_with_no_inherited_consent_and_can_volunteer_later() {
    let (mut template, mut settings) = inputs();
    settings.farming_percent = 0;
    settings.working_percent = 0;
    settings.trading_percent = 0;
    settings.organizing_percent = 0;
    settings.joining_percent = 0;
    let agriculture = template.agriculture.as_mut().unwrap();
    agriculture.volunteers = Some(vec![1]);
    let recruitment = agriculture.recruitment.as_mut().unwrap();
    recruitment.interval = 1;
    recruitment.one_in = 1;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let state = generate(&template, &settings).unwrap();
                assert!(
                    state
                        .agriculture
                        .as_ref()
                        .unwrap()
                        .volunteers
                        .as_ref()
                        .unwrap()
                        .is_empty()
                );
                let mut kernel = Kernel::compile(state).unwrap();
                for _ in 0..4 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                assert!(
                    !result
                        .agriculture
                        .as_ref()
                        .unwrap()
                        .volunteers
                        .as_ref()
                        .unwrap()
                        .is_empty()
                );
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn generated_citizens_review_their_personal_reports_and_can_move() {
    use game_parallel::KnowledgeRecord;
    let (template, mut settings) = inputs();
    settings.working_percent = 0;
    settings.trading_percent = 0;
    settings.organizing_percent = 0;
    settings.joining_percent = 0;
    let mut state = generate(&template, &settings).unwrap();
    let autonomy = state.autonomy.as_mut().unwrap();
    assert_eq!(autonomy.actors.len(), settings.population as usize - 1);
    assert!(autonomy.actors.iter().all(|actor| actor.person != 1));
    autonomy
        .actors
        .iter_mut()
        .find(|actor| actor.person == 2)
        .unwrap()
        .next_review = 0;
    let person = state
        .regions
        .iter_mut()
        .flat_map(|region| &mut region.people)
        .find(|person| person.id == 2)
        .unwrap();
    person.hunger = 7;
    state.regions[1].food = 0;
    let mind = state
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|mind| mind.person == 2)
        .unwrap();
    mind.records = vec![
        KnowledgeRecord {
            region: 2,
            food: 0,
            observed_at: 0,
            witness: 2,
            learned_from: None,
        },
        KnowledgeRecord {
            region: 3,
            food: 100,
            observed_at: 0,
            witness: 3,
            learned_from: Some(3),
        },
    ];
    state.social = None;
    state.agriculture = None;
    state.civic_auto = None;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert!(
                    result
                        .travelers
                        .iter()
                        .any(|traveler| traveler.person.id == 2 && traveler.destination == 3)
                );
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn generated_reports_are_personal_witnessed_and_do_not_reveal_remote_changes() {
    let (template, mut settings) = inputs();
    settings.reporting_percent = 100;
    let state = generate(&template, &settings).unwrap();
    let minds = &state.knowledge.as_ref().unwrap().minds;
    assert!(
        minds
            .iter()
            .find(|mind| mind.person == 1)
            .unwrap()
            .records
            .is_empty()
    );
    for mind in minds.iter().filter(|mind| mind.person != 1) {
        assert_eq!(mind.records.len(), 1);
        let report = &mind.records[0];
        let local = (mind.person - 1) % u64::from(settings.regions) + 1;
        assert_eq!(report.region, local % u64::from(settings.regions) + 1);
        assert_ne!(report.witness, 1);
        assert!(
            state
                .regions
                .iter()
                .find(|region| region.id == report.region)
                .unwrap()
                .people
                .iter()
                .any(|person| person.id == report.witness)
        );
        assert_eq!(report.learned_from, Some(report.witness));
        assert_eq!(report.food, settings.initial_food);
    }
    let mut changed = state.clone();
    changed.regions[0].food = 0;
    assert_eq!(changed.knowledge, state.knowledge);
    changed.validate().unwrap();
    settings.reporting_percent = 101;
    assert!(generate(&template, &settings).is_err());
}
#[test]
fn generated_reports_do_not_invent_a_player_witness_when_no_other_person_is_there() {
    let (template, mut settings) = inputs();
    settings.regions = 2;
    settings.population = 2;
    settings.initial_institutions = false;
    settings.organizing_percent = 0;
    settings.joining_percent = 0;
    settings.trading_percent = 0;
    settings.reporting_percent = 100;
    let world = generate(&template, &settings).unwrap();
    assert!(
        world
            .knowledge
            .as_ref()
            .unwrap()
            .minds
            .iter()
            .all(|mind| mind.records.is_empty())
    );
    world.validate().unwrap();
}
#[test]
fn generated_world_without_a_ruler_gives_the_first_person_normal_agency() {
    let (mut template, mut settings) = inputs();
    template.lineage = None;
    template.guardians = None;
    settings.working_percent = 100;
    settings.farming_percent = 100;
    settings.reporting_percent = 100;
    settings.trading_percent = 100;
    let state = generate(&template, &settings).unwrap();
    assert!(
        state
            .autonomy
            .as_ref()
            .unwrap()
            .actors
            .iter()
            .any(|actor| actor.person == 1)
    );
    assert!(
        state
            .skills
            .as_ref()
            .unwrap()
            .teachers
            .iter()
            .any(|teacher| teacher.person == 1)
    );
    assert!(
        state
            .agriculture
            .as_ref()
            .unwrap()
            .volunteers
            .as_ref()
            .unwrap()
            .contains(&1)
    );
    assert!(
        state
            .market
            .as_ref()
            .unwrap()
            .policies
            .iter()
            .any(|policy| policy.person == 1)
    );
    let mind = state
        .knowledge
        .as_ref()
        .unwrap()
        .minds
        .iter()
        .find(|mind| mind.person == 1)
        .unwrap();
    assert_eq!(mind.records.len(), 1);
    assert_eq!(mind.records[0].region, 2);
    state.validate().unwrap();
}
#[test]
fn concentrated_generated_departures_share_real_provisions_without_losing_people() {
    use game_parallel::KnowledgeRecord;
    let (template, mut settings) = inputs();
    settings.population = 10_000;
    settings.working_percent = 0;
    settings.trading_percent = 0;
    settings.organizing_percent = 0;
    settings.joining_percent = 0;
    let mut state = generate(&template, &settings).unwrap();
    let (first, remaining) = state.regions.split_at_mut(1);
    for region in remaining {
        first[0].people.append(&mut region.people);
    }
    first[0].food = 20_100;
    state.rules.food_capacity = 100_000;
    for person in &mut first[0].people {
        person.hunger = 7;
    }
    for actor in &mut state.autonomy.as_mut().unwrap().actors {
        actor.next_review = 0;
    }
    for mind in &mut state.knowledge.as_mut().unwrap().minds {
        mind.records = vec![
            KnowledgeRecord {
                region: 1,
                food: 0,
                observed_at: 0,
                witness: mind.person,
                learned_from: None,
            },
            KnowledgeRecord {
                region: 2,
                food: 1_000,
                observed_at: 0,
                witness: 2,
                learned_from: None,
            },
        ];
    }
    state.social = None;
    state.agriculture = None;
    state.civic_auto = None;
    state.rules.food_growth = 0;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.travelers.len(), settings.population as usize - 1);
                assert_eq!(result.regions[0].people.len(), 1);
                assert_eq!(
                    result
                        .cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .map(|holding| u64::from(holding.food))
                        .sum::<u64>(),
                    100
                );
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_personal_food_choices_are_rebuilt_for_the_new_population() {
    let (mut template, mut settings) = inputs();
    settings.regions = 3;
    settings.population = 30;
    let cargo = template.cargo.as_mut().unwrap();
    cargo.personal_food_access = true;
    cargo.acquire_when_hungry = vec![2];
    cargo.food_aid = vec![game_parallel::FoodAidPolicy {
        giver: 2,
        recipient: 3,
        amount: 2,
        retain: 0,
        help_value: 1,
        threshold: 1,
        interval: 1,
        requires_request: false,
        enabled: true,
    }];
    let state = generate(&template, &settings).unwrap();
    let cargo = state.cargo.as_ref().unwrap();
    assert_eq!(cargo.acquire_when_hungry, (2..=30).collect::<Vec<_>>());
    assert!(cargo.food_aid.is_empty());
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
}

#[test]
fn generated_personal_food_contacts_are_local_and_worker_invariant() {
    let (template, mut settings) = inputs();
    settings.regions = 4;
    settings.population = 120;
    settings.personal_food_access = true;
    settings.dependent_food_care = true;
    settings.food_asking_percent = 100;
    settings.food_helpful_percent = 100;
    settings.food_request_interval = 1;
    settings.food_request_lifetime = 4;
    let state = generate(&template, &settings).unwrap();
    let cargo = state.cargo.as_ref().unwrap();
    assert!(cargo.personal_food_access);
    assert_eq!(
        cargo.auto_seek_age,
        Some(state.caregiving.as_ref().unwrap().rules.dependent_until)
    );
    assert!(state.caregiving.as_ref().unwrap().inherit_births);
    assert!(state.caregiving.as_ref().unwrap().rules.feed_from_cargo);
    assert!(state.caregiving.as_ref().unwrap().rules.fetch_local_food);
    assert_eq!(cargo.food_askers.len(), 119);
    assert!(!cargo.food_aid.is_empty());
    assert!(
        cargo
            .food_askers
            .iter()
            .all(|policy| policy.person != 1 && (policy.person - 1) % 4 == (policy.giver - 1) % 4)
    );
    assert!(cargo.food_aid.iter().all(|policy| policy.giver != 1
        && (policy.giver - 1) % 4 == (policy.recipient - 1) % 4
        && policy.requires_request));
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for _ in 0..8 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn personal_food_world_continues_through_births_and_maturing_children() {
    let (mut template, mut settings) = inputs();
    settings.regions = 4;
    settings.population = 80;
    settings.initial_food = 1000;
    settings.personal_food_access = true;
    settings.food_stores_per_region = 2;
    settings.food_store_users_percent = 25;
    settings.food_depositors_percent = 10;
    settings.food_store_claim_percent = 100;
    settings.food_store_objection_percent = 100;
    settings.confrontation_percent = 50;
    settings.confrontation_threshold = -1;
    settings.confrontation_interval = 1;
    settings.food_sharing_percent = 50;
    settings.work_memory_limit = 4;
    settings.dependent_food_care = true;
    settings.food_asking_percent = 25;
    settings.food_helpful_percent = 50;
    settings.care_pressure_threshold = Some(3);
    template.rules.food_growth = 20;
    let state = generate(&template, &settings).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=40 {
                    kernel.step().unwrap();
                    if tick == 20 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                let result = kernel.export();
                result.validate().unwrap();
                assert!(
                    result.families.as_ref().unwrap().members.len() > settings.population as usize
                );
                assert!(
                    result
                        .regions
                        .iter()
                        .flat_map(|region| &region.people)
                        .any(|person| person.health > 0)
                );
                assert!(
                    result
                        .cargo
                        .as_ref()
                        .unwrap()
                        .acquire_when_hungry
                        .iter()
                        .any(|&id| id > u64::from(settings.population))
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_food_stores_conserve_starting_food_and_use_local_sources() {
    let (template, mut settings) = inputs();
    settings.regions = 4;
    settings.population = 120;
    settings.personal_food_access = true;
    settings.food_stores_per_region = 2;
    settings.food_store_users_percent = 75;
    settings.food_depositors_percent = 40;
    settings.food_store_claim_percent = 100;
    settings.food_store_objection_percent = 100;
    settings.confrontation_percent = 100;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let world = generate(&template, &settings).unwrap();
                let cargo = world.cargo.as_ref().unwrap();
                assert_eq!(cargo.food_stores.len(), 8);
                assert!(world.conflict.is_some());
                for store in &cargo.food_stores {
                    assert!(store.object_to_taking);
                    let owner = store.claimant.unwrap();
                    assert_ne!(owner, 1);
                    assert!(world.regions.iter().any(|region| region.id == store.region
                        && region.people.iter().any(|person| person.id == owner)));
                    assert!(
                        world
                            .opinions
                            .as_ref()
                            .unwrap()
                            .sensitivities
                            .iter()
                            .any(|row| row.person == owner && row.confrontation.is_some())
                    );
                }
                for region in &world.regions {
                    let stored: u32 = cargo
                        .food_stores
                        .iter()
                        .filter(|store| store.region == region.id)
                        .map(|store| store.food)
                        .sum();
                    assert_eq!(region.food + stored, settings.initial_food);
                }
                for choice in &cargo.food_store_choices {
                    let region = world
                        .regions
                        .iter()
                        .find(|region| {
                            region
                                .people
                                .iter()
                                .any(|person| person.id == choice.person)
                        })
                        .unwrap();
                    assert!(
                        cargo
                            .food_stores
                            .iter()
                            .any(|store| store.id == choice.store && store.region == region.id)
                    );
                }
                for policy in &cargo.food_depositors {
                    let region = world
                        .regions
                        .iter()
                        .find(|region| {
                            region
                                .people
                                .iter()
                                .any(|person| person.id == policy.person)
                        })
                        .unwrap();
                    assert!(
                        cargo
                            .food_stores
                            .iter()
                            .any(|store| store.id == policy.store && store.region == region.id)
                    );
                }
                world
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_claimant_can_confront_a_person_after_a_witnessed_taking() {
    use game_parallel::FoodStoreChoice;
    let (template, mut settings) = inputs();
    settings.regions = 2;
    settings.population = 20;
    settings.working_percent = 0;
    settings.farming_percent = 0;
    settings.trading_percent = 0;
    settings.claim_tolerance_percent = 0;
    settings.personal_food_access = true;
    settings.food_stores_per_region = 2;
    settings.food_store_users_percent = 100;
    settings.food_depositors_percent = 0;
    settings.food_store_claim_percent = 100;
    settings.food_store_objection_percent = 100;
    settings.confrontation_percent = 100;
    settings.confrontation_threshold = -1;
    settings.confrontation_interval = 1;
    let mut world = generate(&template, &settings).unwrap();
    let store = world.cargo.as_ref().unwrap().food_stores[0].clone();
    let owner = store.claimant.unwrap();
    let taker = world
        .regions
        .iter()
        .find(|region| region.id == store.region)
        .unwrap()
        .people
        .iter()
        .find(|person| person.id != owner && person.id != 1)
        .unwrap()
        .id;
    for region in &mut world.regions {
        for person in &mut region.people {
            person.hunger = if person.id == taker {
                world.rules.hunger_satisfied - world.rules.hunger_per_tick
            } else {
                0
            };
        }
    }
    let cargo = world.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry = vec![taker];
    cargo.food_aid.clear();
    cargo.food_askers.clear();
    cargo.food_depositors.clear();
    cargo.food_store_choices = vec![FoodStoreChoice {
        person: taker,
        store: store.id,
        reconsider_on_failure: false,
    }];
    world.opinions.as_mut().unwrap().records.clear();
    world.social = None;
    world.autonomy = None;
    world.market = None;
    world.agreements = None;
    world.agriculture = None;
    world.caregiving = None;
    world.missions = None;
    world.fatigue = None;
    world.disease = None;
    world.goals = None;
    world.dreams = None;
    world.purpose = None;
    if let Some(auto) = &mut world.autowork {
        auto.policies.clear();
        auto.requests = None;
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut kernel = Kernel::compile(world.clone()).unwrap();
        kernel.step().unwrap();
        let first = kernel.export();
        assert!(first.cargo.as_ref().unwrap().holdings.iter().any(|h| h.person == taker && h.food == world.rules.food_per_meal));
        assert!(first.opinions.as_ref().unwrap().index().trust(owner, taker) <= -1);
        kernel.step().unwrap();
        let second = kernel.export();
        assert!(second.conflict.as_ref().unwrap().encounters.iter().any(|fight| fight.fighters.iter().any(|f| f.person == owner && matches!(f.order, game_parallel::CombatOrder::Attack { target } if target == taker))));
        second
    })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generator_rejects_preset_bodies_and_organization_policies() {
    let (template, mut settings) = inputs();
    settings.initial_institutions = true;
    assert!(
        generate(&template, &settings)
            .unwrap_err()
            .contains("preset institutions")
    );
    settings.initial_institutions = false;
    settings.organizing_percent = 1;
    assert!(
        generate(&template, &settings)
            .unwrap_err()
            .contains("preset institutions")
    );
    settings.organizing_percent = 0;
    settings.joining_percent = 1;
    assert!(
        generate(&template, &settings)
            .unwrap_err()
            .contains("preset institutions")
    );
}

#[test]
fn default_generated_profile_continues_with_births_stores_and_reloads() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/parallel/worldgen.toml");
    let state = crate::load(&path, None).unwrap();
    assert!(state.production.as_ref().unwrap().goods.len() >= 330);
    let starting_stores = state.cargo.as_ref().unwrap().food_stores.len();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=40 {
                    kernel.step().unwrap();
                    if tick == 20 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                let result = kernel.export();
                result.validate().unwrap();
                let alive = result
                    .regions
                    .iter()
                    .flat_map(|region| &region.people)
                    .filter(|person| person.health > 0)
                    .count()
                    + result
                        .travelers
                        .iter()
                        .filter(|traveler| traveler.person.health > 0)
                        .count();
                assert!(alive > 0, "all generated people died");
                assert!(result.families.as_ref().unwrap().totals.births > 0);
                assert!(result.cargo.as_ref().unwrap().food_stores.len() > starting_stores);
                assert!(result.institutions.is_none());
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_builder_uses_authored_material_to_make_a_real_store() {
    let (template, mut settings) = inputs();
    settings.population = 20;
    settings.regions = 2;
    settings.personal_food_access = true;
    settings.trading_percent = 100;
    settings.trader_items = 2;
    settings.store_builders_percent = 100;
    settings.store_builder_blueprint = Some(game_parallel::FoodStoreBlueprint {
        id: 7,
        good: 1,
        quantity: 2,
        capacity: 20,
    });
    let mut world = generate(&template, &settings).unwrap();
    let builder = world
        .goods_cargo
        .as_ref()
        .unwrap()
        .holdings
        .iter()
        .find(|row| {
            row.items
                .iter()
                .any(|item| item.good == 1 && item.quantity >= 2)
        })
        .unwrap()
        .person;
    let cargo = world.cargo.as_mut().unwrap();
    cargo
        .food_store_builders
        .retain(|row| row.person == builder);
    cargo.food_store_builders[0].interval = 1;
    cargo.acquire_when_hungry.clear();
    cargo.food_aid.clear();
    cargo.food_askers.clear();
    cargo.food_depositors.clear();
    world.social = None;
    world.autonomy = None;
    world.market = None;
    world.agreements = None;
    world.agriculture = None;
    world.caregiving = None;
    world.missions = None;
    world.fatigue = None;
    world.personality = None;
    world.disease = None;
    world.goals = None;
    world.dreams = None;
    world.purpose = None;
    world.autowork.as_mut().unwrap().policies.clear();
    world.autowork.as_mut().unwrap().requests = None;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert!(
                    result
                        .cargo
                        .as_ref()
                        .unwrap()
                        .food_stores
                        .iter()
                        .any(|store| store.claimant == Some(builder) && store.capacity == 20)
                );
                assert!(
                    result
                        .goods_cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .all(|row| row.person != builder)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));

    // Construction still respects work age when neither farming nor autowork is enabled.
    let mut child_world = world.clone();
    child_world.autowork = None;
    child_world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|row| row.id == builder)
        .unwrap()
        .age = 0;
    let mut child = Kernel::compile(child_world).unwrap();
    child.step().unwrap();
    let mut grown = child.export();
    assert!(grown.cargo.as_ref().unwrap().food_stores.is_empty());
    assert!(
        grown
            .goods_cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .any(|row| row.person == builder)
    );
    let family = grown.families.as_mut().unwrap();
    family
        .members
        .iter_mut()
        .find(|row| row.id == builder)
        .unwrap()
        .age = family.rules.min_work_age;
    let mut grown = Kernel::compile(grown).unwrap();
    grown.step().unwrap();
    assert!(
        grown
            .export()
            .cargo
            .unwrap()
            .food_stores
            .iter()
            .any(|store| store.claimant == Some(builder))
    );
}

#[test]
fn authored_object_and_recipe_catalogs_produce_real_physical_work() {
    use game_parallel::StartWork;
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))
    .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let production = world.production.as_mut().unwrap();
    game_parallel_content_import::augment(
        production,
        &root.join("objects"),
        &root.join("workshop"),
        600_000,
        1000,
        10,
    )
    .unwrap();
    assert!(production.goods.len() >= 330);
    assert!(production.recipes.len() >= 230);
    let food = production
        .goods
        .iter()
        .find(|good| good.name == "roast-meat")
        .unwrap()
        .food
        .as_ref()
        .unwrap();
    assert_eq!((food.portion, food.hunger_relief), (8, 5));
    assert_eq!(food.nutrients.get("protein"), Some(&20));
    let saw = production
        .recipes
        .iter()
        .find(|recipe| recipe.name == "saw-planks")
        .unwrap()
        .clone();
    assert_eq!(saw.ticks, 2);
    assert_eq!(saw.inputs[0].quantity, 8);
    assert_eq!(saw.outputs[0].quantity, 6);
    assert_eq!(
        production
            .goods
            .iter()
            .find(|good| good.id == saw.inputs[0].good)
            .unwrap()
            .name,
        "timber"
    );
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel
                    .start_work(&[StartWork {
                        id: 10,
                        worker: 1,
                        region: 1,
                        recipe: saw.id,
                        units: 1,
                    }])
                    .unwrap();
                for _ in 0..2 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let stock = &result
                    .production
                    .as_ref()
                    .unwrap()
                    .inventories
                    .iter()
                    .find(|row| row.region == 1)
                    .unwrap()
                    .stocks;
                assert_eq!(
                    stock
                        .iter()
                        .find(|row| row.good == saw.inputs[0].good)
                        .unwrap()
                        .quantity,
                    1
                );
                assert_eq!(
                    stock
                        .iter()
                        .find(|row| row.good == saw.outputs[0].good)
                        .unwrap()
                        .quantity,
                    6
                );
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_craft_categories_share_practice_across_real_recipes() {
    use game_parallel::{SkillCatalogTraining, Skills, StartWork};
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))
    .unwrap();
    state.market = None;
    state.autowork = None;
    state.rules.hunger_per_tick = 0;
    state.social = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap()
    .social;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let production = state.production.as_mut().unwrap();
    game_parallel_content_import::augment(
        production,
        &root.join("objects"),
        &root.join("workshop"),
        600_000,
        1000,
        10,
    )
    .unwrap();
    let press = production
        .recipes
        .iter()
        .find(|recipe| recipe.name == "press-fiberboard")
        .unwrap()
        .clone();
    let laminate = production
        .recipes
        .iter()
        .find(|recipe| recipe.name == "laminate-beam")
        .unwrap()
        .clone();
    let mill = production
        .recipes
        .iter()
        .find(|recipe| recipe.name == "press-grain-meal")
        .unwrap()
        .id;
    assert_eq!((press.ticks, laminate.ticks), (6, 12));
    let category = |recipe| {
        &production
            .recipe_categories
            .iter()
            .find(|row| row.recipe == recipe)
            .unwrap()
            .category
    };
    assert_eq!(category(press.id), "carpentry");
    assert_eq!(category(laminate.id), "carpentry");
    assert_eq!(category(mill), "milling");
    let timber = production
        .goods
        .iter()
        .find(|good| good.name == "timber")
        .unwrap()
        .id;
    let resin = production
        .goods
        .iter()
        .find(|good| good.name == "resin")
        .unwrap()
        .id;
    let inventory = production
        .inventories
        .iter_mut()
        .find(|inventory| inventory.region == 1)
        .unwrap();
    inventory
        .stocks
        .iter_mut()
        .find(|stock| stock.good == timber)
        .unwrap()
        .quantity = 16;
    inventory
        .stocks
        .iter_mut()
        .find(|stock| stock.good == resin)
        .unwrap()
        .quantity = 5;
    let mut skills = Skills {
        rules: vec![],
        aliases: vec![],
        catalog_training: Some(SkillCatalogTraining {
            completions_per_level: 2,
            max_level: 3,
            work_per_level: 1,
        }),
        records: vec![],
        events: vec![],
        lessons: vec![],
        teachers: vec![],
        inherit_teachers: false,
    };
    game_parallel_skill_catalog::configure(&mut skills, &production.recipe_categories).unwrap();
    assert_eq!(skills.canonical(press.id), skills.canonical(laminate.id));
    assert_ne!(skills.canonical(press.id), skills.canonical(mill));
    state.skills = Some(skills);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for id in 1..=2 {
                    kernel
                        .start_work(&[StartWork {
                            id,
                            worker: 1,
                            region: 1,
                            recipe: press.id,
                            units: 1,
                        }])
                        .unwrap();
                    for _ in 0..6 {
                        kernel.step().unwrap();
                    }
                    if id == 1 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                let trained = kernel.export();
                let index = trained.skills.as_ref().unwrap().index();
                assert_eq!(index.rate(1, press.id), 2);
                assert_eq!(index.rate(1, laminate.id), 2);
                assert_eq!(index.rate(1, mill), 1);
                assert_eq!(index.rate(2, press.id), 1);
                let canonical = trained.skills.as_ref().unwrap().canonical(press.id);
                assert!(kernel.teach(1, 2, laminate.id).unwrap());
                let taught = kernel.export();
                assert!(taught.skills.as_ref().unwrap().lessons.iter().any(
                    |lesson| lesson.teacher == 1
                        && lesson.student == 2
                        && lesson.recipe == canonical
                ));
                kernel
                    .start_work(&[StartWork {
                        id: 3,
                        worker: 1,
                        region: 1,
                        recipe: laminate.id,
                        units: 1,
                    }])
                    .unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(
                    result
                        .production
                        .as_ref()
                        .unwrap()
                        .jobs
                        .iter()
                        .find(|job| job.id == 3)
                        .unwrap()
                        .remaining,
                    10
                );
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn default_generated_world_enables_authored_catalog_skills() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/parallel/worldgen.toml");
    let state = crate::load(&path, Some(42)).unwrap();
    let production = state.production.as_ref().unwrap();
    let press = production
        .recipes
        .iter()
        .find(|recipe| recipe.name == "press-fiberboard")
        .unwrap()
        .id;
    let laminate = production
        .recipes
        .iter()
        .find(|recipe| recipe.name == "laminate-beam")
        .unwrap()
        .id;
    let skills = state.skills.as_ref().unwrap();
    assert_eq!(skills.canonical(press), skills.canonical(laminate));
    assert!(
        skills
            .rules
            .iter()
            .any(|rule| rule.recipe == skills.canonical(press))
    );
    state.validate().unwrap();
}

#[test]
fn failed_catalog_load_does_not_mutate_the_production_owner() {
    let (world, _) = inputs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut production = world.production.unwrap();
    let before = production.clone();
    assert!(
        game_parallel_content_import::augment(
            &mut production,
            &root.join("objects"),
            &root.join("missing-recipes"),
            600_000,
            1000,
            10
        )
        .is_err()
    );
    assert_eq!(production, before);
    assert!(
        game_parallel_content_import::augment(
            &mut production,
            &root.join("objects"),
            &root.join("workshop"),
            0,
            1000,
            10
        )
        .is_err()
    );
    assert_eq!(production, before);
}

#[test]
fn default_world_continues_across_generations_on_multiple_seeds() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/parallel/worldgen.toml");
    for seed in [42, 43] {
        let state = crate::load(&path, Some(seed)).unwrap();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut kernel = Kernel::compile(state.clone()).unwrap();
                    for tick in 0..200 {
                        kernel.step().unwrap();
                        if tick == 99 {
                            kernel = Kernel::compile(
                                World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                    .unwrap(),
                            )
                            .unwrap();
                        }
                    }
                    let result = kernel.export();
                    let alive = result
                        .regions
                        .iter()
                        .flat_map(|region| &region.people)
                        .filter(|person| person.health > 0)
                        .count()
                        + result
                            .travelers
                            .iter()
                            .filter(|traveler| traveler.person.health > 0)
                            .count();
                    assert!(alive > 0, "seed {seed} lost its population by tick 200");
                    assert!(result.families.as_ref().unwrap().totals.births > 0);
                    result.validate().unwrap();
                    result
                })
        };
        assert_eq!(run(1), run(4));
    }
}
