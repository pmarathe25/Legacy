use game_economy::core_interval::{CoreHost, CoreHostSnapshot, CoreInterval, CoreSnapshot};
use game_economy_person::{
    Person, Rules,
    travel::{Journey, Route, Travel},
};
use game_lot_diet::Effects;
use game_lot_runtime::{Runtime, Snapshot};
use game_lots::{Container, Lot, Place, State};
use game_production_model::{Amount, Good, Recipe, Start};

fn fixture() -> CoreSnapshot {
    let container = |region, good, quantity| {
        Container::new(
            Place::Stock { region, good },
            10,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    item: None,
                    good,
                    quantity,
                    expires_at: None,
                }]
            },
        )
        .unwrap()
    };
    let mut runtime = Runtime::compile(Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![
                container(1, 1, 2),
                container(1, 2, 0),
                container(2, 1, 0),
                container(2, 2, 0),
            ],
        },
        goods: vec![
            Good {
                id: 1,
                name: "input".into(),
                food: None,
            },
            Good {
                id: 2,
                name: "output".into(),
                food: None,
            },
        ],
        recipes: vec![Recipe {
            id: 1,
            name: "work".into(),
            ticks: 1,
            inputs: vec![Amount {
                good: 1,
                quantity: 2,
            }],
            outputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        }],
        tick_millis: 1000,
        work: vec![],
        work_interval: None,
    })
    .unwrap();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            7,
        )
        .unwrap();
    let person = |id, region| Person {
        id,
        region,
        health: 100,
        hunger: 2,
        hunger_remainder: 0,
        immortal: None,
        willing_to_work: false,
        seek_meals: false,
        controlled: false,
    };
    CoreSnapshot {
        seed: 7,
        physical: runtime.snapshot(),
        people: vec![person(1, Some(1)), person(2, None)],
        rules: Rules {
            hunger_per_tick: 1,
            meal_threshold: 1,
            starvation_threshold: 3,
            starvation_damage: 4,
            max_health: 100,
            carrying_capacity: 10,
            max_work_hunger: 10,
        },
        travel: Some(Travel {
            routes: vec![Route {
                source: 1,
                destination: 2,
                ticks: 1,
            }],
            journeys: vec![Journey {
                person: 2,
                source: 1,
                destination: 2,
                remaining: 1,
                progress_millis: 0,
                pending_arrival: false,
            }],
            ..Default::default()
        }),
        nutrition: Some(game_nutrition::Nutrition {
            rules: game_nutrition::Rules {
                interval: 1,
                capacity: [10; 6],
                initial: [2; 6],
                metabolism: [1; 6],
                minimum: [0; 6],
                deficiency_damage: 0,
                scalar_meal: [0; 6],
            },
            people: vec![
                game_nutrition::Person {
                    person: 1,
                    reserves: [2; 6],
                },
                game_nutrition::Person {
                    person: 2,
                    reserves: [2; 6],
                },
            ],
        }),
        pending_food: vec![game_production_model::FoodEffect {
            person: 2,
            at: 1,
            health: 1,
            nutrients: [1; 6],
        }],
    }
}

#[test]
#[ignore = "manual partial-interval transfer scaling benchmark"]
fn interval_shared_transfer_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    assert!(count > 0);
    let mut initial = fixture();
    let autonomous = std::env::var_os("LEGACY_AUTO_MEALS").is_some();
    let eat = autonomous || std::env::var_os("LEGACY_MEALS").is_some();
    if eat {
        initial.physical.goods[0].food = Some(game_production_model::FoodProfile {
            portion: 1,
            hunger_relief: 1,
            nutrients: Default::default(),
            health_effect: 0,
            shelf_days: None,
        });
    }
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = person.clone();
            p.id = id;
            p.seek_meals = autonomous;
            p
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count,
        vec![Lot {
            item: None,
            good: 1,
            quantity: count,
            expires_at: None,
        }],
    )
    .unwrap();
    initial
        .physical
        .custody
        .containers
        .extend((1..=count).map(|id| Container::new(Place::Person(id), 1, vec![]).unwrap()));
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let actions: Vec<_> = (1..=count)
        .map(|id| game_lots::Transaction {
            id,
            movements: vec![game_lots::Movement {
                id,
                from: Place::Stock { region: 1, good: 1 },
                to: Some(Place::Person(id)),
                good: 1,
                quantity: 1,
                item: None,
            }],
            creations: vec![],
        })
        .collect();
    let meals: Vec<_> = (1..=count)
        .map(|id| game_lot_diet::Meal {
            action: id,
            person: id,
            good: 1,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            let mut meal_samples = Vec::new();
            for round in 0..4 {
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let start = std::time::Instant::now();
                let outcomes = core.transfer_goods(0, 300, &actions).unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    outcomes
                        .transactions
                        .iter()
                        .filter(|row| row.accepted)
                        .count(),
                    count as usize
                );
                if eat {
                    let start = std::time::Instant::now();
                    let outcomes = if autonomous {
                        let (chosen, draft) = core.prepare_autonomous_meals(&[]).unwrap().unwrap();
                        assert_eq!(chosen.len(), count as usize);
                        core.commit_meals(draft).unwrap()
                    } else {
                        core.eat(0, 300, &meals).unwrap()
                    };
                    let elapsed = start.elapsed().as_secs_f64();
                    assert_eq!(
                        outcomes
                            .transactions
                            .iter()
                            .filter(|row| row.accepted)
                            .count(),
                        count as usize
                    );
                    if round > 0 {
                        meal_samples.push(elapsed);
                    }
                }
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&core.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} transfers={count} elapsed={:.6}",
                samples[1]
            );
            if eat {
                meal_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} meals={count} autonomous={autonomous} elapsed={:.6}",
                    meal_samples[1]
                );
            }
            #[cfg(feature = "profile")]
            if std::env::var_os("LEGACY_PROFILE").is_some() {
                for (name, sample) in game_diagnostics::snapshot() {
                    if name.starts_with("core.meal.") || name.starts_with("physical.") {
                        println!(
                            "{name}: {:.3} ms/call",
                            sample.nanoseconds as f64 / sample.calls as f64 / 1_000_000.0
                        );
                    }
                }
            }
        });
}

#[test]
#[ignore = "manual identified meal owner scaling benchmark"]
fn interval_identified_meals_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    assert!(count > 0);
    let mut initial = fixture();
    initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
        portion: 1,
        hunger_relief: 1,
        nutrients: Default::default(),
        health_effect: 0,
        shelf_days: None,
    });
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut row = person.clone();
            row.id = id;
            row
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial
        .physical
        .custody
        .containers
        .extend((1..=count).map(|id| {
            Container::new(
                Place::Person(id),
                1,
                vec![Lot {
                    item: Some(id),
                    good: 2,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap()
        }));
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let owner = game_economy_work::tool_wear::ToolWear {
        next_item: count + 1,
        rules: vec![game_economy_work::tool_wear::Rule {
            good: 2,
            durability: 10,
            wear: 1,
            repair_good: 1,
            repair_quantity: 1,
        }],
        conditions: (1..=count)
            .map(|item| game_economy_work::tool_wear::Condition {
                item,
                good: 2,
                damage: 0,
            })
            .collect(),
        repairs: vec![],
        events: vec![],
    };
    let meals: Vec<_> = (1..=count)
        .map(|id| game_lot_diet::Meal {
            action: id,
            person: id,
            good: 2,
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let start = std::time::Instant::now();
                let draft = core.prepare_meals(0, 300, &meals).unwrap().unwrap();
                let next_owner = owner
                    .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
                    .unwrap();
                core.commit_meals(draft).unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert!(next_owner.conditions.is_empty());
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&(core.snapshot(), &next_owner))
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} identified_meals={count} elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore = "manual saved core host work scaling benchmark"]
fn core_host_autonomous_work_benchmark() {
    use std::hash::{Hash, Hasher};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let scheduled = std::env::var_os("LEGACY_SCHEDULED").is_some();
    let continuous = std::env::var_os("LEGACY_CONTINUOUS").is_some();
    assert!(!(scheduled && continuous));
    assert!(count > 1);
    let mut initial = fixture();
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut row = person.clone();
            row.id = id;
            row.willing_to_work = true;
            row
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count * 2,
        vec![Lot {
            item: None,
            good: 1,
            quantity: (count - 1) * 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let snapshot = CoreHostSnapshot {
        core: initial,
        tool_wear: None,
        schedule: None,
        autowork: Some(game_autowork::Autowork {
            next_id: 2,
            policies: (2..=count)
                .map(|person| game_autowork::Policy {
                    person,
                    recipe: 1,
                    enabled: true,
                    disregard_claims: false,
                })
                .collect(),
            requests: None,
        }),
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut host = CoreHost::begin(snapshot.clone(), &[(1, 1)]).unwrap();
                host.progress(400).unwrap();
                if scheduled || continuous {
                    host.enable_schedule(200, 1).unwrap();
                }
                let start = std::time::Instant::now();
                let outcomes = if continuous {
                    let run = host.advance_continuous(1500, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    assert_eq!((run.elapsed_millis, run.decisions, run.boundaries), (1500, 8, 1));
                    run.events.into_iter().find_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::Starts(rows) => Some(rows),
                        _ => None,
                    }).unwrap()
                } else if scheduled {
                    let run = host.advance_scheduled(500, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    assert_eq!((run.elapsed_millis, run.decisions), (500, 3));
                    run.events
                        .into_iter()
                        .find_map(|event| match event {
                            game_economy::core_interval::CoreRunEvent::Starts(rows) => Some(rows),
                            _ => None,
                        })
                        .unwrap()
                } else {
                    host.start_autonomous_work(&[], 1).unwrap()
                };
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    outcomes.iter().filter(|row| row.1).count(),
                    (count - 1) as usize
                );
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&host.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} host_autonomous_starts={} scheduled={scheduled} continuous={continuous} elapsed={:.6}",
                count - 1,
                samples[1]
            );
        });
}

