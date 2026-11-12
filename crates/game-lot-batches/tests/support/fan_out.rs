use super::*;
use game_lot_model::Container;

fn fixture() -> (State, Vec<Transaction>) {
    let source = Place::Stock { region: 1, good: 1 };
    let mut containers = vec![
        Container::new(
            source,
            128,
            vec![Lot {
                batch: None,
                item: None,
                good: 1,
                quantity: 64,
                expires_at: None,
            }],
        )
        .unwrap(),
    ];
    containers.extend((1..=128).map(|id| Container::new(Place::Person(id), 1, vec![]).unwrap()));
    let actions = (1..=128)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                batch: None,
                item: None,
                id,
                from: source,
                to: Some(Place::Person(id)),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        })
        .collect();
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
fn merged_custody_remains_ordered_across_parallel_chunks() {
    let base: Vec<_> = (0..5000)
        .map(|id| Container::new(Place::Person(id * 2 + 1), 1, vec![]).unwrap())
        .collect();
    let additions: Vec<_> = (0..5000)
        .map(|id| Container::new(Place::Person(id * 2 + 2), 1, vec![]).unwrap())
        .collect();
    for workers in [1, 2, 4, 6] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let view = Containers::new(&base, &additions);
                assert_eq!(view.len(), 10_000);
                for id in 1..=10_000 {
                    assert_eq!(
                        view.get((id - 1) as usize).unwrap().place,
                        Place::Person(id)
                    );
                    assert_eq!(view.find(Place::Person(id)), Some((id - 1) as usize));
                }
            });
    }
}

#[test]
fn shared_source_path_resolves_shortage_and_declines_other_topologies() {
    let (state, actions) = fixture();
    let draft = prepare(&state, 1, 17, &actions, true).unwrap().unwrap();
    assert_eq!(
        draft.transactions.iter().filter(|row| row.accepted).count(),
        64
    );
    let mut changed = actions.clone();
    changed[1].movements[0].to = changed[0].movements[0].to;
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[1].movements[0].from = Place::Person(1);
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[1].movements[0].good = 2;
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[1].movements[0].id = changed[0].movements[0].id;
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[1].movements[0].to = Some(Place::Person(9999));
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions;
    let extra = changed[1].movements[0];
    changed[0].movements.push(extra);
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
}

#[test]
fn new_empty_destinations_match_general_arbitration_and_expiry() {
    let source = Place::Stock { region: 1, good: 1 };
    let state = State {
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                source,
                1000,
                vec![
                    Lot {
                        batch: None,
                        item: None,
                        good: 1,
                        quantity: 40,
                        expires_at: Some(1),
                    },
                    Lot {
                        batch: None,
                        item: None,
                        good: 1,
                        quantity: 180,
                        expires_at: Some(4),
                    },
                ],
            )
            .unwrap(),
            Container::new(
                Place::Person(9001),
                10,
                vec![Lot {
                    batch: None,
                    item: None,
                    good: 1,
                    quantity: 2,
                    expires_at: Some(1),
                }],
            )
            .unwrap(),
            Container::new(Place::JobInput(9999), 10, vec![]).unwrap(),
        ],
    };
    let mut additions: Vec<_> = (1..=256)
        .map(|id| {
            Container::new(
                Place::JobInput(id),
                if id % 11 == 0 { 1 } else { 5 },
                vec![],
            )
            .unwrap()
        })
        .collect();
    additions.extend((1..=256).map(|id| Container::new(Place::JobOutput(id), 5, vec![]).unwrap()));
    let actions: Vec<_> = (1..=256)
        .rev()
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                batch: None,
                id,
                item: None,
                from: source,
                to: Some(Place::JobInput(id)),
                good: 1,
                quantity: 1 + id % 3,
            }],
            creations: vec![],
        })
        .collect();
    for workers in [1, 2, 4, 6] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                for advance in [false, true] {
                    let fast = prepare_with_additions(&state, 1, 17, &actions, &additions, advance)
                        .unwrap()
                        .expect("eligible additions");
                    let (general, _) = crate::prepare_unpartitioned_validated(
                        &state,
                        1,
                        17,
                        &actions,
                        advance,
                        additions.clone(),
                        &[],
                    )
                    .unwrap();
                    assert_eq!(fast.transactions, general.transactions);
                    assert_eq!(fast.outcomes.into_owned(), general.outcomes.into_owned());
                    assert_eq!(fast.spoiled, general.spoiled);
                    assert_eq!(fast.changes.into_flat(), general.changes.into_flat());
                }
            });
    }
}

#[test]
fn added_destinations_can_sort_before_the_existing_source() {
    let source = Place::Stock { region: 1, good: 1 };
    let state = State {
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                source,
                256,
                vec![Lot {
                    batch: None,
                    item: None,
                    good: 1,
                    quantity: 128,
                    expires_at: None,
                }],
            )
            .unwrap(),
        ],
    };
    let additions: Vec<_> = (1..=128)
        .map(|id| Container::new(Place::Inventory(id), 2, vec![]).unwrap())
        .collect();
    let actions: Vec<_> = (1..=128)
        .rev()
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                batch: None,
                id,
                item: None,
                from: source,
                to: Some(Place::Inventory(id)),
                good: 1,
                quantity: 1,
            }],
            creations: vec![],
        })
        .collect();
    let fast = prepare_with_additions(&state, 1, 7, &actions, &additions, true)
        .unwrap()
        .unwrap();
    let (general, _) =
        crate::prepare_unpartitioned_validated(&state, 1, 7, &actions, true, additions, &[])
            .unwrap();
    assert_eq!(fast.transactions, general.transactions);
    assert_eq!(fast.outcomes.into_owned(), general.outcomes.into_owned());
    assert_eq!(fast.changes.into_flat(), general.changes.into_flat());
}
