use crate::PackedCargo;
use game_parallel_goods_cargo::{Direction, GoodsCargo, GoodsHolding, Transfer};
use game_parallel_production_model::{Amount, Good, Inventory, PersonSnapshot, Stock};
#[test]
fn packed_transfers_match_sparse_cargo_with_mixed_actions_and_fail_atomically() {
    let count = 9_000u64;
    let people: Vec<PersonSnapshot> = (0..count)
        .map(|id| (id * 2 + 100, Some(1), 100, 0))
        .collect();
    let goods = vec![
        Good {
            id: 1,
            name: "wood".into(),
            food: None,
        },
        Good {
            id: 2,
            name: "tools".into(),
            food: None,
        },
    ];
    let cargo = GoodsCargo {
        capacity: 10,
        holdings: people
            .iter()
            .enumerate()
            .filter(|(index, _)| index % 3 == 0)
            .map(|(_, person)| GoodsHolding {
                person: person.0,
                items: vec![
                    Amount {
                        good: 1,
                        quantity: 2,
                    },
                    Amount {
                        good: 2,
                        quantity: 1,
                    },
                ]
                .into(),
            })
            .rev()
            .collect(),
    };
    let inventory = vec![Inventory {
        region: 1,
        stocks: vec![
            Stock {
                good: 1,
                quantity: count,
                capacity: count * 3,
            },
            Stock {
                good: 2,
                quantity: 0,
                capacity: count * 3,
            },
        ],
    }];
    let requests: Vec<_> = people
        .iter()
        .enumerate()
        .map(|(index, person)| Transfer {
            person: person.0,
            good: 1,
            quantity: 1,
            direction: if index % 3 == 0 {
                Direction::Deposit
            } else {
                Direction::Pickup
            },
        })
        .rev()
        .collect();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut packed = PackedCargo::from_cargo(&cargo, &people);
        let mut stocks = inventory.clone();
        let expected = pool
            .install(|| cargo.transfer(&inventory, &people, &goods, &[], &requests))
            .unwrap();
        pool.install(|| packed.transfer(&mut stocks, &people, &[], &requests))
            .unwrap();
        assert_eq!(packed.cargo(&people), expected.0);
        assert_eq!(stocks, expected.1);
        let before = packed.cargo(&people);
        let before_stocks = stocks.clone();
        let duplicate = [requests[0], requests[0]];
        assert!(
            packed
                .transfer(&mut stocks, &people, &[], &duplicate)
                .is_err()
        );
        assert_eq!(packed.cargo(&people), before);
        assert_eq!(stocks, before_stocks);
    }
}

#[test]
fn uniform_dense_transfers_match_owner_and_reject_whole_batch_on_failure() {
    let count = 6000u64;
    let people: Vec<PersonSnapshot> = (0..count).map(|id| (id, Some(1), 100, 0)).collect();
    let goods = vec![Good {
        id: 1,
        name: "wood".into(),
        food: None,
    }];
    let cargo = GoodsCargo {
        capacity: 4,
        holdings: vec![],
    };
    let inventory = vec![Inventory {
        region: 1,
        stocks: vec![Stock {
            good: 1,
            quantity: count,
            capacity: count * 2,
        }],
    }];
    let take: Vec<_> = (0..count)
        .map(|person| Transfer {
            person,
            good: 1,
            quantity: 1,
            direction: Direction::Pickup,
        })
        .collect();
    let put: Vec<_> = take
        .iter()
        .map(|row| Transfer {
            direction: Direction::Deposit,
            ..*row
        })
        .collect();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut packed = PackedCargo::from_cargo(&cargo, &people);
        let mut stocks = inventory.clone();
        let before = packed.cargo(&people);
        assert!(
            pool.install(|| packed.transfer(&mut stocks, &people, &[123], &take))
                .is_err()
        );
        assert_eq!(packed.cargo(&people), before);
        assert_eq!(stocks, inventory);
        let expected = pool
            .install(|| cargo.transfer(&inventory, &people, &goods, &[], &take))
            .unwrap();
        pool.install(|| packed.transfer(&mut stocks, &people, &[], &take))
            .unwrap();
        assert_eq!(packed.cargo(&people), expected.0);
        assert_eq!(stocks, expected.1);
        let expected = pool
            .install(|| expected.0.transfer(&expected.1, &people, &goods, &[], &put))
            .unwrap();
        pool.install(|| packed.transfer(&mut stocks, &people, &[], &put))
            .unwrap();
        assert_eq!(packed.cargo(&people), expected.0);
        assert_eq!(stocks, expected.1);
    }
}
