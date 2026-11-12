use game_lots::{Container, Lot, Movement, Place, State, Store, Transaction};

fn cohort(batch: u64, quantity: u64, expiry: Option<u64>) -> Lot {
    Lot {
        good: 1,
        quantity,
        batch: Some(batch),
        item: None,
        expires_at: expiry,
    }
}
fn transfer(id: u64, from: u64, to: u64, quantity: u64) -> Transaction {
    Transaction {
        id,
        movements: vec![Movement {
            batch: None,
            id,
            from: Place::Person(from),
            to: Some(Place::Person(to)),
            good: 1,
            quantity,
            item: None,
        }],
        creations: vec![],
    }
}

#[test]
fn generic_transfers_preserve_batch_through_split_merge_and_resume() {
    let mut store = Store::compile(State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                Place::Person(1),
                20,
                vec![cohort(1, 3, Some(4)), cohort(2, 5, Some(4))],
            )
            .unwrap(),
            Container::new(Place::Person(2), 20, vec![cohort(1, 1, Some(4))]).unwrap(),
        ],
    })
    .unwrap();
    let draft = store
        .prepare_transactions_now(7, &[transfer(1, 1, 2, 5)])
        .unwrap();
    assert!(draft.transactions()[0].accepted);
    assert_eq!(
        draft.outcomes()[0]
            .lots
            .iter()
            .map(|row| (row.batch, row.quantity))
            .collect::<Vec<_>>(),
        vec![(Some(1), 3), (Some(2), 2)]
    );
    store.commit(draft).unwrap();
    let state = store.export();
    assert_eq!(state.containers[0].lots, vec![cohort(2, 3, Some(4))]);
    assert_eq!(
        state.containers[1].lots,
        vec![cohort(1, 4, Some(4)), cohort(2, 2, Some(4))]
    );
    let restored: State = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(Store::compile(restored).unwrap().export(), state);
}

#[test]
fn rejected_transfer_and_expiry_do_not_reassign_batches() {
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                Place::Person(1),
                10,
                vec![cohort(1, 3, Some(1)), cohort(2, 4, Some(3))],
            )
            .unwrap(),
            Container::new(Place::Person(2), 1, vec![]).unwrap(),
        ],
    };
    let mut store = Store::compile(initial.clone()).unwrap();
    let draft = store
        .prepare_transactions_now(7, &[transfer(1, 1, 2, 3)])
        .unwrap();
    assert!(!draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    assert_eq!(store.export().containers, initial.containers);
    let draft = store
        .prepare_transactions(7, &[transfer(2, 1, 2, 1)])
        .unwrap();
    assert_eq!(draft.outcomes()[0].lots[0].batch, Some(2));
    store.commit(draft).unwrap();
    assert_eq!(
        store.export().containers[0].lots,
        vec![cohort(2, 3, Some(3))]
    );
}

#[test]
fn shared_source_arbitration_keeps_actual_cohorts_with_each_winner() {
    fn run(workers: usize) -> State {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut store = Store::compile(State {
                    next_batch: None,
                    tick: 0,
                    revision: 0,
                    containers: vec![
                        Container::new(
                            Place::Person(1),
                            10,
                            vec![cohort(1, 2, Some(4)), cohort(2, 3, Some(4))],
                        )
                        .unwrap(),
                        Container::new(Place::Person(2), 10, vec![]).unwrap(),
                        Container::new(Place::Person(3), 10, vec![]).unwrap(),
                    ],
                })
                .unwrap();
                let draft = store
                    .prepare_transactions_now(7, &[transfer(1, 1, 2, 3), transfer(2, 1, 3, 3)])
                    .unwrap();
                assert_eq!(
                    draft
                        .transactions()
                        .iter()
                        .filter(|row| row.accepted)
                        .count(),
                    1
                );
                let outcome = draft
                    .outcomes()
                    .into_iter()
                    .find(|row| row.accepted)
                    .unwrap();
                assert_eq!(
                    outcome
                        .lots
                        .iter()
                        .map(|lot| (lot.batch, lot.quantity))
                        .collect::<Vec<_>>(),
                    vec![(Some(1), 2), (Some(2), 1)]
                );
                store.commit(draft).unwrap();
                let state = store.export();
                assert_eq!(
                    state
                        .containers
                        .iter()
                        .flat_map(|row| &row.lots)
                        .map(|lot| lot.quantity)
                        .sum::<u64>(),
                    5
                );
                state
            })
    }
    assert_eq!(run(1), run(4));
}

