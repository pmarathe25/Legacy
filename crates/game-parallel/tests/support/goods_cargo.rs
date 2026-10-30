use game_parallel::{GoodsDirection, GoodsTransfer, Move, StartWork, World};
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap()
}
fn transfer(person: u64, good: u32, quantity: u64, direction: GoodsDirection) -> GoodsTransfer {
    GoodsTransfer {
        person,
        good,
        quantity,
        direction,
    }
}
#[test]
fn produced_goods_follow_a_person_across_a_route_and_are_deposited() {
    let mut w = world();
    w.start_work(&[StartWork {
        id: 1,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    }])
    .unwrap();
    let before = w.clone();
    assert!(
        w.transfer_goods(&[transfer(1, 1, 1, GoodsDirection::Pickup)])
            .is_err()
    );
    assert_eq!(w, before);
    w.step().unwrap();
    w.step().unwrap();
    w.transfer_goods(&[transfer(1, 2, 2, GoodsDirection::Pickup)])
        .unwrap();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.step().unwrap();
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    assert_eq!(
        w.goods_cargo.as_ref().unwrap().holdings[0].items[0].quantity,
        2
    );
    assert!(
        w.transfer_goods(&[transfer(1, 2, 2, GoodsDirection::Deposit)])
            .is_err()
    );
    w.step().unwrap();
    w.transfer_goods(&[transfer(1, 2, 2, GoodsDirection::Deposit)])
        .unwrap();
    assert!(w.goods_cargo.as_ref().unwrap().holdings.is_empty());
    assert_eq!(
        w.production.as_ref().unwrap().inventories[1].stocks[1].quantity,
        2
    );
    w.validate().unwrap();
}
#[test]
fn batch_conflicts_and_capacity_fail_without_partial_transfers() {
    let mut w = world();
    let before = w.clone();
    assert!(
        w.transfer_goods(&[
            transfer(1, 1, 5, GoodsDirection::Pickup),
            transfer(2, 1, 5, GoodsDirection::Pickup)
        ])
        .is_err()
    );
    assert_eq!(w, before);
    assert!(
        w.transfer_goods(&[transfer(1, 1, 11, GoodsDirection::Pickup)])
            .is_err()
    );
    assert_eq!(w, before);
    w.transfer_goods(&[
        transfer(1, 1, 4, GoodsDirection::Pickup),
        transfer(2, 1, 5, GoodsDirection::Pickup),
    ])
    .unwrap();
    assert_eq!(
        w.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        0
    );
    assert_eq!(
        w.goods_cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .map(|h| h.items[0].quantity)
            .sum::<u64>(),
        9
    );
    let before = w.clone();
    assert!(
        w.transfer_goods(&[
            transfer(1, 1, 1, GoodsDirection::Deposit),
            transfer(1, 1, 1, GoodsDirection::Deposit)
        ])
        .is_err()
    );
    assert_eq!(w, before);
}

