use super::*;
use game_lot_runtime::{Runtime, Snapshot};
use game_lots::{Container, Lot, State};
use game_production_model::FoodProfile;
use std::collections::BTreeMap;
fn food() -> Good {
    Good {
        id: 1,
        name: "meal".into(),
        food: Some(FoodProfile {
            portion: 2,
            hunger_relief: 5,
            nutrients: BTreeMap::from([("energy".into(), 7)]),
            health_effect: -1,
            shelf_days: Some(2),
        }),
    }
}
fn runtime(expiry: u64) -> Runtime {
    Runtime::compile(Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![
                Container::new(
                    Place::Person(1),
                    4,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: Some(expiry),
                    }],
                )
                .unwrap(),
            ],
        },
        goods: vec![food()],
        recipes: vec![],
        tick_millis: 86_400_000,
        work: vec![],
        work_interval: None,
    })
    .unwrap()
}
#[test]
fn only_actual_unexpired_meals_produce_nutrition_and_health_effects() {
    for (expiry, eaten) in [(1, false), (3, true)] {
        let mut runtime = runtime(expiry);
        let before = runtime.snapshot();
        let meal = plan(
            &[Meal {
                action: 1,
                person: 1,
                good: 1,
            }],
            &[food()],
        )
        .unwrap();
        let draft = runtime.prepare_advance(&[], meal.actions(), 1).unwrap();
        let effects = meal.effects(draft.events()).unwrap();
        assert_eq!(runtime.snapshot(), before);
        if eaten {
            assert_eq!(effects.hunger_relief, vec![(1, 5)]);
            assert_eq!(
                (
                    effects.pending[0].person,
                    effects.pending[0].at,
                    effects.pending[0].health,
                    effects.pending[0].nutrients[0]
                ),
                (1, 2, -1, 7)
            );
        } else {
            assert!(effects.hunger_relief.is_empty() && effects.pending.is_empty());
        }
        runtime.commit_advance(draft).unwrap();
        assert_eq!(runtime.custody(Place::Person(1)).unwrap().used, 0);
    }
}
#[test]
fn duplicate_meals_and_effect_tick_overflow_are_rejected() {
    let meal = Meal {
        action: 1,
        person: 1,
        good: 1,
    };
    assert!(plan(&[meal, Meal { action: 2, ..meal }], &[food()]).is_err());
    let mut snapshot = runtime(3).snapshot();
    snapshot.custody.tick = u64::MAX - 1;
    snapshot.custody.containers[0] = Container::new(
        Place::Person(1),
        4,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let runtime = Runtime::compile(snapshot).unwrap();
    let plan = plan(&[meal], &[food()]).unwrap();
    let draft = runtime.prepare_advance(&[], plan.actions(), 1).unwrap();
    assert!(plan.effects(draft.events()).is_err());
    assert_eq!(runtime.custody(Place::Person(1)).unwrap().used, 2);
}

#[test]
fn many_dated_meals_produce_the_same_personal_effects_on_one_and_four_workers() {
    let count = 4096u64;
    let state = Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: (1..=count)
                .map(|person| {
                    Container::new(
                        Place::Person(person),
                        2,
                        vec![Lot {
                            item: None,
                            good: 1,
                            quantity: 2,
                            expires_at: Some(if person % 2 == 0 { 1 } else { 3 }),
                        }],
                    )
                    .unwrap()
                })
                .collect(),
        },
        goods: vec![food()],
        recipes: vec![],
        tick_millis: 86_400_000,
        work: vec![],
        work_interval: None,
    };
    let meals: Vec<_> = (1..=count)
        .rev()
        .map(|person| Meal {
            action: person,
            person,
            good: 1,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = Runtime::compile(state.clone()).unwrap();
                let plan = plan(&meals, &[food()]).unwrap();
                let draft = runtime.prepare_advance(&[], plan.actions(), 42).unwrap();
                let effects = plan.effects(draft.events()).unwrap();
                assert_eq!(effects.hunger_relief.len(), count as usize / 2);
                runtime.commit_advance(draft).unwrap();
                (runtime.snapshot(), effects)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn pending_food_validation_matches_the_reference_across_workers() {
    let people: HashSet<_> = (1..=128).collect();
    let effects: Vec<_> = (1..=128)
        .map(|person| FoodEffect {
            person,
            at: 2,
            health: 0,
            nutrients: [1; 6],
        })
        .collect();
    let run = |rows: &[FoodEffect], workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| validate_pending(rows, &people, 1))
    };
    assert!(run(&effects, 1).is_ok());
    assert_eq!(run(&effects, 1), run(&effects, 4));
    for change in 0..3 {
        let mut rows = effects.clone();
        match change {
            0 => rows[100].at = 1,
            1 => rows[100].person = 999,
            _ => rows[100].nutrients = [0; 6],
        }
        let expected = game_production_model::FoodEffects {
            pending: rows.clone(),
        }
        .validate(&people, 1)
        .map_err(str::to_owned);
        assert_eq!(run(&rows, 1), expected);
        assert_eq!(run(&rows, 4), expected);
    }
}

#[test]
#[ignore = "manual pending-food validation benchmark"]
fn pending_food_validation_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000_000u64);
    let people: HashSet<_> = (1..=count).collect();
    let effects: Vec<_> = (1..=count)
        .map(|person| FoodEffect {
            person,
            at: 2,
            health: 1,
            nutrients: [0; 6],
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..6 {
                let started = std::time::Instant::now();
                assert!(validate_pending(&effects, &people, 1).is_ok());
                let elapsed = started.elapsed().as_secs_f64();
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} effects={count} elapsed={:.6}",
                samples[2]
            );
        });
}
