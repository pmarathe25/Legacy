use game_lots::{
    Container, Creation, Lot, Movement, Place, SimpleTransfer, State, Store, Transaction,
};

fn item(id: u64) -> Lot {
    Lot {
        batch: None,
        good: 1,
        quantity: 1,
        item: Some(id),
        expires_at: None,
    }
}

fn store() -> Store {
    Store::compile(State {
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(Place::Person(1), 4, vec![item(10), item(11)]).unwrap(),
            Container::new(Place::Person(2), 4, vec![item(12)]).unwrap(),
        ],
    })
    .unwrap()
}

fn transfer(id: u64, to: Option<Place>) -> Transaction {
    Transaction {
        id,
        movements: vec![Movement {
            batch: None,
            item: None,
            id,
            from: Place::Person(1),
            to,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    }
}

fn exact(id: u64, target: Option<Place>, item: u64) -> Transaction {
    let mut action = transfer(id, target);
    action.movements[0].item = Some(item);
    action
}

fn seed_with_first_before_second() -> u64 {
    (0..1000)
        .find(|&seed| game_genetics::random(seed, 1, 1) < game_genetics::random(seed, 2, 1))
        .unwrap()
}

#[test]
fn exact_selection_moves_the_requested_page_and_rejects_missing_items() {
    let mut owner = store();
    let draft = owner
        .prepare_transactions(1, &[exact(1, Some(Place::Person(2)), 11)])
        .unwrap();
    assert!(draft.transactions()[0].accepted);
    assert_eq!(draft.outcomes()[0].lots[0].item, Some(11));
    owner.commit(draft).unwrap();
    assert_eq!(owner.item_place(11), Some(Place::Person(2)));
    assert_eq!(owner.item_place(10), Some(Place::Person(1)));
    let draft = owner
        .prepare_transactions(2, &[exact(2, Some(Place::Person(2)), 11)])
        .unwrap();
    assert!(!draft.transactions()[0].accepted);
    assert!(draft.item_updates().is_empty());
}

#[test]
fn independent_consumption_preserves_identified_item_tracking() {
    let containers = (1..=256)
        .map(|id| Container::new(Place::Person(id), 1, vec![item(id)]).unwrap())
        .collect();
    let mut owner = Store::compile(State {
        tick: 0,
        revision: 0,
        containers,
    })
    .unwrap();
    let actions: Vec<_> = (1..=256)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                batch: None,
                id,
                from: Place::Person(id),
                to: None,
                good: 1,
                quantity: 1,
                item: None,
            }],
            creations: vec![],
        })
        .collect();
    let draft = owner.prepare_transactions_now(7, &actions).unwrap();
    assert!(draft.transactions().iter().all(|row| row.accepted));
    assert_eq!(draft.item_updates().len(), 256);
    owner.commit(draft).unwrap();
    for id in 1..=256 {
        assert_eq!(owner.item_place(id), None);
    }
    Store::compile(owner.export()).unwrap();
}

#[test]
fn a_rejected_exact_reservation_does_not_change_a_later_generic_allocation() {
    let mut owner = Store::compile(State {
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(Place::Person(1), 2, vec![item(10), item(11)]).unwrap(),
            Container::new(Place::Person(2), 1, vec![item(12)]).unwrap(),
            Container::new(Place::Person(3), 1, vec![]).unwrap(),
        ],
    })
    .unwrap();
    let draft = owner
        .prepare_transactions(
            seed_with_first_before_second(),
            &[
                exact(1, Some(Place::Person(2)), 10),
                transfer(2, Some(Place::Person(3))),
            ],
        )
        .unwrap();
    assert!(!draft.transactions()[0].accepted);
    assert!(draft.transactions()[1].accepted);
    assert_eq!(draft.outcomes()[1].lots[0].item, Some(11));
    owner.commit(draft).unwrap();
    assert_eq!(owner.item_place(10), Some(Place::Person(1)));
    assert_eq!(owner.item_place(11), Some(Place::Person(3)));
}

#[test]
fn earlier_generic_claim_can_take_the_page_a_later_exact_claim_wants() {
    let mut owner = store();
    let draft = owner
        .prepare_transactions(
            seed_with_first_before_second(),
            &[
                transfer(1, Some(Place::Person(2))),
                exact(2, Some(Place::Person(2)), 10),
            ],
        )
        .unwrap();
    assert!(draft.transactions()[0].accepted);
    assert!(!draft.transactions()[1].accepted);
    assert_eq!(draft.outcomes()[0].lots[0].item, Some(10));
    owner.commit(draft).unwrap();
    assert_eq!(owner.item_place(10), Some(Place::Person(2)));
    assert_eq!(owner.item_place(11), Some(Place::Person(1)));
}

#[test]
fn duplicate_exact_legs_reject_the_entire_transaction() {
    let mut owner = store();
    let mut action = exact(1, None, 10);
    let mut other = action.movements[0];
    other.id = 2;
    action.movements.push(other);
    let draft = owner.prepare_transactions(1, &[action]).unwrap();
    assert!(!draft.transactions()[0].accepted);
    assert!(draft.item_updates().is_empty());
    owner.commit(draft).unwrap();
    assert_eq!(owner.item_place(10), Some(Place::Person(1)));
}

