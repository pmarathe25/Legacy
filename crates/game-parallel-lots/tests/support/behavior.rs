use crate::{Container, Creation, Lot, Movement, Place, State, Store, Transaction, deadline};

fn lot(good: u32, quantity: u64, expires_at: Option<u64>) -> Lot {
    Lot {
        good,
        quantity,
        expires_at,
    }
}
fn container(place: Place, capacity: u64, lots: Vec<Lot>) -> Container {
    Container::new(place, capacity, lots).unwrap()
}
fn store(mut containers: Vec<Container>) -> Store {
    containers.sort_unstable_by_key(|row| row.place);
    Store::compile(State {
        tick: 0,
        revision: 0,
        containers,
    })
    .unwrap()
}
fn quantity(state: &State, place: Place, good: u32, expiry: Option<u64>) -> u64 {
    state
        .containers
        .iter()
        .find(|row| row.place == place)
        .unwrap()
        .lots
        .iter()
        .filter(|lot| lot.good == good && lot.expires_at == expiry)
        .map(|lot| lot.quantity)
        .sum()
}

#[test]
fn compact_consumption_keeps_mixed_fefo_arbitration_and_atomic_state() {
    let source = Place::Stock { region: 1, good: 1 };
    let destination = Place::Person(1);
    let initial = vec![
        container(source, 20, vec![lot(1, 3, Some(2)), lot(1, 4, Some(5))]),
        container(destination, 20, vec![]),
    ];
    let actions: Vec<_> = (1..=4)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                id,
                from: source,
                to: (id == 2).then_some(destination),
                good: 1,
                quantity: 2,
            }],
            creations: vec![],
        })
        .collect();
    for seed in 0..20 {
        let mut exact = store(initial.clone());
        let mut compact = store(initial.clone());
        let full = exact
            .prepare_with_empty(seed, &actions, vec![], true)
            .unwrap();
        let short = compact
            .prepare_with_empty_compact(seed, &actions, vec![], &[1, 3, 4], true)
            .unwrap();
        assert_eq!(full.transactions(), short.transactions());
        let full_outcomes = full.outcomes().to_vec();
        let short_outcomes = short.outcomes().to_vec();
        assert_eq!(full_outcomes[1], short_outcomes[1]);
        assert!(
            short_outcomes
                .iter()
                .filter(|row| row.id != 2)
                .all(|row| row.lots.is_empty())
        );
        exact.commit(full).unwrap();
        compact.commit(short).unwrap();
        assert_eq!(exact.export(), compact.export());
        assert!(
            compact
                .prepare_with_empty_compact(seed, &actions, vec![], &[2], true)
                .is_err()
        );
        assert!(
            compact
                .prepare_with_empty_compact(seed, &actions, vec![], &[99], true)
                .is_err()
        );
    }
}

#[test]
fn late_empty_holders_do_not_shift_planned_physical_changes() {
    let mut store = store(vec![
        container(Place::Person(1), 10, vec![lot(1, 3, Some(5))]),
        container(Place::Person(3), 10, vec![]),
    ]);
    let mut draft = store
        .prepare(
            7,
            &[Movement {
                id: 1,
                from: Place::Person(1),
                to: Some(Place::Person(3)),
                good: 1,
                quantity: 2,
            }],
        )
        .unwrap();
    assert!(
        store
            .add_empty_in(&mut draft, vec![container(Place::Person(1), 10, vec![])])
            .is_err()
    );
    store
        .add_empty_in(&mut draft, vec![container(Place::Person(2), 10, vec![])])
        .unwrap();
    store.commit(draft).unwrap();
    let result = store.export();
    assert_eq!(quantity(&result, Place::Person(1), 1, Some(5)), 1);
    assert_eq!(quantity(&result, Place::Person(2), 1, Some(5)), 0);
    assert_eq!(quantity(&result, Place::Person(3), 1, Some(5)), 2);
    Store::compile(result).unwrap();
}

