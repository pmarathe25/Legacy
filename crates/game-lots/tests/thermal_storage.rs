use game_lot_model::{AgingRate, Freshness};
use game_lots::{
    Container, Lot, Movement, Place, Spoilage, State, StorageClimate, StorageTemperature, Store,
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
