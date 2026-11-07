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
fn unpartitioned(
    owner: &Store,
    seed: u64,
    actions: &[Transaction],
    additions: Vec<Container>,
    compact: &[u64],
    advance: bool,
) -> crate::Draft {
    let (inner, additions) = game_parallel_lot_batches::prepare_unpartitioned(
        &owner.state,
        owner.instance,
        seed,
        actions,
        advance,
        additions,
        compact,
    )
    .unwrap();
    crate::Draft {
        inner,
        additions,
        retire: Vec::new(),
    }
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
fn independent_compact_consumption_matches_general_planning_with_expiry() {
    let initial: Vec<_> = (1..=300)
        .map(|id| {
            container(
                Place::Person(id),
                20,
                vec![lot(1, 2, Some(1)), lot(1, 3, Some(3)), lot(2, 2, None)],
            )
        })
        .collect();
    let actions: Vec<_> = (1..=256)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                id: 1000 - id,
                from: Place::Person(id),
                to: None,
                good: if id % 3 == 0 { 2 } else { 1 },
                quantity: if id % 5 == 0 { 4 } else { 2 },
            }],
            creations: vec![],
        })
        .collect();
    let ids: Vec<_> = actions.iter().map(|action| action.id).collect();
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
                let mut general = store(initial.clone());
                let mut fast = store(initial.clone());
                let full = general
                    .prepare_with_empty(13, &actions, vec![], true)
                    .unwrap();
                let compact = fast
                    .prepare_with_empty_compact(13, &actions, vec![], &ids, true)
                    .unwrap();
                assert_eq!(full.transactions(), compact.transactions());
                assert_eq!(full.spoiled(), compact.spoiled());
                for (a, b) in full.outcomes().iter().zip(compact.outcomes()) {
                    assert_eq!((a.id, a.accepted), (b.id, b.accepted));
                    assert!(b.lots.is_empty());
                }
                general.commit(full).unwrap();
                fast.commit(compact).unwrap();
                assert_eq!(general.export(), fast.export());
                fast.export()
            })
    };
    assert_eq!(run(1, false), run(4, true));
    let mut overlapping = actions;
    overlapping[1].movements[0].from = overlapping[0].movements[0].from;
    let mut general = store(initial.clone());
    let mut compact = store(initial);
    let full = general
        .prepare_with_empty(13, &overlapping, vec![], true)
        .unwrap();
    let short = compact
        .prepare_with_empty_compact(13, &overlapping, vec![], &ids, true)
        .unwrap();
    assert_eq!(full.transactions(), short.transactions());
    general.commit(full).unwrap();
    compact.commit(short).unwrap();
    assert_eq!(general.export(), compact.export());
}

#[test]
fn shared_receipt_lots_are_isolated_and_serialize_as_flat_arrays() {
    use std::sync::Arc;
    let arena: Arc<[Lot]> = vec![lot(1, 2, Some(4)), lot(1, 3, None), lot(2, 1, None)].into();
    let first = crate::LotList::shared(arena.clone(), 0..2);
    let second = crate::LotList::shared(arena.clone(), 2..3);
    let empty = crate::LotList::shared(arena.clone(), 1..1);
    drop(arena);
    assert_eq!(first.as_slice(), &[lot(1, 2, Some(4)), lot(1, 3, None)]);
    assert_eq!(second.as_slice(), &[lot(2, 1, None)]);
    assert!(empty.is_empty());
    let serialized = serde_json::to_value(&first).unwrap();
    assert!(serialized.is_array());
    let restored: crate::LotList = serde_json::from_value(serialized).unwrap();
    assert_eq!(first, restored);
    assert_eq!(first.clone(), restored);
}

