use game_parallel::{Amount, Goal, Person, Recipe, World};
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/goal_planning.toml"
    ))
    .unwrap();
    state.goals.as_mut().unwrap().people[0].target = 1;
    state
}
#[test]
fn missing_intermediate_is_made_before_the_goal_and_consumes_actual_inputs() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    let production = state.production.as_ref().unwrap();
    assert_eq!(production.jobs[0].recipe, 2);
    assert_eq!(production.inventories[0].stocks[2].quantity, 0);
    assert_eq!(state.goals.as_ref().unwrap().people[0].progress, 0);
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
        .unwrap();
    for _ in 0..5 {
        kernel.step().unwrap();
    }
    let result = kernel.export();
    assert_eq!(result.goals.as_ref().unwrap().people[0].progress, 1);
    let stock = &result.production.as_ref().unwrap().inventories[0].stocks;
    assert_eq!(
        (stock[0].quantity, stock[1].quantity, stock[2].quantity),
        (0, 2, 0)
    );
    result.validate().unwrap();
}
#[test]
fn unavailable_inputs_and_recipe_cycles_back_off_the_original_goal() {
    for cycle in [false, true] {
        let mut state = world();
        state.production.as_mut().unwrap().inventories[0].stocks[2].quantity = 0;
        if cycle {
            state.production.as_mut().unwrap().recipes.push(Recipe {
                id: 3,
                name: "cycle".into(),
                ticks: 1,
                inputs: vec![Amount {
                    good: 1,
                    quantity: 1,
                }],
                outputs: vec![Amount {
                    good: 3,
                    quantity: 1,
                }],
            });
        }
        let mut kernel = Kernel::compile(state).unwrap();
        kernel.step().unwrap();
        let result = kernel.export();
        assert!(result.production.unwrap().jobs.is_empty());
        let goals = result.goals.unwrap();
        assert_eq!(goals.events[0].recipe, 1);
        assert!(goals.events[0].blocked);
        assert!(goals.people[0].next_review > result.tick);
    }
}
#[test]
fn competing_plans_reserve_real_stock_and_back_off_the_losing_final_goal() {
    let mut state = world();
    let mut second = state.goals.as_ref().unwrap().people[0].clone();
    second.person = 2;
    state.goals.as_mut().unwrap().people.push(second);
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    let jobs = &result.production.as_ref().unwrap().jobs;
    assert_eq!(jobs.len(), 1);
    assert_eq!((jobs[0].worker, jobs[0].recipe), (1, 2));
    let event = &result.goals.as_ref().unwrap().events[0];
    assert_eq!((event.person, event.recipe, event.blocked), (2, 1, true));
}
#[test]
fn many_local_plans_are_worker_invariant_after_reload() {
    let count = 2000u64;
    let mut state = world();
    state.rules.hunger_per_tick = 0;
    state.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    state.regions[1].people.clear();
    let stocks = &mut state.production.as_mut().unwrap().inventories[0].stocks;
    stocks[0].capacity = count * 3;
    stocks[1].capacity = count * 2;
    stocks[2].capacity = count;
    stocks[2].quantity = count / 2;
    state.goals.as_mut().unwrap().people = (1..=count)
        .map(|person| Goal {
            person,
            recipe: 1,
            priority: 1,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=8 {
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
    let result = run(1);
    assert_eq!(result, run(4));
    result.validate().unwrap();
}

#[test]
fn planner_follows_multiple_stages_and_skips_cyclic_alternative_producers() {
    use game_parallel::{Good, Stock};
    let mut state = world();
    let production = state.production.as_mut().unwrap();
    production.goods.push(Good {
        id: 4,
        name: "ore".into(),
        food: None,
    });
    production.inventories[0].stocks[2].quantity = 0;
    production.inventories[0].stocks.push(Stock {
        good: 4,
        quantity: 1,
        capacity: 10,
    });
    production.recipes.push(Recipe {
        id: 0,
        name: "cyclic timber".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
    });
    production.recipes.push(Recipe {
        id: 3,
        name: "make logs".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 4,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().production.unwrap().jobs[0].recipe, 3);
    for _ in 0..10 {
        kernel.step().unwrap();
    }
    let result = kernel.export();
    assert_eq!(result.goals.unwrap().people[0].progress, 1);
    let stocks = &result.production.unwrap().inventories[0].stocks;
    assert_eq!(
        (
            stocks[0].quantity,
            stocks[1].quantity,
            stocks[2].quantity,
            stocks[3].quantity
        ),
        (0, 2, 0, 0)
    );
}