#[test]
fn interval_abandonment_stops_progress_and_returns_real_inputs_at_boundary() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let before = core.snapshot();
                assert!(core.abandon_work(0, 299, &[(1, 1)]).is_err());
                assert!(core.abandon_work(0, 300, &[(1, 2)]).is_err());
                assert!(core.abandon_work(0, 300, &[(1, 1), (1, 1)]).is_err());
                assert_eq!(core.snapshot(), before);
                core.abandon_work(0, 300, &[(1, 1)]).unwrap();
                let stopped = core.snapshot();
                assert_eq!(
                    stopped.physical.work[0].phase,
                    game_lot_runtime::Phase::Returning
                );
                assert!(
                    stopped
                        .physical
                        .work_interval
                        .as_ref()
                        .unwrap()
                        .rates
                        .is_empty()
                );
                let input = stopped
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::JobInput(1))
                    .unwrap();
                assert_eq!(input.lots[0].quantity, 2);
                let saved = serde_json::to_string(&stopped).unwrap();
                let mut core = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                core.progress(700).unwrap();
                assert_eq!(
                    core.snapshot().physical.work[0].job.remaining,
                    stopped.physical.work[0].job.remaining
                );
                let events = core.settle().unwrap();
                assert_eq!(events.returned, vec![1]);
                assert!(events.finished.is_empty());
                let result = core.snapshot();
                let stock = result
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                assert_eq!(stock.lots[0].quantity, 2);
                (result, events)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn abandoning_interval_work_frees_the_worker_without_freeing_reserved_goods() {
    let mut initial = fixture();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
    core.progress(300).unwrap();
    core.abandon_work(0, 300, &[(1, 1)]).unwrap();
    let start = Start {
        id: 2,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert_eq!(
        core.start_work(0, 300, &[(start, 1)]).unwrap(),
        vec![(2, true)]
    );
    let snapshot = core.snapshot();
    assert_eq!(
        snapshot.physical.work[0].phase,
        game_lot_runtime::Phase::Returning
    );
    assert_eq!(
        snapshot.physical.work[1].phase,
        game_lot_runtime::Phase::Working
    );
    assert_eq!(snapshot.physical.work_interval.unwrap().rates, vec![(2, 1)]);
    assert_eq!(
        snapshot
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::JobInput(1))
            .unwrap()
            .used,
        2
    );
    assert_eq!(
        snapshot
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::JobInput(2))
            .unwrap()
            .used,
        2
    );
}

