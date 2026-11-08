use super::*;
use game_lots::TransactionOutcome;

fn events(physical_actions: Vec<TransactionOutcome>) -> Events {
    Events {
        tick: 1,
        started: vec![],
        start_denied: vec![],
        abandoned: vec![],
        finished: vec![],
        delivered: vec![],
        output_lost: vec![],
        interrupted: vec![],
        returned: vec![],
        empty_returns: vec![],
        spoiled: vec![],
        physical_actions,
        physical_movements: Default::default(),
    }
}

#[test]
fn gift_receipts_match_when_physical_results_are_aligned_or_reordered() {
    let mut plan = Plan {
        actors: vec![1, 2, 3],
        actions: (11..=13)
            .map(|id| Transaction {
                id,
                movements: vec![Movement {
                    item: None,
                    id,
                    from: Place::Person(id - 10),
                    to: Some(Place::Person(99)),
                    good: 2,
                    quantity: id - 10,
                }],
                creations: vec![],
            })
            .collect(),
        records: (11..=13).map(|id| (id, id - 10, 99, 2, id - 10)).collect(),
        work_gifts: vec![],
    };
    let ordered = vec![
        TransactionOutcome {
            id: 5,
            accepted: true,
        },
        TransactionOutcome {
            id: 11,
            accepted: true,
        },
        TransactionOutcome {
            id: 12,
            accepted: false,
        },
        TransactionOutcome {
            id: 13,
            accepted: true,
        },
        TransactionOutcome {
            id: 15,
            accepted: true,
        },
    ];
    let expected = vec![(11, 1, 99, 2, 1), (13, 3, 99, 2, 3)];
    assert_eq!(plan.practice_gifts(&events(ordered.clone())), expected);
    plan.actions.clear();
    assert_eq!(plan.practice_gifts(&events(ordered.clone())), expected);
    let mut missing = ordered.clone();
    missing.retain(|row| row.id != 12);
    assert_eq!(plan.practice_gifts(&events(missing)), expected);
    let mut reversed = ordered;
    reversed.reverse();
    assert_eq!(plan.practice_gifts(&events(reversed)), expected);
    assert!(plan.practice_gifts(&events(vec![])).is_empty());
}