#[test]
fn compact_exact_consumption_removes_only_the_selected_item() {
    let mut owner = store();
    let mut draft = owner
        .prepare_with_empty_compact(1, &[exact(1, None, 11)], vec![], &[1], true)
        .unwrap();
    assert!(draft.transactions()[0].accepted);
    assert!(draft.outcomes()[0].lots.is_empty());
    assert_eq!(draft.item_updates(), &[(11, None)]);
    draft.take_receipts();
    owner.commit(draft).unwrap();
    assert_eq!(owner.item_place(10), Some(Place::Person(1)));
    assert_eq!(owner.item_place(11), None);
}

#[test]
fn item_identity_survives_transfer_and_stale_drafts_cannot_change_it() {
    let mut owner = store();
    let action = transfer(1, Some(Place::Person(2)));
    let stale = owner
        .prepare_transactions(1, std::slice::from_ref(&action))
        .unwrap();
    let draft = owner.prepare_transactions(1, &[action]).unwrap();
    assert_eq!(draft.outcomes()[0].lots[0].item, Some(10));
    assert_eq!(draft.item_updates(), &[(10, Some(Place::Person(2)))]);
    owner.commit(draft).unwrap();
    assert!(owner.commit(stale).is_err());
    let state = owner.export();
    assert_eq!(owner.item_place(10), Some(Place::Person(2)));
    assert_eq!(state.containers[0].lots, vec![item(11)]);
    assert_eq!(state.containers[1].lots, vec![item(10), item(12)]);
    let json = serde_json::to_string(&state).unwrap();
    Store::compile(serde_json::from_str(&json).unwrap()).unwrap();
}

#[test]
fn next_item_identity_skips_live_ids_and_detects_overflow() {
    let owner = store();
    assert_eq!(owner.next_unused_item(0), None);
    assert_eq!(owner.next_unused_item(1), Some(1));
    assert_eq!(owner.next_unused_item(10), Some(13));
    assert_eq!(owner.next_unused_items(9, 5), Some(vec![9, 13, 14, 15, 16]));
    assert_eq!(owner.next_unused_items(0, 0), None);
    assert_eq!(owner.next_unused_items(u64::MAX, 2), None);
    let owner = Store::compile(State {
        tick: 0,
        revision: 0,
        containers: vec![Container::new(Place::Person(1), 1, vec![item(u64::MAX)]).unwrap()],
    })
    .unwrap();
    assert_eq!(owner.next_unused_item(u64::MAX), None);
    assert_eq!(owner.next_unused_items(u64::MAX, 1), None);
}

#[test]
fn item_lookup_falls_back_when_identifiers_have_gaps() {
    let owner = Store::compile(State {
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(Place::Person(1), 3, vec![item(10), item(12), item(13)]).unwrap(),
        ],
    })
    .unwrap();
    assert_eq!(owner.item_place(10), Some(Place::Person(1)));
    assert_eq!(owner.item_place(11), None);
    assert_eq!(owner.item_place(12), Some(Place::Person(1)));
    assert_eq!(owner.item_place(13), Some(Place::Person(1)));
}

#[test]
fn duplicate_identity_and_invalid_items_are_rejected() {
    assert!(
        Container::new(
            Place::Person(1),
            4,
            vec![Lot {
                quantity: 2,
                ..item(1)
            }]
        )
        .is_err()
    );
    assert!(
        Container::new(
            Place::Person(1),
            4,
            vec![Lot {
                expires_at: Some(2),
                ..item(1)
            }]
        )
        .is_err()
    );
    assert!(
        Store::compile(State {
            tick: 0,
            revision: 0,
            containers: vec![
                Container::new(Place::Person(1), 4, vec![item(1)]).unwrap(),
                Container::new(Place::Person(2), 4, vec![item(1)]).unwrap(),
            ]
        })
        .is_err()
    );
    let owner = store();
    let creation = |id, identity| Transaction {
        id,
        movements: vec![],
        creations: vec![Creation {
            id,
            to: Place::Person(2),
            lot: item(identity),
        }],
    };
    assert!(owner.prepare_transactions(1, &[creation(1, 10)]).is_err());
    assert!(
        owner
            .prepare_transactions(1, &[creation(1, 20), creation(2, 20)])
            .is_err()
    );
}

#[test]
fn unchecked_batch_entry_points_do_not_bypass_item_tracking() {
    let state = store().export();
    let action = transfer(1, Some(Place::Person(2)));
    assert!(
        game_lot_batches::prepare(
            &state,
            1,
            1,
            std::slice::from_ref(&action),
            true,
            vec![],
            &[]
        )
        .is_err()
    );
    assert!(
        game_lot_batches::prepare_unpartitioned(&state, 1, 1, &[action], true, vec![], &[])
            .is_err()
    );
}