#[test]
fn local_interval_transfer_moves_real_custody_and_preserves_inflight_work() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[1].region = Some(1);
                initial.travel = None;
                initial.physical.custody.containers.extend([
                    Container::new(
                        Place::Person(1),
                        5,
                        vec![Lot {
                            item: None,
                            good: 1,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                    Container::new(Place::Person(2), 5, vec![]).unwrap(),
                ]);
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let action = game_lots::Transaction {
                    id: 1,
                    movements: vec![game_lots::Movement {
                        id: 1,
                        from: Place::Person(1),
                        to: Some(Place::Person(2)),
                        good: 1,
                        quantity: 1,
                        item: None,
                    }],
                    creations: vec![],
                };
                let before = core.snapshot();
                assert!(
                    core.transfer_goods(0, 299, std::slice::from_ref(&action))
                        .is_err()
                );
                let mut invalid = action.clone();
                invalid.movements[0].to = Some(Place::Stock { region: 2, good: 1 });
                assert!(core.transfer_goods(0, 300, &[invalid]).is_err());
                let mut invalid = action.clone();
                invalid.movements[0].to = None;
                assert!(core.transfer_goods(0, 300, &[invalid]).is_err());
                assert_eq!(core.snapshot(), before);
                assert!(
                    core.transfer_goods(0, 300, std::slice::from_ref(&action))
                        .unwrap()
                        .transactions[0]
                        .accepted
                );
                assert!(!core.transfer_goods(0, 300, &[action]).unwrap().transactions[0].accepted);
                let saved = core.snapshot();
                assert_eq!(
                    saved.physical.work_interval.as_ref().unwrap().rates,
                    vec![(1, 1)]
                );
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(2))
                        .unwrap()
                        .used,
                    1
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                core.progress(700).unwrap();
                let events = core.settle().unwrap();
                assert_eq!(events.finished, vec![1]);
                (core.snapshot(), events)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn interval_meals_consume_real_food_and_apply_effects_at_the_right_time() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: -3,
                    shelf_days: None,
                });
                initial.physical.custody.containers.extend([
                    Container::new(
                        Place::Person(1),
                        2,
                        vec![Lot {
                            item: None,
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                    Container::new(Place::Person(2), 2, vec![]).unwrap(),
                ]);
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                initial.nutrition = None;
                initial.pending_food.clear();
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(500).unwrap();
                let meals = [
                    game_lot_diet::Meal {
                        action: 1,
                        person: 1,
                        good: 2,
                    },
                    game_lot_diet::Meal {
                        action: 2,
                        person: 2,
                        good: 2,
                    },
                ];
                let before = core.snapshot();
                assert!(core.eat(0, 499, &meals).is_err());
                assert!(
                    core.eat(
                        0,
                        500,
                        &[game_lot_diet::Meal {
                            action: 3,
                            person: 1,
                            good: 1
                        }]
                    )
                    .is_err()
                );
                assert_eq!(core.snapshot(), before);
                let outcomes = core.eat(0, 500, &meals).unwrap();
                assert!(outcomes.transactions[0].accepted);
                assert!(!outcomes.transactions[1].accepted);
                let saved = core.snapshot();
                assert_eq!(
                    (saved.people[0].hunger, saved.people[0].hunger_remainder),
                    (1, 500)
                );
                assert_eq!(saved.people[0].health, 100);
                assert_eq!(saved.pending_food.len(), 1);
                assert_eq!(saved.pending_food[0].at, 1);
                assert_eq!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Person(1))
                        .unwrap()
                        .used,
                    0
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                core.progress(500).unwrap();
                core.settle().unwrap();
                let result = core.snapshot();
                assert_eq!(result.people[0].hunger, 2);
                assert_eq!(result.people[0].health, 97);
                assert!(result.pending_food.is_empty());
                result
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn identified_meals_stage_item_removal_with_hunger_and_later_production() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[0].seek_meals = true;
                initial.rules.meal_threshold = 2;
                initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: 0,
                    shelf_days: None,
                });
                initial.physical.custody.containers.push(
                    Container::new(
                        Place::Person(1),
                        2,
                        vec![Lot {
                            item: Some(10),
                            good: 2,
                            quantity: 1,
                            expires_at: None,
                        }],
                    )
                    .unwrap(),
                );
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let mut owner = game_economy_work::tool_wear::ToolWear {
                    next_item: 11,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 10,
                        wear: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![game_economy_work::tool_wear::Condition {
                        item: 10,
                        good: 2,
                        damage: 0,
                    }],
                    repairs: vec![],
                    events: vec![],
                };
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(400).unwrap();
                let meal = [game_lot_diet::Meal {
                    action: 1,
                    person: 1,
                    good: 2,
                }];
                let before = core.snapshot();
                assert!(core.eat(0, 400, &meal).is_err());
                assert_eq!(core.snapshot(), before);
                let stale = core.prepare_meals(0, 400, &meal).unwrap().unwrap();
                let (chosen, draft) = core.prepare_autonomous_meals(&[]).unwrap().unwrap();
                assert_eq!(
                    chosen.iter().map(|meal| meal.person).collect::<Vec<_>>(),
                    vec![1]
                );
                assert_eq!(draft.item_updates(), &[(10, None)]);
                let next_owner = owner
                    .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
                    .unwrap();
                core.validate_meals(&draft).unwrap();
                assert!(core.commit_meals(draft).unwrap().transactions[0].accepted);
                owner = next_owner;
                assert!(owner.conditions.is_empty());
                let after = core.snapshot();
                assert_eq!(after.people[0].hunger, 1);
                assert!(core.commit_meals(stale).is_err());
                assert_eq!(core.snapshot(), after);
                let saved = serde_json::to_string(&(after, owner)).unwrap();
                let (snapshot, mut owner): (CoreSnapshot, game_economy_work::tool_wear::ToolWear) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                core.progress(600).unwrap();
                let draft = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap();
                let next_owner = owner
                    .stage_identified_outputs(
                        draft.produced_items(),
                        draft.next_produced_item().unwrap(),
                        draft.item_updates(),
                    )
                    .unwrap();
                core.commit_settlement(draft).unwrap();
                owner = next_owner;
                let result = core.snapshot();
                let physical = Runtime::compile(result.physical.clone()).unwrap();
                assert_eq!(physical.item_place(10), None);
                assert_eq!(physical.item_place(11), Some(Place::JobOutput(1)));
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                (result, owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn autonomous_interval_meals_follow_personal_preferences_and_occupancy() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.rules.meal_threshold = 2;
                initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
                    portion: 2,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: 0,
                    shelf_days: None,
                });
                let mut people: Vec<_> = (1..=5)
                    .map(|id| {
                        let mut p = initial.people[0].clone();
                        p.id = id;
                        p.seek_meals = true;
                        p
                    })
                    .collect();
                people[1].controlled = true;
                people[2].seek_meals = false;
                people[3].hunger = 0;
                initial.people = people;
                initial
                    .physical
                    .custody
                    .containers
                    .extend((1..=5).map(|id| {
                        Container::new(
                            Place::Person(id),
                            2,
                            vec![
                                Lot {
                                    item: None,
                                    good: 2,
                                    quantity: 1,
                                    expires_at: Some(1),
                                },
                                Lot {
                                    item: None,
                                    good: 2,
                                    quantity: 1,
                                    expires_at: Some(3),
                                },
                            ],
                        )
                        .unwrap()
                    }));
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(400).unwrap();
                let before = core.snapshot();
                assert!(core.prepare_autonomous_meals(&[2, 1]).is_err());
                assert_eq!(core.snapshot(), before);
                let (meals, draft) = core.prepare_autonomous_meals(&[1]).unwrap().unwrap();
                assert_eq!(
                    meals
                        .iter()
                        .map(|meal| (meal.person, meal.good))
                        .collect::<Vec<_>>(),
                    vec![(5, 2)]
                );
                assert!(draft.item_updates().is_empty());
                assert!(core.commit_meals(draft).unwrap().transactions[0].accepted);
                let after = core.snapshot();
                assert_eq!(after.people[4].hunger, 1);
                assert_eq!(after.people[0].hunger, 2);
                assert!(core.prepare_autonomous_meals(&[1]).unwrap().is_none());
                let saved = serde_json::to_string(&after).unwrap();
                let resumed = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(resumed.prepare_autonomous_meals(&[1]).unwrap().is_none());
                resumed.snapshot()
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn autonomous_interval_work_respects_personal_choices_and_real_stock() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.people = (1..=7)
                    .map(|id| {
                        let mut person = initial.people[0].clone();
                        person.id = id;
                        person.willing_to_work = true;
                        person
                    })
                    .collect();
                initial.people[2].willing_to_work = false;
                initial.people[3].controlled = true;
                initial.people[5].hunger = 11;
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut owner = game_autowork::Autowork {
                    next_id: 2,
                    policies: (2..=7)
                        .map(|person| game_autowork::Policy {
                            person,
                            recipe: 1,
                            enabled: person != 5,
                            disregard_claims: false,
                        })
                        .collect(),
                    requests: None,
                };
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress(300).unwrap();
                let before = core.snapshot();
                let owner_before = owner.clone();
                assert!(
                    core.start_autonomous_work(0, 299, &mut owner, &[], 2)
                        .is_err()
                );
                assert!(
                    core.start_autonomous_work(0, 300, &mut owner, &[7, 2], 2)
                        .is_err()
                );
                assert_eq!(core.snapshot(), before);
                assert_eq!(owner, owner_before);
                let mut invalid = owner.clone();
                invalid.policies.push(invalid.policies[0].clone());
                let invalid_before = invalid.clone();
                assert!(
                    core.start_autonomous_work(0, 300, &mut invalid, &[], 2)
                        .is_err()
                );
                assert_eq!(invalid, invalid_before);
                assert_eq!(core.snapshot(), before);
                assert_eq!(
                    core.start_autonomous_work(0, 300, &mut owner, &[], 2)
                        .unwrap(),
                    vec![(2, true)]
                );
                assert_eq!(owner.next_id, 3);
                let after = core.snapshot();
                assert_eq!(
                    after
                        .physical
                        .work
                        .iter()
                        .map(|row| row.job.worker)
                        .collect::<Vec<_>>(),
                    vec![1, 2]
                );
                assert_eq!(
                    after
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                        .unwrap()
                        .used,
                    0
                );
                let saved = serde_json::to_string(&(after, owner)).unwrap();
                let (snapshot, owner): (CoreSnapshot, game_autowork::Autowork) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                core.progress(500).unwrap();
                assert_eq!(
                    core.publish_early_work().unwrap().unwrap().finished,
                    vec![2]
                );
                (core.snapshot(), owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn opening_offset_actions_do_not_receive_prior_progress() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
    let start = Start {
        id: 2,
        worker: 2,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert_eq!(
        core.start_work(0, 0, &[(start, 2)]).unwrap(),
        vec![(2, true)]
    );
    let snapshot = core.snapshot();
    assert_eq!(snapshot.physical.work[1].job.remaining, 1);
    assert_eq!(snapshot.physical.work[1].progress_remainder, 0);
    core.progress(250).unwrap();
    assert_eq!(core.snapshot().physical.work[1].job.remaining, 1);
    assert_eq!(core.snapshot().physical.work[1].progress_remainder, 500);
}

#[test]
fn saved_core_host_coordinates_personal_work_meals_and_tool_ownership() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.people[0].willing_to_work = true;
                initial.people[0].seek_meals = true;
                initial.people[1].region = Some(1);
                initial.people[1].willing_to_work = true;
                initial.people[1].seek_meals = true;
                initial.travel = None;
                initial.nutrition = None;
                initial.pending_food.clear();
                initial.rules.meal_threshold = 2;
                initial.physical.goods[0].food = Some(game_production_model::FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: Default::default(),
                    health_effect: 0,
                    shelf_days: None,
                });
                let stock = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                    .unwrap();
                *stock = Container::new(
                    stock.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                initial
                    .physical
                    .custody
                    .containers
                    .extend((1..=2).map(|id| {
                        Container::new(
                            Place::Person(id),
                            2,
                            vec![Lot {
                                item: None,
                                good: 1,
                                quantity: 1,
                                expires_at: None,
                            }],
                        )
                        .unwrap()
                    }));
                initial
                    .physical
                    .custody
                    .containers
                    .sort_by_key(|row| row.place);
                let snapshot = CoreHostSnapshot {
                    core: initial,
                    schedule: None,
                    autowork: Some(game_autowork::Autowork {
                        next_id: 2,
                        policies: vec![game_autowork::Policy {
                            person: 2,
                            recipe: 1,
                            enabled: true,
                            disregard_claims: false,
                        }],
                        requests: None,
                    }),
                    tool_wear: Some(game_economy_work::tool_wear::ToolWear {
                        next_item: 10,
                        rules: vec![game_economy_work::tool_wear::Rule {
                            good: 2,
                            durability: 10,
                            wear: 1,
                            repair_good: 1,
                            repair_quantity: 1,
                        }],
                        conditions: vec![],
                        repairs: vec![],
                        events: vec![],
                    }),
                };
                let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
                host.progress(300).unwrap();
                assert_eq!(
                    host.eat_autonomously(&[])
                        .unwrap()
                        .iter()
                        .map(|meal| meal.person)
                        .collect::<Vec<_>>(),
                    vec![2]
                );
                assert!(host.start_autonomous_work(&[2], 2).unwrap().is_empty());
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                host.progress(1).unwrap();
                assert_eq!(host.start_autonomous_work(&[], 2).unwrap(), vec![(2, true)]);
                host.progress(500).unwrap();
                assert_eq!(
                    host.publish_early_work().unwrap().unwrap().finished,
                    vec![2]
                );
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(
                    host.snapshot().tool_wear.as_ref().unwrap().conditions[0].item,
                    10
                );
                host.progress(199).unwrap();
                let events = host.settle().unwrap();
                assert_eq!(events.finished, vec![1]);
                assert_eq!(events.delivered, vec![2]);
                let result = host.snapshot();
                let physical = Runtime::compile(result.core.physical.clone()).unwrap();
                assert_eq!(
                    physical.item_place(10),
                    Some(Place::Stock { region: 1, good: 2 })
                );
                assert_eq!(physical.item_place(11), Some(Place::JobOutput(1)));
                let owner = result.tool_wear.as_ref().unwrap();
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                assert_eq!(result.autowork.as_ref().unwrap().next_id, 3);
                result
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn core_host_rejects_unowned_identified_consumption_without_mutation() {
    let mut initial = fixture();
    let input = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(
        input.place,
        2,
        vec![
            Lot {
                item: Some(10),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
            Lot {
                item: Some(11),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
        ],
    )
    .unwrap();
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            schedule: None,
        },
        &[(1, 2)],
    )
    .unwrap();
    host.progress(500).unwrap();
    let before = host.snapshot();
    assert!(host.publish_early_work().is_err());
    assert_eq!(host.snapshot(), before);
}

#[test]
fn scheduled_core_host_is_partition_independent_at_tied_deadlines() {
    let run = |workers, chunks: &[u64], reload| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[0].willing_to_work = true;
        initial.people[1].willing_to_work = true;
        initial.travel.as_mut().unwrap().journeys[0].progress_millis = 500;
        let stock = initial.physical.custody.containers.iter_mut().find(|row| row.place == Place::Stock { region: 2, good: 1 }).unwrap();
        *stock = Container::new(stock.place, 10, vec![Lot { item: None, good: 1, quantity: 2, expires_at: None }]).unwrap();
        let snapshot = CoreHostSnapshot {
            core: initial,
            autowork: Some(game_autowork::Autowork { next_id: 2, policies: vec![game_autowork::Policy { person: 2, recipe: 1, enabled: true, disregard_claims: false }], requests: None }),
            tool_wear: None, schedule: None,
        };
        let mut host = CoreHost::begin(snapshot, &[(1, 2)]).unwrap();
        host.progress(100).unwrap();
        host.enable_schedule(200, 2).unwrap();
        let mut events = Vec::new();
        let mut decisions = 0;
        for &chunk in chunks {
            let result = host.advance_scheduled(chunk, &[]).unwrap();
            assert_eq!(result.elapsed_millis, chunk);
            assert_eq!(result.failure, None);
            decisions += result.decisions;
            events.extend(result.events);
            if reload {
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            }
        }
        assert_eq!(decisions, 5);
        assert!(matches!(&events[0], game_economy::core_interval::CoreRunEvent::Arrivals(arrival) if arrival.people == vec![2]));
        assert!(matches!(&events[1], game_economy::core_interval::CoreRunEvent::Work(work) if work.finished == vec![1]));
        assert!(matches!(&events[2], game_economy::core_interval::CoreRunEvent::Starts(starts) if starts == &vec![(2, true)]));
        let boundary = host.snapshot();
        assert_eq!(boundary.core.physical.work_interval.as_ref().unwrap().elapsed_millis, 1000);
        let events_at_boundary = host.settle().unwrap();
        assert_eq!(events_at_boundary.finished, vec![2]);
        assert_eq!(events_at_boundary.delivered, vec![1]);
        (host.snapshot(), events)
    })
    };
    assert_eq!(run(1, &[900], false), run(6, &[200, 200, 500], true));
}

