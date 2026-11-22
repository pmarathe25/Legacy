use game_lot_model::{AgingRate, Freshness};
use game_lots::{
    Container, Lot, Movement, Place, Spoilage, State, StorageClimate, StorageTemperature, Store,
    Transaction,
};

#[test]
fn moving_part_of_a_batch_preserves_its_remaining_age_across_climates() {
    let run = |workers, resume| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut opening = State {
                    tick: 0,
                    revision: 0,
                    next_batch: None,
                    expiry_millis: Some(0),
                    storage_climate: Some(StorageClimate {
                        ambient_celsius: 20,
                        places: vec![
                            StorageTemperature {
                                place: Place::Person(3),
                                celsius: 36,
                            },
                            StorageTemperature {
                                place: Place::Person(2),
                                celsius: 4,
                            },
                        ],
                        aging_rates: vec![
                            AgingRate {
                                from_celsius: 35,
                                per_thousand: 2100,
                            },
                            AgingRate {
                                from_celsius: -20,
                                per_thousand: 500,
                            },
                            AgingRate {
                                from_celsius: 10,
                                per_thousand: 1000,
                            },
                        ],
                    }),
                    containers: vec![
                        Container::new(
                            Place::Person(1),
                            10,
                            vec![Lot {
                                good: 1,
                                quantity: 4,
                                batch: Some(9),
                                item: None,
                                expires_at: Some(20),
                                freshness: Freshness::Standard,
                            }],
                        )
                        .unwrap(),
                        Container::new(Place::Person(2), 10, vec![]).unwrap(),
                        Container::new(Place::Person(3), 10, vec![]).unwrap(),
                    ],
                };
                Store::date_authored_custody(&mut opening).unwrap();
                let mut store = Store::compile(opening).unwrap();
                let mut receipts = Vec::new();
                let advance = |store: &mut Store, receipts: &mut Vec<Spoilage>, at| {
                    let stage = store.prepare_expiry_at(at).unwrap();
                    receipts.extend(store.commit_expiry_at(stage).unwrap());
                };
                let transfer = |store: &mut Store, from, to, quantity| {
                    let stage = store
                        .prepare_now(
                            7,
                            &[Movement {
                                id: 1,
                                from: Place::Person(from),
                                to: Some(Place::Person(to)),
                                good: 1,
                                quantity,
                                batch: Some(9),
                                item: None,
                            }],
                        )
                        .unwrap();
                    assert!(store.commit(stage).unwrap().0[0].accepted);
                };
                advance(&mut store, &mut receipts, 7);
                transfer(&mut store, 1, 3, 2);
                assert_eq!(store.next_expiry(), Some(14));
                let moved = &store.container(Place::Person(3)).unwrap().lots[0];
                assert_eq!(moved.freshness, Freshness::Thermal { slack: 1700 });
                if resume {
                    let save = serde_json::to_string(&store.export()).unwrap();
                    store = Store::compile(serde_json::from_str(&save).unwrap()).unwrap();
                    advance(&mut store, &mut receipts, 9);
                }
                advance(&mut store, &mut receipts, 10);
                transfer(&mut store, 3, 2, 2);
                transfer(&mut store, 2, 1, 1);
                assert_eq!(store.next_expiry(), Some(17));
                for at in [17, 20, 24] {
                    advance(&mut store, &mut receipts, at);
                }
                (store.export(), receipts)
            })
    };
    let whole = run(1, false);
    assert_eq!(whole, run(4, true));
    let (_, receipts) = whole;
    let consumed: Vec<_> = receipts
        .into_iter()
        .map(|row| (row.place, row.quantity, row.expires_at, row.batch))
        .collect();
    assert_eq!(
        consumed,
        [
            (Place::Person(1), 1, 17, Some(9)),
            (Place::Person(1), 2, 20, Some(9)),
            (Place::Person(2), 1, 24, Some(9)),
        ]
    );
}

