use game_lot_runtime::{Phase, Runtime, Snapshot};
use game_lots::{Container, Lot, Place, State};
use game_production_model::{Amount, FoodProfile, Good, Recipe, Start};
use std::collections::BTreeMap;

#[test]
fn interval_construction_is_identical_for_ordered_and_reversed_batches() {
    fn run(reverse: bool) -> Snapshot {
        let mut runtime = fixture(false, 1);
        runtime.begin_work_interval(&[(1, 1)]).unwrap();
        let mut actions: Vec<_> = (1..=2)
            .map(|id| game_lots::Transaction {
                id,
                movements: vec![game_lots::Movement {
                    id,
                    from: Place::Stock { region: 1, good: 1 },
                    to: Some(Place::Site(id + 10)),
                    good: 1,
                    quantity: 1,
                    item: None,
                }],
                creations: vec![],
            })
            .collect();
        let mut containers: Vec<_> = (1..=2)
            .map(|id| (id, Container::new(Place::Site(id + 10), 1, vec![]).unwrap()))
            .collect();
        if reverse {
            actions.reverse();
            containers.reverse();
        }
        let draft = runtime
            .prepare_interval_construction(&actions, &containers, 7)
            .unwrap();
        let result = runtime.commit_interval_consumption(draft).unwrap();
        assert!(result.transactions.iter().all(|row| row.accepted));
        runtime.snapshot()
    }
    assert_eq!(run(false), run(true));
}

#[test]
fn interval_fan_out_matches_ordinary_transactions_under_shared_stock_contention() {
    let mut initial = fixture(false, 1).snapshot();
    for id in 1..=300 {
        initial
            .custody
            .containers
            .push(Container::new(Place::Person(id), 1, vec![]).unwrap());
    }
    initial
        .custody
        .containers
        .sort_unstable_by_key(|container| container.place);
    let mut runtime = Runtime::compile(initial).unwrap();
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    let mut reference = game_lots::Store::compile(runtime.snapshot().custody).unwrap();
    let actions: Vec<_> = (1..=300)
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
    let ordinary = reference.prepare_transactions_now(7, &actions).unwrap();
    let draft = runtime.prepare_interval_transfer(&actions, 7).unwrap();
    assert_eq!(draft.transactions(), ordinary.transactions());
    let expected = reference.commit(ordinary).unwrap();
    let actual = runtime.commit_interval_consumption(draft).unwrap();
    assert_eq!(actual.movements, expected.0);
    assert_eq!(actual.spoiled, expected.1);
    assert_eq!(
        runtime.snapshot().custody.containers,
        reference.containers()
    );
}

#[test]
fn interval_construction_fan_out_matches_general_custody_staging() {
    let mut runtime = fixture(false, 1);
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    let mut reference = game_lots::Store::compile(runtime.snapshot().custody).unwrap();
    let actions: Vec<_> = (1..=300)
        .map(|id| game_lots::Transaction {
            id,
            movements: vec![game_lots::Movement {
                id,
                from: Place::Stock { region: 1, good: 1 },
                to: Some(Place::Site(id + 10)),
                good: 1,
                quantity: 1,
                item: None,
            }],
            creations: vec![],
        })
        .collect();
    let containers: Vec<_> = (1..=300)
        .map(|id| (id, Container::new(Place::Site(id + 10), 1, vec![]).unwrap()))
        .collect();
    let mut ordinary = reference
        .prepare_with_empty(
            7,
            &actions,
            containers
                .iter()
                .map(|(_, container)| container.clone())
                .collect(),
            false,
        )
        .unwrap();
    let draft = runtime
        .prepare_interval_construction(&actions, &containers, 7)
        .unwrap();
    assert_eq!(draft.transactions(), ordinary.transactions());
    let rejected: Vec<_> = ordinary
        .transactions()
        .iter()
        .filter(|row| !row.accepted)
        .map(|row| Place::Site(row.id + 10))
        .collect();
    reference.retire_in(&mut ordinary, rejected).unwrap();
    let expected = reference.commit(ordinary).unwrap();
    let actual = runtime.commit_interval_consumption(draft).unwrap();
    assert_eq!(actual.movements, expected.0);
    assert_eq!(actual.spoiled, expected.1);
    assert_eq!(
        runtime.snapshot().custody.containers,
        reference.containers()
    );
}

