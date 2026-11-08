use super::*;
use game_lots::{Container, Lot, State};
use game_production_model::Amount;

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
                    item: None,
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
        creations: vec![game_lots::Creation {
            id,
            to: Place::Stock { region: 1, good: 2 },
            lot: Lot {
                item: None,
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
                    animal_yield: &[],
                    animal_products: &[],
                    writing: &[],
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
                animal_yield: &[],
                animal_products: &[],
                writing: &[],
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
fn animal_yields_are_unique_canonical_and_subject_to_person_capacity() {
    use game_lot_content::AnimalYield;
    let mut snapshot = runtime().snapshot();
    snapshot
        .custody
        .containers
        .push(Container::new(Place::Person(1), 2, vec![]).unwrap());
    snapshot
        .custody
        .containers
        .push(Container::new(Place::Person(2), 2, vec![]).unwrap());
    let mut world = Runtime::compile(snapshot).unwrap();
    let yield_ = AnimalYield {
        action: 10,
        animal: 3,
        hunter: 1,
        good: 1,
        quantity: 2,
    };
    let invalid = AnimalYield {
        action: 11,
        hunter: 2,
        ..yield_
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
                    harvest: &[],
                    animal_yield: &[yield_, invalid],
                    animal_products: &[],
                    writing: &[],
                    compact_consumption: &[]
                },
                7
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
                harvest: &[],
                animal_yield: &[yield_],
                animal_products: &[],
                writing: &[],
                compact_consumption: &[],
            },
            7,
        )
        .unwrap();
    assert!(
        draft
            .events()
            .physical_actions
            .iter()
            .any(|event| event.id == 10 && event.accepted)
    );
    world.commit_advance(draft).unwrap();
    assert_eq!(
        world
            .snapshot()
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        2
    );
    let next = AnimalYield {
        action: 12,
        animal: 4,
        ..yield_
    };
    let draft = world
        .prepare_with_growth(
            &[],
            &[],
            &[],
            &[],
            Growth {
                harvest: &[],
                animal_yield: &[next],
                animal_products: &[],
                writing: &[],
                compact_consumption: &[],
            },
            7,
        )
        .unwrap();
    assert!(
        draft
            .events()
            .physical_actions
            .iter()
            .any(|event| event.id == 12 && !event.accepted)
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
            item: None,
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
            item: None,
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
            item: None,
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
            item: None,
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
    use game_production_model::FoodProfile;
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
            item: None,
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
    use game_lots::{Movement, Transaction};
    let mut snapshot = runtime().snapshot();
    snapshot
        .custody
        .containers
        .push(Container::new(Place::Person(1), 10, vec![]).unwrap());
    let mut runtime = Runtime::compile(snapshot).unwrap();
    let take = Transaction {
        id: 1,
        movements: vec![Movement {
            item: None,
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
            item: None,
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
    use game_lots::{Creation, Movement, Transaction};
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
            item: None,
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
                item: None,
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
    use game_lots::{Movement, Transaction};
    let mut runtime = runtime();
    runtime.add_people(&[(1, 2), (2, 2)]).unwrap();
    let take = Transaction {
        id: 1,
        movements: vec![Movement {
            item: None,
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
            item: None,
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
    use game_lots::{Movement, Transaction};
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
            item: None,
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
                let receipt = events.physical_movements.iter().next().unwrap();
                assert_eq!((receipt.action, receipt.movement.id), (31, 97));
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
fn external_receipts_keep_original_ids_across_parallel_chunks_and_compact_actions() {
    use game_lots::Movement;
    let actions: Vec<_> = (1..=2300)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                item: None,
                id: 10000 - id,
                from: Place::Stock { region: 1, good: 1 },
                to: None,
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        })
        .collect();
    let compact: Vec<_> = (1..=2300).filter(|id| id % 3 == 0).collect();
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut actions = actions.clone();
                if reverse {
                    actions.reverse();
                }
                let world = runtime();
                let draft = world
                    .prepare_with_growth(
                        &[],
                        &actions,
                        &[],
                        &[],
                        Growth {
                            harvest: &[],
                            animal_yield: &[],
                            animal_products: &[],
                            writing: &[],
                            compact_consumption: &compact,
                        },
                        19,
                    )
                    .unwrap();
                let events = draft.events();
                assert_eq!(events.physical_actions.len(), 2300);
                assert_eq!(events.physical_movements.len(), 2300 - compact.len());
                for receipt in &events.physical_movements {
                    assert_ne!(receipt.action % 3, 0);
                    assert_eq!(receipt.request.id, 10000 - receipt.action);
                    assert_eq!(receipt.movement.id, receipt.request.id);
                    assert_eq!(
                        receipt.movement.accepted,
                        events.physical_actions[(receipt.action - 1) as usize].accepted
                    );
                }
                (
                    events.physical_actions.clone(),
                    events
                        .physical_movements
                        .iter()
                        .map(|receipt| {
                            (
                                receipt.action,
                                receipt.request.id,
                                receipt.movement.accepted,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
fn flat_transfers_match_general_runtime_and_rollback() {
    use game_lots::{Movement, SimpleTransfer};
    let mut snapshot = runtime().snapshot();
    let mut containers = Vec::new();
    let mut transfers = Vec::new();
    for id in 1..=256 {
        containers.push(
            Container::new(
                Place::Person(id),
                10,
                vec![
                    Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: Some(1),
                    },
                    Lot {
                        item: None,
                        good: 1,
                        quantity: 3,
                        expires_at: Some(3),
                    },
                ],
            )
            .unwrap(),
        );
        containers.push(
            Container::new(
                Place::Person(id + 1000),
                if id % 4 == 0 { 1 } else { 10 },
                vec![],
            )
            .unwrap(),
        );
        transfers.push(SimpleTransfer {
            action: id * 2,
            movement: Movement {
                item: None,
                id: 10_000 - id,
                from: Place::Person(id),
                to: Some(Place::Person(id + 1000)),
                good: 1,
                quantity: 2,
            },
        });
    }
    containers.sort_unstable_by_key(|row| row.place);
    snapshot.custody.containers = containers;
    let actions: Vec<_> = transfers
        .iter()
        .map(|row| Transaction {
            id: row.action,
            movements: vec![row.movement],
            creations: vec![],
        })
        .collect();
    let run = |workers, reverse, aligned| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut transfers = transfers.clone();
                let mut actions = actions.clone();
                if aligned {
                    for (transfer, action) in transfers.iter_mut().zip(&mut actions) {
                        transfer.movement.id = transfer.action + 100;
                        action.movements[0].id = transfer.movement.id;
                    }
                }
                if reverse {
                    transfers.reverse();
                    actions.reverse();
                }
                let mut flat = Runtime::compile(snapshot.clone()).unwrap();
                let mut partitioned = Runtime::compile(snapshot.clone()).unwrap();
                let mut general = Runtime::compile(snapshot.clone()).unwrap();
                let before = flat.snapshot();
                let draft = flat.prepare_flat_transfers(&transfers).unwrap().unwrap();
                let stale = flat.prepare_flat_transfers(&transfers).unwrap().unwrap();
                assert_eq!(flat.snapshot(), before);
                let other = general.prepare_advance(&[], &actions, 19).unwrap();
                let direct = partitioned
                    .prepare_partitioned_flat_transfers(&transfers, 19)
                    .unwrap()
                    .unwrap();
                assert!(
                    draft.events() == other.events(),
                    "flat and general receipts differ"
                );
                assert_eq!(direct.events(), other.events());
                let events = flat.commit_advance(draft).unwrap();
                assert!(flat.commit_advance(stale).is_err());
                general.commit_advance(other).unwrap();
                partitioned.commit_advance(direct).unwrap();
                assert_eq!(flat.snapshot(), general.snapshot());
                assert_eq!(partitioned.snapshot(), general.snapshot());
                (flat.snapshot(), events)
            })
    };
    for aligned in [false, true] {
        assert_eq!(run(1, false, aligned), run(4, true, aligned));
    }
    let mut duplicate = transfers.clone();
    duplicate[1].action = duplicate[0].action;
    assert!(
        Runtime::compile(snapshot.clone())
            .unwrap()
            .prepare_flat_transfers(&duplicate)
            .is_err()
    );
    assert!(
        Runtime::compile(snapshot.clone())
            .unwrap()
            .prepare_partitioned_flat_transfers(&duplicate, 19)
            .is_err()
    );
    let mut invalid_good = transfers.clone();
    invalid_good[0].movement.good = 999;
    assert!(
        Runtime::compile(snapshot.clone())
            .unwrap()
            .prepare_partitioned_flat_transfers(&invalid_good, 19)
            .is_err()
    );
    let mut overlap = transfers;
    overlap[1].movement.from = overlap[0].movement.from;
    overlap[3].movement.to = overlap[2].movement.to;
    overlap[4].movement.to = Some(overlap[5].movement.from);
    for seed in [1, 13, 27] {
        let run_mixed = |workers, reverse| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut rows = overlap.clone();
                    if reverse {
                        rows.reverse();
                    }
                    let actions: Vec<_> = rows
                        .iter()
                        .map(|row| Transaction {
                            id: row.action,
                            movements: vec![row.movement],
                            creations: vec![],
                        })
                        .collect();
                    let mut flat = Runtime::compile(snapshot.clone()).unwrap();
                    let mut general = Runtime::compile(snapshot.clone()).unwrap();
                    let before = flat.snapshot();
                    let draft = flat
                        .prepare_partitioned_flat_transfers(&rows, seed)
                        .unwrap()
                        .unwrap();
                    assert_eq!(flat.snapshot(), before);
                    let other = general.prepare_advance(&[], &actions, seed).unwrap();
                    assert!(
                        draft.events() == other.events(),
                        "partitioned and general receipts differ"
                    );
                    let events = flat.commit_advance(draft).unwrap();
                    general.commit_advance(other).unwrap();
                    assert_eq!(flat.snapshot(), general.snapshot());
                    (flat.snapshot(), events)
                })
        };
        assert_eq!(run_mixed(1, false), run_mixed(4, true));
    }
    assert!(
        Runtime::compile(snapshot)
            .unwrap()
            .prepare_flat_transfers(&overlap)
            .unwrap()
            .is_none()
    );
}

#[test]
fn shared_source_flat_runtime_matches_general_arbitration() {
    use game_lots::{Movement, SimpleTransfer};
    let source = Place::Person(1);
    let mut snapshot = runtime().snapshot();
    let mut containers = vec![
        Container::new(
            source,
            400,
            vec![
                Lot {
                    item: None,
                    good: 1,
                    quantity: 80,
                    expires_at: Some(1),
                },
                Lot {
                    item: None,
                    good: 1,
                    quantity: 120,
                    expires_at: Some(3),
                },
                Lot {
                    item: None,
                    good: 1,
                    quantity: 50,
                    expires_at: None,
                },
            ],
        )
        .unwrap(),
    ];
    containers.extend((1..=256).map(|id| {
        Container::new(
            Place::Person(id + 1000),
            if id % 7 == 0 { 1 } else { 5 },
            vec![],
        )
        .unwrap()
    }));
    containers.sort_unstable_by_key(|row| row.place);
    snapshot.custody.containers = containers;
    let transfers: Vec<_> = (1..=256)
        .map(|id| SimpleTransfer {
            action: id * 3,
            movement: Movement {
                item: None,
                id: 9000 - id,
                from: source,
                to: Some(Place::Person(id + 1000)),
                good: 1,
                quantity: 1 + id % 3,
            },
        })
        .collect();
    for seed in [1, 13, 27] {
        let run = |workers, reverse| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut rows = transfers.clone();
                    if reverse {
                        rows.reverse();
                    }
                    let actions: Vec<_> = rows
                        .iter()
                        .map(|row| Transaction {
                            id: row.action,
                            movements: vec![row.movement],
                            creations: vec![],
                        })
                        .collect();
                    let mut flat = Runtime::compile(snapshot.clone()).unwrap();
                    let mut general = Runtime::compile(snapshot.clone()).unwrap();
                    let before = flat.snapshot();
                    let draft = flat
                        .prepare_partitioned_flat_transfers(&rows, seed)
                        .unwrap()
                        .unwrap();
                    let stale = flat
                        .prepare_partitioned_flat_transfers(&rows, seed)
                        .unwrap()
                        .unwrap();
                    assert_eq!(flat.snapshot(), before);
                    let reference = general.prepare_advance(&[], &actions, seed).unwrap();
                    assert_eq!(draft.events(), reference.events());
                    let events = flat.commit_advance(draft).unwrap();
                    assert!(flat.commit_advance(stale).is_err());
                    general.commit_advance(reference).unwrap();
                    assert_eq!(flat.snapshot(), general.snapshot());
                    (flat.snapshot(), events)
                })
        };
        assert_eq!(run(1, false), run(4, true));
    }
}

#[test]
fn shared_destination_flat_runtime_matches_general_arbitration() {
    use game_lots::{Movement, SimpleTransfer};
    let destination = Place::Person(1000);
    let mut snapshot = runtime().snapshot();
    let mut containers: Vec<_> = (1..=256)
        .map(|id| {
            Container::new(
                Place::Person(id),
                10,
                vec![
                    Lot {
                        item: None,
                        good: 1,
                        quantity: 1,
                        expires_at: Some(1),
                    },
                    Lot {
                        item: None,
                        good: 1,
                        quantity: if id % 9 == 0 { 1 } else { 3 },
                        expires_at: Some(3 + id % 2),
                    },
                ],
            )
            .unwrap()
        })
        .collect();
    containers.push(
        Container::new(
            destination,
            130,
            vec![Lot {
                item: None,
                good: 1,
                quantity: 20,
                expires_at: Some(1),
            }],
        )
        .unwrap(),
    );
    containers.sort_unstable_by_key(|row| row.place);
    snapshot.custody.containers = containers;
    let transfers: Vec<_> = (1..=256)
        .map(|id| SimpleTransfer {
            action: id * 3,
            movement: Movement {
                item: None,
                id: 9000 - id,
                from: Place::Person(id),
                to: Some(destination),
                good: 1,
                quantity: 1 + id % 3,
            },
        })
        .collect();
    for seed in [1, 13, 27] {
        let run = |workers, reverse, capacity| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut rows = transfers.clone();
                    if reverse {
                        rows.reverse();
                    }
                    let actions: Vec<_> = rows
                        .iter()
                        .map(|row| Transaction {
                            id: row.action,
                            movements: vec![row.movement],
                            creations: vec![],
                        })
                        .collect();
                    let mut initial = snapshot.clone();
                    initial
                        .custody
                        .containers
                        .iter_mut()
                        .find(|row| row.place == destination)
                        .unwrap()
                        .capacity = capacity;
                    let mut flat = Runtime::compile(initial.clone()).unwrap();
                    let mut general = Runtime::compile(initial).unwrap();
                    let before = flat.snapshot();
                    let draft = flat
                        .prepare_partitioned_flat_transfers(&rows, seed)
                        .unwrap()
                        .unwrap();
                    let stale = flat
                        .prepare_partitioned_flat_transfers(&rows, seed)
                        .unwrap()
                        .unwrap();
                    assert_eq!(flat.snapshot(), before);
                    let reference = general.prepare_advance(&[], &actions, seed).unwrap();
                    assert_eq!(draft.events(), reference.events());
                    let events = flat.commit_advance(draft).unwrap();
                    assert!(flat.commit_advance(stale).is_err());
                    general.commit_advance(reference).unwrap();
                    assert_eq!(flat.snapshot(), general.snapshot());
                    (flat.snapshot(), events)
                })
        };
        for capacity in [130, 1000] {
            assert_eq!(run(1, false, capacity), run(4, true, capacity));
        }
    }
}