#[test]
fn earliest_food_moves_first_and_expires_in_its_new_custody() {
    let source = Place::Inventory(1);
    let a = Place::Person(1);
    let b = Place::Person(2);
    let mut store = store(vec![
        container(
            source,
            20,
            vec![lot(1, 3, Some(2)), lot(1, 5, Some(4)), lot(1, 7, None)],
        ),
        container(a, 10, vec![]),
        container(b, 10, vec![]),
    ]);
    let draft = store
        .prepare(
            42,
            &[
                Movement {
                    id: 1,
                    from: source,
                    to: Some(a),
                    good: 1,
                    quantity: 5,
                },
                Movement {
                    id: 2,
                    from: source,
                    to: Some(b),
                    good: 1,
                    quantity: 2,
                },
            ],
        )
        .unwrap();
    let (outcomes, spoiled) = store.commit(draft).unwrap();
    assert!(spoiled.is_empty());
    assert!(outcomes.iter().all(|row| row.accepted));
    assert_eq!(
        outcomes
            .iter()
            .flat_map(|row| &row.lots)
            .filter(|lot| lot.expires_at == Some(2))
            .map(|lot| lot.quantity)
            .sum::<u64>(),
        3
    );
    let state = store.export();
    assert_eq!(quantity(&state, source, 1, Some(4)), 1);
    assert_eq!(quantity(&state, source, 1, None), 7);
    let draft = store.prepare(42, &[]).unwrap();
    let (_, spoiled) = store.commit(draft).unwrap();
    assert_eq!(spoiled.iter().map(|row| row.quantity).sum::<u64>(), 3);
    assert!(
        spoiled
            .iter()
            .all(|row| row.expires_at == 2 && (row.place == a || row.place == b))
    );
    let result = store.export();
    assert_eq!(
        result.containers.iter().map(|row| row.used).sum::<u64>(),
        12
    );
    assert_eq!(Store::compile(result.clone()).unwrap().export(), result);
}

#[test]
fn capacity_rejection_does_not_lose_source_lots_or_prevent_other_transfers() {
    let source = Place::Inventory(1);
    let small = Place::Person(1);
    let roomy = Place::Person(2);
    let mut store = store(vec![
        container(source, 20, vec![lot(1, 10, Some(5))]),
        container(small, 2, vec![]),
        container(roomy, 10, vec![]),
    ]);
    let (outcomes, _) = store
        .commit(
            store
                .prepare(
                    7,
                    &[
                        Movement {
                            id: 1,
                            from: source,
                            to: Some(small),
                            good: 1,
                            quantity: 3,
                        },
                        Movement {
                            id: 2,
                            from: source,
                            to: Some(roomy),
                            good: 1,
                            quantity: 4,
                        },
                    ],
                )
                .unwrap(),
        )
        .unwrap();
    assert!(!outcomes[0].accepted);
    assert!(outcomes[1].accepted);
    let state = store.export();
    assert_eq!(quantity(&state, source, 1, Some(5)), 6);
    assert_eq!(quantity(&state, roomy, 1, Some(5)), 4);
    assert_eq!(state.containers.iter().map(|row| row.used).sum::<u64>(), 10);
}

#[test]
fn movements_use_one_snapshot_and_expired_waiting_output_is_not_refreshed() {
    let a = Place::Person(1);
    let b = Place::Person(2);
    let c = Place::Person(3);
    let job = Place::JobOutput(7);
    let mut store = store(vec![
        container(a, 5, vec![lot(1, 1, Some(3))]),
        container(b, 5, vec![]),
        container(c, 5, vec![]),
        container(job, 5, vec![lot(1, 2, Some(1))]),
    ]);
    let (events, spoiled) = store
        .commit(
            store
                .prepare(
                    1,
                    &[
                        Movement {
                            id: 1,
                            from: a,
                            to: Some(b),
                            good: 1,
                            quantity: 1,
                        },
                        Movement {
                            id: 2,
                            from: b,
                            to: Some(c),
                            good: 1,
                            quantity: 1,
                        },
                        Movement {
                            id: 3,
                            from: job,
                            to: Some(c),
                            good: 1,
                            quantity: 2,
                        },
                    ],
                )
                .unwrap(),
        )
        .unwrap();
    assert!(events[0].accepted);
    assert!(!events[1].accepted && !events[2].accepted);
    assert_eq!(spoiled[0].place, job);
    assert_eq!((spoiled[0].quantity, spoiled[0].expires_at), (2, 1));
    let state = store.export();
    assert_eq!(quantity(&state, b, 1, Some(3)), 1);
    assert_eq!(quantity(&state, c, 1, Some(3)), 0);
    assert_eq!(state.containers.iter().map(|row| row.used).sum::<u64>(), 1);
}