#[test]
fn invalid_batch_cannot_be_created_or_combined_with_a_unique_item() {
    let bad = Lot {
        batch: Some(0),
        ..cohort(1, 1, None)
    };
    assert!(Container::new(Place::Person(1), 10, vec![bad.clone()]).is_err());
    let mixed = Lot {
        item: Some(3),
        ..cohort(1, 1, None)
    };
    assert!(Container::new(Place::Person(1), 10, vec![mixed.clone()]).is_err());
    let store = Store::compile(State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![Container::new(Place::Person(1), 10, vec![]).unwrap()],
    })
    .unwrap();
    for (index, lot) in [bad, mixed].into_iter().enumerate() {
        let action = Transaction {
            id: index as u64 + 1,
            movements: vec![],
            creations: vec![game_lots::Creation {
                id: index as u64 + 1,
                to: Place::Person(1),
                lot,
            }],
        };
        assert!(store.prepare_transactions_now(7, &[action]).is_err());
    }
}

#[test]
fn batch_creation_is_published_only_with_the_atomic_physical_action() {
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(Place::Person(1), 10, vec![cohort(1, 2, None)]).unwrap(),
            Container::new(Place::Person(2), 10, vec![]).unwrap(),
        ],
    };
    let mut store = Store::compile(initial.clone()).unwrap();
    let mut action = transfer(1, 1, 2, 3);
    action.creations.push(game_lots::Creation {
        id: 2,
        to: Place::Person(2),
        lot: cohort(5, 2, None),
    });
    let draft = store.prepare_transactions_now(7, &[action]).unwrap();
    assert!(!draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    assert_eq!(store.export().containers, initial.containers);
    let mut action = transfer(3, 1, 2, 1);
    action.creations.push(game_lots::Creation {
        id: 4,
        to: Place::Person(2),
        lot: cohort(5, 2, None),
    });
    let draft = store.prepare_transactions_now(7, &[action]).unwrap();
    assert!(draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    assert_eq!(
        store.export().containers[1].lots,
        vec![cohort(1, 1, None), cohort(5, 2, None)]
    );
}

#[test]
#[ignore = "physical batch transfer scaling measurement"]
fn batch_provenance_scaling() {
    let count = std::env::var("LEGACY_BATCH_PEOPLE")
        .ok()
        .and_then(|row| row.parse::<u64>().ok())
        .unwrap_or(50_000);
    let targeted = std::env::var_os("LEGACY_BATCH_TARGETED").is_some();
    let mut containers = Vec::with_capacity((count * 2) as usize);
    let actions: Vec<_> = (1..=count)
        .map(|id| {
            containers.push(
                Container::new(
                    Place::Person(id),
                    10,
                    vec![cohort(id * 2, 1, Some(5)), cohort(id * 2 + 1, 1, Some(5))],
                )
                .unwrap(),
            );
            let mut action = transfer(id, id, count + id, 1);
            if targeted {
                action.movements[0].batch = Some(id * 2 + 1);
            }
            action
        })
        .collect();
    containers.extend(
        (1..=count).map(|id| Container::new(Place::Person(count + id), 10, vec![]).unwrap()),
    );
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers,
    };
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut store = Store::compile(initial.clone()).unwrap();
            let start = std::time::Instant::now();
            pool.install(|| {
                let draft = store.prepare_transactions_now(7, &actions).unwrap();
                assert!(draft.transactions().iter().all(|row| row.accepted));
                store.commit(draft).unwrap();
            });
            times.push(start.elapsed());
            let state = store.export();
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} independent batch transfers",
            times[1]
        );
    }
}