#[test]
fn storage_changes_share_a_draft_with_custody_and_keep_remaining_age() {
    let run = |workers, round_trip| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let source = Place::Person(1);
                let destination = Place::Inventory(3);
                let opening = State {
                    tick: 0,
                    revision: 0,
                    next_batch: None,
                    expiry_millis: Some(0),
                    storage_climate: Some(StorageClimate {
                        ambient_celsius: 20,
                        places: Vec::new(),
                        aging_rates: vec![
                            AgingRate {
                                from_celsius: -20,
                                per_thousand: 500,
                            },
                            AgingRate {
                                from_celsius: 10,
                                per_thousand: 1000,
                            },
                            AgingRate {
                                from_celsius: 25,
                                per_thousand: 2000,
                            },
                        ],
                    }),
                    containers: vec![
                        Container::new(
                            source,
                            3,
                            vec![Lot {
                                good: 1,
                                quantity: 2,
                                batch: Some(9),
                                item: None,
                                expires_at: Some(20),
                                freshness: Freshness::Standard,
                            }],
                        )
                        .unwrap(),
                    ],
                };
                let mut store = Store::compile(opening).unwrap();
                let mut construction = store.prepare_transactions_now(1, &[]).unwrap();
                store
                    .add_empty_in(
                        &mut construction,
                        vec![Container::new(destination, 3, vec![]).unwrap()],
                    )
                    .unwrap();
                store
                    .set_holder_temperatures_in(&mut construction, &[(destination, Some(4))])
                    .unwrap();
                assert!(store.container(destination).is_none());
                store.commit(construction).unwrap();
                assert_eq!(store.storage_celsius(destination), 4);

                let move_one = || Transaction {
                    id: 1,
                    movements: vec![Movement {
                        id: 1,
                        from: source,
                        to: Some(destination),
                        good: 1,
                        quantity: 1,
                        batch: Some(9),
                        item: None,
                    }],
                    creations: Vec::new(),
                };
                let advance = |store: &mut Store, at| {
                    let draft = store.prepare_expiry_at(at).unwrap();
                    assert!(store.commit_expiry_at(draft).unwrap().is_empty());
                };
                advance(&mut store, 7);
                let first_move = store.prepare_transactions_now(1, &[move_one()]).unwrap();
                store.commit(first_move).unwrap();
                assert_eq!(
                    store.container(destination).unwrap().lots[0].expires_at,
                    Some(33)
                );
                if round_trip {
                    advance(&mut store, 11);
                    let checkpoint = serde_json::to_vec(&store.export()).unwrap();
                    store = Store::compile(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                }
                advance(&mut store, 17);
                let before = store.revision();
                let stale = store.prepare_transactions_now(1, &[]).unwrap();
                let mut collapse = store.prepare_transactions_now(1, &[move_one()]).unwrap();
                store
                    .set_holder_temperatures_in(&mut collapse, &[(destination, None)])
                    .unwrap();
                store.commit(collapse).unwrap();
                assert_eq!(store.revision(), before + 1);
                assert!(store.commit(stale).is_err());
                assert_eq!(store.storage_celsius(destination), 20);
                let ages = |store: &Store| {
                    store
                        .container(destination)
                        .unwrap()
                        .lots
                        .iter()
                        .map(|lot| (lot.expires_at, lot.quantity, lot.batch))
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    ages(&store),
                    [(Some(20), 1, Some(9)), (Some(25), 1, Some(9))]
                );
                store.change_ambient_temperature(28).unwrap();
                assert_eq!(
                    ages(&store),
                    [(Some(19), 1, Some(9)), (Some(21), 1, Some(9))]
                );
                store.export()
            })
    };
    let whole = run(1, false);
    let split = run(6, true);
    assert_eq!(whole.containers, split.containers);
    assert_eq!(whole.storage_climate, split.storage_climate);
    assert_eq!(whole.expiry_millis, split.expiry_millis);
}