#[test]
fn rejected_and_stale_drafts_leave_every_lot_unchanged() {
    let source = Place::Inventory(1);
    let target = Place::Person(1);
    let mut store = store(vec![
        container(source, 10, vec![lot(1, 4, Some(4))]),
        container(target, 10, vec![]),
    ]);
    let movement = Movement {
        id: 1,
        from: source,
        to: Some(target),
        good: 1,
        quantity: 1,
    };
    let before = store.export();
    let stale = store.prepare(3, &[movement]).unwrap();
    assert!(store.prepare(3, &[movement, movement]).is_err());
    assert!(
        store
            .prepare(
                3,
                &[Movement {
                    to: Some(Place::Person(999)),
                    ..movement
                }]
            )
            .is_err()
    );
    assert_eq!(store.export(), before);
    let draft = store.prepare(3, &[movement]).unwrap();
    store.commit(draft).unwrap();
    let after = store.export();
    assert!(store.commit(stale).is_err());
    assert_eq!(store.export(), after);
    let json = serde_json::to_string(&after).unwrap();
    let saved = serde_json::from_str(&json).unwrap();
    assert_eq!(Store::compile(saved).unwrap().export(), after);
    assert!(deadline(u64::MAX, 1, 1).is_err());
    assert!(deadline(1, 0, 1).is_err());
    assert_eq!(deadline(3, 2, 10).unwrap(), 23);
}

#[test]
fn concentrated_sources_and_destinations_conserve_lots_across_workers_and_input_order() {
    let count = 10_000u64;
    for hot_source in [true, false] {
        let source = Place::Inventory(1);
        let mut containers = vec![container(
            source,
            count,
            if hot_source {
                vec![lot(1, count / 2, Some(5))]
            } else {
                vec![]
            },
        )];
        containers.extend((1..=count).map(|person| {
            container(
                Place::Person(person),
                1,
                if hot_source {
                    vec![]
                } else {
                    vec![lot(1, 1, Some(5))]
                },
            )
        }));
        let initial = store(containers).export();
        let movements: Vec<_> = (1..=count)
            .map(|person| Movement {
                id: person,
                from: if hot_source {
                    source
                } else {
                    Place::Person(person)
                },
                to: Some(if hot_source {
                    Place::Person(person)
                } else {
                    source
                }),
                good: 1,
                quantity: 1,
            })
            .collect();
        let mut initial = initial;
        if !hot_source {
            initial.containers[0].capacity = count / 2;
        }
        let run = |workers, reverse| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut store = Store::compile(initial.clone()).unwrap();
                    let mut requests = movements.clone();
                    if reverse {
                        requests.reverse();
                    }
                    let draft = store.prepare(17, &requests).unwrap();
                    let (outcomes, spoiled) = store.commit(draft).unwrap();
                    assert!(spoiled.is_empty());
                    assert_eq!(
                        outcomes.iter().filter(|row| row.accepted).count(),
                        count as usize / 2
                    );
                    let result = store.export();
                    assert_eq!(
                        result.containers.iter().map(|row| row.used).sum::<u64>(),
                        if hot_source { count / 2 } else { count }
                    );
                    (result, outcomes)
                })
        };
        assert_eq!(run(1, false), run(4, true));
    }
}