#[test]
fn concurrent_fan_in_keeps_different_batches_separate() {
    let mut store = Store::compile(State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(Place::Person(1), 10, vec![cohort(1, 2, Some(5))]).unwrap(),
            Container::new(Place::Person(2), 10, vec![cohort(2, 2, Some(5))]).unwrap(),
            Container::new(Place::Person(3), 10, vec![]).unwrap(),
        ],
    })
    .unwrap();
    let actions = [transfer(1, 1, 3, 1), transfer(2, 2, 3, 1)];
    let initial = store.export();
    let mut general = Store::compile(initial).unwrap();
    let movements: Vec<_> = actions.iter().map(|row| row.movements[0]).collect();
    let generic = general.prepare_now(7, &movements).unwrap();
    general.commit(generic).unwrap();
    let draft = store.prepare_transactions_now(7, &actions).unwrap();
    assert!(draft.transactions().iter().all(|row| row.accepted));
    store.commit(draft).unwrap();
    assert_eq!(store.export(), general.export());
    assert_eq!(
        store.export().containers[2].lots,
        vec![cohort(1, 1, Some(5)), cohort(2, 1, Some(5))]
    );
}

#[test]
fn exact_batch_selection_draws_only_requested_cohorts_and_can_span_expiries() {
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                Place::Person(1),
                20,
                vec![
                    cohort(1, 2, Some(1)),
                    cohort(2, 2, Some(2)),
                    cohort(2, 3, Some(4)),
                ],
            )
            .unwrap(),
            Container::new(Place::Person(2), 20, vec![]).unwrap(),
        ],
    };
    let mut action = transfer(1, 1, 2, 4);
    action.movements[0].batch = Some(2);
    let mut store = Store::compile(initial).unwrap();
    let draft = store.prepare_transactions_now(7, &[action]).unwrap();
    assert!(draft.transactions()[0].accepted);
    assert_eq!(
        draft.outcomes()[0]
            .lots
            .iter()
            .map(|row| (row.batch, row.quantity, row.expires_at))
            .collect::<Vec<_>>(),
        vec![(Some(2), 2, Some(2)), (Some(2), 2, Some(4))]
    );
    store.commit(draft).unwrap();
    assert_eq!(
        store.export().containers[0].lots,
        vec![cohort(1, 2, Some(1)), cohort(2, 1, Some(4))]
    );
}

#[test]
fn generic_and_targeted_batch_claims_compete_in_seeded_order() {
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                Place::Person(1),
                20,
                vec![cohort(1, 2, Some(1)), cohort(2, 3, Some(2))],
            )
            .unwrap(),
            Container::new(Place::Person(2), 20, vec![]).unwrap(),
            Container::new(Place::Person(3), 20, vec![]).unwrap(),
        ],
    };
    let generic = transfer(1, 1, 2, 3);
    let mut exact = transfer(2, 1, 3, 3);
    exact.movements[0].batch = Some(2);
    for generic_first in [false, true] {
        let seed = (0..1000)
            .find(|&seed| {
                (game_genetics::random(seed, 1, 0) < game_genetics::random(seed, 2, 0))
                    == generic_first
            })
            .unwrap();
        let mut reference = None;
        for workers in [1, 4] {
            let state = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut store = Store::compile(initial.clone()).unwrap();
                    let draft = store
                        .prepare_transactions_now(seed, &[generic.clone(), exact.clone()])
                        .unwrap();
                    assert_eq!(
                        draft
                            .transactions()
                            .iter()
                            .map(|row| row.accepted)
                            .collect::<Vec<_>>(),
                        vec![generic_first, !generic_first]
                    );
                    store.commit(draft).unwrap();
                    store.export()
                });
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
    }
}