#[test]
fn large_competing_goods_pickups_conserve_stock_across_workers() {
    use game_parallel::Person;
    let mut initial = world();
    let count = 5_000u64;
    initial.regions[0].people = (100..count + 100)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    let stock = &mut initial.production.as_mut().unwrap().inventories[0].stocks[0];
    stock.capacity = count;
    stock.quantity = count;
    let requests: Vec<_> = (100..count + 100)
        .map(|id| transfer(id, 1, 1, GoodsDirection::Pickup))
        .collect();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        let mut batch = requests.clone();
        if threads == 4 {
            batch.reverse();
        }
        pool.install(|| w.transfer_goods(&batch).unwrap());
        let cargo = w.goods_cargo.as_ref().unwrap();
        assert_eq!(cargo.holdings.len(), count as usize);
        assert_eq!(
            cargo
                .holdings
                .iter()
                .map(|h| h.items[0].quantity)
                .sum::<u64>(),
            count
        );
        assert_eq!(
            w.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
            0
        );
        w.validate().unwrap();
        results.push(w);
    }
    assert_eq!(results[0], results[1]);
}
#[test]
fn invalid_carried_goods_reject_tick_without_mutation() {
    use game_parallel::{Amount, GoodsHolding};
    let mut w = world();
    w.goods_cargo.as_mut().unwrap().holdings.push(GoodsHolding {
        person: 1,
        items: vec![Amount {
            good: 999,
            quantity: 1,
        }]
        .into(),
    });
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn deceased_cargo_remains_physical_until_local_recovery() {
    let mut w = world();
    w.transfer_goods(&[transfer(1, 1, 4, GoodsDirection::Pickup)])
        .unwrap();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.travelers[0].person.health = 0;
    let before = w.clone();
    assert!(w.recover_goods(2, 1, 1, 4).is_err());
    assert_eq!(w, before);
    w.step().unwrap();
    w.step().unwrap();
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.recover_goods(3, 1, 1, 4).unwrap();
    assert!(w.goods_cargo.as_ref().unwrap().holdings.is_empty());
    assert_eq!(
        w.production.as_ref().unwrap().inventories[1].stocks[0].quantity,
        4
    );
}

#[test]
fn accepted_exchange_is_atomic_and_transfers_only_owned_possessions() {
    use game_parallel::{Amount, GoodsExchange};
    let mut w = world();
    w.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 2;
    w.transfer_goods(&[
        transfer(1, 1, 4, GoodsDirection::Pickup),
        transfer(2, 2, 2, GoodsDirection::Pickup),
    ])
    .unwrap();
    let good = |id, quantity| Amount { good: id, quantity };
    let deal = GoodsExchange {
        a: 1,
        b: 2,
        offer_a: vec![good(1, 3)].into(),
        offer_b: vec![good(2, 2)].into(),
    };
    let mut invalid = deal.clone();
    invalid.offer_b[0].quantity = 3;
    let before = w.clone();
    assert!(w.exchange_goods(&[invalid]).is_err());
    assert_eq!(w, before);
    assert!(w.exchange_goods(&[deal.clone(), deal.clone()]).is_err());
    assert_eq!(w, before);
    w.exchange_goods(&[deal]).unwrap();
    let holdings = &w.goods_cargo.as_ref().unwrap().holdings;
    assert_eq!(holdings[0].items.as_slice(), &[good(1, 1), good(2, 2)]);
    assert_eq!(holdings[1].items.as_slice(), &[good(1, 3)]);
    w.validate().unwrap();
}
#[test]
fn exchange_cannot_use_remote_dead_or_busy_participants() {
    use game_parallel::{Amount, GoodsExchange};
    let mut w = world();
    w.transfer_goods(&[transfer(1, 1, 1, GoodsDirection::Pickup)])
        .unwrap();
    let deal = GoodsExchange {
        a: 1,
        b: 2,
        offer_a: vec![Amount {
            good: 1,
            quantity: 1,
        }]
        .into(),
        offer_b: vec![].into(),
    };
    w.move_people(&[Move {
        person: 2,
        destination: 2,
    }])
    .unwrap();
    let before = w.clone();
    assert!(w.exchange_goods(std::slice::from_ref(&deal)).is_err());
    assert_eq!(w, before);
    w.travelers[0].person.health = 0;
    w.step().unwrap();
    w.step().unwrap();
    let before = w.clone();
    assert!(w.exchange_goods(&[deal]).is_err());
    assert_eq!(w, before);
}

#[test]
fn independent_exchanges_are_deterministic_at_scale() {
    use game_parallel::{Amount, GoodsExchange, GoodsHolding, Person};
    let mut initial = world();
    let count = 2_000u64;
    initial.regions[0].people = (100..100 + 2 * count)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    initial.goods_cargo.as_mut().unwrap().holdings = (0..2 * count)
        .map(|index| GoodsHolding {
            person: index + 100,
            items: vec![Amount {
                good: (index % 2 + 1) as u32,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let batch: Vec<_> = (0..count)
        .map(|pair| GoodsExchange {
            a: 100 + pair * 2,
            b: 101 + pair * 2,
            offer_a: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
            offer_b: vec![Amount {
                good: 2,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        let mut requests = batch.clone();
        if threads == 4 {
            requests.reverse();
        }
        pool.install(|| w.exchange_goods(&requests).unwrap());
        let holdings = &w.goods_cargo.as_ref().unwrap().holdings;
        assert_eq!(holdings.len(), (count * 2) as usize);
        assert!(
            holdings
                .iter()
                .enumerate()
                .all(|(index, h)| h.items[0].good == if index % 2 == 0 { 2 } else { 1 })
        );
        w.validate().unwrap();
        results.push(w);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn sparse_unordered_holdings_are_resolved_without_losing_unmodified_people() {
    use game_parallel::{Amount, GoodsHolding, Person};
    let mut initial = world();
    initial.regions[0].people = [1, 9, 30]
        .into_iter()
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    initial.goods_cargo.as_mut().unwrap().holdings = [30, 1, 9]
        .into_iter()
        .map(|person| GoodsHolding {
            person,
            items: vec![Amount {
                good: 1,
                quantity: 2,
            }]
            .into(),
        })
        .collect();
    let mut results = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut world = initial.clone();
        pool.install(|| {
            world
                .transfer_goods(&[
                    transfer(1, 1, 1, GoodsDirection::Deposit),
                    transfer(30, 1, 1, GoodsDirection::Pickup),
                ])
                .unwrap()
        });
        let holdings = &world.goods_cargo.as_ref().unwrap().holdings;
        assert_eq!(
            holdings
                .iter()
                .map(|h| (h.person, h.items[0].quantity))
                .collect::<Vec<_>>(),
            vec![(1, 1), (9, 2), (30, 3)]
        );
        world.validate().unwrap();
        results.push(world);
    }
    assert_eq!(results[0], results[1]);
}