#[test]
fn recipe_transaction_reserves_all_inputs_and_creates_output_once() {
    let ingredients = Place::Inventory(1);
    let job = Place::JobOutput(9);
    let mut store = store(vec![
        container(ingredients, 20, vec![lot(1, 3, Some(4)), lot(2, 3, None)]),
        container(job, 3, vec![]),
    ]);
    let recipe = Transaction {
        id: 1,
        movements: vec![
            Movement {
                id: 1,
                from: ingredients,
                to: None,
                good: 1,
                quantity: 2,
            },
            Movement {
                id: 2,
                from: ingredients,
                to: None,
                good: 2,
                quantity: 2,
            },
        ],
        creations: vec![Creation {
            id: 3,
            to: job,
            lot: lot(3, 2, Some(3)),
        }],
    };
    let mut blocked = recipe.clone();
    blocked.creations[0].lot.quantity = 4;
    let draft = store.prepare_transactions(7, &[blocked]).unwrap();
    assert_eq!(draft.creations(), &[(3, false)]);
    assert!(draft.outcomes().iter().all(|row| !row.accepted));
    let (events, _) = store.commit(draft).unwrap();
    assert!(events.iter().all(|row| row.lots.is_empty()));
    let state = store.export();
    assert_eq!(quantity(&state, ingredients, 1, Some(4)), 3);
    assert_eq!(quantity(&state, ingredients, 2, None), 3);
    assert_eq!(quantity(&state, job, 3, Some(3)), 0);

    let draft = store.prepare_transactions(7, &[recipe]).unwrap();
    assert_eq!(draft.creations(), &[(3, true)]);
    assert!(draft.outcomes().iter().all(|row| row.accepted));
    store.commit(draft).unwrap();
    let state = store.export();
    assert_eq!(quantity(&state, ingredients, 1, Some(4)), 1);
    assert_eq!(quantity(&state, ingredients, 2, None), 1);
    assert_eq!(quantity(&state, job, 3, Some(3)), 2);
    let saved = serde_json::to_string(&state).unwrap();
    let mut restored = Store::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    let (_, spoiled) = restored.commit(restored.prepare(7, &[]).unwrap()).unwrap();
    assert_eq!(spoiled.len(), 1);
    assert_eq!(
        (spoiled[0].place, spoiled[0].good, spoiled[0].quantity),
        (job, 3, 2)
    );
}

#[test]
fn invalid_creation_or_duplicate_leg_does_not_change_the_snapshot() {
    let source = Place::Inventory(1);
    let target = Place::JobOutput(1);
    assert!(Container::new(source, 1, vec![lot(0, 1, None)]).is_err());
    let store = store(vec![
        container(source, 5, vec![lot(1, 1, None)]),
        container(target, 5, vec![]),
    ]);
    let before = store.export();
    let mut action = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: source,
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![Creation {
            id: 2,
            to: target,
            lot: lot(2, 1, Some(1)),
        }],
    };
    assert!(store.prepare_transactions(1, &[action.clone()]).is_err());
    action.creations[0].lot.expires_at = Some(3);
    action.creations[0].id = 1;
    assert!(store.prepare_transactions(1, &[action]).is_err());
    assert_eq!(store.export(), before);
}

#[test]
fn competing_recipes_are_atomic_and_deterministic_across_workers_and_order() {
    let input = Place::Inventory(1);
    let output = Place::Inventory(2);
    let count = 2048u64;
    let initial = store(vec![
        container(input, count, vec![lot(1, count / 2, Some(10))]),
        container(output, count / 2, vec![]),
    ])
    .export();
    let actions: Vec<_> = (1..=count)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                id,
                from: input,
                to: None,
                good: 1,
                quantity: 1,
            }],
            creations: vec![Creation {
                id: count + id,
                to: output,
                lot: lot(2, 1, Some(7)),
            }],
        })
        .collect();
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut store = Store::compile(initial.clone()).unwrap();
                let mut actions = actions.clone();
                if reverse {
                    actions.reverse();
                }
                let draft = store.prepare_transactions(123, &actions).unwrap();
                let transactions = draft.transactions().to_vec();
                let creations = draft.creations().to_vec();
                let granted = transactions.iter().filter(|row| row.accepted).count() as u64;
                assert_eq!(granted, count / 2);
                assert_eq!(creations.iter().filter(|row| row.1).count() as u64, granted);
                let (moves, _) = store.commit(draft).unwrap();
                assert_eq!(
                    moves.iter().filter(|row| row.accepted).count() as u64,
                    granted
                );
                let state = store.export();
                assert_eq!(quantity(&state, input, 1, Some(10)), count / 2 - granted);
                assert_eq!(quantity(&state, output, 2, Some(7)), granted);
                (state, transactions, creations, moves)
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
fn a_draft_cannot_be_committed_to_a_different_store_at_the_same_revision() {
    let a = Place::Inventory(1);
    let b = Place::Person(1);
    let state = store(vec![
        container(a, 10, vec![lot(1, 4, None)]),
        container(b, 10, vec![]),
    ])
    .export();
    let first = Store::compile(state.clone()).unwrap();
    let mut second = Store::compile(state.clone()).unwrap();
    let draft = first
        .prepare(
            1,
            &[Movement {
                id: 1,
                from: a,
                to: Some(b),
                good: 1,
                quantity: 2,
            }],
        )
        .unwrap();
    assert!(second.commit(draft).is_err());
    assert_eq!(second.export(), state);
}