#[test]
fn grouped_sources_match_general_runtime_with_expiry_and_capacity() {
    use game_lots::{Movement, SimpleTransfer};
    let mut snapshot = runtime().snapshot();
    let mut containers = Vec::new();
    for id in 1..=8 {
        let good = if id % 2 == 0 { 2 } else { 1 };
        containers.push(
            Container::new(
                Place::Inventory(id),
                150,
                vec![
                    Lot {
                        item: None,
                        good,
                        quantity: 15,
                        expires_at: Some(1),
                    },
                    Lot {
                        item: None,
                        good,
                        quantity: 40,
                        expires_at: Some(3),
                    },
                    Lot {
                        item: None,
                        good,
                        quantity: 30,
                        expires_at: None,
                    },
                ],
            )
            .unwrap(),
        );
    }
    containers.extend((1..=512).map(|id| {
        Container::new(
            Place::Person(id),
            if id % 7 == 0 { 1 } else { 6 },
            if id % 13 == 0 {
                vec![Lot {
                    item: None,
                    good: 1,
                    quantity: 1,
                    expires_at: Some(1),
                }]
            } else {
                vec![]
            },
        )
        .unwrap()
    }));
    containers.push(
        Container::new(
            Place::Person(9999),
            10,
            vec![Lot {
                item: None,
                good: 2,
                quantity: 2,
                expires_at: Some(1),
            }],
        )
        .unwrap(),
    );
    containers.sort_unstable_by_key(|row| row.place);
    snapshot.custody.containers = containers;
    let transfers: Vec<_> = (1..=512)
        .map(|id| {
            let source = (id % 8) + 1;
            SimpleTransfer {
                action: id * 3,
                movement: Movement {
                    item: None,
                    id: 9000 - id,
                    from: Place::Inventory(source),
                    to: Some(Place::Person(id)),
                    good: if source % 2 == 0 { 2 } else { 1 },
                    quantity: 1 + id % 3,
                },
            }
        })
        .collect();
    for seed in [1, 13, 27] {
        let run = |workers, reverse| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut rows = transfers.clone();
                    if reverse {
                        rows.reverse();
                    }
                    let actions: Vec<_> = rows
                        .iter()
                        .map(|row| Transaction {
                            id: row.action,
                            movements: vec![row.movement],
                            creations: vec![],
                        })
                        .collect();
                    let mut flat = Runtime::compile(snapshot.clone()).unwrap();
                    let mut general = Runtime::compile(snapshot.clone()).unwrap();
                    let before = flat.snapshot();
                    let draft = flat
                        .prepare_partitioned_flat_transfers(&rows, seed)
                        .unwrap()
                        .unwrap();
                    let stale = flat
                        .prepare_partitioned_flat_transfers(&rows, seed)
                        .unwrap()
                        .unwrap();
                    assert_eq!(flat.snapshot(), before);
                    let reference = general.prepare_advance(&[], &actions, seed).unwrap();
                    assert_eq!(draft.events(), reference.events());
                    let events = flat.commit_advance(draft).unwrap();
                    assert!(flat.commit_advance(stale).is_err());
                    general.commit_advance(reference).unwrap();
                    assert_eq!(flat.snapshot(), general.snapshot());
                    (flat.snapshot(), events)
                })
        };
        assert_eq!(run(1, false), run(4, true));
    }
}