#[test]
fn scheduled_decision_resumes_after_work_failure_without_repeating_meal() {
    let mut initial = fixture();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial.people[1].region = Some(1);
    initial.people[1].seek_meals = true;
    initial.people[1].willing_to_work = true;
    let mut third = initial.people[0].clone();
    third.id = 3;
    third.willing_to_work = true;
    initial.people.push(third);
    initial.physical.goods[1].food = Some(game_production_model::FoodProfile {
        portion: 1,
        hunger_relief: 1,
        nutrients: Default::default(),
        health_effect: 0,
        shelf_days: None,
    });
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    initial.physical.custody.containers.push(
        Container::new(
            Place::Person(2),
            2,
            vec![Lot {
                item: None,
                good: 2,
                quantity: 2,
                expires_at: None,
            }],
        )
        .unwrap(),
    );
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let snapshot = CoreHostSnapshot {
        core: initial,
        tool_wear: None,
        schedule: None,
        autowork: Some(game_autowork::Autowork {
            next_id: u64::MAX,
            policies: vec![game_autowork::Policy {
                person: 3,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            }],
            requests: None,
        }),
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.progress(100).unwrap();
    host.enable_schedule(200, 1).unwrap();
    let result = host.advance_scheduled(0, &[]).unwrap();
    assert_eq!(result.elapsed_millis, 0);
    assert!(result.failure.is_some());
    assert!(
        matches!(&result.events[..], [game_economy::core_interval::CoreRunEvent::Meals(meals)] if meals == &vec![(2, 2)])
    );
    let mut saved = host.snapshot();
    assert!(matches!(
        saved.schedule.as_ref().unwrap().phase,
        game_economy::core_interval::DecisionPhase::Work { .. }
    ));
    saved.autowork.as_mut().unwrap().next_id = 2;
    let saved = serde_json::to_string(&saved).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    let result = host.advance_scheduled(0, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert_eq!(result.decisions, 1);
    assert!(
        matches!(&result.events[..], [game_economy::core_interval::CoreRunEvent::Starts(starts)] if starts == &vec![(2, true)])
    );
    let snapshot = host.snapshot();
    assert_eq!(
        snapshot
            .core
            .physical
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
    assert_eq!(snapshot.core.people[1].hunger, 1);
    assert_eq!(snapshot.core.physical.work[1].job.worker, 3);
}

#[test]
fn scheduled_runner_defers_perishable_work_and_publishes_other_completion() {
    let mut initial = fixture();
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    initial.physical.goods.push(Good {
        id: 3,
        name: "perishable".into(),
        food: Some(game_production_model::FoodProfile {
            portion: 1,
            hunger_relief: 1,
            nutrients: Default::default(),
            health_effect: 0,
            shelf_days: Some(1),
        }),
    });
    let mut recipe = initial.physical.recipes[0].clone();
    recipe.id = 2;
    recipe.outputs[0].good = 3;
    initial.physical.recipes.push(recipe);
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    initial
        .physical
        .custody
        .containers
        .push(Container::new(Place::Stock { region: 1, good: 3 }, 10, vec![]).unwrap());
    initial
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let mut physical = Runtime::compile(initial.physical).unwrap();
    physical
        .start(
            &[Start {
                id: 2,
                worker: 2,
                region: 1,
                recipe: 2,
                units: 1,
            }],
            7,
        )
        .unwrap();
    initial.physical = physical.snapshot();
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            schedule: None,
        },
        &[(1, 2), (2, 2)],
    )
    .unwrap();
    host.progress(100).unwrap();
    host.enable_schedule(600, 1).unwrap();
    let result = host.advance_scheduled(900, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert!(result.boundary);
    assert!(
        matches!(&result.events[0], game_economy::core_interval::CoreRunEvent::Deferred(jobs) if jobs == &vec![2])
    );
    assert!(
        matches!(&result.events[1], game_economy::core_interval::CoreRunEvent::Work(events) if events.finished == vec![1])
    );
    assert_eq!(host.settle().unwrap().finished, vec![2]);
}

#[test]
fn continuous_core_host_crosses_boundaries_without_partition_or_reload_drift() {
    let run = |workers, chunks: &[u64], reload| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = fixture();
        initial.people[0].willing_to_work = true;
        initial.people[1].region = Some(1);
        initial.people[1].willing_to_work = true;
        initial.travel = None;
        initial.nutrition = None;
        initial.pending_food.clear();
        let stock = initial.physical.custody.containers.iter_mut().find(|row| row.place == Place::Stock { region: 1, good: 1 }).unwrap();
        *stock = Container::new(stock.place, 10, vec![Lot { item: None, good: 1, quantity: 2, expires_at: None }]).unwrap();
        let tool_wear = game_economy_work::tool_wear::ToolWear { next_item: 10,
            rules: vec![game_economy_work::tool_wear::Rule { good: 2, durability: 10, wear: 1, repair_good: 1, repair_quantity: 1 }],
            conditions: vec![], repairs: vec![], events: vec![] };
        let snapshot = CoreHostSnapshot { core: initial, tool_wear: Some(tool_wear), schedule: None,
            autowork: Some(game_autowork::Autowork { next_id: 2, policies: vec![game_autowork::Policy { person: 2, recipe: 1, enabled: true, disregard_claims: false }], requests: None }) };
        let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
        host.enable_schedule(500, 1).unwrap();
        let mut events = Vec::new();
        let mut boundaries = 0;
        let mut decisions = 0;
        for &chunk in chunks {
            let result = host.advance_continuous(chunk, &[]).unwrap();
            assert_eq!(result.failure, None);
            assert_eq!(result.elapsed_millis, chunk);
            boundaries += result.boundaries;
            decisions += result.decisions;
            events.extend(result.events);
            if reload {
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            }
        }
        assert_eq!(boundaries, 2);
        assert_eq!(decisions, 5);
        let snapshot = host.snapshot();
        assert_eq!(snapshot.core.physical.custody.tick, 2);
        assert_eq!(snapshot.core.physical.work_interval.as_ref().unwrap().elapsed_millis, 400);
        let physical = Runtime::compile(snapshot.core.physical.clone()).unwrap();
        assert_eq!(physical.item_place(10), Some(Place::Stock { region: 1, good: 2 }));
        assert_eq!(physical.item_place(11), Some(Place::Stock { region: 1, good: 2 }));
        snapshot.tool_wear.as_ref().unwrap().validate(&physical, &[1,2].into_iter().collect(), &[1,2].into_iter().collect(), 2).unwrap();
        assert!(matches!(&events[0], game_economy::core_interval::CoreRunEvent::Starts(rows) if rows == &vec![(2, true)]));
        (snapshot, events)
    })
    };
    assert_eq!(
        run(1, &[2400], false),
        run(6, &[400, 600, 0, 300, 700, 400], true)
    );
}

#[test]
fn long_decision_period_survives_save_and_crosses_a_boundary() {
    let snapshot = CoreHostSnapshot {
        core: fixture(),
        autowork: None,
        tool_wear: None,
        schedule: None,
    };
    let mut host = CoreHost::begin(snapshot, &[(1, 1)]).unwrap();
    host.progress(100).unwrap();
    host.enable_schedule(u64::MAX, 1).unwrap();
    assert_eq!(host.advance_continuous(0, &[]).unwrap().decisions, 1);
    let before = host.snapshot();
    assert!(
        before
            .schedule
            .as_ref()
            .unwrap()
            .next_decision_millis
            .unwrap()
            > u128::from(u64::MAX)
    );
    let saved = serde_json::to_string(&before).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(host.snapshot(), before);
    let result = host.advance_continuous(1000, &[]).unwrap();
    assert_eq!(result.failure, None);
    assert_eq!(result.boundaries, 1);
    assert_eq!(result.decisions, 0);
}

#[test]
fn continuous_host_carries_long_work_rate_and_fractional_progress() {
    let run = |chunks: &[u64], reload| {
        let mut initial = fixture();
        initial.physical.recipes[0].ticks = 3;
        initial.physical.work[0].job.remaining = 3;
        let mut host = CoreHost::begin(
            CoreHostSnapshot {
                core: initial,
                autowork: None,
                tool_wear: None,
                schedule: None,
            },
            &[(1, 1)],
        )
        .unwrap();
        host.enable_schedule(10_000, 1).unwrap();
        let mut events = Vec::new();
        for &chunk in chunks {
            let run = host.advance_continuous(chunk, &[]).unwrap();
            assert_eq!(run.failure, None);
            events.extend(run.events);
            if reload {
                let saved = serde_json::to_string(&host.snapshot()).unwrap();
                host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
            }
        }
        let snapshot = host.snapshot();
        assert_eq!(snapshot.core.physical.custody.tick, 3);
        assert_eq!(
            snapshot
                .core
                .physical
                .work_interval
                .as_ref()
                .unwrap()
                .elapsed_millis,
            0
        );
        assert!(events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Boundary(events) if events.finished == vec![1])));
        (snapshot, events)
    };
    assert_eq!(run(&[3000], false), run(&[333, 667, 1300, 700], true));
}