#[test]
fn custody_lifecycle_is_atomic_and_invalidates_prepared_actions() {
    let stock = Place::Stock { region: 1, good: 1 };
    let job = Place::JobInput(5);
    let mut store = store(vec![container(stock, 10, vec![lot(1, 3, Some(8))])]);
    let stale = store.prepare(1, &[]).unwrap();
    store.add_empty(vec![container(job, 10, vec![])]).unwrap();
    assert!(store.commit(stale).is_err());
    let before = store.export();
    assert!(store.add_empty(vec![container(job, 10, vec![])]).is_err());
    assert!(store.remove_empty(vec![stock]).is_err());
    assert_eq!(store.export(), before);
    let draft = store
        .prepare(
            1,
            &[Movement {
                id: 1,
                from: stock,
                to: Some(job),
                good: 1,
                quantity: 2,
            }],
        )
        .unwrap();
    store.commit(draft).unwrap();
    assert!(store.remove_empty(vec![job]).is_err());
    let draft = store
        .prepare(
            1,
            &[Movement {
                id: 2,
                from: job,
                to: Some(stock),
                good: 1,
                quantity: 2,
            }],
        )
        .unwrap();
    store.commit(draft).unwrap();
    store.remove_empty(vec![job]).unwrap();
    let saved = serde_json::to_string(&store.export()).unwrap();
    let restored = Store::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(quantity(&restored.export(), stock, 1, Some(8)), 3);
}

#[test]
fn independently_limited_stock_cannot_receive_a_different_good() {
    let stock = Place::Stock { region: 1, good: 1 };
    let other = Place::Person(1);
    assert!(Container::new(stock, 10, vec![lot(2, 1, None)]).is_err());
    let store = store(vec![
        container(stock, 10, vec![]),
        container(other, 10, vec![lot(2, 1, None)]),
    ]);
    let before = store.export();
    assert!(
        store
            .prepare(
                1,
                &[Movement {
                    id: 1,
                    from: other,
                    to: Some(stock),
                    good: 2,
                    quantity: 1
                }]
            )
            .is_err()
    );
    let action = Transaction {
        id: 1,
        movements: vec![],
        creations: vec![Creation {
            id: 1,
            to: stock,
            lot: lot(2, 1, None),
        }],
    };
    assert!(store.prepare_transactions(1, &[action]).is_err());
    assert_eq!(store.export(), before);
}

#[test]
fn reserved_ingredients_can_spoil_and_cancellation_returns_only_surviving_lots() {
    let stock = Place::Stock { region: 1, good: 1 };
    let reserved = Place::JobInput(9);
    let output = Place::JobOutput(9);
    let mut store = store(vec![
        container(stock, 10, vec![lot(1, 2, Some(2)), lot(1, 3, Some(8))]),
        container(reserved, 10, vec![]),
        container(output, 10, vec![]),
    ]);
    let reserve = Movement {
        id: 1,
        from: stock,
        to: Some(reserved),
        good: 1,
        quantity: 5,
    };
    let draft = store.prepare(5, &[reserve]).unwrap();
    store.commit(draft).unwrap();
    let finish = Transaction {
        id: 2,
        movements: vec![Movement {
            id: 2,
            from: reserved,
            to: None,
            good: 1,
            quantity: 5,
        }],
        creations: vec![Creation {
            id: 3,
            to: output,
            lot: lot(2, 1, Some(7)),
        }],
    };
    let draft = store.prepare_transactions(5, &[finish]).unwrap();
    assert!(!draft.transactions()[0].accepted);
    let (_, spoiled) = store.commit(draft).unwrap();
    assert_eq!((spoiled[0].place, spoiled[0].quantity), (reserved, 2));
    let state = store.export();
    assert_eq!(quantity(&state, output, 2, Some(7)), 0);
    assert_eq!(quantity(&state, reserved, 1, Some(8)), 3);
    let draft = store
        .prepare(
            5,
            &[Movement {
                id: 4,
                from: reserved,
                to: Some(stock),
                good: 1,
                quantity: 3,
            }],
        )
        .unwrap();
    store.commit(draft).unwrap();
    store.remove_empty(vec![reserved, output]).unwrap();
    assert_eq!(quantity(&store.export(), stock, 1, Some(8)), 3);
}

