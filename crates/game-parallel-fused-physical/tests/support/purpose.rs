use game_parallel::{Purpose, PurposePerson, PurposeRules, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/goals.toml")).unwrap();
    state.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 0;
    state.purpose = Some(Purpose {
        rules: PurposeRules {
            max: 20,
            per_tick: 1,
            blocked_increase: 2,
            completion_relief: 6,
            damage_threshold: 8,
            damage: 2,
        },
        people: vec![
            PurposePerson {
                person: 1,
                deprivation: 5,
                eligible_at: 0,
            },
            PurposePerson {
                person: 2,
                deprivation: 5,
                eligible_at: 0,
            },
        ],
    });
    state
}
#[test]
fn blocked_and_successful_goal_outcomes_change_purpose_and_severe_deprivation_harms_health() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let mut first = kernel.export();
    assert_eq!(first.purpose.as_ref().unwrap().people[0].deprivation, 8);
    assert_eq!(first.purpose.as_ref().unwrap().people[1].deprivation, 6);
    assert_eq!(first.regions[0].people[0].health, 98);
    first.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 3;
    first.goals.as_mut().unwrap().people[0].next_review = first.tick;
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&first).unwrap()).unwrap())
        .unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let previous = kernel.export().purpose.unwrap().people[0].deprivation;
    kernel.step().unwrap();
    let after = kernel.export();
    assert_eq!(after.goals.unwrap().people[0].progress, 1);
    assert_eq!(
        after.purpose.unwrap().people[0].deprivation,
        (previous + 1).saturating_sub(6)
    );
}
#[test]
fn late_failure_does_not_publish_purpose_changes_or_health_damage() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let content = world();
    state.production = content.production;
    state.autowork = content.autowork;
    state.goals = content.goals;
    state.purpose = content.purpose;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn newborns_are_enrolled_but_purpose_does_not_accrue_until_adulthood() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    let content = world();
    state.production = content.production;
    state.autowork = content.autowork;
    state.goals = content.goals;
    state.purpose = content.purpose;
    let first_newborn = state.families.as_ref().unwrap().next_id;
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..8 {
        kernel.step().unwrap();
    }
    let saved = kernel.export();
    let family = saved.families.as_ref().unwrap();
    let children: Vec<_> = family
        .members
        .iter()
        .filter(|member| member.id >= first_newborn)
        .collect();
    assert!(!children.is_empty());
    for child in children {
        let need = saved
            .purpose
            .as_ref()
            .unwrap()
            .people
            .iter()
            .find(|row| row.person == child.id)
            .unwrap();
        assert_eq!(
            need.eligible_at,
            child.born.unwrap() + family.rules.adult_age
        );
        assert_eq!(need.deprivation, 0);
    }
    assert_eq!(
        Kernel::compile(World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap())
            .unwrap()
            .export(),
        saved
    );
}
#[test]
fn purpose_rejects_invalid_rules_and_unsupported_reference_hosts() {
    let mut state = world();
    state.purpose.as_mut().unwrap().rules.damage_threshold = 0;
    assert!(state.validate().is_err());
    let mut state = world();
    let before = state.clone();
    assert!(state.step().is_err());
    assert_eq!(state, before);
    assert!(game_parallel_blocks::Engine::compile(state).is_err());
}
#[test]
fn purpose_effects_and_goal_outcomes_are_worker_invariant() {
    let state = world();
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
                kernel.export()
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn authored_purpose_fixture_runs() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/purpose.toml")).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..3 {
        kernel.step().unwrap();
    }
    assert_eq!(kernel.export().goals.unwrap().people[0].progress, 1);
}
