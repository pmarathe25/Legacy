use game_lots::{
    Container, Creation, Footprint, Footprints, Lot, Movement, Place, SimpleTransfer, State, Store,
    Transaction,
};

fn footprints() -> Footprints {
    Footprints::new(vec![Footprint { good: 2, units: 3 }]).unwrap()
}

fn lot(good: u32, quantity: u64) -> Lot {
    Lot {
        good,
        quantity,
        batch: None,
        item: None,
        expires_at: None,
        freshness: Default::default(),
    }
}

fn initial(mut containers: Vec<Container>) -> State {
    containers.sort_unstable_by_key(|row| row.place);
    State {
        tick: 0,
        revision: 0,
        next_batch: None,
        expiry_millis: Some(0),
        storage_climate: None,
        footprints: footprints(),
        containers,
    }
}

fn holder(place: Place, capacity: u64, lots: Vec<Lot>) -> Container {
    Container::with_footprints(place, capacity, lots, &footprints()).unwrap()
}

fn movement(id: u64, from: Place, to: Option<Place>, good: u32, quantity: u64) -> Movement {
    Movement {
        id,
        from,
        to,
        good,
        quantity,
        batch: None,
        item: None,
    }
}

#[test]
fn a_move_can_reuse_its_own_weighted_space_and_expiry_releases_capacity() {
    let bag = Place::Person(1);
    let store = Place::Inventory(1);
    let mut perishable = lot(2, 1);
    perishable.expires_at = Some(30);
    let mut unique = lot(2, 1);
    unique.item = Some(99);
    let state = initial(vec![
        holder(bag, 4, vec![perishable, lot(1, 1)]),
        holder(store, 6, vec![unique]),
    ]);
    let mut forged = state.clone();
    forged.containers[1].used = 2;
    assert!(Store::compile(forged).is_err());
    let mut owner = Store::compile(state).unwrap();

    let input = Movement {
        item: Some(99),
        ..movement(2, store, Some(bag), 2, 1)
    };
    let consume = movement(1, bag, None, 2, 1);
    let oversized = Transaction {
        id: 1,
        movements: vec![consume, input],
        creations: vec![Creation {
            id: 3,
            to: bag,
            lot: lot(2, u64::MAX),
        }],
    };
    let denied = owner.prepare_transactions_now(7, &[oversized]).unwrap();
    assert!(!denied.transactions()[0].accepted);
    owner.commit(denied).unwrap();
    assert_eq!(owner.item_place(99), Some(store));
    assert_eq!(owner.container(bag).unwrap().used, 4);

    let exchange = Transaction {
        id: 2,
        movements: vec![consume, input],
        creations: vec![],
    };
    let draft = owner.prepare_transactions_now(7, &[exchange]).unwrap();
    assert!(draft.transactions()[0].accepted);
    let stale = owner
        .prepare_now(7, &[movement(11, bag, None, 1, 1)])
        .unwrap();
    owner.commit(draft).unwrap();
    assert!(owner.commit(stale).is_err());
    assert_eq!(owner.item_place(99), Some(bag));
    assert_eq!(owner.container(bag).unwrap().used, 4);
    assert_eq!(owner.container(store).unwrap().used, 0);

    let pending = Lot {
        expires_at: Some(30),
        ..lot(2, 2)
    };
    let create = Transaction {
        id: 4,
        movements: vec![],
        creations: vec![Creation {
            id: 4,
            to: store,
            lot: pending,
        }],
    };
    let draft = owner.prepare_transactions_now(7, &[create]).unwrap();
    assert!(draft.transactions()[0].accepted);
    owner.commit(draft).unwrap();
    assert_eq!(owner.container(store).unwrap().used, 6);
    let saved = serde_json::to_vec(&owner.export()).unwrap();
    let mut owner = Store::compile(serde_json::from_slice(&saved).unwrap()).unwrap();
    let spoilage = owner.prepare_expiry_at(30).unwrap();
    let recorded = owner.commit_expiry_at(spoilage).unwrap();
    assert_eq!(recorded[0].quantity, 2);
    assert_eq!(owner.container(store).unwrap().used, 0);
    assert_eq!(owner.item_place(99), Some(bag));
}

#[test]
fn mixed_goods_contending_for_one_holder_preserve_quantities_and_worker_determinism() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                const PEOPLE: u64 = 4096;
                let destination = Place::Inventory(1);
                let mut containers = vec![holder(destination, PEOPLE, vec![])];
                let mut actions = Vec::new();
                for person in 1..=PEOPLE {
                    let origin = Place::Person(person);
                    let good = if person % 2 == 0 { 2 } else { 1 };
                    containers.push(holder(origin, 3, vec![lot(good, 1)]));
                    actions.push(Transaction {
                        id: person,
                        movements: vec![movement(person, origin, Some(destination), good, 1)],
                        creations: vec![],
                    });
                }
                let opening = initial(containers);
                let (reference, _) = game_lot_batches::prepare_unpartitioned(
                    &opening,
                    1,
                    7,
                    &actions,
                    false,
                    vec![],
                    &[],
                )
                .unwrap();
                let mut owner = Store::compile(opening).unwrap();
                let draft = owner.prepare_transactions_now(7, &actions).unwrap();
                let receipts = draft.transactions().to_vec();
                assert_eq!(receipts, reference.transactions);
                owner.commit(draft).unwrap();
                let state = owner.export();
                let destination = owner.container(destination).unwrap();
                assert!((PEOPLE - 2..=PEOPLE).contains(&destination.used));
                assert_eq!(
                    destination.lots.iter().map(|lot| lot.quantity).sum::<u64>(),
                    receipts.iter().filter(|receipt| receipt.accepted).count() as u64
                );
                for good in [1, 2] {
                    assert_eq!(
                        state
                            .containers
                            .iter()
                            .flat_map(|row| &row.lots)
                            .filter(|lot| lot.good == good)
                            .map(|lot| lot.quantity)
                            .sum::<u64>(),
                        PEOPLE / 2
                    );
                }
                assert!(Store::compile(state.clone()).is_ok());
                (state, receipts)
            })
    };
    assert_eq!(run(1), run(6));
}

#[test]
fn a_shared_stock_fans_out_to_private_custody_in_weighted_units() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let source = Place::Stock { region: 1, good: 2 };
                let mut owner =
                    Store::compile(initial(vec![holder(source, 1500, vec![lot(2, 500)])])).unwrap();
                let additions: Vec<_> = (1..=256)
                    .map(|id| holder(Place::Person(id), if id % 2 == 0 { 5 } else { 6 }, vec![]))
                    .collect();
                let transfers: Vec<_> = (1..=256)
                    .map(|id| SimpleTransfer {
                        action: id,
                        movement: movement(id, source, Some(Place::Person(id)), 2, 2),
                    })
                    .collect();
                let draft = owner
                    .prepare_fan_out_flat_with_empty(7, &transfers, additions, false)
                    .unwrap()
                    .expect("shared stock should use the packed fan-out");
                let receipts = draft.transactions().to_vec();
                assert!(receipts.iter().all(|row| !row.accepted || row.id % 2 == 1));
                owner.commit(draft).unwrap();
                let state = owner.export();
                let stock = owner.container(source).unwrap();
                let delivered = receipts.iter().filter(|row| row.accepted).count() as u64;
                assert_eq!(stock.lots[0].quantity + 2 * delivered, 500);
                assert_eq!(stock.used + 6 * delivered, 1500);
                assert!(Store::compile(state.clone()).is_ok());
                (state, receipts)
            })
    };
    assert_eq!(run(1), run(6));
}