#[test]
fn shared_destination_transfers_match_general_planning_and_preserve_receipts() {
    let mut initial = Vec::new();
    let mut actions = Vec::new();
    for id in 1..=256u64 {
        initial.push(container(
            Place::Person(id),
            20,
            vec![
                lot(1, 1, Some(1)),
                lot(1, 2, Some(if id % 4 == 0 { 3 + id } else { 3 })),
                lot(1, 3, None),
                lot(2, 4, None),
            ],
        ));
        actions.push(Transaction {
            id,
            movements: vec![Movement {
                id: 1000 - id,
                from: Place::Person(id),
                to: Some(Place::Person(if id % 2 == 0 { 1000 } else { 1001 })),
                good: if id % 3 == 0 { 2 } else { 1 },
                quantity: 3,
            }],
            creations: vec![],
        });
    }
    initial.push(container(
        Place::Person(1000),
        1000,
        vec![lot(1, 1, Some(1)), lot(1, 2, Some(3)), lot(2, 2, None)],
    ));
    initial.push(container(Place::Person(1001), 1000, vec![lot(1, 2, None)]));
    initial.push(container(Place::Person(9999), 10, vec![lot(1, 2, Some(1))]));
    let run =
        |workers, reverse: bool, advance: bool, limited: bool, overlap: bool, shortage: bool| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut actions = actions.clone();
                    let mut holders = initial.clone();
                    if limited {
                        *holders
                            .iter_mut()
                            .find(|row| row.place == Place::Person(1000))
                            .unwrap() = container(Place::Person(1000), 10, vec![lot(1, 2, None)]);
                    }
                    if overlap {
                        actions[1].movements[0].from = actions[0].movements[0].from;
                    }
                    if shortage {
                        actions[0].movements[0].quantity = 50;
                    }
                    if reverse {
                        actions.reverse();
                    }
                    let mut fast = store(holders.clone());
                    let mut reference = store(holders);
                    let draft = fast
                        .prepare_with_empty(17, &actions, vec![], advance)
                        .unwrap();
                    let stale = fast
                        .prepare_with_empty(17, &actions, vec![], advance)
                        .unwrap();
                    let general = unpartitioned(&reference, 17, &actions, vec![], &[], advance);
                    assert_eq!(draft.transactions(), general.transactions());
                    assert_eq!(draft.outcomes(), general.outcomes());
                    assert_eq!(draft.spoiled(), general.spoiled());
                    let receipts = draft.outcomes();
                    fast.commit(draft).unwrap();
                    assert!(fast.commit(stale).is_err());
                    reference.commit(general).unwrap();
                    assert_eq!(fast.export(), reference.export());
                    assert_eq!(
                        Store::compile(fast.export()).unwrap().export(),
                        fast.export()
                    );
                    assert_eq!(receipts.len(), actions.len());
                    fast.export()
                })
        };
    for advance in [false, true] {
        for (limited, overlap, shortage) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert_eq!(
                run(1, false, advance, limited, overlap, shortage),
                run(4, true, advance, limited, overlap, shortage)
            );
        }
    }
}

