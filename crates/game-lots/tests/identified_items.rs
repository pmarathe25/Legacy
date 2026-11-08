use game_lots::{
    Container, Creation, Lot, Movement, Place, SimpleTransfer, State, Store, Transaction,
};

fn item(id: u64) -> Lot {
    Lot {
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
            id,
            from: Place::Person(1),
            to,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    }
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
    let (state, actions) = independent_items(2048);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut owner = Store::compile(state.clone()).unwrap();
                let draft = owner.prepare_transactions(7, &actions).unwrap();
                assert!(draft.transactions().iter().all(|row| row.accepted));
                owner.commit(draft).unwrap();
                owner.export()
            })
    };
    assert_eq!(run(1), run(4));
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
            let (state, actions) = independent_items(count);
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
            println!("items={count} workers={workers} seconds={:.6}", samples[1]);
        });
}