fn fixture(perishable: bool, output_quantity: u64) -> Runtime {
    let stock = |good, quantity| {
        Container::new(
            Place::Stock { region: 1, good },
            100,
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
            containers: vec![stock(1, 4), stock(2, 0)],
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
                food: perishable.then(|| FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: BTreeMap::new(),
                    health_effect: 0,
                    shelf_days: Some(1),
                }),
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
                quantity: output_quantity,
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
    runtime
}

#[test]
fn stopping_at_interval_opening_keeps_inputs_until_boundary() {
    let mut runtime = fixture(false, 1);
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    let before = runtime.snapshot();
    assert!(runtime.abandon_interval_work(&[(1, 2)]).is_err());
    assert_eq!(runtime.snapshot(), before);
    runtime.abandon_interval_work(&[(1, 1)]).unwrap();
    assert_eq!(runtime.work()[0].phase, Phase::Returning);
    assert_eq!(runtime.custody(Place::JobInput(1)).unwrap().used, 2);
    assert!(runtime.work_interval().unwrap().rates.is_empty());
    let saved = serde_json::to_string(&runtime.snapshot()).unwrap();
    let mut resumed = Runtime::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    resumed.progress_work_interval(1000).unwrap();
    resumed.settle_work_interval(7).unwrap();
    assert!(resumed.custody(Place::JobInput(1)).is_none());
    assert_eq!(
        resumed
            .custody(Place::Stock { region: 1, good: 1 })
            .unwrap()
            .used,
        4
    );
}
#[test]
fn early_completion_is_atomic_timed_and_resumable() {
    let run = |workers, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = fixture(false, 1);
                runtime.begin_work_interval(&[(1, 2)]).unwrap();
                assert_eq!(runtime.progress_work_interval(499).unwrap(), 499);
                assert!(runtime.prepare_early_work(7).unwrap().is_none());
                let saved = serde_json::to_string(&runtime.snapshot()).unwrap();
                runtime = Runtime::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for &chunk in chunks {
                    runtime.progress_work_interval(chunk).unwrap();
                }
                let draft = runtime.prepare_early_work(7).unwrap().unwrap();
                assert_eq!(
                    (draft.events().tick, draft.events().offset_millis),
                    (0, 500)
                );
                assert_eq!(draft.events().finished, vec![1]);
                assert_eq!(runtime.tick(), 0);
                assert_eq!(runtime.custody(Place::JobOutput(1)).unwrap().used, 0);
                let events = runtime.commit_early_work(draft).unwrap();
                assert_eq!(runtime.tick(), 0);
                assert_eq!(runtime.custody(Place::JobOutput(1)).unwrap().used, 1);
                assert!(runtime.custody(Place::JobInput(1)).is_none());
                assert_eq!(runtime.work()[0].phase, Phase::Waiting);
                assert!(runtime.work_interval().unwrap().rates.is_empty());
                let saved = serde_json::to_string(&runtime.snapshot()).unwrap();
                runtime = Runtime::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(runtime.prepare_early_work(7).unwrap().is_none());
                assert_eq!(runtime.progress_work_interval(500).unwrap(), 500);
                runtime.settle_work_interval(7).unwrap();
                assert_eq!(runtime.tick(), 1);
                assert_eq!(
                    runtime
                        .custody(Place::Stock { region: 1, good: 2 })
                        .unwrap()
                        .used,
                    1
                );
                (runtime.snapshot(), events)
            })
    };
    assert_eq!(run(1, &[1]), run(4, &[1]));
}