#[test]
fn independent_transfers_match_general_transactions_with_expiry_and_capacity() {
    let mut initial = Vec::new();
    let mut actions = Vec::new();
    for id in 1..=256 {
        initial.push(container(
            Place::Person(id),
            20,
            vec![
                lot(1, 2, Some(1)),
                lot(1, 3, Some(3)),
                lot(1, 4, None),
                lot(2, 2, None),
            ],
        ));
        initial.push(container(
            Place::Person(id + 1000),
            if id % 7 == 0 { 3 } else { 20 },
            if id % 7 == 0 {
                vec![lot(1, 2, Some(3)), lot(2, 1, None)]
            } else {
                vec![lot(1, 2, Some(3)), lot(2, 1, Some(1)), lot(2, 1, None)]
            },
        ));
        actions.push(Transaction {
            id,
            movements: vec![Movement {
                id: 1000 - id,
                from: Place::Person(id),
                to: Some(Place::Person(id + 1000)),
                good: if id % 3 == 0 { 2 } else { 1 },
                quantity: if id % 5 == 0 { 8 } else { 4 },
            }],
            creations: vec![],
        });
    }
    initial.push(container(Place::Person(9000), 10, vec![lot(1, 2, Some(1))]));
    let run = |workers, reverse: bool, overlap: bool, advance: bool, sparse: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut actions = actions.clone();
                if overlap {
                    actions[1].movements[0].from = actions[0].movements[0].from;
                    actions[3].movements[0].to = actions[2].movements[0].to;
                    actions[4].movements[0].to = Some(actions[5].movements[0].from);
                    actions[6].movements[0].from = actions[2].movements[0].to.unwrap();
                }
                if reverse {
                    actions.reverse();
                }
                let mut holders = initial.clone();
                if sparse {
                    holders.extend(
                        (10_000..11_000).map(|id| container(Place::Person(id), 10, vec![])),
                    );
                }
                let mut fast = store(holders.clone());
                let mut flat = store(holders.clone());
                let mut partitioned = store(holders.clone());
                let mut general = store(holders);
                let transfers: Vec<_> = actions
                    .iter()
                    .map(|action| crate::SimpleTransfer {
                        action: action.id,
                        movement: action.movements[0],
                    })
                    .collect();
                let flat_draft = flat.prepare_flat_transfers(&transfers, advance).unwrap();
                assert_eq!(flat_draft.is_none(), overlap);
                let partitioned_draft = partitioned
                    .prepare_partitioned_flat(13, &transfers, advance)
                    .unwrap()
                    .unwrap();
                let staged = fast
                    .prepare_with_empty(13, &actions, vec![], advance)
                    .unwrap();
                let stale = fast
                    .prepare_with_empty(13, &actions, vec![], advance)
                    .unwrap();
                let full = unpartitioned(
                    &general,
                    13,
                    &actions,
                    vec![container(Place::Inventory(99999), 10, vec![])],
                    &[],
                    advance,
                );
                assert_eq!(staged.transactions(), full.transactions());
                assert_eq!(staged.outcomes(), full.outcomes());
                assert_eq!(staged.spoiled(), full.spoiled());
                assert_eq!(partitioned_draft.transactions(), full.transactions());
                assert_eq!(partitioned_draft.outcomes(), full.outcomes());
                assert_eq!(partitioned_draft.spoiled(), full.spoiled());
                partitioned.commit(partitioned_draft).unwrap();
                if let Some(flat_draft) = flat_draft {
                    assert_eq!(flat_draft.transactions(), full.transactions());
                    assert_eq!(flat_draft.outcomes(), full.outcomes());
                    assert_eq!(flat_draft.spoiled(), full.spoiled());
                    flat.commit(flat_draft).unwrap();
                }
                fast.commit(staged).unwrap();
                assert!(fast.commit(stale).is_err());
                general.commit(full).unwrap();
                let result = fast.export();
                let mut expected = general.export();
                expected
                    .containers
                    .retain(|c| c.place != Place::Inventory(99999));
                assert_eq!(result, expected);
                assert_eq!(partitioned.export(), result);
                if !overlap {
                    assert_eq!(flat.export(), result);
                }
                assert_eq!(Store::compile(result.clone()).unwrap().export(), result);
                result
            })
    };
    assert_eq!(
        run(1, false, false, true, false),
        run(4, true, false, true, false)
    );
    assert_eq!(
        run(1, false, true, true, false),
        run(4, true, true, true, false)
    );
    assert_eq!(
        run(1, false, false, false, false),
        run(4, true, false, false, false)
    );
    assert_eq!(
        run(1, false, false, true, true),
        run(4, true, false, true, true)
    );
    assert_eq!(
        run(1, false, true, true, true),
        run(4, true, true, true, true)
    );
    let mut invalid = actions.clone();
    invalid[1].movements[0].id = invalid[0].movements[0].id;
    assert!(
        store(initial.clone())
            .prepare_with_empty(13, &invalid, vec![], true)
            .is_err()
    );
    invalid = actions;
    invalid[1].movements[0].to = Some(invalid[1].movements[0].from);
    assert!(
        store(initial)
            .prepare_with_empty(13, &invalid, vec![], true)
            .is_err()
    );
}

#[test]
fn ordered_disjoint_ranges_and_interleaved_endpoints_match_general_arbitration() {
    for destination_first in [false, true] {
        let mut holders = Vec::new();
        let mut transfers = Vec::new();
        for id in 1..=256 {
            let (source, destination) = if destination_first {
                (id + 1000, id)
            } else {
                (2 * id - 1, 2 * id)
            };
            holders.push(container(
                Place::Person(source),
                10,
                vec![lot(1, 1, Some(1)), lot(1, 3, Some(4))],
            ));
            holders.push(container(Place::Person(destination), 10, vec![]));
            transfers.push(crate::SimpleTransfer {
                action: id,
                movement: Movement {
                    id: 1000 - id,
                    from: Place::Person(source),
                    to: Some(Place::Person(destination)),
                    good: 1,
                    quantity: 2,
                },
            });
        }
        holders.sort_unstable_by_key(|row| row.place);
        let actions: Vec<_> = transfers
            .iter()
            .map(|row| Transaction {
                id: row.action,
                movements: vec![row.movement],
                creations: vec![],
            })
            .collect();
        let run = |workers, reverse| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut transfers = transfers.clone();
                    let mut actions = actions.clone();
                    if reverse {
                        transfers.reverse();
                        actions.reverse();
                    }
                    let mut fast = store(holders.clone());
                    let mut general = store(holders.clone());
                    let draft = fast
                        .prepare_flat_transfers(&transfers, true)
                        .unwrap()
                        .unwrap();
                    let reference = unpartitioned(
                        &general,
                        7,
                        &actions,
                        vec![container(Place::Inventory(99999), 10, vec![])],
                        &[],
                        true,
                    );
                    assert_eq!(draft.transactions(), reference.transactions());
                    assert_eq!(draft.outcomes(), reference.outcomes());
                    assert_eq!(draft.spoiled(), reference.spoiled());
                    fast.commit(draft).unwrap();
                    general.commit(reference).unwrap();
                    let mut expected = general.export();
                    expected
                        .containers
                        .retain(|row| row.place != Place::Inventory(99999));
                    assert_eq!(fast.export(), expected);
                    fast.export()
                })
        };
        assert_eq!(run(1, false), run(4, true));
    }
}