#[test]
fn packed_receipts_outlive_the_runtime_and_can_be_owned() {
    use game_lots::{Movement, SimpleTransfer};
    let mut snapshot = runtime().snapshot();
    let mut containers = Vec::new();
    let mut transfers = Vec::new();
    for id in 1..=128 {
        containers.push(
            Container::new(
                Place::Person(id),
                10,
                vec![
                    Lot {
                        item: None,
                        good: 1,
                        quantity: 1,
                        expires_at: Some(2),
                    },
                    Lot {
                        item: None,
                        good: 1,
                        quantity: 2,
                        expires_at: Some(4),
                    },
                ],
            )
            .unwrap(),
        );
        containers.push(Container::new(Place::Person(id + 1000), 10, vec![]).unwrap());
        transfers.push(SimpleTransfer {
            action: id,
            movement: Movement {
                item: None,
                id,
                from: Place::Person(id),
                to: Some(Place::Person(id + 1000)),
                good: 1,
                quantity: 2,
            },
        });
    }
    containers.sort_unstable_by_key(|row| row.place);
    snapshot.custody.containers = containers;
    let events = {
        let mut runtime = Runtime::compile(snapshot).unwrap();
        let draft = runtime.prepare_flat_transfers(&transfers).unwrap().unwrap();
        runtime.commit_advance(draft).unwrap()
    };
    assert_eq!(events.physical_movements.len(), 128);
    for receipt in &events.physical_movements {
        assert!(receipt.movement.accepted);
        assert_eq!(
            receipt.movement.lots,
            &[
                Lot {
                    item: None,
                    good: 1,
                    quantity: 1,
                    expires_at: Some(2)
                },
                Lot {
                    item: None,
                    good: 1,
                    quantity: 1,
                    expires_at: Some(4)
                },
            ]
        );
    }
    let owned = events.physical_movements.into_owned();
    assert_eq!(owned.len(), 128);
    assert_eq!(owned[0].movement.lots.as_slice()[0].expires_at, Some(2));
}