#[test]
fn staged_new_custody_rolls_back_with_the_rest_of_a_failed_generation() {
    let stock = Place::Stock { region: 1, good: 1 };
    let job = Place::JobInput(3);
    let mut store = store(vec![container(stock, 5, vec![lot(1, 2, None)])]);
    let before = store.export();
    let original = store.prepare(1, &[]).unwrap();
    {
        let stage = store.stage(vec![container(job, 5, vec![])]).unwrap();
        let tx = Transaction {
            id: 1,
            movements: vec![Movement {
                id: 1,
                from: stock,
                to: Some(job),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        };
        let draft = stage.prepare_transactions(1, &[tx]).unwrap();
        assert!(draft.transactions()[0].accepted);
        drop(draft);
    }
    assert_eq!(store.export(), before);
    assert!(store.commit(original).is_err());
    let draft = store.prepare(1, &[]).unwrap();
    store.commit(draft).unwrap();
    let stage = store.stage(vec![container(job, 5, vec![])]).unwrap();
    let tx = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: stock,
            to: Some(job),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let draft = stage.prepare_transactions(1, &[tx]).unwrap();
    stage.commit(draft).unwrap();
    assert_eq!(store.container(job).unwrap().used, 1);
    assert_eq!(store.container(stock).unwrap().used, 1);
}

#[test]
fn a_discarded_stage_draft_cannot_commit_during_a_later_stage() {
    let stock = Place::Stock { region: 1, good: 1 };
    let first = Place::JobInput(1);
    let second = Place::JobInput(2);
    let mut store = store(vec![container(stock, 5, vec![lot(1, 2, None)])]);
    let before = store.export();
    let draft = {
        let stage = store.stage(vec![container(first, 5, vec![])]).unwrap();
        let action = Transaction {
            id: 1,
            movements: vec![Movement {
                id: 1,
                from: stock,
                to: Some(first),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        };
        stage.prepare_transactions(1, &[action]).unwrap()
    };
    let stage = store.stage(vec![container(second, 5, vec![])]).unwrap();
    assert!(stage.commit(draft).is_err());
    assert_eq!(store.export(), before);
}

#[test]
fn immediate_actions_preserve_time_and_still_invalidate_old_drafts() {
    let stock = Place::Stock { region: 1, good: 1 };
    let person = Place::Person(1);
    let mut store = store(vec![
        container(stock, 5, vec![lot(1, 2, Some(1))]),
        container(person, 5, vec![]),
    ]);
    let old = store.prepare(1, &[]).unwrap();
    let transfer = Movement {
        id: 1,
        from: stock,
        to: Some(person),
        good: 1,
        quantity: 1,
    };
    let draft = store.prepare_now(1, &[transfer]).unwrap();
    store.commit(draft).unwrap();
    assert_eq!(store.export().tick, 0);
    assert_eq!(store.container(person).unwrap().lots[0].expires_at, Some(1));
    assert!(store.commit(old).is_err());
    let draft = store.prepare(1, &[]).unwrap();
    let (_, spoiled) = store.commit(draft).unwrap();
    assert_eq!(spoiled.iter().map(|row| row.quantity).sum::<u64>(), 2);
}

#[test]
fn independent_owners_can_compose_colliding_action_and_leg_ids() {
    let one = Place::Stock { region: 1, good: 1 };
    let two = Place::Stock { region: 1, good: 2 };
    let person = Place::Person(1);
    let mut store = store(vec![
        container(one, 1, vec![lot(1, 1, None)]),
        container(two, 1, vec![lot(2, 1, None)]),
        container(person, 2, vec![]),
    ]);
    let action = |from, good| Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from,
            to: Some(person),
            good,
            quantity: 1,
        }],
        creations: vec![],
    };
    let combined = crate::compose(vec![(2, action(two, 2)), (1, action(one, 1))]).unwrap();
    assert_eq!(combined.keys, vec![(1, 1), (2, 1)]);
    let draft = store
        .prepare_transactions_now(1, &combined.transactions)
        .unwrap();
    assert!(draft.transactions().iter().all(|row| row.accepted));
    store.commit(draft).unwrap();
    assert_eq!(store.container(person).unwrap().used, 2);
    assert!(crate::compose(vec![(1, action(one, 1)), (1, action(two, 2))]).is_err());
}

#[test]
fn detached_new_custody_is_invisible_until_commit_and_uses_merged_indices() {
    let source = Place::Person(2);
    let new = Place::Person(1);
    let mut store = store(vec![container(source, 5, vec![lot(1, 3, Some(5))])]);
    let before = store.export();
    let action = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: source,
            to: Some(new),
            good: 1,
            quantity: 2,
        }],
        creations: vec![],
    };
    let draft = store
        .prepare_with_empty(
            1,
            std::slice::from_ref(&action),
            vec![container(new, 5, vec![])],
            false,
        )
        .unwrap();
    assert!(draft.transactions()[0].accepted);
    assert_eq!(store.export(), before);
    store.commit(draft).unwrap();
    assert_eq!(store.export().tick, 0);
    assert_eq!(store.container(source).unwrap().used, 1);
    assert_eq!(store.container(new).unwrap().used, 2);
    assert_eq!(store.container(new).unwrap().lots[0].expires_at, Some(5));
    assert!(
        store
            .prepare_with_empty(1, &[action], vec![container(new, 5, vec![])], false)
            .is_err()
    );
}