fn many_cohort_transfers(count: u64, cohorts: u64) -> (Vec<Container>, Vec<crate::SimpleTransfer>) {
    let mut holders = Vec::new();
    let mut transfers = Vec::new();
    for person in 1..=count {
        let mut source = vec![lot(1, 1, Some(1))];
        let mut destination = vec![lot(1, 1, Some(1))];
        for index in 0..cohorts {
            source.push(lot(1, 1, Some(2 * index + 4)));
            destination.push(lot(
                1,
                1,
                Some(2 * index + if index % 3 == 0 { 4 } else { 3 }),
            ));
        }
        holders.push(container(Place::Person(person), 3 * cohorts + 5, source));
        holders.push(container(
            Place::Person(count + person),
            if person % 7 == 0 {
                cohorts + 1
            } else {
                3 * cohorts + 5
            },
            destination,
        ));
        transfers.push(crate::SimpleTransfer {
            action: person,
            movement: Movement {
                id: person,
                from: Place::Person(person),
                to: Some(Place::Person(count + person)),
                good: 1,
                quantity: if person % 5 == 0 {
                    cohorts / 2
                } else {
                    cohorts
                },
            },
        });
    }
    holders.sort_unstable_by_key(|row| row.place);
    (holders, transfers)
}

#[test]
fn many_interleaved_dated_cohorts_match_the_general_arbiter() {
    let (holders, transfers) = many_cohort_transfers(128, 256);
    let actions: Vec<_> = transfers
        .iter()
        .map(|row| Transaction {
            id: row.action,
            movements: vec![row.movement],
            creations: vec![],
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut fast = store(holders.clone());
                let mut general = store(holders.clone());
                let draft = fast
                    .prepare_flat_transfers(&transfers, true)
                    .unwrap()
                    .unwrap();
                let reference = unpartitioned(
                    &general,
                    17,
                    &actions,
                    vec![container(Place::Inventory(99999), 10, vec![])],
                    &[],
                    true,
                );
                assert_eq!(draft.transactions(), reference.transactions());
                assert_eq!(draft.outcomes(), reference.outcomes());
                assert_eq!(draft.spoiled(), reference.spoiled());
                fast.commit(draft).unwrap();
                general.commit(reference).unwrap();
                let mut expected = general.export();
                expected
                    .containers
                    .retain(|row| row.place != Place::Inventory(99999));
                assert_eq!(fast.export(), expected);
                fast.export()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn ordered_change_runs_support_later_custody_addition_and_retirement() {
    let (holders, transfers) = many_cohort_transfers(128, 4);
    let actions: Vec<_> = transfers
        .iter()
        .map(|row| Transaction {
            id: row.action,
            movements: vec![row.movement],
            creations: vec![],
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut fast = store(holders.clone());
                let mut general = store(holders.clone());
                let mut staged = fast
                    .prepare_flat_transfers(&transfers, true)
                    .unwrap()
                    .unwrap();
                let stale = fast
                    .prepare_flat_transfers(&transfers, true)
                    .unwrap()
                    .unwrap();
                assert!(matches!(
                    &staged.inner.changes,
                    game_parallel_lot_planning::Changes::Runs(_)
                ));
                fast.add_empty_in(&mut staged, vec![]).unwrap();
                fast.retire_in(&mut staged, vec![]).unwrap();
                assert!(matches!(
                    &staged.inner.changes,
                    game_parallel_lot_planning::Changes::Runs(_)
                ));
                let added = vec![
                    container(Place::Inventory(10), 10, vec![]),
                    container(Place::Person(900), 10, vec![]),
                ];
                fast.add_empty_in(&mut staged, added.clone()).unwrap();
                let mut reference = unpartitioned(&general, 7, &actions, added, &[], true);
                assert_eq!(staged.outcomes(), reference.outcomes());
                assert_eq!(staged.spoiled(), reference.spoiled());
                fast.retire_in(&mut staged, vec![Place::Person(1)]).unwrap();
                general
                    .retire_in(&mut reference, vec![Place::Person(1)])
                    .unwrap();
                fast.commit(staged).unwrap();
                assert!(fast.commit(stale).is_err());
                general.commit(reference).unwrap();
                let state = fast.export();
                assert_eq!(state, general.export());
                assert_eq!(Store::compile(state.clone()).unwrap().export(), state);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn mixed_atomic_actions_match_general_arbitration() {
    let (mut initial, transfers) = many_cohort_transfers(256, 8);
    initial.extend([
        container(Place::Person(600), 10, vec![lot(1, 2, None)]),
        container(Place::Person(601), 10, vec![]),
        container(Place::Person(602), 10, vec![]),
    ]);
    let mut actions: Vec<_> = transfers
        .iter()
        .map(|row| Transaction {
            id: row.action,
            movements: vec![row.movement],
            creations: vec![],
        })
        .collect();
    actions.push(Transaction {
        id: 1000,
        movements: vec![
            Movement {
                id: 1000,
                from: Place::Person(1),
                to: Some(Place::Person(2)),
                good: 1,
                quantity: 2,
            },
            Movement {
                id: 1001,
                from: Place::Person(3),
                to: Some(Place::Person(4)),
                good: 1,
                quantity: 100,
            },
        ],
        creations: vec![Creation {
            id: 1002,
            to: Place::Person(5),
            lot: lot(1, 2, Some(4)),
        }],
    });
    actions.push(Transaction {
        id: 1003,
        movements: vec![Movement {
            id: 1003,
            from: Place::Person(6),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    });
    actions.push(Transaction {
        id: 1004,
        movements: vec![Movement {
            id: 1004,
            from: Place::Person(600),
            to: Some(Place::Person(601)),
            good: 1,
            quantity: 1,
        }],
        creations: vec![Creation {
            id: 1005,
            to: Place::Person(602),
            lot: lot(1, 2, Some(4)),
        }],
    });
    let run = |workers, reverse: bool, seed, advance, with_additions| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut actions = actions.clone();
                let additions = if with_additions {
                    actions.push(Transaction {
                        id: 1006,
                        movements: vec![Movement {
                            id: 1006,
                            from: Place::Person(10),
                            to: Some(Place::Person(550)),
                            good: 1,
                            quantity: 1,
                        }],
                        creations: vec![Creation {
                            id: 1007,
                            to: Place::Person(700),
                            lot: lot(1, 1, Some(4)),
                        }],
                    });
                    vec![
                        container(Place::Inventory(99999), 10, vec![]),
                        container(Place::Person(550), 10, vec![]),
                        container(Place::Person(700), 10, vec![]),
                    ]
                } else {
                    vec![]
                };
                if reverse {
                    actions.reverse();
                }
                let mut fast = store(initial.clone());
                let mut general = store(initial.clone());
                let staged = fast
                    .prepare_with_empty_compact(seed, &actions, additions.clone(), &[1003], advance)
                    .unwrap();
                let stale = fast
                    .prepare_with_empty_compact(seed, &actions, additions.clone(), &[1003], advance)
                    .unwrap();
                let full = unpartitioned(&general, seed, &actions, additions, &[1003], advance);
                assert_eq!(staged.transactions(), full.transactions());
                assert_eq!(staged.outcomes(), full.outcomes());
                assert_eq!(staged.creations(), full.creations());
                assert_eq!(staged.spoiled(), full.spoiled());
                assert!(
                    !staged
                        .transactions()
                        .iter()
                        .find(|row| row.id == 1000)
                        .unwrap()
                        .accepted
                );
                assert!(
                    staged
                        .transactions()
                        .iter()
                        .find(|row| row.id == 1004)
                        .unwrap()
                        .accepted
                );
                fast.commit(staged).unwrap();
                assert!(fast.commit(stale).is_err());
                general.commit(full).unwrap();
                let result = fast.export();
                assert_eq!(result, general.export());
                assert_eq!(Store::compile(result.clone()).unwrap().export(), result);
                result
            })
    };
    for seed in [1, 13] {
        for advance in [false, true] {
            for with_additions in [false, true] {
                assert_eq!(
                    run(1, false, seed, advance, with_additions),
                    run(4, true, seed, advance, with_additions)
                );
            }
        }
    }
    // IDs must still be checked across both partitions.
    actions.last_mut().unwrap().creations[0].id = 7;
    assert!(
        store(initial)
            .prepare_with_empty_compact(1, &actions, vec![], &[1003], true)
            .is_err()
    );
}

#[test]
#[ignore = "manual mixed atomic action benchmark"]
fn mixed_atomic_action_benchmark() {
    let count = std::env::var("LEGACY_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let with_additions = std::env::var_os("LEGACY_ADDITIONS").is_some();
    let additions = if with_additions {
        vec![container(Place::Inventory(1), 10, vec![])]
    } else {
        vec![]
    };
    let mut holders = Vec::new();
    let mut actions = Vec::new();
    for id in 1..=count {
        holders.push(container(Place::Person(id), 10, vec![lot(1, 2, Some(5))]));
        holders.push(container(Place::Person(count + id), 10, vec![]));
        actions.push(Transaction {
            id,
            movements: vec![Movement {
                id,
                from: Place::Person(id),
                to: Some(Place::Person(count + id)),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        });
    }
    actions.push(Transaction {
        id: count + 1,
        movements: vec![Movement {
            id: count + 1,
            from: Place::Person(1),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![Creation {
            id: count + 2,
            to: if with_additions {
                Place::Inventory(1)
            } else {
                Place::Person(2)
            },
            lot: lot(1, 1, Some(5)),
        }],
    });
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let owner = store(holders);
            let general = unpartitioned(&owner, 13, &actions, additions.clone(), &[], true);
            let partitioned = owner
                .prepare_with_empty(13, &actions, additions.clone(), true)
                .unwrap();
            assert_eq!(general.transactions(), partitioned.transactions());
            assert_eq!(general.outcomes(), partitioned.outcomes());
            assert_eq!(general.creations(), partitioned.creations());
            for mode in ["general", "partitioned"] {
                let mut samples = Vec::new();
                for round in 0..5 {
                    let start = std::time::Instant::now();
                    let draft = if mode == "general" {
                        unpartitioned(&owner, 13, &actions, additions.clone(), &[], true)
                    } else {
                        owner
                            .prepare_with_empty(13, &actions, additions.clone(), true)
                            .unwrap()
                    };
                    let elapsed = start.elapsed().as_secs_f64();
                    assert_eq!(draft.transactions().len(), actions.len());
                    if round > 0 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} transfers={count} additions={with_additions} mode={mode} prepare={:.6}",
                    samples[2]
                );
            }
        });
}

#[test]
#[ignore = "manual many-cohort physical transfer benchmark"]
fn many_cohort_transfer_benchmark() {
    let count = std::env::var("LEGACY_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256);
    let cohorts = std::env::var("LEGACY_COHORTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let (holders, transfers) = many_cohort_transfers(count, cohorts);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let owner = store(holders);
            let mut samples = Vec::new();
            for round in 0..6 {
                let start = std::time::Instant::now();
                let draft = owner
                    .prepare_flat_transfers(&transfers, true)
                    .unwrap()
                    .unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(draft.transactions().len(), count as usize);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} transfers={count} cohorts={cohorts} prepare={:.6}",
                samples[2]
            );
        });
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
fn a_partially_consumed_single_lot_retains_its_expiry_until_it_spoils() {
    let person = Place::Person(1);
    let mut store = store(vec![container(person, 10, vec![lot(1, 5, Some(2))])]);
    let draft = store
        .prepare(
            1,
            &[Movement {
                id: 1,
                from: person,
                to: None,
                good: 1,
                quantity: 2,
            }],
        )
        .unwrap();
    let (outcomes, _) = store.commit(draft).unwrap();
    assert!(outcomes[0].accepted);
    assert_eq!(store.container(person).unwrap().used, 3);
    assert_eq!(store.container(person).unwrap().next_expiry, Some(2));
    let draft = store.prepare(1, &[]).unwrap();
    let (_, spoiled) = store.commit(draft).unwrap();
    assert_eq!(spoiled[0].quantity, 3);
    assert_eq!(store.container(person).unwrap().used, 0);
    assert_eq!(store.container(person).unwrap().next_expiry, None);
}

#[test]
fn single_source_move_preserves_expiry_order_and_rejects_an_overfull_destination() {
    let source = Place::Inventory(1);
    let target = Place::Person(1);
    let initial = vec![
        container(
            source,
            20,
            vec![lot(1, 3, Some(1)), lot(1, 2, Some(3)), lot(1, 4, Some(5))],
        ),
        container(target, 4, vec![]),
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut store = store(initial.clone());
                let draft = store
                    .prepare(
                        42,
                        &[Movement {
                            id: 1,
                            from: source,
                            to: Some(target),
                            good: 1,
                            quantity: 4,
                        }],
                    )
                    .unwrap();
                let (outcomes, spoiled) = store.commit(draft).unwrap();
                assert!(outcomes[0].accepted);
                assert_eq!(spoiled.iter().map(|row| row.quantity).sum::<u64>(), 3);
                let state = store.export();
                assert_eq!(quantity(&state, target, 1, Some(3)), 2);
                assert_eq!(quantity(&state, target, 1, Some(5)), 2);
                assert_eq!(quantity(&state, source, 1, Some(5)), 2);
                state
            })
    };
    assert_eq!(run(1), run(4));
    let mut store = store(initial);
    let draft = store
        .prepare(
            42,
            &[Movement {
                id: 1,
                from: source,
                to: Some(target),
                good: 1,
                quantity: 5,
            }],
        )
        .unwrap();
    let (outcomes, _) = store.commit(draft).unwrap();
    assert!(!outcomes[0].accepted);
    assert_eq!(quantity(&store.export(), target, 1, Some(3)), 0);
    assert_eq!(quantity(&store.export(), source, 1, Some(3)), 2);
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
fn a_single_action_cannot_overfill_a_destination_with_multiple_legs() {
    let a = Place::Person(1);
    let b = Place::Person(2);
    let target = Place::Person(3);
    let initial = vec![
        container(a, 1, vec![lot(1, 1, None)]),
        container(b, 1, vec![lot(2, 1, None)]),
        container(target, 1, vec![]),
    ];
    let action = Transaction {
        id: 1,
        movements: vec![
            Movement {
                id: 1,
                from: a,
                to: Some(target),
                good: 1,
                quantity: 1,
            },
            Movement {
                id: 2,
                from: b,
                to: Some(target),
                good: 2,
                quantity: 1,
            },
        ],
        creations: vec![],
    };
    let mut store = store(initial);
    let draft = store.prepare_transactions_now(1, &[action]).unwrap();
    assert!(!draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    assert_eq!(store.container(target).unwrap().used, 0);
    assert_eq!(store.container(a).unwrap().used, 1);
    assert_eq!(store.container(b).unwrap().used, 1);
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
fn stock_only_views_match_full_views_at_both_expiry_boundaries() {
    let inventory = Place::Inventory(1);
    let person = Place::Person(1);
    let store = store(vec![
        container(inventory, 10, vec![lot(1, 3, None)]),
        container(
            Place::Stock { region: 1, good: 1 },
            10,
            vec![lot(1, 2, Some(1)), lot(1, 3, Some(4))],
        ),
        container(
            Place::Stock { region: 1, good: 2 },
            10,
            vec![lot(2, 1, None)],
        ),
        container(Place::Stock { region: 2, good: 1 }, 10, vec![]),
        container(person, 10, vec![lot(1, 1, None)]),
        container(Place::JobInput(1), 10, vec![]),
    ]);
    let before = store.export();
    for boundary in [0, 1] {
        let full = store.view(boundary).unwrap();
        let stocks = store.stock_view(boundary).unwrap();
        assert_eq!(
            (stocks.boundary, stocks.revision),
            (full.boundary, full.revision)
        );
        assert_eq!(
            stocks.containers,
            full.containers
                .into_iter()
                .filter(|row| matches!(row.place, Place::Stock { .. }))
                .collect::<Vec<_>>()
        );
        assert!(stocks.container(inventory).is_none());
        assert!(stocks.container(person).is_none());
    }
    assert!(store.stock_view(2).is_err());
    assert_eq!(store.export(), before);
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

#[test]
fn moving_receipts_out_of_a_draft_preserves_commit_and_stale_validation() {
    let source = Place::Person(1);
    let target = Place::Person(2);
    let mut store = store(vec![
        container(source, 10, vec![lot(1, 2, Some(1)), lot(1, 3, Some(5))]),
        container(target, 10, vec![]),
    ]);
    let action = Transaction {
        id: 7,
        movements: vec![Movement {
            id: 8,
            from: source,
            to: Some(target),
            good: 1,
            quantity: 2,
        }],
        creations: vec![],
    };
    let mut draft = store
        .prepare_with_empty(1, std::slice::from_ref(&action), vec![], true)
        .unwrap();
    let stale = store
        .prepare_with_empty(1, &[action], vec![], true)
        .unwrap();
    let expected_outcomes = draft.outcomes().to_vec();
    let expected_spoilage = draft.spoiled().to_vec();
    let (outcomes, spoiled) = draft.take_receipts();
    assert_eq!(outcomes, expected_outcomes);
    assert_eq!(spoiled, expected_spoilage);
    assert_eq!(outcomes[0].lots, vec![lot(1, 2, Some(5))]);
    assert_eq!(spoiled[0].quantity, 2);
    assert!(draft.outcomes().is_empty() && draft.spoiled().is_empty());
    let (returned, returned_spoilage) = store.commit(draft).unwrap();
    assert!(returned.is_empty() && returned_spoilage.is_empty());
    assert_eq!(quantity(&store.export(), target, 1, Some(5)), 2);
    assert!(store.commit(stale).is_err());
}

#[test]
fn inventory_views_aggregate_small_and_large_cohorts_at_each_boundary() {
    let lots: Vec<_> = (1..=5)
        .flat_map(|good| (1..=500).map(move |expiry| lot(good, u64::from(good), Some(expiry))))
        .collect();
    let store = store(vec![
        container(Place::Person(1), 10_000, lots.clone()),
        container(
            Place::Person(2),
            100,
            vec![lot(1, 2, Some(1)), lot(1, 3, Some(3)), lot(2, 4, None)],
        ),
    ]);
    let before = store.export();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let current = store.view(0).unwrap();
                let next = store.view(1).unwrap();
                for good in 1..=5 {
                    assert_eq!(
                        current.container(Place::Person(1)).unwrap().quantity(good),
                        u64::from(good) * 500
                    );
                    assert_eq!(
                        next.container(Place::Person(1)).unwrap().quantity(good),
                        u64::from(good) * 499
                    );
                }
                assert_eq!(current.container(Place::Person(2)).unwrap().used, 9);
                assert_eq!(next.container(Place::Person(2)).unwrap().used, 7);
                (current, next)
            })
    };
    assert_eq!(run(1), run(4));
    assert_eq!(store.export(), before);
}

#[test]
#[ignore = "manual independent physical transfer benchmark"]
fn independent_transfer_benchmark() {
    let count = std::env::var("LEGACY_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let mut initial = Vec::with_capacity((count * 2) as usize);
    let mut actions = Vec::with_capacity(count as usize);
    for id in 1..=count {
        initial.push(container(Place::Person(id), 10, vec![lot(1, 1, Some(5))]));
        actions.push(Transaction {
            id,
            movements: vec![Movement {
                id,
                from: Place::Person(id),
                to: Some(Place::Person(count + id)),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        });
    }
    initial.extend((count + 1..=count * 2).map(|id| container(Place::Person(id), 10, vec![])));
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let store = store(initial);
            let mut samples = Vec::new();
            for round in 0..6 {
                let start = std::time::Instant::now();
                let draft = store
                    .prepare_with_empty(13, &actions, vec![], true)
                    .unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(draft.transactions().len(), count as usize);
                assert!(draft.transactions().iter().all(|row| row.accepted));
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} transfers={count} prepare={:.6}",
                samples[2]
            );
        });
}

#[test]
#[ignore = "manual mixed physical transfer benchmark"]
fn partitioned_transfer_benchmark() {
    let count = std::env::var("LEGACY_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let mut initial = Vec::with_capacity((count * 2) as usize);
    let mut actions = Vec::with_capacity(count as usize);
    for id in 1..=count {
        initial.push(container(Place::Person(id), 10, vec![lot(1, 1, Some(5))]));
        actions.push(Transaction {
            id,
            movements: vec![Movement {
                id,
                from: Place::Person(if id == 2 { 1 } else { id }),
                to: Some(Place::Person(count + id)),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        });
    }
    initial.extend((count + 1..=count * 2).map(|id| container(Place::Person(id), 10, vec![])));
    let transfers: Vec<_> = actions
        .iter()
        .map(|action| crate::SimpleTransfer {
            action: action.id,
            movement: action.movements[0],
        })
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let owner = store(initial);
            let general = unpartitioned(&owner, 13, &actions, vec![], &[], true);
            let partitioned = owner
                .prepare_partitioned_flat(13, &transfers, true)
                .unwrap()
                .unwrap();
            assert_eq!(general.transactions(), partitioned.transactions());
            assert_eq!(general.outcomes(), partitioned.outcomes());
            for mode in ["general", "partitioned"] {
                let mut samples = Vec::new();
                for round in 0..5 {
                    let start = std::time::Instant::now();
                    let result = if mode == "general" {
                        unpartitioned(&owner, 13, &actions, vec![], &[], true)
                    } else {
                        owner
                            .prepare_partitioned_flat(13, &transfers, true)
                            .unwrap()
                            .unwrap()
                    };
                    let elapsed = start.elapsed().as_secs_f64();
                    assert_eq!(result.transactions().len(), count as usize);
                    if round > 0 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} transfers={count} mode={mode} prepare={:.6}",
                    samples[2]
                );
            }
        });
}