#[test]
fn targeted_batch_rejects_missing_insufficient_or_invalid_requests() {
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(Place::Person(1), 10, vec![cohort(1, 2, None)]).unwrap(),
            Container::new(Place::Person(2), 10, vec![]).unwrap(),
        ],
    };
    let store = Store::compile(initial.clone()).unwrap();
    for batch in [Some(2), Some(1)] {
        let mut action = transfer(1, 1, 2, 3);
        action.movements[0].batch = batch;
        let draft = store.prepare_transactions_now(7, &[action]).unwrap();
        assert!(!draft.transactions()[0].accepted);
        assert_eq!(store.export(), initial);
    }
    let mut invalid = transfer(1, 1, 2, 1);
    invalid.movements[0].batch = Some(0);
    assert!(
        store
            .prepare_transactions_now(7, &[invalid.clone()])
            .is_err()
    );
    invalid.movements[0].batch = Some(1);
    invalid.movements[0].item = Some(1);
    assert!(store.prepare_transactions_now(7, &[invalid]).is_err());
    let mut targeted = transfer(3, 1, 2, 1);
    targeted.movements[0].batch = Some(1);
    let rows = [game_lots::SimpleTransfer {
        action: 3,
        movement: targeted.movements[0],
    }];
    assert!(
        store
            .prepare_flat_transfers(&rows, false)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .prepare_partitioned_flat(7, &rows, false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn independent_targeted_fast_path_matches_general_arbiter() {
    let count = 256;
    let mut containers = Vec::new();
    let mut actions = Vec::new();
    for id in 1..=count {
        containers.push(
            Container::new(
                Place::Person(id),
                10,
                vec![cohort(id * 2, 2, Some(2)), cohort(id * 2 + 1, 2, Some(3))],
            )
            .unwrap(),
        );
        let mut action = transfer(id, id, count + id, if id % 4 == 0 { 3 } else { 1 });
        action.movements[0].batch = Some(id * 2 + 1);
        actions.push(action);
    }
    containers.extend(
        (1..=count).map(|id| Container::new(Place::Person(count + id), 10, vec![]).unwrap()),
    );
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers,
    };
    for workers in [1, 4] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut fast = Store::compile(initial.clone()).unwrap();
                let mut general = Store::compile(initial.clone()).unwrap();
                let mut flat = Store::compile(initial.clone()).unwrap();
                let moves: Vec<_> = actions.iter().map(|action| action.movements[0]).collect();
                let flat_moves: Vec<_> = actions
                    .iter()
                    .map(|action| game_lots::SimpleTransfer {
                        action: action.id,
                        movement: action.movements[0],
                    })
                    .collect();
                let fast_draft = fast.prepare_transactions_now(7, &actions).unwrap();
                let general_draft = general.prepare_now(7, &moves).unwrap();
                let flat_draft = flat
                    .prepare_flat_transfers(&flat_moves, false)
                    .unwrap()
                    .unwrap();
                assert_eq!(fast_draft.outcomes(), general_draft.outcomes());
                assert_eq!(flat_draft.outcomes(), general_draft.outcomes());
                fast.commit(fast_draft).unwrap();
                general.commit(general_draft).unwrap();
                flat.commit(flat_draft).unwrap();
                assert_eq!(fast.export(), general.export());
                assert_eq!(flat.export(), general.export());
            });
    }
}

