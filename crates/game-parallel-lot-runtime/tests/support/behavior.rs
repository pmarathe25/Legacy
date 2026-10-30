use super::*;
use game_parallel_lots::{Container, Lot, State};
use game_parallel_production_model::Amount;

fn runtime() -> Runtime {
    let good = |id| Good {
        id,
        name: format!("good-{id}"),
        food: None,
    };
    let stock = |good, quantity| {
        Container::new(
            Place::Stock { region: 1, good },
            10,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    good,
                    quantity,
                    expires_at: None,
                }]
            },
        )
        .unwrap()
    };
    Runtime::compile(Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![stock(1, 4), stock(2, 0)],
        },
        goods: vec![good(1), good(2)],
        recipes: vec![Recipe {
            id: 1,
            name: "work".into(),
            ticks: 2,
            inputs: vec![Amount {
                good: 1,
                quantity: 2,
            }],
            outputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        }],
        tick_millis: 86_400_000,
        work: vec![],
    })
    .unwrap()
}
#[test]
fn environmental_outputs_compete_for_real_capacity_and_must_be_fresh() {
    let mut world = runtime();
    let output = |id, expires_at| Transaction {
        id,
        movements: vec![],
        creations: vec![game_parallel_lots::Creation {
            id,
            to: Place::Stock { region: 1, good: 2 },
            lot: Lot {
                good: 2,
                quantity: 7,
                expires_at,
            },
        }],
    };
    let before = world.snapshot();
    assert!(
        world
            .prepare_with_growth(
                &[],
                &[],
                &[],
                &[],
                Growth {
                    harvest: &[output(1, Some(5))],
                    compact_consumption: &[]
                },
                1
            )
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    let draft = world
        .prepare_with_growth(
            &[],
            &[],
            &[],
            &[],
            Growth {
                harvest: &[output(1, None), output(2, None)],
                compact_consumption: &[],
            },
            1,
        )
        .unwrap();
    assert_eq!(
        draft
            .events()
            .physical_actions
            .iter()
            .filter(|r| r.accepted)
            .count(),
        1
    );
    world.commit_advance(draft).unwrap();
    assert_eq!(
        world
            .custody(Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        7
    );
}
#[test]
fn actual_work_progresses_only_when_person_chooses_and_survives_reload() {
    let mut runtime = runtime();
    assert_eq!(
        runtime
            .start(
                &[Start {
                    id: 1,
                    worker: 1,
                    region: 1,
                    recipe: 1,
                    units: 1
                }],
                1
            )
            .unwrap(),
        vec![(1, true)]
    );
    runtime.advance(&[], 1).unwrap();
    assert_eq!(runtime.work()[0].job.remaining, 2);
    runtime.advance(&[(1, 1)], 1).unwrap();
    let saved = serde_json::to_string(&runtime.snapshot()).unwrap();
    let mut resumed = Runtime::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(runtime.advance(&[(1, 1)], 1).unwrap().finished, vec![1]);
    assert_eq!(resumed.advance(&[(1, 1)], 1).unwrap().finished, vec![1]);
    assert_eq!(runtime.advance(&[], 1).unwrap().delivered, vec![1]);
    assert_eq!(resumed.advance(&[], 1).unwrap().delivered, vec![1]);
    assert_eq!(runtime.snapshot(), resumed.snapshot());
}
#[test]
fn worker_can_abandon_even_when_reserved_material_cannot_yet_fit() {
    let mut runtime = runtime();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    let mut saved = runtime.snapshot();
    saved.custody.containers[0] = Container::new(
        Place::Stock { region: 1, good: 1 },
        4,
        vec![Lot {
            good: 1,
            quantity: 4,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut runtime = Runtime::compile(saved).unwrap();
    runtime.abandon(1, 1).unwrap();
    assert_eq!(runtime.work()[0].phase, Phase::Returning);
    assert!(runtime.advance(&[], 1).unwrap().returned.is_empty());
    assert_eq!(
        runtime
            .start(
                &[Start {
                    id: 2,
                    worker: 1,
                    region: 1,
                    recipe: 1,
                    units: 1
                }],
                1
            )
            .unwrap(),
        vec![(2, true)]
    );
    assert_eq!(runtime.advance(&[], 1).unwrap().returned, vec![1]);
    assert_eq!(runtime.work().len(), 1);
    assert_eq!(runtime.work()[0].job.id, 2);
}

fn concentrated(count: u64) -> (Snapshot, Vec<Start>) {
    let mut snapshot = runtime().snapshot();
    snapshot.custody.containers[0] = Container::new(
        Place::Stock { region: 1, good: 1 },
        count * 2,
        vec![Lot {
            good: 1,
            quantity: count,
            expires_at: None,
        }],
    )
    .unwrap();
    snapshot.custody.containers[1] =
        Container::new(Place::Stock { region: 1, good: 2 }, count, vec![]).unwrap();
    snapshot.recipes[0].ticks = 1;
    let requests = (1..=count)
        .map(|id| Start {
            id,
            worker: id,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    (snapshot, requests)
}
#[test]
fn concentrated_jobs_and_custody_match_across_worker_counts() {
    let (snapshot, requests) = concentrated(8192);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = Runtime::compile(snapshot.clone()).unwrap();
                let started = runtime.start(&requests, 9).unwrap();
                assert_eq!(started.iter().filter(|row| row.1).count(), 4096);
                let rates: Vec<_> = started
                    .iter()
                    .filter(|row| row.1)
                    .map(|row| (row.0, 1))
                    .collect();
                let finished = runtime.advance(&rates, 9).unwrap();
                let delivered = runtime.advance(&[], 9).unwrap();
                (runtime.snapshot(), finished, delivered)
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
#[ignore]
fn concentrated_production_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_JOBS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000);
    let (snapshot, requests) = concentrated(count);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        let mut samples = Vec::new();
        for round in 0..4 {
            let mut runtime = Runtime::compile(snapshot.clone()).unwrap();
            let start = std::time::Instant::now();
            let admitted = runtime.start(&requests, 9).unwrap();
            let rates: Vec<_> = admitted
                .iter()
                .filter(|row| row.1)
                .map(|row| (row.0, 1))
                .collect();
            let finished = runtime.advance(&rates, 9).unwrap();
            let delivered = runtime.advance(&[], 9).unwrap();
            let elapsed = start.elapsed().as_secs_f64();
            assert_eq!(rates.len(), count as usize / 2);
            assert_eq!(finished.finished.len(), rates.len());
            assert_eq!(delivered.delivered.len(), rates.len());
            if round > 0 {
                samples.push(elapsed);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "workers={workers} jobs={count} admitted={} elapsed={:.6}",
            count / 2,
            samples[1]
        );
    });
}

#[test]
fn loading_rejects_orphan_or_excess_job_custody() {
    let mut runtime = runtime();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    let snapshot = runtime.snapshot();
    let mut orphan = snapshot.clone();
    orphan.work.clear();
    assert!(Runtime::compile(orphan).is_err());
    let mut excess = snapshot;
    let input = excess
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(
        Place::JobInput(1),
        10,
        vec![Lot {
            good: 1,
            quantity: 3,
            expires_at: None,
        }],
    )
    .unwrap();
    assert!(Runtime::compile(excess).is_err());
    let mut forged = runtime.snapshot();
    forged.work[0].job.outputs[0].quantity = 7;
    assert!(Runtime::compile(forged).is_err());
    let mut invalid_progress = runtime.snapshot();
    invalid_progress.work[0].job.remaining = u64::MAX;
    assert!(Runtime::compile(invalid_progress).is_err());
}

#[test]
fn expiry_interrupts_a_real_job_and_frees_the_worker() {
    let mut snapshot = runtime().snapshot();
    snapshot.custody.containers[0] = Container::new(
        Place::Stock { region: 1, good: 1 },
        10,
        vec![Lot {
            good: 1,
            quantity: 2,
            expires_at: Some(2),
        }],
    )
    .unwrap();
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    runtime.advance(&[(1, 1)], 1).unwrap();
    let events = runtime.advance(&[(1, 1)], 1).unwrap();
    assert_eq!(events.interrupted, vec![1]);
    assert_eq!(events.empty_returns, vec![1]);
    assert!(events.returned.is_empty());
    assert_eq!(events.spoiled[0].quantity, 2);
    assert!(events.finished.is_empty());
    assert!(runtime.work().is_empty());
    assert_eq!(
        runtime
            .snapshot()
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        0
    );
}
#[test]
fn blocked_finished_food_spoils_without_being_recreated() {
    use game_parallel_production_model::FoodProfile;
    use std::collections::BTreeMap;
    let mut snapshot = runtime().snapshot();
    snapshot.recipes[0].ticks = 1;
    snapshot.goods[1].food = Some(FoodProfile {
        portion: 1,
        hunger_relief: 1,
        nutrients: BTreeMap::new(),
        health_effect: 0,
        shelf_days: Some(2),
    });
    snapshot.custody.containers[1] = Container::new(
        Place::Stock { region: 1, good: 2 },
        1,
        vec![Lot {
            good: 2,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    assert_eq!(runtime.advance(&[(1, 1)], 1).unwrap().finished, vec![1]);
    assert!(runtime.advance(&[], 1).unwrap().delivered.is_empty());
    let events = runtime.advance(&[], 1).unwrap();
    assert_eq!(events.output_lost, vec![1]);
    assert!(events.delivered.is_empty());
    assert_eq!(events.spoiled[0].place, Place::JobOutput(1));
    assert_eq!(events.spoiled[0].quantity, 1);
    assert!(runtime.work().is_empty());
    assert_eq!(
        runtime
            .snapshot()
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        1
    );
}

#[test]
fn physical_transfers_and_consumption_use_the_same_lots_as_production() {
    use game_parallel_lots::{Movement, Transaction};
    let mut snapshot = runtime().snapshot();
    snapshot
        .custody
        .containers
        .push(Container::new(Place::Person(1), 10, vec![]).unwrap());
    let mut runtime = Runtime::compile(snapshot).unwrap();
    let take = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: Place::Stock { region: 1, good: 1 },
            to: Some(Place::Person(1)),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let result = runtime.actions_now(&[take], 1).unwrap();
    assert!(result.transactions[0].accepted);
    let consume = Transaction {
        id: 2,
        movements: vec![Movement {
            id: 2,
            from: Place::Person(1),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let result = runtime.actions_now(&[consume], 1).unwrap();
    assert!(result.transactions[0].accepted);
    assert_eq!(result.movements[0].lots[0].quantity, 1);
    assert_eq!(runtime.snapshot().custody.tick, 0);
    let state = runtime.snapshot();
    assert_eq!(
        state
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Stock { region: 1, good: 1 })
            .unwrap()
            .used,
        3
    );
    assert_eq!(
        state
            .custody
            .containers
            .iter()
            .find(|row| row.place == Place::Person(1))
            .unwrap()
            .used,
        0
    );
}
#[test]
fn external_taking_remains_physical_and_cannot_create_goods() {
    use game_parallel_lots::{Creation, Movement, Transaction};
    let mut runtime = runtime();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    let before = runtime.snapshot();
    let theft = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: Place::JobInput(1),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let creation = Transaction {
        id: 2,
        movements: vec![],
        creations: vec![Creation {
            id: 2,
            to: Place::Stock { region: 1, good: 1 },
            lot: Lot {
                good: 1,
                quantity: 1,
                expires_at: None,
            },
        }],
    };
    assert!(runtime.actions_now(&[creation], 1).is_err());
    assert_eq!(runtime.snapshot(), before);
    let result = runtime.actions_now(&[theft], 1).unwrap();
    assert!(result.transactions[0].accepted);
    let events = runtime.advance(&[(1, 2)], 1).unwrap();
    assert_eq!(events.interrupted, vec![1]);
}

#[test]
fn people_keep_possessions_until_they_are_physically_recovered() {
    use game_parallel_lots::{Movement, Transaction};
    let mut runtime = runtime();
    runtime.add_people(&[(1, 2), (2, 2)]).unwrap();
    let take = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: Place::Stock { region: 1, good: 1 },
            to: Some(Place::Person(1)),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    assert!(runtime.actions_now(&[take], 1).unwrap().transactions[0].accepted);
    assert!(runtime.retire_person(1).is_err());
    let recover = Transaction {
        id: 2,
        movements: vec![Movement {
            id: 2,
            from: Place::Person(1),
            to: Some(Place::Person(2)),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    assert!(runtime.actions_now(&[recover], 1).unwrap().transactions[0].accepted);
    runtime.retire_person(1).unwrap();
    assert!(runtime.custody(Place::Person(1)).is_none());
    assert_eq!(runtime.custody(Place::Person(2)).unwrap().used, 1);
}

#[test]
fn external_taking_and_job_completion_compete_for_the_same_snapshot() {
    use game_parallel_lots::{Movement, Transaction};
    let mut initial = runtime();
    initial
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    let snapshot = initial.snapshot();
    let action = Transaction {
        id: 31,
        movements: vec![Movement {
            id: 97,
            from: Place::JobInput(1),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = Runtime::compile(snapshot.clone()).unwrap();
                let events = runtime
                    .advance_with_actions(&[(1, 2)], std::slice::from_ref(&action), 19)
                    .unwrap();
                assert_eq!(events.physical_actions.len(), 1);
                assert_eq!(events.physical_actions[0].id, 31);
                assert_eq!(events.physical_movements.len(), 1);
                assert_eq!(
                    (
                        events.physical_movements[0].action,
                        events.physical_movements[0].movement.id
                    ),
                    (31, 97)
                );
                assert_ne!(
                    events.physical_actions[0].accepted,
                    !events.finished.is_empty()
                );
                (runtime.snapshot(), events)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn detached_generation_rolls_back_and_abandonment_invalidates_it() {
    use game_parallel_lots::{Movement, Transaction};
    let mut runtime = runtime();
    runtime.add_people(&[(2, 5)]).unwrap();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    let before = runtime.snapshot();
    let action = Transaction {
        id: 10,
        movements: vec![Movement {
            id: 12,
            from: Place::Stock { region: 1, good: 1 },
            to: Some(Place::Person(2)),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let draft = runtime.prepare_advance(&[(1, 1)], &[action], 1).unwrap();
    assert!(draft.events().physical_actions[0].accepted);
    assert_eq!(draft.events().physical_movements[0].movement.id, 12);
    assert_eq!(runtime.snapshot(), before);
    runtime.abandon(1, 1).unwrap();
    let abandoned = runtime.snapshot();
    assert!(runtime.validate_advance(&draft).is_err());
    assert!(runtime.commit_advance(draft).is_err());
    assert_eq!(runtime.snapshot(), abandoned);
}

#[test]
fn starts_and_other_actions_arbitrate_together_and_rejected_custody_disappears() {
    use game_parallel_lots::{Movement, Transaction};
    let mut runtime = runtime();
    runtime.add_people(&[(3, 4)]).unwrap();
    let initial = runtime.snapshot();
    let starts = [
        Start {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        },
        Start {
            id: 2,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        },
    ];
    let action = Transaction {
        id: 7,
        movements: vec![Movement {
            id: 8,
            from: Place::Stock { region: 1, good: 1 },
            to: Some(Place::Person(3)),
            good: 1,
            quantity: 2,
        }],
        creations: vec![],
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = Runtime::compile(initial.clone()).unwrap();
                let draft = runtime
                    .prepare_with_starts(&[], std::slice::from_ref(&action), &starts, 1)
                    .unwrap();
                assert_eq!(runtime.snapshot(), initial);
                assert_eq!(
                    draft.events().started.len()
                        + usize::from(draft.events().physical_actions[0].accepted),
                    2
                );
                let events = runtime.commit_advance(draft).unwrap();
                for id in &events.start_denied {
                    assert!(runtime.custody(Place::JobInput(*id)).is_none());
                    assert!(runtime.custody(Place::JobOutput(*id)).is_none());
                }
                assert_eq!(
                    runtime
                        .custody(Place::Stock { region: 1, good: 1 })
                        .unwrap()
                        .used,
                    0
                );
                (runtime.snapshot(), events)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_recipe_produces_real_dated_food_and_it_expires_after_transport_and_reload() {
    use game_parallel_lots::{Movement, Transaction};
    use game_parallel_production::{Inventory, Production};
    use std::path::Path;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut production = Production {
        max_work_hunger: 10,
        goods: vec![],
        recipes: vec![],
        recipe_categories: vec![],
        inventories: vec![Inventory {
            region: 1,
            stocks: vec![],
        }],
        jobs: vec![],
        events: vec![],
    };
    game_parallel_content_import::augment(
        &mut production,
        &root.join("objects"),
        &root.join("workshop"),
        86_400_000,
        100_000,
        1,
    )
    .unwrap();
    let recipe = production
        .recipes
        .iter()
        .find(|recipe| {
            let food = production
                .goods
                .iter()
                .find(|good| good.id == recipe.outputs[0].good)
                .and_then(|good| good.food.as_ref())
                .and_then(|food| food.shelf_days);
            food.is_some_and(|days| (2..=10).contains(&days))
                && recipe.inputs.iter().all(|input| {
                    production
                        .goods
                        .iter()
                        .find(|good| good.id == input.good)
                        .unwrap()
                        .food
                        .as_ref()
                        .and_then(|food| food.shelf_days)
                        .is_none_or(|days| days > 2)
                })
        })
        .unwrap()
        .clone();
    let output = recipe.outputs[0].good;
    let shelf = production
        .goods
        .iter()
        .find(|good| good.id == output)
        .unwrap()
        .food
        .as_ref()
        .unwrap()
        .shelf_days
        .unwrap();
    for input in &recipe.inputs {
        production.inventories[0]
            .stocks
            .iter_mut()
            .find(|stock| stock.good == input.good)
            .unwrap()
            .quantity = input.quantity;
    }
    let catalog = Catalog::from_tick_millis(&production.goods, 86_400_000).unwrap();
    let custody = catalog.fresh_world(&production.inventories, 0).unwrap();
    let mut runtime = Runtime::compile(Snapshot {
        custody,
        goods: production.goods,
        recipes: production.recipes,
        tick_millis: 86_400_000,
        work: vec![],
    })
    .unwrap();
    runtime.add_people(&[(1, 100_000)]).unwrap();
    let start = Start {
        id: 1,
        worker: 1,
        region: 1,
        recipe: recipe.id,
        units: 1,
    };
    let draft = runtime.prepare_with_starts(&[], &[], &[start], 1).unwrap();
    assert_eq!(draft.events().started, vec![1]);
    runtime.commit_advance(draft).unwrap();
    assert_eq!(runtime.advance(&[(1, 1)], 1).unwrap().finished, vec![1]);
    assert_eq!(runtime.advance(&[], 1).unwrap().delivered, vec![1]);
    let pickup = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: Place::Stock {
                region: 1,
                good: output,
            },
            to: Some(Place::Person(1)),
            good: output,
            quantity: recipe.outputs[0].quantity,
        }],
        creations: vec![],
    };
    assert!(runtime.actions_now(&[pickup], 1).unwrap().transactions[0].accepted);
    let saved = serde_json::to_string(&runtime.snapshot()).unwrap();
    let mut runtime = Runtime::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    let expiry = 2 + u64::from(shelf);
    let mut found = false;
    while runtime.snapshot().custody.tick < expiry {
        let events = runtime.advance(&[], 1).unwrap();
        found |= events.spoiled.iter().any(|item| {
            item.place == Place::Person(1) && item.good == output && item.expires_at == expiry
        });
    }
    assert!(found);
}

#[test]
fn a_worker_can_leave_in_a_detached_generation_even_if_return_is_blocked() {
    let mut runtime = runtime();
    runtime
        .start(
            &[Start {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            1,
        )
        .unwrap();
    let mut saved = runtime.snapshot();
    saved.custody.containers[0] = Container::new(
        Place::Stock { region: 1, good: 1 },
        4,
        vec![Lot {
            good: 1,
            quantity: 4,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut runtime = Runtime::compile(saved).unwrap();
    let before = runtime.snapshot();
    assert!(
        runtime
            .prepare_with_choices(&[(1, 1)], &[], &[], &[(1, 1)], 1)
            .is_err()
    );
    let draft = runtime
        .prepare_with_choices(&[], &[], &[], &[(1, 1)], 1)
        .unwrap();
    assert_eq!(draft.events().abandoned, vec![1]);
    assert!(draft.events().returned.is_empty());
    assert_eq!(runtime.snapshot(), before);
    runtime.commit_advance(draft).unwrap();
    assert_eq!(runtime.work()[0].phase, Phase::Returning);
    assert_eq!(runtime.custody(Place::JobInput(1)).unwrap().used, 2);
    assert_eq!(
        runtime
            .start(
                &[Start {
                    id: 2,
                    worker: 1,
                    region: 1,
                    recipe: 1,
                    units: 1
                }],
                1
            )
            .unwrap(),
        vec![(2, true)]
    );
}
