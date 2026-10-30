use game_parallel::World;
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/dreams.toml")).unwrap()
}
#[test]
fn living_aspiration_creates_work_goal_then_renews_it_after_real_progress() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert!(state.production.as_ref().unwrap().jobs.is_empty());
    assert_eq!(state.goals.as_ref().unwrap().people[0].target, 1);
    assert_eq!(state.dreams.as_ref().unwrap().events[0].target, 1);
    kernel.step().unwrap();
    let saved = kernel.export();
    assert_eq!(saved.production.as_ref().unwrap().jobs[0].worker, 1);
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap())
        .unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    let goal = &state.goals.as_ref().unwrap().people[0];
    assert_eq!((goal.progress, goal.target), (1, 2));
    assert!(state.goals.unwrap().events[0].completed);
    assert_eq!(state.dreams.unwrap().events[0].target, 2);
}
#[test]
fn dead_people_do_not_form_new_goals_and_review_overflow_rolls_back() {
    let mut dead = world();
    dead.regions[0].people[0].health = 0;
    let mut kernel = Kernel::compile(dead).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert!(result.goals.unwrap().people.is_empty());
    assert_eq!(result.dreams.unwrap().people[0].next_review, 0);
    let mut invalid = world();
    invalid.dreams.as_mut().unwrap().people[0].review_interval = u64::MAX;
    let mut kernel = Kernel::compile(invalid).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn aspiration_reviews_are_worker_invariant_after_reload() {
    let state = world();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=7 {
                    kernel.step().unwrap();
                    if tick == 3 {
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
    assert_eq!(run(1), run(4));
}
#[test]
fn changing_an_aspiration_is_checked_and_invalidates_a_prepared_tick() {
    use game_parallel::SetDream;
    let mut kernel = Kernel::compile(world()).unwrap();
    let draft = kernel.prepare().unwrap();
    let request = SetDream {
        person: 1,
        recipe: 1,
        priority: 8,
        batch: 2,
        review_interval: 5,
        enabled: true,
    };
    kernel.set_dream(request).unwrap();
    assert!(kernel.commit(draft).is_err());
    kernel.step().unwrap();
    assert_eq!(kernel.export().goals.unwrap().people[0].target, 2);
    let before = kernel.export();
    assert!(
        kernel
            .set_dream(SetDream {
                review_interval: 0,
                ..request
            })
            .is_err()
    );
    assert!(
        kernel
            .set_dream(SetDream {
                recipe: 999,
                ..request
            })
            .is_err()
    );
    assert_eq!(kernel.export(), before);
}