#[test]
fn continuous_host_retains_rate_for_work_paused_at_a_boundary() {
    let mut initial = fixture();
    initial.physical.recipes[0].ticks = 3;
    initial.physical.work[0].job.remaining = 3;
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    host.enable_schedule(10_000, 1).unwrap();
    assert_eq!(host.advance_continuous(1000, &[1]).unwrap().failure, None);
    let paused = host.snapshot();
    assert_eq!(paused.core.physical.work[0].job.remaining, 2);
    assert!(
        paused
            .core
            .physical
            .work_interval
            .as_ref()
            .unwrap()
            .rates
            .is_empty()
    );
    assert_eq!(
        paused.schedule.as_ref().unwrap().carried_rates,
        vec![(1, 1)]
    );
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    let resumed = host.snapshot();
    assert_eq!(resumed.core.physical.work[0].job.remaining, 2);
    assert_eq!(
        resumed.core.physical.work_interval.as_ref().unwrap().rates,
        vec![(1, 1)]
    );
    assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
    assert_eq!(host.snapshot().core.physical.work[0].job.remaining, 1);
}

#[test]
fn continuous_host_keeps_completed_interval_after_boundary_failure() {
    let mut initial = fixture();
    initial.physical.custody.tick = u64::MAX;
    initial.people[1].region = Some(1);
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let mut host = CoreHost::begin(
        CoreHostSnapshot {
            core: initial,
            autowork: None,
            tool_wear: None,
            schedule: None,
        },
        &[(1, 1)],
    )
    .unwrap();
    host.enable_schedule(10_000, 1).unwrap();
    let run = host.advance_continuous(1000, &[]).unwrap();
    assert_eq!(run.elapsed_millis, 1000);
    assert!(run.boundary);
    assert!(run.failure.is_some());
    assert_eq!(run.boundaries, 0);
    let before = host.snapshot();
    assert_eq!(
        before
            .core
            .physical
            .work_interval
            .as_ref()
            .unwrap()
            .elapsed_millis,
        1000
    );
    let saved = serde_json::to_string(&before).unwrap();
    let mut host = CoreHost::resume(serde_json::from_str(&saved).unwrap()).unwrap();
    assert!(host.advance_continuous(0, &[]).unwrap().failure.is_some());
    assert_eq!(host.snapshot(), before);
}

#[test]
fn unsupported_completed_work_can_be_deferred_without_blocking_other_jobs() {
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut initial = fixture();
                    initial.people[1].region = Some(1);
                    initial.travel = None;
                    initial.nutrition = None;
                    initial.pending_food.clear();
                    initial.physical.goods.push(Good {
                        id: 3,
                        name: "perishable".into(),
                        food: Some(game_production_model::FoodProfile {
                            portion: 1,
                            hunger_relief: 1,
                            nutrients: Default::default(),
                            health_effect: 0,
                            shelf_days: Some(1),
                        }),
                    });
                    let mut recipe = initial.physical.recipes[0].clone();
                    recipe.id = 2;
                    recipe.outputs[0].good = 3;
                    initial.physical.recipes.push(recipe);
                    let stock = initial
                        .physical
                        .custody
                        .containers
                        .iter_mut()
                        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
                        .unwrap();
                    *stock = Container::new(
                        stock.place,
                        10,
                        vec![Lot {
                            item: None,
                            good: 1,
                            quantity: 2,
                            expires_at: None,
                        }],
                    )
                    .unwrap();
                    initial.physical.custody.containers.push(
                        Container::new(Place::Stock { region: 1, good: 3 }, 10, vec![]).unwrap(),
                    );
                    initial
                        .physical
                        .custody
                        .containers
                        .sort_by_key(|row| row.place);
                    let mut physical = Runtime::compile(initial.physical).unwrap();
                    assert!(
                        physical
                            .start(
                                &[Start {
                                    id: 2,
                                    worker: 2,
                                    region: 1,
                                    recipe: 2,
                                    units: 1
                                }],
                                7
                            )
                            .unwrap()[0]
                            .1
                    );
                    initial.physical = physical.snapshot();
                    let mut core = CoreInterval::begin(initial, &[(1, 2), (2, 2)]).unwrap();
                    core.progress(400).unwrap();
                    let before = core.snapshot();
                    assert!(core.defer_work(0, 400, &[2]).is_err());
                    assert_eq!(core.snapshot(), before);
                    core.progress(100).unwrap();
                    let due = core.snapshot();
                    assert!(core.publish_early_work().is_err());
                    assert!(core.defer_work(0, 500, &[2, 2]).is_err());
                    assert_eq!(core.snapshot(), due);
                    core.defer_work(0, 500, &[2]).unwrap();
                    let mut core = CoreInterval::resume(
                        serde_json::from_str(&serde_json::to_string(&core.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        core.publish_early_work().unwrap().unwrap().finished,
                        vec![1]
                    );
                    assert_eq!(core.next_deadline(None).unwrap().after_millis, 500);
                    assert!(
                        core.start_work(
                            0,
                            500,
                            &[(
                                Start {
                                    id: 3,
                                    worker: 2,
                                    region: 1,
                                    recipe: 1,
                                    units: 1
                                },
                                1
                            )]
                        )
                        .is_err()
                    );
                    core.progress(500).unwrap();
                    let events = core.settle().unwrap();
                    assert_eq!(events.finished, vec![2]);
                    let result = core.snapshot();
                    assert_eq!(
                        result
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|row| row.place == Place::JobOutput(2))
                            .unwrap()
                            .used,
                        1
                    );
                    result
                })
        };
    assert_eq!(run(1), run(6));
}