#[test]
fn starting_interval_work_invalidates_an_existing_completion_draft() {
    let mut runtime = fixture(false, 1);
    runtime.begin_work_interval(&[(1, 2)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let stale = runtime.prepare_early_work(7).unwrap().unwrap();
    let start = Start {
        id: 2,
        worker: 2,
        region: 1,
        recipe: 1,
        units: 1,
    };
    assert_eq!(
        runtime.start_interval_work(&[(start, 1)], 7).unwrap(),
        vec![(2, true)]
    );
    let before = runtime.snapshot();
    assert!(runtime.commit_early_work(stale).is_err());
    assert_eq!(runtime.snapshot(), before);
    let current = runtime.prepare_early_work(7).unwrap().unwrap();
    assert_eq!(
        runtime.commit_early_work(current).unwrap().finished,
        vec![1]
    );
    assert_eq!(runtime.work_interval().unwrap().rates, vec![(2, 1)]);
}

#[test]
fn abandoning_another_job_invalidates_a_completion_draft() {
    let mut runtime = fixture(false, 1);
    runtime
        .start(
            &[Start {
                id: 2,
                worker: 2,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            7,
        )
        .unwrap();
    runtime.begin_work_interval(&[(1, 2), (2, 1)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let stale = runtime.prepare_early_work(7).unwrap().unwrap();
    runtime.abandon_interval_work(&[(2, 2)]).unwrap();
    let before = runtime.snapshot();
    assert!(runtime.commit_early_work(stale).is_err());
    assert_eq!(runtime.snapshot(), before);
    assert_eq!(runtime.work_interval().unwrap().rates, vec![(1, 2)]);
}

#[test]
fn deferring_one_completion_invalidates_old_drafts_and_keeps_its_materials() {
    let mut runtime = fixture(false, 1);
    runtime
        .start(
            &[Start {
                id: 2,
                worker: 2,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            7,
        )
        .unwrap();
    runtime.begin_work_interval(&[(1, 2), (2, 2)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let stale = runtime.prepare_early_work(7).unwrap().unwrap();
    runtime.defer_interval_completions(&[2]).unwrap();
    let before = runtime.snapshot();
    assert!(runtime.commit_early_work(stale).is_err());
    assert_eq!(runtime.snapshot(), before);
    assert_eq!(runtime.custody(Place::JobInput(2)).unwrap().used, 2);
    let draft = runtime.prepare_early_work(7).unwrap().unwrap();
    assert_eq!(runtime.commit_early_work(draft).unwrap().finished, vec![1]);
    runtime.progress_work_interval(500).unwrap();
    assert_eq!(runtime.settle_work_interval(7).unwrap().finished, vec![2]);
}

#[test]
fn identified_boundary_settlement_stays_staged_until_owner_validation() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = fixture(false, 1);
                runtime.begin_work_interval(&[(1, 1)]).unwrap();
                runtime.progress_work_interval(1000).unwrap();
                let before = runtime.snapshot();
                let goods = [2];
                assert!(
                    runtime
                        .prepare_interval_settlement(
                            7,
                            Some(game_lot_runtime::IdentifiedProduction {
                                goods: &goods,
                                next_item: 0
                            })
                        )
                        .is_err()
                );
                assert!(
                    runtime
                        .prepare_interval_settlement(
                            7,
                            Some(game_lot_runtime::IdentifiedProduction {
                                goods: &goods,
                                next_item: u64::MAX
                            })
                        )
                        .is_err()
                );
                assert_eq!(runtime.snapshot(), before);
                let draft = runtime
                    .prepare_interval_settlement(
                        7,
                        Some(game_lot_runtime::IdentifiedProduction {
                            goods: &goods,
                            next_item: 10,
                        }),
                    )
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
                assert_eq!(draft.item_updates(), &[(10, Some(Place::JobOutput(1)))]);
                assert_eq!(runtime.snapshot(), before);
                // A host that cannot prepare its metadata can drop the draft and retry.
                drop(draft);
                let saved = serde_json::to_string(&runtime.snapshot()).unwrap();
                let mut runtime = Runtime::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                let stale = runtime
                    .prepare_interval_settlement(
                        7,
                        Some(game_lot_runtime::IdentifiedProduction {
                            goods: &goods,
                            next_item: 10,
                        }),
                    )
                    .unwrap();
                let draft = runtime
                    .prepare_interval_settlement(
                        7,
                        Some(game_lot_runtime::IdentifiedProduction {
                            goods: &goods,
                            next_item: 10,
                        }),
                    )
                    .unwrap();
                runtime.validate_interval_settlement(&draft).unwrap();
                assert_eq!(
                    runtime.commit_interval_settlement(draft).unwrap().finished,
                    vec![1]
                );
                assert_eq!(runtime.item_place(10), Some(Place::JobOutput(1)));
                let committed = runtime.snapshot();
                assert!(runtime.commit_interval_settlement(stale).is_err());
                assert_eq!(runtime.snapshot(), committed);
                assert_eq!(
                    Runtime::compile(committed.clone()).unwrap().item_place(10),
                    Some(Place::JobOutput(1))
                );
                committed
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn invalid_identified_boundary_output_keeps_interval_resumable() {
    let mut runtime = fixture(true, 1);
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    runtime.progress_work_interval(1000).unwrap();
    let before = runtime.snapshot();
    assert!(
        runtime
            .prepare_interval_settlement(
                7,
                Some(game_lot_runtime::IdentifiedProduction {
                    goods: &[2],
                    next_item: 10
                })
            )
            .is_err()
    );
    assert_eq!(runtime.snapshot(), before);
    let draft = runtime.prepare_interval_settlement(7, None).unwrap();
    assert_eq!(
        runtime.commit_interval_settlement(draft).unwrap().finished,
        vec![1]
    );
}

#[test]
fn staged_boundary_allocation_skips_existing_item_identities() {
    let mut snapshot = fixture(false, 1).snapshot();
    snapshot.custody.containers.push(
        Container::new(
            Place::Person(9),
            1,
            vec![Lot {
                item: Some(10),
                good: 2,
                quantity: 1,
                expires_at: None,
            }],
        )
        .unwrap(),
    );
    snapshot.custody.containers.sort_by_key(|row| row.place);
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    runtime.progress_work_interval(1000).unwrap();
    let draft = runtime
        .prepare_interval_settlement(
            7,
            Some(game_lot_runtime::IdentifiedProduction {
                goods: &[2],
                next_item: 10,
            }),
        )
        .unwrap();
    assert_eq!(draft.produced_items()[0].item, 11);
    assert_eq!(draft.next_produced_item(), Some(12));
    runtime.commit_interval_settlement(draft).unwrap();
    assert_eq!(runtime.item_place(10), Some(Place::Person(9)));
    assert_eq!(runtime.item_place(11), Some(Place::JobOutput(1)));
}

#[test]
fn rejected_identified_output_does_not_create_item_metadata() {
    let mut snapshot = fixture(false, 2).snapshot();
    let output = snapshot
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobOutput(1))
        .unwrap();
    *output = Container::new(output.place, 1, vec![]).unwrap();
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    runtime.progress_work_interval(1000).unwrap();
    let draft = runtime
        .prepare_interval_settlement(
            7,
            Some(game_lot_runtime::IdentifiedProduction {
                goods: &[2],
                next_item: 10,
            }),
        )
        .unwrap();
    assert!(draft.produced_items().is_empty());
    assert!(draft.item_updates().is_empty());
    assert_eq!(draft.next_produced_item(), Some(12));
    runtime.commit_interval_settlement(draft).unwrap();
    assert_eq!(runtime.item_place(10), None);
    assert_eq!(runtime.item_place(11), None);
}

#[test]
fn rejected_identified_early_output_does_not_publish_item_metadata() {
    let mut snapshot = fixture(false, 2).snapshot();
    let output = snapshot
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobOutput(1))
        .unwrap();
    *output = Container::new(output.place, 1, vec![]).unwrap();
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime.begin_work_interval(&[(1, 2)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let draft = runtime
        .prepare_early_work_with_identified(
            7,
            Some(game_lot_runtime::IdentifiedProduction {
                goods: &[2],
                next_item: 10,
            }),
        )
        .unwrap()
        .unwrap();
    assert!(draft.produced_items().is_empty());
    assert!(draft.item_updates().is_empty());
    assert_eq!(draft.next_produced_item(), Some(12));
    assert_eq!(draft.events().rejected, vec![1]);
    runtime.commit_early_work(draft).unwrap();
    assert_eq!(runtime.item_place(10), None);
    assert_eq!(runtime.item_place(11), None);
}

#[test]
fn early_identified_multiunit_output_assigns_unique_operations_and_items() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = fixture(false, 2);
                runtime.begin_work_interval(&[(1, 2)]).unwrap();
                runtime.progress_work_interval(500).unwrap();
                let draft = runtime
                    .prepare_early_work_with_identified(
                        7,
                        Some(game_lot_runtime::IdentifiedProduction {
                            goods: &[2],
                            next_item: 10,
                        }),
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    draft
                        .produced_items()
                        .iter()
                        .map(|item| item.item)
                        .collect::<Vec<_>>(),
                    vec![10, 11]
                );
                assert_eq!(draft.next_produced_item(), Some(12));
                assert_eq!(runtime.commit_early_work(draft).unwrap().finished, vec![1]);
                assert_eq!(runtime.item_place(10), Some(Place::JobOutput(1)));
                assert_eq!(runtime.item_place(11), Some(Place::JobOutput(1)));
                runtime.snapshot()
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn interval_transfer_invalidates_completion_drafts_and_keeps_work_live() {
    let mut snapshot = fixture(false, 1).snapshot();
    snapshot
        .custody
        .containers
        .push(Container::new(Place::Person(2), 2, vec![]).unwrap());
    snapshot.custody.containers.sort_by_key(|row| row.place);
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime.begin_work_interval(&[(1, 2)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let stale = runtime.prepare_early_work(7).unwrap().unwrap();
    let action = game_lots::Transaction {
        id: 1,
        movements: vec![game_lots::Movement {
            id: 1,
            from: Place::Stock { region: 1, good: 1 },
            to: Some(Place::Person(2)),
            good: 1,
            quantity: 1,
            item: None,
        }],
        creations: vec![],
    };
    let mut invalid = action.clone();
    invalid.movements[0].from = Place::JobInput(1);
    let before = runtime.snapshot();
    assert!(runtime.transfer_interval(&[invalid], 7).is_err());
    assert_eq!(runtime.snapshot(), before);
    assert!(
        runtime
            .transfer_interval(&[action], 7)
            .unwrap()
            .transactions[0]
            .accepted
    );
    let before = runtime.snapshot();
    assert!(runtime.commit_early_work(stale).is_err());
    assert_eq!(runtime.snapshot(), before);
    let draft = runtime.prepare_early_work(7).unwrap().unwrap();
    assert_eq!(runtime.commit_early_work(draft).unwrap().finished, vec![1]);
    assert_eq!(runtime.custody(Place::Person(2)).unwrap().used, 1);
}

#[test]
fn interval_consumption_draft_expires_when_time_moves_without_work_rates() {
    let mut snapshot = fixture(false, 1).snapshot();
    snapshot.custody.containers.push(
        Container::new(
            Place::Person(2),
            2,
            vec![Lot {
                item: None,
                good: 1,
                quantity: 1,
                expires_at: None,
            }],
        )
        .unwrap(),
    );
    snapshot.custody.containers.sort_by_key(|row| row.place);
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime.begin_work_interval(&[]).unwrap();
    runtime.progress_work_interval(200).unwrap();
    let action = game_lots::Transaction {
        id: 1,
        movements: vec![game_lots::Movement {
            id: 1,
            from: Place::Person(2),
            to: None,
            good: 1,
            quantity: 1,
            item: None,
        }],
        creations: vec![],
    };
    let draft = runtime.prepare_interval_consumption(&[action], 7).unwrap();
    runtime.progress_work_interval(1).unwrap();
    let before = runtime.snapshot();
    assert!(runtime.commit_interval_consumption(draft).is_err());
    assert_eq!(runtime.snapshot(), before);
    assert_eq!(runtime.custody(Place::Person(2)).unwrap().used, 1);
}

#[test]
fn rejected_interval_starts_retire_both_new_holders() {
    let mut runtime = fixture(false, 1);
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    runtime.progress_work_interval(400).unwrap();
    let requests: Vec<_> = (2..=257)
        .map(|id| {
            (
                Start {
                    id,
                    worker: id,
                    region: 1,
                    recipe: 1,
                    units: 1,
                },
                1,
            )
        })
        .collect();
    let outcomes = runtime.start_interval_work(&requests, 7).unwrap();
    assert_eq!(outcomes.iter().filter(|row| row.1).count(), 1);
    for (id, accepted) in outcomes {
        assert_eq!(runtime.custody(Place::JobInput(id)).is_some(), accepted);
        assert_eq!(runtime.custody(Place::JobOutput(id)).is_some(), accepted);
    }
    let saved = runtime.snapshot();
    assert_eq!(Runtime::compile(saved.clone()).unwrap().snapshot(), saved);
}

#[test]
fn packed_interval_starts_fall_back_for_identified_source_items() {
    let mut snapshot = fixture(false, 1).snapshot();
    let source = snapshot
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *source = Container::new(
        source.place,
        100,
        vec![
            Lot {
                item: Some(41),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
            Lot {
                item: Some(42),
                good: 1,
                quantity: 1,
                expires_at: None,
            },
        ],
    )
    .unwrap();
    let mut runtime = Runtime::compile(snapshot).unwrap();
    runtime.begin_work_interval(&[(1, 1)]).unwrap();
    runtime.progress_work_interval(400).unwrap();
    let requests: Vec<_> = (2..=257)
        .map(|id| {
            (
                Start {
                    id,
                    worker: id,
                    region: 1,
                    recipe: 1,
                    units: 1,
                },
                1,
            )
        })
        .collect();
    let outcomes = runtime.start_interval_work(&requests, 7).unwrap();
    let accepted: Vec<_> = outcomes
        .iter()
        .filter(|row| row.1)
        .map(|row| row.0)
        .collect();
    assert_eq!(accepted.len(), 1);
    let input = runtime.custody(Place::JobInput(accepted[0])).unwrap();
    assert_eq!(
        input
            .lots
            .iter()
            .filter_map(|lot| lot.item)
            .collect::<Vec<_>>(),
        vec![41, 42]
    );
    let saved = runtime.snapshot();
    assert_eq!(Runtime::compile(saved.clone()).unwrap().snapshot(), saved);
}
#[test]
fn early_completions_preserve_other_jobs_and_the_quantum_boundary() {
    let mut runtime = fixture(false, 1);
    runtime
        .start(
            &[Start {
                id: 2,
                worker: 2,
                region: 1,
                recipe: 1,
                units: 1,
            }],
            7,
        )
        .unwrap();
    runtime.begin_work_interval(&[(1, 2), (2, 1)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let draft = runtime.prepare_early_work(7).unwrap().unwrap();
    assert_eq!(draft.events().finished, vec![1]);
    runtime.commit_early_work(draft).unwrap();
    assert_eq!(runtime.work_interval().unwrap().rates, vec![(2, 1)]);
    assert_eq!(runtime.work()[1].job.remaining, 1);
    assert_eq!(runtime.work()[1].progress_remainder, 500);
    assert_eq!(runtime.progress_work_interval(500).unwrap(), 500);
    let events = runtime.settle_work_interval(7).unwrap();
    assert_eq!(events.finished, vec![2]);
    assert_eq!(runtime.tick(), 1);
    assert_eq!(
        runtime
            .custody(Place::Stock { region: 1, good: 2 })
            .unwrap()
            .used,
        1
    );
    assert_eq!(runtime.custody(Place::JobOutput(2)).unwrap().used, 1);
}

#[test]
fn stale_and_unsupported_early_completions_leave_state_unchanged() {
    let mut runtime = fixture(false, 1);
    runtime.begin_work_interval(&[(1, 2)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let draft = runtime.prepare_early_work(7).unwrap().unwrap();
    runtime.progress_work_interval(1).unwrap();
    let before = runtime.snapshot();
    assert!(runtime.commit_early_work(draft).is_err());
    assert_eq!(runtime.snapshot(), before);
    let draft = runtime.prepare_early_work(7).unwrap().unwrap();
    let competing = runtime.prepare_early_work(7).unwrap().unwrap();
    runtime.commit_early_work(draft).unwrap();
    let before = runtime.snapshot();
    assert!(runtime.commit_early_work(competing).is_err());
    assert_eq!(runtime.snapshot(), before);
    assert!(runtime.prepare_early_work(7).unwrap().is_none());
    let mut perishable = fixture(true, 1);
    perishable.begin_work_interval(&[(1, 2)]).unwrap();
    perishable.progress_work_interval(500).unwrap();
    let before = perishable.snapshot();
    assert!(perishable.prepare_early_work(7).is_err());
    assert_eq!(perishable.snapshot(), before);
    let mut overflow = fixture(false, 1).snapshot();
    overflow.custody.revision = u64::MAX;
    let mut overflow = Runtime::compile(overflow).unwrap();
    overflow.begin_work_interval(&[(1, 2)]).unwrap();
    overflow.progress_work_interval(500).unwrap();
    let before = overflow.snapshot();
    assert!(overflow.prepare_early_work(7).is_err());
    assert_eq!(overflow.snapshot(), before);
}

#[test]
fn early_completion_can_use_input_expiring_at_the_next_boundary() {
    let mut state = fixture(false, 1).snapshot();
    state.goods[0].food = Some(FoodProfile {
        portion: 1,
        hunger_relief: 1,
        nutrients: BTreeMap::new(),
        health_effect: 0,
        shelf_days: Some(1),
    });
    let input = state
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(
        input.place,
        input.capacity,
        vec![Lot {
            item: None,
            good: 1,
            quantity: 2,
            expires_at: Some(1),
        }],
    )
    .unwrap();
    let mut runtime = Runtime::compile(state).unwrap();
    runtime.begin_work_interval(&[(1, 2)]).unwrap();
    runtime.progress_work_interval(500).unwrap();
    let draft = runtime.prepare_early_work(7).unwrap().unwrap();
    assert_eq!(draft.events().finished, vec![1]);
    runtime.commit_early_work(draft).unwrap();
    assert_eq!(runtime.custody(Place::JobOutput(1)).unwrap().used, 1);
}

#[test]
fn expired_lots_are_rejected_before_early_work() {
    let mut state = fixture(false, 1).snapshot();
    let stock = state
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            item: None,
            good: 2,
            quantity: 1,
            expires_at: Some(0),
        }],
    )
    .unwrap();
    assert!(Runtime::compile(state).is_err());
}
#[test]
fn early_completion_interruption_and_capacity_rejection_do_not_duplicate_goods() {
    let mut missing = fixture(false, 1).snapshot();
    let input = missing
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobInput(1))
        .unwrap();
    *input = Container::new(input.place, input.capacity, vec![]).unwrap();
    let mut missing = Runtime::compile(missing).unwrap();
    missing.begin_work_interval(&[(1, 2)]).unwrap();
    missing.progress_work_interval(500).unwrap();
    let draft = missing.prepare_early_work(7).unwrap().unwrap();
    assert_eq!(draft.events().interrupted, vec![1]);
    missing.commit_early_work(draft).unwrap();
    assert_eq!(missing.work()[0].phase, Phase::Returning);
    assert!(missing.work_interval().unwrap().rates.is_empty());
    assert_eq!(missing.custody(Place::JobOutput(1)).unwrap().used, 0);
    let mut full = fixture(false, 2).snapshot();
    let output = full
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::JobOutput(1))
        .unwrap();
    *output = Container::new(output.place, 1, vec![]).unwrap();
    let mut full = Runtime::compile(full).unwrap();
    full.begin_work_interval(&[(1, 2)]).unwrap();
    full.progress_work_interval(500).unwrap();
    let draft = full.prepare_early_work(7).unwrap().unwrap();
    assert_eq!(draft.events().rejected, vec![1]);
    full.commit_early_work(draft).unwrap();
    assert_eq!(full.work()[0].phase, Phase::Working);
    assert!(full.work_interval().unwrap().rates.is_empty());
    assert!(full.prepare_early_work(7).unwrap().is_none());
    assert_eq!(full.progress_work_interval(500).unwrap(), 500);
    assert_eq!(full.custody(Place::JobInput(1)).unwrap().used, 2);
    assert_eq!(full.custody(Place::JobOutput(1)).unwrap().used, 0);
}

#[test]
#[ignore = "manual early work completion benchmark"]
fn early_work_completion_benchmark() {
    use std::hash::{Hash, Hasher};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_JOBS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(50_000u64);
    let mut base = fixture(false, 1).snapshot();
    base.work.clear();
    base.custody
        .containers
        .retain(|row| !matches!(row.place, Place::JobInput(_) | Place::JobOutput(_)));
    let input = base
        .custody
        .containers
        .iter_mut()
        .find(|row| row.place == Place::Stock { region: 1, good: 1 })
        .unwrap();
    *input = Container::new(
        input.place,
        count * 2,
        vec![Lot {
            item: None,
            good: 1,
            quantity: count * 2,
            expires_at: None,
        }],
    )
    .unwrap();
    let starts: Vec<_> = (1..=count)
        .map(|id| Start {
            id,
            worker: id,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    let rates: Vec<_> = (1..=count).map(|id| (id, 2)).collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut runtime = Runtime::compile(base.clone()).unwrap();
                assert!(runtime.start(&starts, 7).unwrap().iter().all(|row| row.1));
                runtime.begin_work_interval(&rates).unwrap();
                runtime.progress_work_interval(500).unwrap();
                let started = std::time::Instant::now();
                let draft = runtime.prepare_early_work(7).unwrap().unwrap();
                let events = runtime.commit_early_work(draft).unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                assert_eq!(events.finished.len(), count as usize);
                assert!(events.interrupted.is_empty() && events.rejected.is_empty());
                if round == 3 {
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    serde_json::to_vec(&runtime.snapshot())
                        .unwrap()
                        .hash(&mut hash);
                    println!("fingerprint={:016x}", hash.finish());
                }
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!("workers={workers} jobs={count} elapsed={:.6}", samples[1]);
            #[cfg(feature = "profile")]
            if std::env::var_os("LEGACY_PROFILE").is_some() {
                for (name, measurement) in game_diagnostics::snapshot() {
                    if name.starts_with("physical.early.") {
                        println!(
                            "profile {name} calls={} mean_ms={:.3}",
                            measurement.calls,
                            measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                        );
                    }
                }
            }
        });
}
