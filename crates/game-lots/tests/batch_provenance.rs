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
            transfer(id, id, count + id, 1)
        })
        .collect();
    containers.extend(
        (1..=count).map(|id| Container::new(Place::Person(count + id), 10, vec![]).unwrap()),
    );
    let initial = State {
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