#[test]
fn ordered_single_leg_receipts_match_general_merge_with_other_work() {
    use game_lots::Movement;
    let actions: Vec<_> = (1..=2300)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                item: None,
                id: 10_000 - id,
                from: Place::Stock { region: 1, good: 1 },
                to: None,
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        })
        .collect();
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
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut actions = actions.clone();
                if reverse {
                    actions.reverse();
                }
                let mut world = Runtime::compile(snapshot.clone()).unwrap();
                let events = world.advance_with_actions(&[(1, 2)], &actions, 19).unwrap();
                assert_eq!(events.physical_movements.len(), actions.len());
                for receipt in &events.physical_movements {
                    assert_eq!(receipt.request.id, 10_000 - receipt.action);
                    assert_eq!(receipt.movement.id, receipt.request.id);
                    assert_eq!(
                        receipt.movement.accepted,
                        events.physical_actions[(receipt.action - 1) as usize].accepted
                    );
                }
                (world.snapshot(), events)
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
fn detached_generation_rolls_back_and_abandonment_invalidates_it() {
    use game_lots::{Movement, Transaction};
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
            item: None,
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
    assert_eq!(
        draft
            .events()
            .physical_movements
            .iter()
            .next()
            .unwrap()
            .movement
            .id,
        12
    );
    assert_eq!(runtime.snapshot(), before);
    runtime.abandon(1, 1).unwrap();
    let abandoned = runtime.snapshot();
    assert!(runtime.validate_advance(&draft).is_err());
    assert!(runtime.commit_advance(draft).is_err());
    assert_eq!(runtime.snapshot(), abandoned);
}