#[test]
fn compact_consumption_retires_identity_even_if_receipts_are_drained() {
    let mut owner = store();
    let mut draft = owner
        .prepare_with_empty_compact(1, &[transfer(1, None)], vec![], &[1], true)
        .unwrap();
    draft.take_receipts();
    assert!(draft.item_updates().contains(&(10, None)));
    owner.commit(draft).unwrap();
    assert_eq!(owner.item_place(10), None);
    let creation = Transaction {
        id: 2,
        movements: vec![],
        creations: vec![Creation {
            id: 2,
            to: Place::Person(2),
            lot: item(10),
        }],
    };
    assert!(owner.prepare_transactions(2, &[creation]).is_ok());
}

#[test]
fn full_custody_can_replace_a_consumed_item_atomically() {
    let mut owner = Store::compile(State {
        tick: 0,
        revision: 0,
        containers: vec![Container::new(Place::Person(1), 1, vec![item(10)]).unwrap()],
    })
    .unwrap();
    let action = Transaction {
        id: 1,
        movements: vec![Movement {
            batch: None,
            item: None,
            id: 1,
            from: Place::Person(1),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![Creation {
            id: 2,
            to: Place::Person(1),
            lot: item(20),
        }],
    };
    let draft = owner.prepare_transactions(1, &[action]).unwrap();
    assert!(draft.transactions()[0].accepted);
    owner.commit(draft).unwrap();
    assert_eq!(
        owner.container(Place::Person(1)).unwrap().lots,
        vec![item(20)]
    );
    assert_eq!(owner.item_place(10), None);
    assert_eq!(owner.item_place(20), Some(Place::Person(1)));
}

fn independent_items(count: u64) -> (State, Vec<Transaction>) {
    let mut containers = Vec::new();
    let mut actions = Vec::new();
    for id in 1..=count {
        let source = Place::Person(id * 2 - 1);
        let destination = Place::Person(id * 2);
        containers.push(Container::new(source, 1, vec![item(id)]).unwrap());
        containers.push(Container::new(destination, 1, vec![]).unwrap());
        actions.push(Transaction {
            id,
            movements: vec![Movement {
                batch: None,
                item: None,
                id,
                from: source,
                to: Some(destination),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        });
    }
    (
        State {
            tick: 0,
            revision: 0,
            containers,
        },
        actions,
    )
}

#[test]
fn independent_identified_transfers_match_across_worker_counts() {
    let (state, mut actions) = independent_items(2048);
    let run = |workers, actions: &[Transaction]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut owner = Store::compile(state.clone()).unwrap();
                let draft = owner.prepare_transactions(7, actions).unwrap();
                assert!(draft.transactions().iter().all(|row| row.accepted));
                owner.commit(draft).unwrap();
                owner.export()
            })
    };
    assert_eq!(run(1, &actions), run(4, &actions));
    for action in &mut actions {
        action.movements[0].item = Some(action.id);
    }
    assert_eq!(run(1, &actions), run(4, &actions));
}

#[test]
fn unrelated_fungible_transfers_keep_the_fast_path_in_a_world_with_items() {
    let mut containers = vec![Container::new(Place::Person(1), 1, vec![item(1)]).unwrap()];
    let mut transfers = Vec::new();
    for id in 1..=256 {
        let source = Place::Person(2 * id);
        let destination = Place::Person(2 * id + 1);
        containers.push(
            Container::new(
                source,
                1,
                vec![Lot {
                    batch: None,
                    good: 2,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                }],
            )
            .unwrap(),
        );
        containers.push(Container::new(destination, 1, vec![]).unwrap());
        transfers.push(SimpleTransfer {
            action: id,
            movement: Movement {
                batch: None,
                item: None,
                id,
                from: source,
                to: Some(destination),
                good: 2,
                quantity: 1,
            },
        });
    }
    let mut owner = Store::compile(State {
        tick: 0,
        revision: 0,
        containers,
    })
    .unwrap();
    let draft = owner
        .prepare_flat_transfers(&transfers, true)
        .unwrap()
        .expect("unrelated items must not disable the fast path");
    owner.commit(draft).unwrap();
    assert_eq!(
        owner.container(Place::Person(1)).unwrap().lots,
        vec![item(1)]
    );
    Store::compile(owner.export()).unwrap();
}

#[test]
#[ignore = "performance measurement"]
fn independent_identified_transfers_benchmark() {
    let count = std::env::var("LEGACY_ITEMS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let (state, mut actions) = independent_items(count);
            let exact = std::env::var_os("LEGACY_EXACT").is_some();
            if exact {
                for action in &mut actions {
                    action.movements[0].item = Some(action.id);
                }
            }
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut owner = Store::compile(state.clone()).unwrap();
                let start = std::time::Instant::now();
                let draft = owner.prepare_transactions(7, &actions).unwrap();
                assert!(draft.transactions().iter().all(|row| row.accepted));
                owner.commit(draft).unwrap();
                if round > 0 {
                    samples.push(start.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "items={count} workers={workers} exact={exact} seconds={:.6}",
                samples[1]
            );
        });
}