#[test]
#[ignore = "concentrated distinct batch selection scaling measurement"]
fn concentrated_distinct_batch_scaling() {
    let count = std::env::var("LEGACY_BATCH_PEOPLE")
        .ok()
        .and_then(|row| row.parse::<u64>().ok())
        .unwrap_or(20_000);
    let mut containers = vec![
        Container::new(
            Place::Person(1),
            count,
            (1..=count).map(|id| cohort(id, 1, Some(5))).collect(),
        )
        .unwrap(),
    ];
    containers
        .extend((2..=count + 1).map(|id| Container::new(Place::Person(id), 2, vec![]).unwrap()));
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers,
    };
    let actions: Vec<_> = (1..=count)
        .map(|id| {
            let mut action = transfer(id, 1, id + 1, 1);
            action.movements[0].batch = Some(id);
            action
        })
        .collect();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut store = Store::compile(initial.clone()).unwrap();
            let start = std::time::Instant::now();
            pool.install(|| {
                let draft = store.prepare_transactions_now(7, &actions).unwrap();
                assert!(draft.transactions().iter().all(|row| row.accepted));
                store.commit(draft).unwrap();
            });
            times.push(start.elapsed());
            let state = store.export();
            assert!(state.containers[0].lots.is_empty());
            assert_eq!(
                state.containers.iter().map(|row| row.used).sum::<u64>(),
                count
            );
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} distinct batches in shared stock",
            times[1]
        );
    }
}

#[test]
fn parallel_batch_groups_match_ordered_reference_for_contested_and_missing_batches() {
    let batches = 30;
    let count = 127;
    let mut containers = vec![
        Container::new(
            Place::Person(1),
            200,
            (1..=batches).map(|id| cohort(id, 3, Some(5))).collect(),
        )
        .unwrap(),
    ];
    containers
        .extend((2..=count + 2).map(|id| Container::new(Place::Person(id), 10, vec![]).unwrap()));
    let initial = State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers,
    };
    let actions: Vec<_> = (1..=count)
        .map(|id| {
            let mut action = transfer(id, 1, id + 1, if id.is_multiple_of(3) { 2 } else { 1 });
            action.movements[0].batch = Some(id % batches + 1);
            action
        })
        .collect();
    let mut with_extra = actions.clone();
    let mut extra = transfer(count + 1, 1, count + 2, 1);
    extra.movements[0].batch = Some(999);
    with_extra.push(extra);
    let mut reference = Store::compile(initial.clone()).unwrap();
    let draft = reference.prepare_transactions_now(7, &actions).unwrap();
    let expected_outcomes = draft.outcomes();
    reference.commit(draft).unwrap();
    for workers in [1, 4, 6] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut parallel = Store::compile(initial.clone()).unwrap();
                let draft = parallel.prepare_transactions_now(7, &with_extra).unwrap();
                assert_eq!(
                    &draft.outcomes()[..count as usize],
                    expected_outcomes.as_slice()
                );
                assert!(!draft.transactions().last().unwrap().accepted);
                parallel.commit(draft).unwrap();
                assert_eq!(parallel.export(), reference.export());
            });
    }
}

#[test]
fn large_exact_source_update_preserves_unselected_and_expires_old_batches() {
    let count = 2100;
    let mut containers = vec![
        Container::new(
            Place::Person(1),
            count,
            (1..=count)
                .map(|id| cohort(id, 1, Some(if id % 3 == 0 { 1 } else { 4 })))
                .collect(),
        )
        .unwrap(),
    ];
    containers
        .extend((2..=count + 1).map(|id| Container::new(Place::Person(id), 1, vec![]).unwrap()));
    let mut actions = Vec::new();
    for id in 1..=count {
        if id % 2 == 0 {
            let mut action = transfer(id, 1, id + 1, 1);
            action.movements[0].batch = Some(id);
            actions.push(action);
        }
    }
    let mut store = Store::compile(State {
        next_batch: None,
        tick: 0,
        revision: 0,
        containers,
    })
    .unwrap();
    let draft = store.prepare_transactions(7, &actions).unwrap();
    store.commit(draft).unwrap();
    let source = &store.export().containers[0];
    assert_eq!(
        source.used,
        (1..=count).filter(|id| id % 2 != 0 && id % 3 != 0).count() as u64
    );
    assert!(
        source
            .lots
            .iter()
            .all(|lot| lot.batch.is_some_and(|id| id % 2 != 0 && id % 3 != 0))
    );
}
