use game_lots::{BatchCount, Container, Creation, Lot, Place, State, Store, Transaction};

fn lot(batch: Option<u64>) -> Lot {
    Lot {
        good: 1,
        quantity: 1,
        batch,
        item: None,
        expires_at: None,
    }
}
fn creation(id: u64, to: u64, batch: u64) -> Transaction {
    Transaction {
        id,
        movements: vec![],
        creations: vec![Creation {
            id,
            to: Place::Person(to),
            lot: lot(Some(batch)),
        }],
    }
}
fn owner() -> Store {
    Store::compile(State {
        tick: 0,
        revision: 0,
        next_batch: None,
        containers: vec![
            Container::new(Place::Person(1), 10, vec![lot(Some(5))]).unwrap(),
            Container::new(Place::Person(2), 10, vec![]).unwrap(),
            Container::new(Place::Person(3), 1, vec![lot(None)]).unwrap(),
        ],
    })
    .unwrap()
}

#[test]
fn speculative_ranges_are_owner_ordered_and_commit_atomically() {
    let mut store = owner();
    assert_eq!(store.enable_batch_allocator(1).unwrap(), 6);
    let counts = [
        BatchCount { owner: 2, count: 2 },
        BatchCount { owner: 1, count: 1 },
    ];
    let stale = store.plan_batches(&counts).unwrap();
    let discarded = store.plan_batches(&counts).unwrap();
    assert_eq!(discarded.range(1).unwrap().start, 6);
    assert_eq!(discarded.range(2).unwrap().start, 7);
    let actions = [creation(1, 2, 6), creation(2, 3, 7)];
    let draft = store
        .prepare_transactions_with_batches(7, &actions, false, discarded, &[1, 2])
        .unwrap();
    assert_eq!(
        draft
            .transactions()
            .iter()
            .map(|row| row.accepted)
            .collect::<Vec<_>>(),
        vec![true, false]
    );
    drop(draft);
    assert_eq!(store.export().next_batch, Some(6));
    let allocation = store.plan_batches(&counts).unwrap();
    let draft = store
        .prepare_transactions_with_batches(7, &actions, false, allocation, &[1, 2])
        .unwrap();
    store.commit(draft).unwrap();
    assert_eq!(store.export().next_batch, Some(9));
    assert!(
        store
            .prepare_transactions_with_batches(7, &actions, false, stale, &[1, 2])
            .is_err()
    );
    let saved = store.export();
    let loaded: State = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
    assert_eq!(Store::compile(loaded).unwrap().export(), saved);
}

#[test]
fn allocations_reject_wrong_owner_unreserved_ids_and_overflow() {
    let mut store = owner();
    store.enable_batch_allocator(1).unwrap();
    assert!(
        store
            .prepare_transactions_now(7, &[creation(1, 2, 6)])
            .is_err()
    );
    let plan = store
        .plan_batches(&[BatchCount { owner: 1, count: 1 }])
        .unwrap();
    assert!(
        store
            .prepare_transactions_with_batches(7, &[creation(1, 2, 6)], false, plan, &[2])
            .is_err()
    );
    assert_eq!(store.export().next_batch, Some(6));
    assert!(
        store
            .plan_batches(&[
                BatchCount { owner: 1, count: 1 },
                BatchCount { owner: 1, count: 1 }
            ])
            .is_err()
    );
    let mut invalid = store.export();
    invalid.next_batch = Some(5);
    assert!(Store::compile(invalid).is_err());
    let mut exhausted = owner();
    exhausted.enable_batch_allocator(u64::MAX).unwrap();
    assert!(
        exhausted
            .plan_batches(&[BatchCount { owner: 1, count: 1 }])
            .is_err()
    );
    let foreign = store
        .plan_batches(&[BatchCount { owner: 1, count: 1 }])
        .unwrap();
    let copy = Store::compile(store.export()).unwrap();
    assert!(
        copy.prepare_transactions_with_batches(7, &[creation(1, 2, 6)], false, foreign, &[1])
            .is_err()
    );
}

#[test]
fn older_unallocated_saves_still_load_without_a_high_water_mark() {
    let original = owner().export();
    let json = serde_json::to_string(&original).unwrap();
    assert!(!json.contains("next_batch"));
    let restored: State = serde_json::from_str(&json).unwrap();
    assert_eq!(Store::compile(restored).unwrap().export(), original);
}

#[test]
fn allocation_can_share_an_atomic_draft_with_new_custody_and_compact_consumption() {
    let mut store = owner();
    store.enable_batch_allocator(1).unwrap();
    let plan = store
        .plan_batches(&[BatchCount { owner: 9, count: 1 }])
        .unwrap();
    let consume = Transaction {
        id: 1,
        movements: vec![game_lots::Movement {
            id: 1,
            from: Place::Person(1),
            to: None,
            good: 1,
            quantity: 1,
            item: None,
            batch: None,
        }],
        creations: vec![],
    };
    let actions = [consume, creation(2, 4, 6)];
    let additions = vec![Container::new(Place::Person(4), 10, vec![]).unwrap()];
    let draft = store
        .prepare_with_empty_compact_and_batches(7, &actions, additions, &[1], false, plan, &[1, 9])
        .unwrap();
    assert!(draft.transactions().iter().all(|row| row.accepted));
    assert!(store.container(Place::Person(4)).is_none());
    assert_eq!(store.export().next_batch, Some(6));
    store.commit(draft).unwrap();
    assert_eq!(store.export().next_batch, Some(7));
    assert_eq!(
        store.container(Place::Person(4)).unwrap().lots[0].batch,
        Some(6)
    );
    assert_eq!(store.container(Place::Person(1)).unwrap().used, 0);
}

#[test]
#[ignore = "batch allocation and creation scaling measurement"]
fn independent_batch_creation_scaling() {
    use rayon::prelude::*;
    let per_owner = std::env::var("LEGACY_BATCH_PER_OWNER")
        .ok()
        .and_then(|row| row.parse::<u64>().ok())
        .unwrap_or(800);
    let count = per_owner * 64;
    let initial = State {
        tick: 0,
        revision: 0,
        next_batch: Some(1),
        containers: (1..=count)
            .map(|id| Container::new(Place::Person(id), 1, vec![]).unwrap())
            .collect(),
    };
    let counts: Vec<_> = (1..=64)
        .map(|owner| BatchCount {
            owner,
            count: per_owner,
        })
        .collect();
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
                let plan = store.plan_batches(&counts).unwrap();
                let actions: Vec<_> = (0..count)
                    .into_par_iter()
                    .map(|index| {
                        let owner = (index % 64 + 1) as u32;
                        let id = plan.range(owner).unwrap().start + index / 64;
                        creation(index + 1, index + 1, id)
                    })
                    .collect();
                let owners: Vec<_> = (0..count)
                    .into_par_iter()
                    .map(|index| (index % 64 + 1) as u32)
                    .collect();
                let draft = store
                    .prepare_transactions_with_batches(7, &actions, false, plan, &owners)
                    .unwrap();
                assert!(draft.transactions().iter().all(|row| row.accepted));
                store.commit(draft).unwrap();
            });
            times.push(start.elapsed());
            let state = store.export();
            assert_eq!(state.next_batch, Some(count + 1));
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} allocated batches across 64 owners",
            times[1]
        );
    }
}