#[test]
fn identified_interval_settlement_stages_item_owner_and_person_effects_together() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 1)]).unwrap();
                let owner = game_economy_work::tool_wear::ToolWear {
                    next_item: 10,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 10,
                        wear: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![],
                    repairs: vec![],
                    events: vec![],
                };
                core.progress(1000).unwrap();
                let before = core.snapshot();
                let goods = [2];
                assert!(
                    core.prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 0
                    }))
                    .is_err()
                );
                assert_eq!(core.snapshot(), before);
                let draft = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 10,
                    }))
                    .unwrap();
                assert_eq!(draft.events().finished, vec![1]);
                assert_eq!(
                    draft.produced_items(),
                    &[game_lot_runtime::ProducedItem {
                        job: 1,
                        item: 10,
                        good: 2
                    }]
                );
                assert_eq!(draft.next_produced_item(), Some(11));
                assert_eq!(core.snapshot(), before);
                // A failed item-owner preparation discards the draft and changes no owner.
                assert!(
                    owner
                        .stage_identified_outputs(draft.produced_items(), 0, draft.item_updates())
                        .is_err()
                );
                assert!(owner.conditions.is_empty());
                drop(draft);
                let saved = serde_json::to_string(&(core.snapshot(), owner)).unwrap();
                let (snapshot, mut owner): (CoreSnapshot, game_economy_work::tool_wear::ToolWear) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                let stale = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 10,
                    }))
                    .unwrap();
                let draft = core
                    .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &goods,
                        next_item: 10,
                    }))
                    .unwrap();
                let next_owner = owner
                    .stage_identified_outputs(
                        draft.produced_items(),
                        draft.next_produced_item().unwrap(),
                        draft.item_updates(),
                    )
                    .unwrap();
                core.validate_settlement(&draft).unwrap();
                core.commit_settlement(draft).unwrap();
                owner = next_owner;
                assert_eq!(
                    owner.conditions,
                    vec![game_economy_work::tool_wear::Condition {
                        item: 10,
                        good: 2,
                        damage: 0
                    }]
                );
                let result = core.snapshot();
                assert_eq!(result.people[0].health, 96);
                let output = result
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::JobOutput(1))
                    .unwrap();
                assert_eq!(output.lots[0].item, Some(10));
                let physical = Runtime::compile(result.physical.clone()).unwrap();
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                let saved = serde_json::to_string(&(result.clone(), owner.clone())).unwrap();
                let (restored, restored_owner): (
                    CoreSnapshot,
                    game_economy_work::tool_wear::ToolWear,
                ) = serde_json::from_str(&saved).unwrap();
                assert_eq!(restored, result);
                assert_eq!(restored_owner, owner);
                assert!(core.commit_settlement(stale).is_err());
                assert_eq!(core.snapshot(), result);
                (result, owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn identified_early_work_stages_tool_owner_and_survives_later_delivery() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
                core.progress(500).unwrap();
                let owner = game_economy_work::tool_wear::ToolWear {
                    next_item: 10,
                    rules: vec![game_economy_work::tool_wear::Rule {
                        good: 2,
                        durability: 10,
                        wear: 1,
                        repair_good: 1,
                        repair_quantity: 1,
                    }],
                    conditions: vec![],
                    repairs: vec![],
                    events: vec![],
                };
                let before = core.snapshot();
                let draft = core
                    .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    draft.produced_items(),
                    &[game_lot_runtime::ProducedItem {
                        job: 1,
                        item: 10,
                        good: 2
                    }]
                );
                assert!(
                    owner
                        .stage_identified_outputs(draft.produced_items(), 0, draft.item_updates())
                        .is_err()
                );
                assert_eq!(core.snapshot(), before);
                drop(draft);
                let saved = serde_json::to_string(&(core.snapshot(), owner)).unwrap();
                let (snapshot, owner): (CoreSnapshot, game_economy_work::tool_wear::ToolWear) =
                    serde_json::from_str(&saved).unwrap();
                let mut core = CoreInterval::resume(snapshot).unwrap();
                let stale = core
                    .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap()
                    .unwrap();
                let draft = core
                    .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                        goods: &[2],
                        next_item: owner.next_item,
                    }))
                    .unwrap()
                    .unwrap();
                let owner = owner
                    .stage_identified_outputs(
                        draft.produced_items(),
                        draft.next_produced_item().unwrap(),
                        draft.item_updates(),
                    )
                    .unwrap();
                core.validate_early_work(&draft).unwrap();
                assert_eq!(core.commit_early_work(draft).unwrap().finished, vec![1]);
                assert!(core.commit_early_work(stale).is_err());
                let early = core.snapshot();
                assert_eq!(
                    early
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::JobOutput(1))
                        .unwrap()
                        .lots[0]
                        .item,
                    Some(10)
                );
                core.progress(500).unwrap();
                assert_eq!(core.settle().unwrap().delivered, vec![1]);
                let result = core.snapshot();
                let physical = Runtime::compile(result.physical.clone()).unwrap();
                assert_eq!(
                    physical.item_place(10),
                    Some(Place::Stock { region: 1, good: 2 })
                );
                owner
                    .validate(
                        &physical,
                        &[1, 2].into_iter().collect(),
                        &[1, 2].into_iter().collect(),
                        1,
                    )
                    .unwrap();
                (result, owner)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn early_work_consuming_identified_inputs_requires_owner_staging() {
    let mut initial = fixture();
    let input = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(
        input.place,
        2,
        vec![
            Lot {
                item: Some(10),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
            Lot {
                item: Some(11),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
        ],
    )
    .unwrap();
    let owner = game_economy_work::tool_wear::ToolWear {
        next_item: 12,
        rules: vec![game_economy_work::tool_wear::Rule {
            good: 1,
            durability: 10,
            wear: 1,
            repair_good: 2,
            repair_quantity: 1,
        }],
        conditions: vec![
            game_economy_work::tool_wear::Condition {
                item: 10,
                good: 1,
                damage: 0,
            },
            game_economy_work::tool_wear::Condition {
                item: 11,
                good: 1,
                damage: 0,
            },
        ],
        repairs: vec![],
        events: vec![],
    };
    let mut boundary = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
    boundary.progress(1000).unwrap();
    let before_boundary = boundary.snapshot();
    assert!(boundary.settle().is_err());
    assert_eq!(boundary.snapshot(), before_boundary);
    let draft = boundary.prepare_settlement(None).unwrap();
    let boundary_owner = owner
        .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
        .unwrap();
    boundary.commit_settlement(draft).unwrap();
    assert!(boundary_owner.conditions.is_empty());
    let mut core = CoreInterval::begin(initial, &[(1, 2)]).unwrap();
    core.progress(500).unwrap();
    let before = core.snapshot();
    assert!(core.publish_early_work().is_err());
    assert_eq!(core.snapshot(), before);
    let draft = core.prepare_early_work(None).unwrap().unwrap();
    assert_eq!(draft.item_updates(), &[(10, None), (11, None)]);
    let owner = owner
        .stage_identified_outputs(&[], owner.next_item, draft.item_updates())
        .unwrap();
    core.commit_early_work(draft).unwrap();
    assert!(owner.conditions.is_empty());
    let physical = Runtime::compile(core.snapshot().physical).unwrap();
    assert_eq!(physical.item_place(10), None);
    assert_eq!(physical.item_place(11), None);
}

#[test]
fn composed_core_interval_is_chunk_independent_and_resumable() {
    let initial = fixture();
    let run = |workers, chunks: &[u64], reload| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 2)]).unwrap();
                let opening = core.snapshot();
                assert_eq!(core.progress(0).unwrap(), 0);
                assert!(core.settle().is_err());
                assert_eq!(core.snapshot(), opening);
                for (index, &chunk) in chunks.iter().enumerate() {
                    assert_eq!(core.progress(chunk).unwrap(), chunk);
                    if index == 0 && reload {
                        let saved = serde_json::to_string(&core.snapshot()).unwrap();
                        core = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                    }
                }
                let pending = core.snapshot();
                assert_eq!(pending.people[1].region, None);
                assert!(pending.travel.as_ref().unwrap().journeys[0].pending_arrival);
                assert_eq!(pending.physical.work[0].job.remaining, 0);
                assert_eq!(core.progress(1).unwrap(), 0);
                let events = core.settle().unwrap();
                assert!(core.settle().is_err());
                (core.snapshot(), events)
            })
    };
    let (whole, whole_events) = run(1, &[1000], false);
    let (split, split_events) = run(4, &[333, 167, 500], true);
    assert_eq!(whole, split);
    assert_eq!(whole_events, split_events);
    assert_eq!(whole.people[0].health, 96);
    assert_eq!(whole.people[1].region, Some(2));
    assert!(whole.travel.as_ref().unwrap().journeys.is_empty());
    let mut physical = Runtime::compile(initial.physical.clone()).unwrap();
    let expected_events = physical.advance(&[(1, 2)], initial.seed).unwrap();
    let expected_person = game_economy_person::prepare(
        &initial.people,
        &initial.rules,
        initial.nutrition.as_ref(),
        &initial.pending_food,
        Effects {
            hunger_relief: vec![],
            pending: vec![],
        },
        1,
    );
    let mut people = expected_person.people;
    let expected_travel = initial.travel.as_ref().unwrap().advance(&mut people);
    assert_eq!(whole.physical, physical.snapshot());
    assert_eq!(whole.people, people);
    assert_eq!(whole.nutrition, expected_person.nutrition);
    assert_eq!(whole.pending_food, expected_person.pending);
    assert_eq!(whole.travel, Some(expected_travel));
    assert_eq!(whole_events, expected_events);
}

#[test]
fn early_arrival_is_resumable_and_does_not_run_periodic_effects() {
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel.as_mut().unwrap().journeys[0].progress_millis = 500;
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                assert!(core.publish_arrivals().is_err());
                for &chunk in chunks {
                    assert_eq!(core.progress_to_deadline(chunk, None).unwrap(), chunk);
                }
                assert!(core.next_deadline(None).unwrap().arrival);
                let before = core.snapshot();
                assert_eq!(before.people[1].region, None);
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                let arrived = core.publish_arrivals().unwrap().unwrap();
                assert_eq!((arrived.tick, arrived.offset_millis), (0, 500));
                assert_eq!(arrived.people, vec![2]);
                let after = core.snapshot();
                assert_eq!(after.people[1].region, Some(2));
                assert_eq!(after.people[1].health, 100);
                assert_eq!(after.physical.custody.tick, 0);
                assert_eq!(after.pending_food.len(), 1);
                assert!(after.travel.as_ref().unwrap().journeys.is_empty());
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(core.publish_arrivals().unwrap(), None);
                assert!(!core.next_deadline(None).unwrap().arrival);
                assert_eq!(core.progress_to_deadline(500, None).unwrap(), 500);
                assert!(core.publish_arrivals().is_err());
                core.settle().unwrap();
                core.snapshot()
            })
    };
    let one = run(1, &[500]);
    let multiple = run(4, &[125, 375]);
    assert_eq!(one, multiple);
    assert_eq!(one.people[1].region, Some(2));
    assert_eq!(one.physical.custody.tick, 1);
}

#[test]
fn arrival_can_start_work_that_completes_before_the_boundary() {
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel.as_mut().unwrap().journeys[0].progress_millis = 600;
                let input = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 2, good: 1 })
                    .unwrap();
                *input = Container::new(
                    input.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let start = Start {
                    id: 2,
                    worker: 2,
                    region: 2,
                    recipe: 1,
                    units: 1,
                };
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                assert_eq!(core.progress_to_deadline(400, None).unwrap(), 400);
                let before = core.snapshot();
                assert!(core.start_work(0, 400, &[(start, 2)]).is_err());
                assert_eq!(core.snapshot(), before);
                assert_eq!(core.publish_arrivals().unwrap().unwrap().people, vec![2]);
                let saved = serde_json::to_string(&core.snapshot()).unwrap();
                let mut core = CoreInterval::resume(serde_json::from_str(&saved).unwrap()).unwrap();
                let before = core.snapshot();
                assert!(core.start_work(0, 399, &[(start, 2)]).is_err());
                assert!(core.start_work(0, 400, &[(start, 0)]).is_err());
                assert_eq!(core.snapshot(), before);
                assert_eq!(
                    core.start_work(0, 400, &[(start, 2)]).unwrap(),
                    vec![(2, true)]
                );
                let after = core.snapshot();
                assert_eq!(after.physical.custody.tick, 0);
                assert_eq!(
                    after
                        .physical
                        .work_interval
                        .as_ref()
                        .unwrap()
                        .elapsed_millis,
                    400
                );
                assert_eq!(
                    after.physical.work_interval.as_ref().unwrap().rates,
                    vec![(1, 1), (2, 2)]
                );
                assert!(core.start_work(0, 400, &[(start, 2)]).is_err());
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap(),
                )
                .unwrap();
                for &chunk in chunks {
                    assert_eq!(core.progress_to_deadline(chunk, None).unwrap(), chunk);
                }
                assert_eq!(core.elapsed_millis(), Some(900));
                assert_eq!(
                    core.publish_early_work().unwrap().unwrap().finished,
                    vec![2]
                );
                let intermediate = core.snapshot();
                assert_eq!(intermediate.people[1].health, 100);
                assert!(intermediate.physical.custody.containers.iter().any(|row| {
                    row.place == Place::JobOutput(2)
                        && row
                            .lots
                            .iter()
                            .any(|lot| lot.good == 2 && lot.quantity == 1)
                }));
                assert_eq!(core.progress_to_deadline(100, None).unwrap(), 100);
                let events = core.settle().unwrap();
                (core.snapshot(), events)
            })
    };
    assert_eq!(run(1, &[500]), run(4, &[100, 150, 250]));
}

#[test]
fn rejected_interval_work_reservation_is_inert() {
    let mut initial = fixture();
    initial.travel.as_mut().unwrap().journeys[0].progress_millis = 600;
    let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
    core.progress_to_deadline(400, None).unwrap();
    core.publish_arrivals().unwrap();
    let before = core.snapshot();
    let start = Start {
        id: 2,
        worker: 2,
        region: 2,
        recipe: 1,
        units: 1,
    };
    let remote = Start { region: 1, ..start };
    assert!(core.start_work(0, 400, &[(remote, 2)]).is_err());
    let busy = Start {
        worker: 1,
        region: 1,
        ..start
    };
    assert!(core.start_work(0, 400, &[(busy, 2)]).is_err());
    assert_eq!(
        core.start_work(0, 400, &[(start, 2)]).unwrap(),
        vec![(2, false)]
    );
    assert_eq!(core.snapshot(), before);
}