#[test]
fn detached_new_custody_is_discarded_on_stale_commit() {
    let stock = Place::Stock { region: 1, good: 1 };
    let input = Place::JobInput(1);
    let mut store = store(vec![container(stock, 5, vec![lot(1, 2, None)])]);
    let action = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: stock,
            to: Some(input),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let draft = store
        .prepare_with_empty(1, &[action], vec![container(input, 5, vec![])], false)
        .unwrap();
    let other = store.prepare_now(1, &[]).unwrap();
    store.commit(other).unwrap();
    let before = store.export();
    assert!(store.commit(draft).is_err());
    assert_eq!(store.export(), before);
    assert!(store.container(input).is_none());
}

#[test]
fn rejected_starts_can_retire_temporary_custody_in_the_same_boundary() {
    let stock = Place::Stock { region: 1, good: 1 };
    let input = Place::JobInput(1);
    let output = Place::JobOutput(1);
    let mut store = store(vec![container(stock, 5, vec![])]);
    let action = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: stock,
            to: Some(input),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let mut draft = store
        .prepare_with_empty(
            1,
            &[action],
            vec![container(input, 5, vec![]), container(output, 5, vec![])],
            false,
        )
        .unwrap();
    assert!(!draft.transactions()[0].accepted);
    store.retire_in(&mut draft, vec![input, output]).unwrap();
    store.commit(draft).unwrap();
    assert!(store.container(input).is_none());
    assert!(store.container(output).is_none());
    assert_eq!(store.export().tick, 0);
}

#[test]
fn retirement_checks_the_resulting_lots_before_any_publication() {
    let input = Place::JobInput(1);
    let stock = Place::Stock { region: 1, good: 1 };
    let mut store = store(vec![
        container(input, 5, vec![lot(1, 2, None)]),
        container(stock, 5, vec![]),
    ]);
    let transfer = Movement {
        id: 1,
        from: input,
        to: Some(stock),
        good: 1,
        quantity: 2,
    };
    let mut draft = store.prepare_now(1, &[transfer]).unwrap();
    assert!(store.retire_in(&mut draft, vec![stock]).is_err());
    store.retire_in(&mut draft, vec![input]).unwrap();
    store.validate_commit(&draft).unwrap();
    store.commit(draft).unwrap();
    assert!(store.container(input).is_none());
    assert_eq!(store.container(stock).unwrap().used, 2);
}