#[test]
fn starts_and_other_actions_arbitrate_together_and_rejected_custody_disappears() {
    use game_lots::{Movement, Transaction};
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
            item: None,
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
    use game_lots::{Movement, Transaction};
    use game_production::{Inventory, Production};
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
    game_content_import::augment(
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
            item: None,
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
            item: None,
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

#[test]
fn renewable_products_and_hunts_use_distinct_atomic_animal_claims() {
    use game_lot_content::{AnimalProduct, AnimalYield};
    let mut snapshot = runtime().snapshot();
    snapshot
        .custody
        .containers
        .push(Container::new(Place::Person(1), 3, vec![]).unwrap());
    snapshot
        .custody
        .containers
        .push(Container::new(Place::Person(2), 3, vec![]).unwrap());
    let mut world = Runtime::compile(snapshot).unwrap();
    let hunt = AnimalYield {
        action: 10,
        animal: 3,
        hunter: 1,
        good: 1,
        quantity: 1,
    };
    let product = AnimalProduct {
        action: 11,
        animal: 4,
        collector: 2,
        good: 1,
        quantity: 1,
    };
    let before = world.snapshot();
    for invalid in [
        AnimalProduct {
            animal: 3,
            ..product
        },
        AnimalProduct {
            collector: 1,
            ..product
        },
        AnimalProduct {
            action: 10,
            ..product
        },
    ] {
        assert!(
            world
                .prepare_with_growth(
                    &[],
                    &[],
                    &[],
                    &[],
                    Growth {
                        harvest: &[],
                        animal_yield: &[hunt],
                        animal_products: &[invalid],
                        writing: &[],
                        compact_consumption: &[]
                    },
                    7
                )
                .is_err()
        );
        assert_eq!(world.snapshot(), before);
    }
    let draft = world
        .prepare_with_growth(
            &[],
            &[],
            &[],
            &[],
            Growth {
                harvest: &[],
                animal_yield: &[hunt],
                animal_products: &[product],
                writing: &[],
                compact_consumption: &[],
            },
            7,
        )
        .unwrap();
    assert_eq!(
        draft
            .events()
            .physical_actions
            .iter()
            .filter(|e| e.accepted && (e.id == 10 || e.id == 11))
            .count(),
        2
    );
    world.commit_advance(draft).unwrap();
    let after = world.snapshot();
    assert_eq!(
        after
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(1))
            .unwrap()
            .used,
        1
    );
    assert_eq!(
        after
            .custody
            .containers
            .iter()
            .find(|c| c.place == Place::Person(2))
            .unwrap()
            .used,
        1
    );
}

#[test]
fn identified_job_outputs_avoid_simultaneous_written_item_and_respect_stale_drafts() {
    let mut initial = runtime().snapshot();
    for id in [3, 4, 5] {
        initial.goods.push(Good {
            id,
            name: format!("good-{id}"),
            food: None,
        });
    }
    initial.recipes[0].outputs[0].quantity = 2;
    initial.custody.containers.push(
        Container::new(
            Place::Person(1),
            10,
            vec![
                Lot {
                    good: 3,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                },
                Lot {
                    good: 5,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                },
            ],
        )
        .unwrap(),
    );
    let mut world = Runtime::compile(initial).unwrap();
    world
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
    let page = WritingPage {
        action: 1,
        writer: 1,
        paper: 3,
        paper_quantity: 1,
        ink: 5,
        ink_quantity: 1,
        page: 4,
        item: 100,
    };
    let prepare = |world: &Runtime| {
        world
            .prepare_with_identified_production(
                &[(1, 2)],
                &[],
                &[],
                &[],
                (
                    Growth {
                        writing: &[page],
                        ..Growth::default()
                    },
                    &[],
                ),
                1,
                Some(IdentifiedProduction {
                    goods: &[2],
                    next_item: 1,
                }),
            )
            .unwrap()
    };
    let unchanged = world.snapshot();
    assert!(
        world
            .prepare_with_identified_production(
                &[(1, 2)],
                &[],
                &[],
                &[],
                (
                    Growth {
                        writing: &[page],
                        ..Growth::default()
                    },
                    &[]
                ),
                1,
                Some(IdentifiedProduction {
                    goods: &[2],
                    next_item: u64::MAX
                }),
            )
            .is_err()
    );
    assert_eq!(world.snapshot(), unchanged);
    let draft = prepare(&world);
    let stale = prepare(&world);
    assert_eq!(
        draft
            .produced_items()
            .iter()
            .map(|item| (item.job, item.item, item.good))
            .collect::<Vec<_>>(),
        vec![(1, 101, 2), (1, 102, 2)]
    );
    assert_eq!(draft.next_produced_item(), Some(103));
    assert_eq!(world.item_place(101), None);
    world.commit_advance(draft).unwrap();
    assert!(world.commit_advance(stale).is_err());
    assert_eq!(world.item_place(100), Some(Place::Person(1)));
    assert_eq!(world.item_place(101), Some(Place::JobOutput(1)));
    assert_eq!(world.item_place(102), Some(Place::JobOutput(1)));
}

#[test]
fn spoiled_job_inputs_do_not_allocate_or_publish_tool_identities() {
    let mut world = runtime();
    world
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
    let mut snapshot = world.snapshot();
    let input = snapshot
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(
        input.place,
        input.capacity,
        vec![Lot {
            good: 1,
            quantity: 2,
            item: None,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut world = Runtime::compile(snapshot).unwrap();
    let draft = world
        .prepare_with_identified_production(
            &[(1, 2)],
            &[],
            &[],
            &[],
            (Growth::default(), &[]),
            1,
            Some(IdentifiedProduction {
                goods: &[2],
                next_item: 1,
            }),
        )
        .unwrap();
    assert!(draft.produced_items().is_empty());
    assert_eq!(draft.next_produced_item(), Some(1));
    assert_eq!(draft.events().interrupted, vec![1]);
    world.commit_advance(draft).unwrap();
    assert_eq!(world.item_place(1), None);
}