#[test]
fn interval_work_rejects_incapacitated_or_overhungry_people() {
    for (health, hunger) in [(0, 0), (100, 11)] {
        let mut initial = fixture();
        let mut person = initial.people[0].clone();
        person.id = 3;
        person.health = health;
        person.hunger = hunger;
        initial.people.push(person);
        initial.nutrition = None;
        let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
        core.progress(400).unwrap();
        let before = core.snapshot();
        let start = Start {
            id: 2,
            worker: 3,
            region: 1,
            recipe: 1,
            units: 1,
        };
        assert!(core.start_work(0, 400, &[(start, 1)]).is_err());
        assert_eq!(core.snapshot(), before);
    }
}

#[test]
fn mixed_interval_admission_preserves_rejected_stock_and_resumes() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = fixture();
                initial.travel.as_mut().unwrap().journeys[0].progress_millis = 600;
                let mut third = initial.people[0].clone();
                third.id = 3;
                third.region = Some(2);
                initial.people.push(third);
                initial.nutrition = None;
                let input = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == Place::Stock { region: 2, good: 1 })
                    .unwrap();
                *input = Container::new(
                    input.place,
                    10,
                    vec![Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut core = CoreInterval::begin(initial, &[(1, 1)]).unwrap();
                core.progress_to_deadline(400, None).unwrap();
                core.publish_arrivals().unwrap();
                let requests = [2, 3].map(|id| {
                    (
                        Start {
                            id,
                            worker: id,
                            region: 2,
                            recipe: 1,
                            units: 1,
                        },
                        2,
                    )
                });
                let results = core.start_work(0, 400, &requests).unwrap();
                assert_eq!(results.iter().filter(|row| row.1).count(), 1);
                let saved = core.snapshot();
                assert_eq!(saved.physical.work.len(), 2);
                assert_eq!(
                    saved.physical.work_interval.as_ref().unwrap().rates.len(),
                    2
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                core.progress_to_deadline(500, None).unwrap();
                core.publish_early_work().unwrap();
                core.progress_to_deadline(100, None).unwrap();
                core.settle().unwrap();
                (results, core.snapshot())
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_worker_can_start_again_after_early_completion() {
    let mut initial = fixture();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        10,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut core = CoreInterval::begin(initial, &[(1, 2)]).unwrap();
    core.progress_to_deadline(500, None).unwrap();
    let second = Start {
        id: 2,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert!(core.start_work(0, 500, &[(second, 2)]).is_err());
    core.publish_early_work().unwrap();
    assert_eq!(
        core.start_work(0, 500, &[(second, 2)]).unwrap(),
        vec![(2, true)]
    );
    core.progress_to_deadline(500, None).unwrap();
    let events = core.settle().unwrap();
    assert_eq!(events.finished, vec![2]);
}

#[test]
#[ignore = "manual interval work admission scaling benchmark"]
fn interval_work_admission_benchmark() {
    use std::hash::{Hash, Hasher};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50_000);
    assert!(count >= 2);
    let stock_units = std::env::var("LEGACY_ADMISSION_STOCK")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(count - 1)
        .min(count - 1);
    let defer = std::env::var_os("LEGACY_DEFER").is_some();
    let identified = std::env::var_os("LEGACY_IDENTIFIED_SETTLE").is_some();
    let identified_early = std::env::var_os("LEGACY_IDENTIFIED_EARLY").is_some();
    let automatic = std::env::var_os("LEGACY_AUTOWORK").is_some();
    assert!(
        [
            defer,
            identified,
            identified_early,
            automatic,
            std::env::var_os("LEGACY_ABANDON").is_some()
        ]
        .iter()
        .filter(|&&value| value)
        .count()
            <= 1
    );
    assert!(!identified_early || stock_units > 0);
    let mut initial = fixture();
    let person = initial.people[0].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = person.clone();
            p.id = id;
            if automatic {
                p.willing_to_work = true;
            }
            p
        })
        .collect();
    initial.travel = None;
    initial.nutrition = None;
    initial.pending_food.clear();
    let stock = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        count * 2,
        if stock_units == 0 {
            vec![]
        } else {
            vec![Lot {
                item: None,
                good: 1,
                quantity: stock_units * 2,
                expires_at: None,
            }]
        },
    )
    .unwrap();
    let starts: Vec<_> = (2..=count)
        .map(|id| {
            (
                Start {
                    id,
                    worker: id,
                    region: 1,
                    recipe: 1,
                    units: 1,
                },
                if defer || identified || identified_early {
                    2
                } else {
                    1
                },
            )
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            let mut abandonment_samples = Vec::new();
            let mut deferral_samples = Vec::new();
            let mut settlement_samples = Vec::new();
            let mut early_samples = Vec::new();
            for round in 0..4 {
                let mut settled_owner = None;
                let mut auto = automatic.then(|| game_autowork::Autowork {
                    next_id: 2,
                    policies: (2..=count)
                        .map(|person| game_autowork::Policy {
                            person,
                            recipe: 1,
                            enabled: true,
                            disregard_claims: false,
                        })
                        .collect(),
                    requests: None,
                });
                let mut core = CoreInterval::begin(initial.clone(), &[(1, 1)]).unwrap();
                assert_eq!(core.progress(400).unwrap(), 400);
                let start = std::time::Instant::now();
                let result = if let Some(owner) = &mut auto {
                    core.start_autonomous_work(0, 400, owner, &[], 1).unwrap()
                } else {
                    core.start_work(0, 400, &starts).unwrap()
                };
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    result.len(),
                    if automatic {
                        stock_units as usize
                    } else {
                        starts.len()
                    }
                );
                assert_eq!(
                    result.iter().filter(|row| row.1).count() as u64,
                    stock_units
                );
                if std::env::var_os("LEGACY_ABANDON").is_some() {
                    let requests: Vec<_> = result
                        .iter()
                        .filter(|row| row.1)
                        .map(|row| (row.0, row.0))
                        .collect();
                    let start = std::time::Instant::now();
                    core.abandon_work(0, 400, &requests).unwrap();
                    if round > 0 {
                        abandonment_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if defer {
                    core.progress(500).unwrap();
                    let jobs: Vec<_> = result.iter().filter(|row| row.1).map(|row| row.0).collect();
                    let start = std::time::Instant::now();
                    core.defer_work(0, 900, &jobs).unwrap();
                    if round > 0 {
                        deferral_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if identified {
                    core.progress(600).unwrap();
                    let owner = game_economy_work::tool_wear::ToolWear {
                        next_item: 1_000_000,
                        rules: vec![game_economy_work::tool_wear::Rule {
                            good: 2,
                            durability: 10,
                            wear: 1,
                            repair_good: 1,
                            repair_quantity: 1,
                        }],
                        conditions: vec![],
                        repairs: vec![],
                        events: vec![],
                    };
                    let start = std::time::Instant::now();
                    let draft = core
                        .prepare_settlement(Some(game_lot_runtime::IdentifiedProduction {
                            goods: &[2],
                            next_item: 1_000_000,
                        }))
                        .unwrap();
                    let next_owner = owner
                        .stage_identified_outputs(
                            draft.produced_items(),
                            draft.next_produced_item().unwrap(),
                            draft.item_updates(),
                        )
                        .unwrap();
                    assert_eq!(next_owner.conditions.len(), stock_units as usize + 1);
                    core.commit_settlement(draft).unwrap();
                    settled_owner = Some(next_owner);
                    if round > 0 {
                        settlement_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if identified_early {
                    core.progress(500).unwrap();
                    let owner = game_economy_work::tool_wear::ToolWear {
                        next_item: 1_000_000,
                        rules: vec![game_economy_work::tool_wear::Rule {
                            good: 2,
                            durability: 10,
                            wear: 1,
                            repair_good: 1,
                            repair_quantity: 1,
                        }],
                        conditions: vec![],
                        repairs: vec![],
                        events: vec![],
                    };
                    let start = std::time::Instant::now();
                    let draft = core
                        .prepare_early_work(Some(game_lot_runtime::IdentifiedProduction {
                            goods: &[2],
                            next_item: 1_000_000,
                        }))
                        .unwrap()
                        .unwrap();
                    let next_owner = owner
                        .stage_identified_outputs(
                            draft.produced_items(),
                            draft.next_produced_item().unwrap(),
                            draft.item_updates(),
                        )
                        .unwrap();
                    assert_eq!(next_owner.conditions.len(), stock_units as usize);
                    core.commit_early_work(draft).unwrap();
                    settled_owner = Some(next_owner);
                    if round > 0 {
                        early_samples.push(start.elapsed().as_secs_f64());
                    }
                }
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&core.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    if let Some(owner) = settled_owner {
                        serde_json::to_vec(&owner).unwrap().hash(&mut hash);
                    }
                    if let Some(owner) = auto {
                        serde_json::to_vec(&owner).unwrap().hash(&mut hash);
                    }
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} starts={} stock_units={stock_units} elapsed={:.6}",
                starts.len(),
                samples[1]
            );
            if automatic {
                println!(
                    "workers={workers} autonomous_starts={stock_units} elapsed={:.6}",
                    samples[1]
                );
            }
            if !abandonment_samples.is_empty() {
                abandonment_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} abandoned={stock_units} elapsed={:.6}",
                    abandonment_samples[1]
                );
            }
            if defer {
                deferral_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} deferred={stock_units} elapsed={:.6}",
                    deferral_samples[1]
                );
            }
            if identified {
                settlement_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} identified={} elapsed={:.6}",
                    stock_units + 1,
                    settlement_samples[1]
                );
            }
            if identified_early {
                early_samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} early_identified={stock_units} elapsed={:.6}",
                    early_samples[1]
                );
            }
            #[cfg(feature = "profile")]
            if std::env::var_os("LEGACY_PROFILE").is_some() {
                for (name, sample) in game_diagnostics::snapshot() {
                    if name.starts_with("physical.") || name.starts_with("work.start.") {
                        println!(
                            "{name}: {:.3} ms/call",
                            sample.nanoseconds as f64 / sample.calls as f64 / 1_000_000.0
                        );
                    }
                }
            }
        });
}