#[test]
fn a_two_way_exchange_fits_full_bags_and_keeps_original_expiry() {
    let a = Place::Person(1);
    let b = Place::Person(2);
    let mut store = store(vec![
        container(a, 1, vec![lot(1, 1, Some(4))]),
        container(b, 1, vec![lot(2, 1, Some(6))]),
    ]);
    let exchange = Transaction {
        id: 1,
        movements: vec![
            Movement {
                id: 1,
                from: a,
                to: Some(b),
                good: 1,
                quantity: 1,
            },
            Movement {
                id: 2,
                from: b,
                to: Some(a),
                good: 2,
                quantity: 1,
            },
        ],
        creations: vec![],
    };
    let draft = store.prepare_transactions_now(1, &[exchange]).unwrap();
    assert!(draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    assert_eq!(store.container(a).unwrap().lots, vec![lot(2, 1, Some(6))]);
    assert_eq!(store.container(b).unwrap().lots, vec![lot(1, 1, Some(4))]);
}
#[test]
fn one_action_cannot_spend_another_actions_outgoing_capacity() {
    let a = Place::Person(1);
    let b = Place::Person(2);
    let c = Place::Person(3);
    let mut store = store(vec![
        container(a, 1, vec![lot(1, 1, None)]),
        container(b, 1, vec![lot(2, 1, None)]),
        container(c, 1, vec![]),
    ]);
    let leave = Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: a,
            to: Some(c),
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let enter = Transaction {
        id: 2,
        movements: vec![Movement {
            id: 2,
            from: b,
            to: Some(a),
            good: 2,
            quantity: 1,
        }],
        creations: vec![],
    };
    let draft = store.prepare_transactions_now(1, &[leave, enter]).unwrap();
    assert!(draft.transactions()[0].accepted);
    assert!(!draft.transactions()[1].accepted);
    store.commit(draft).unwrap();
    assert_eq!(store.container(b).unwrap().used, 1);
    assert_eq!(store.container(c).unwrap().used, 1);
    assert_eq!(store.container(a).unwrap().used, 0);
}

#[test]
fn overlapping_full_container_exchanges_conserve_every_good_across_workers() {
    let initial = store(
        (1..=64)
            .map(|id| {
                container(
                    Place::Person(id),
                    2,
                    vec![lot(1, 1, Some(9)), lot(2, 1, Some(12))],
                )
            })
            .collect(),
    )
    .export();
    let actions: Vec<_> = (1..=512)
        .map(|id| {
            let a = Place::Person(1 + (id * 17 % 64));
            let b = Place::Person(1 + ((id * 17 + 1 + id % 63) % 64));
            Transaction {
                id,
                movements: vec![
                    Movement {
                        id: id * 2 - 1,
                        from: a,
                        to: Some(b),
                        good: 1,
                        quantity: 1,
                    },
                    Movement {
                        id: id * 2,
                        from: b,
                        to: Some(a),
                        good: 2,
                        quantity: 1,
                    },
                ],
                creations: vec![],
            }
        })
        .collect();
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut store = Store::compile(initial.clone()).unwrap();
                let mut actions = actions.clone();
                if reverse {
                    actions.reverse();
                }
                let draft = store.prepare_transactions_now(31, &actions).unwrap();
                let outcomes = draft.transactions().to_vec();
                store.commit(draft).unwrap();
                let state = store.export();
                assert!(state.containers.iter().all(|row| row.used <= row.capacity));
                for good in 1..=2 {
                    assert_eq!(
                        state
                            .containers
                            .iter()
                            .flat_map(|row| &row.lots)
                            .filter(|row| row.good == good)
                            .map(|row| row.quantity)
                            .sum::<u64>(),
                        64
                    );
                }
                assert!(Store::compile(state.clone()).is_ok());
                (state, outcomes)
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
fn custody_ids_cannot_be_zero_even_when_empty() {
    assert!(Container::new(Place::Person(0), 1, vec![]).is_err());
    assert!(Container::new(Place::JobInput(0), 1, vec![]).is_err());
    assert!(Container::new(Place::Stock { region: 0, good: 1 }, 1, vec![]).is_err());
    assert!(Container::new(Place::Stock { region: 1, good: 0 }, 1, vec![]).is_err());
}

#[test]
fn read_only_views_use_the_requested_boundary_without_mutating_custody() {
    let person = Place::Person(1);
    let stock = Place::Stock { region: 1, good: 1 };
    let store = store(vec![
        container(
            person,
            10,
            vec![lot(1, 2, Some(1)), lot(1, 3, Some(5)), lot(2, 1, None)],
        ),
        container(stock, 7, vec![]),
    ]);
    let before = store.export();
    let current = store.view(0).unwrap();
    let next = store.view(1).unwrap();
    assert_eq!(current.container(person).unwrap().quantity(1), 5);
    assert_eq!(next.container(person).unwrap().quantity(1), 3);
    assert_eq!(next.container(person).unwrap().used, 4);
    assert_eq!(next.container(stock).unwrap().capacity, 7);
    assert_eq!(next.container(stock).unwrap().quantity(1), 0);
    assert!(store.view(2).is_err());
    assert_eq!(store.export(), before);
}
