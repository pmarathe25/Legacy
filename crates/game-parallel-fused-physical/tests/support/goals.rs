use game_parallel::{StartWork, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/goals.toml")).unwrap()
}
#[test]
fn personal_craft_goal_starts_actual_work_and_finishes_after_two_completions() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    assert_eq!(first.production.as_ref().unwrap().jobs[0].worker, 1);
    assert_eq!(first.goals.as_ref().unwrap().people[0].progress, 0);
    kernel.step().unwrap();
    kernel.step().unwrap();
    let progress = kernel.export();
    assert_eq!(progress.goals.as_ref().unwrap().people[0].progress, 1);
    assert!(!progress.goals.as_ref().unwrap().events[0].completed);
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&progress).unwrap()).unwrap())
        .unwrap();
    for _ in 0..3 {
        kernel.step().unwrap();
    }
    let done = kernel.export();
    assert_eq!(done.goals.as_ref().unwrap().people[0].progress, 2);
    assert!(done.goals.as_ref().unwrap().events[0].completed);
    kernel.step().unwrap();
    let after = kernel.export();
    assert!(after.production.unwrap().jobs.is_empty());
    assert!(after.goals.unwrap().events.is_empty());
}
#[test]
fn blocked_output_cannot_repeat_goal_progress() {
    let mut state = world();
    state.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 4;
    state.goals.as_mut().unwrap().people[0].target = 1;
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..3 {
        kernel.step().unwrap();
    }
    let first = kernel.export();
    assert_eq!(first.goals.as_ref().unwrap().people[0].progress, 1);
    assert_eq!(first.production.as_ref().unwrap().jobs.len(), 1);
    for _ in 0..4 {
        kernel.step().unwrap();
    }
    let after = kernel.export();
    assert_eq!(after.goals.unwrap().people[0].progress, 1);
    assert_eq!(after.production.unwrap().jobs.len(), 1);
}
#[test]
fn late_family_failure_rolls_back_completed_goal_and_work() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let content = world();
    state.production = content.production;
    state.autowork = content.autowork;
    state.goals = content.goals;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    state
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    state.production.as_mut().unwrap().jobs[0].remaining = 1;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn blocked_goal_backs_off_and_a_lower_priority_feasible_plan_can_run() {
    use game_parallel::{Amount, Goal, Recipe};
    let mut state = world();
    state.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 0;
    state.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 1;
    state.production.as_mut().unwrap().recipes.push(Recipe {
        id: 2,
        name: "recover supplies".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 1,
            quantity: 3,
        }],
    });
    state.goals.as_mut().unwrap().people[0].target = 1;
    state.goals.as_mut().unwrap().people.push(Goal {
        person: 1,
        recipe: 2,
        priority: 1,
        target: 1,
        progress: 0,
        next_review: 0,
        enabled: true,
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let blocked = kernel.export();
    assert!(blocked.production.as_ref().unwrap().jobs.is_empty());
    assert!(blocked.goals.as_ref().unwrap().events[0].blocked);
    assert_eq!(blocked.goals.as_ref().unwrap().people[0].next_review, 4);
    kernel.step().unwrap();
    assert_eq!(kernel.export().production.unwrap().jobs[0].recipe, 2);
    kernel.step().unwrap();
    assert_eq!(kernel.export().goals.unwrap().people[1].progress, 1);
    kernel.step().unwrap();
    assert!(kernel.export().production.unwrap().jobs.is_empty());
    kernel.step().unwrap();
    assert_eq!(kernel.export().production.unwrap().jobs[0].recipe, 1);
    kernel.step().unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().goals.unwrap().people[0].progress, 1);
}

#[test]
fn failed_tick_does_not_publish_goal_backoff() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let content = world();
    state.production = content.production;
    state.autowork = content.autowork;
    state.goals = content.goals;
    state.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 0;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn retry_overflow_rejects_tick_without_partial_mutation() {
    let mut state = world();
    state.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 0;
    state.goals.as_mut().unwrap().retry_interval = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn changing_personal_goal_is_checked_persistent_and_invalidates_a_prepared_tick() {
    use game_parallel::SetGoal;
    let mut kernel = Kernel::compile(world()).unwrap();
    let draft = kernel.prepare().unwrap();
    let request = SetGoal {
        person: 2,
        recipe: 1,
        priority: 9,
        target: 1,
        enabled: true,
    };
    kernel.set_goal(request).unwrap();
    assert!(kernel.commit(draft).is_err());
    let saved = kernel.export();
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap())
        .unwrap();
    kernel.step().unwrap();
    assert!(
        kernel
            .export()
            .production
            .unwrap()
            .jobs
            .iter()
            .any(|job| job.worker == 2)
    );
    let before = kernel.export();
    assert!(
        kernel
            .set_goal(SetGoal {
                recipe: 999,
                ..request
            })
            .is_err()
    );
    assert!(
        kernel
            .set_goal(SetGoal {
                person: 999,
                ..request
            })
            .is_err()
    );
    assert!(
        kernel
            .set_goal(SetGoal {
                target: 0,
                ..request
            })
            .is_err()
    );
    assert_eq!(kernel.export(), before);
}
#[test]
fn revising_a_goal_preserves_earned_progress_and_rejects_a_smaller_target() {
    use game_parallel::SetGoal;
    let mut kernel = Kernel::compile(world()).unwrap();
    for _ in 0..6 {
        kernel.step().unwrap();
    }
    let before = kernel.export();
    assert_eq!(before.goals.as_ref().unwrap().people[0].progress, 2);
    let request = SetGoal {
        person: 1,
        recipe: 1,
        priority: 4,
        target: 3,
        enabled: false,
    };
    kernel.set_goal(request).unwrap();
    let goal = &kernel.export().goals.unwrap().people[0];
    assert_eq!(
        (goal.progress, goal.priority, goal.target, goal.enabled),
        (2, 4, 3, false)
    );
    let before = kernel.export();
    assert!(
        kernel
            .set_goal(SetGoal {
                target: 1,
                ..request
            })
            .is_err()
    );
    assert_eq!(kernel.export(), before);
}