#[test]
fn core_publishes_early_work_without_advancing_periodic_systems() {
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
                for &chunk in chunks {
                    assert_eq!(core.progress_to_deadline(chunk, Some(3)).unwrap(), chunk);
                }
                assert_eq!(core.progress_to_deadline(1, Some(3)).unwrap(), 0);
                let before = core.snapshot();
                assert_eq!(before.physical.custody.tick, 0);
                assert_eq!(before.people[0].health, 100);
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                let early = core.publish_early_work().unwrap().unwrap();
                assert_eq!((early.tick, early.offset_millis), (0, 500));
                assert_eq!(early.finished, vec![1]);
                let after = core.snapshot();
                assert_eq!(after.physical.custody.tick, 0);
                assert_eq!(after.people[0].health, 100);
                assert_eq!(
                    after
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::JobOutput(1))
                        .unwrap()
                        .used,
                    1
                );
                let mut core = CoreInterval::resume(
                    serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(core.publish_early_work().unwrap().is_none());
                let next = core.next_deadline(Some(3)).unwrap();
                assert_eq!(next.after_millis, 500);
                assert!(next.quantum && next.arrival && next.hunger);
                assert_eq!(core.progress_to_deadline(1000, Some(3)).unwrap(), 500);
                let boundary = core.settle().unwrap();
                let state = core.snapshot();
                assert_eq!(state.physical.custody.tick, 1);
                assert_eq!(state.people[0].health, 96);
                assert_eq!(state.people[1].region, Some(2));
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    1
                );
                assert!(boundary.finished.is_empty());
                (state, early, boundary)
            })
    };
    assert_eq!(run(1, &[500]), run(4, &[333, 167]));
}

#[test]
fn core_deadline_reports_earliest_and_tied_causes_without_publishing() {
    let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
    let opening = core.snapshot();
    let deadline = core.next_deadline(Some(3)).unwrap();
    assert_eq!(deadline.after_millis, 500);
    assert!(deadline.work);
    assert!(!deadline.arrival && !deadline.hunger && !deadline.quantum);
    assert_eq!(core.snapshot(), opening);
    core.progress(333).unwrap();
    assert_eq!(core.next_deadline(Some(3)).unwrap().after_millis, 167);
    core.progress(167).unwrap();
    let pending = core.next_deadline(Some(3)).unwrap();
    assert_eq!(pending.after_millis, 0);
    assert!(pending.work);
    assert!(core.settle().is_err());

    let mut tied = CoreInterval::begin(fixture(), &[]).unwrap();
    let deadline = tied.next_deadline(Some(3)).unwrap();
    assert_eq!(deadline.after_millis, 1000);
    assert!(deadline.quantum && deadline.arrival && deadline.hunger);
    assert!(!deadline.work);
    tied.progress(1000).unwrap();
    let deadline = tied.next_deadline(Some(3)).unwrap();
    assert_eq!(deadline.after_millis, 0);
    assert!(deadline.quantum && deadline.arrival && deadline.hunger);
}

#[test]
fn composed_interval_rejects_invalid_reload_without_mutation() {
    let mut dead_worker = fixture();
    dead_worker.people[0].health = 0;
    assert!(CoreInterval::begin(dead_worker.clone(), &[(1, 2)]).is_err());
    assert!(CoreInterval::begin(dead_worker, &[]).is_ok());
    let mut core = CoreInterval::begin(fixture(), &[(1, 2)]).unwrap();
    assert_eq!(core.progress(250).unwrap(), 250);
    let before = core.snapshot();
    let mut invalid = before.clone();
    invalid.people[1].region = Some(1);
    assert!(CoreInterval::resume(invalid).is_err());
    let mut invalid = before.clone();
    invalid.people[0].health = 0;
    assert!(CoreInterval::resume(invalid).is_err());
    assert_eq!(core.snapshot(), before);
    assert_eq!(core.progress(u64::MAX).unwrap(), 750);
}

#[test]
fn interval_rejects_unordered_journeys_and_preserves_failed_settlement() {
    let mut unordered = fixture();
    let mut second = unordered.people[1].clone();
    second.id = 3;
    unordered.people.push(second);
    let travel = unordered.travel.as_mut().unwrap();
    let mut journey = travel.journeys[0].clone();
    journey.person = 3;
    travel.journeys.push(journey);
    assert!(CoreInterval::begin(unordered.clone(), &[(1, 2)]).is_ok());
    unordered.travel.as_mut().unwrap().journeys.reverse();
    assert!(CoreInterval::begin(unordered, &[(1, 2)]).is_err());

    let mut overflow = fixture();
    overflow.physical.custody.tick = u64::MAX;
    overflow.pending_food.clear();
    let mut core = CoreInterval::begin(overflow, &[(1, 2)]).unwrap();
    core.progress(1000).unwrap();
    let before = core.snapshot();
    assert!(core.settle().is_err());
    assert_eq!(core.snapshot(), before);
    let saved = serde_json::to_string(&before).unwrap();
    assert_eq!(
        CoreInterval::resume(serde_json::from_str(&saved).unwrap())
            .unwrap()
            .snapshot(),
        before
    );
}

#[test]
fn incapacitated_immortal_does_not_travel_before_boundary_recovery() {
    let mut initial = fixture();
    initial.people[1].health = 0;
    initial.people[1].hunger = 0;
    initial.people[1].immortal = Some(game_economy_person::Immortal {
        recovery_per_tick: 5,
    });
    initial.pending_food.clear();
    let mut core = CoreInterval::begin(initial, &[(1, 2)]).unwrap();
    core.progress(1000).unwrap();
    let pending = core.snapshot();
    assert_eq!(pending.travel.as_ref().unwrap().journeys[0].remaining, 1);
    assert_eq!(
        pending.travel.as_ref().unwrap().journeys[0].progress_millis,
        0
    );
    core.settle().unwrap();
    let settled = core.snapshot();
    assert_eq!(settled.people[1].health, 5);
    assert_eq!(settled.people[1].region, None);
    assert_eq!(settled.travel.unwrap().journeys[0].remaining, 1);
}

#[test]
#[ignore = "manual composed interval scaling benchmark"]
fn composed_core_progress_benchmark() {
    use std::hash::{Hash, Hasher};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000u64);
    let deadline = std::env::var_os("LEGACY_CORE_DEADLINE").is_some();
    let arrivals = std::env::var_os("LEGACY_CORE_ARRIVALS").is_some();
    let many_jobs = std::env::var_os("LEGACY_CORE_JOBS").is_some();
    assert!(count >= 2 && count.is_multiple_of(2));
    let mut initial = fixture();
    let resident = initial.people[0].clone();
    let traveler = initial.people[1].clone();
    initial.people = (1..=count)
        .map(|id| {
            let mut p = if id % 2 == 0 {
                traveler.clone()
            } else {
                resident.clone()
            };
            p.id = id;
            p
        })
        .collect();
    initial.travel.as_mut().unwrap().journeys = (1..=count)
        .filter(|id| id % 2 == 0)
        .map(|person| Journey {
            person,
            source: 1,
            destination: 2,
            remaining: 1,
            progress_millis: if arrivals { 500 } else { 0 },
            pending_arrival: false,
        })
        .collect();
    let rates: Vec<_> = if many_jobs {
        let stock = initial
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|row| row.place == Place::Stock { region: 1, good: 1 })
            .unwrap();
        *stock = Container::new(
            stock.place,
            count,
            if count == 2 {
                vec![]
            } else {
                vec![Lot {
                    item: None,
                    good: 1,
                    quantity: count - 2,
                    expires_at: None,
                }]
            },
        )
        .unwrap();
        let mut runtime = Runtime::compile(initial.physical.clone()).unwrap();
        let starts: Vec<_> = (3..=count)
            .step_by(2)
            .map(|id| Start {
                id,
                worker: id,
                region: 1,
                recipe: 1,
                units: 1,
            })
            .collect();
        assert!(
            runtime
                .start(&starts, initial.seed)
                .unwrap()
                .iter()
                .all(|row| row.1)
        );
        initial.physical = runtime.snapshot();
        (1..=count).step_by(2).map(|id| (id, 2)).collect()
    } else {
        vec![(1, 2)]
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut interval = CoreInterval::begin(initial.clone(), &rates).unwrap();
                if arrivals {
                    assert_eq!(interval.progress(500).unwrap(), 500);
                }
                let started = std::time::Instant::now();
                if arrivals {
                    assert_eq!(interval.publish_arrivals().unwrap().unwrap().people.len(), (count / 2) as usize);
                } else if deadline {
                    assert_eq!(interval.next_deadline(Some(3)).unwrap().after_millis, 500);
                } else {
                    assert_eq!(interval.progress(500).unwrap(), 500);
                }
                let elapsed = started.elapsed().as_secs_f64();
                if round == 3 {
                    let state = interval.snapshot();
                    assert!(
                        state
                            .people
                            .iter()
                            .all(|person| person.hunger_remainder == if deadline { 0 } else { 500 })
                    );
                    assert!(
                        state
                            .travel
                            .as_ref()
                            .unwrap()
                            .journeys
                            .iter()
                            .all(
                                |journey| journey.progress_millis == if deadline { 0 } else { 500 }
                            )
                    );
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&state).unwrap().hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} travelers={} jobs={} deadline={deadline} arrivals={arrivals} elapsed={:.6}",
                count / 2,
                rates.len(),
                samples[1]
            );
        });
}
