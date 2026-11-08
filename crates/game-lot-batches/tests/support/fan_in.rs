use super::*;

fn fixture() -> (State, Vec<Transaction>) {
    let mut containers: Vec<_> = (1..=128)
        .map(|id| {
            Container::new(
                Place::Person(id),
                10,
                vec![Lot {
                    item: None,
                    good: 1,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap()
        })
        .collect();
    containers.push(Container::new(Place::Person(1000), 128, vec![]).unwrap());
    let actions = (1..=128)
        .map(|id| Transaction {
            id,
            movements: vec![Movement {
                item: None,
                id,
                from: Place::Person(id),
                to: Some(Place::Person(1000)),
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
fn fast_path_handles_contention_and_declines_invalid_topologies() {
    let (state, actions) = fixture();
    assert!(prepare(&state, 1, 17, &actions, true).unwrap().is_some());
    let mut changed = actions.clone();
    changed[1].movements[0].from = changed[0].movements[0].from;
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[0].movements[0].to = Some(Place::Person(2));
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[0].movements[0].to = Some(Place::Person(9999));
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[1].movements[0].id = changed[0].movements[0].id;
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
    let mut changed = actions.clone();
    changed[0].movements[0].quantity = 2;
    let shortage = prepare(&state, 1, 17, &changed, true).unwrap().unwrap();
    assert_eq!(
        shortage
            .transactions
            .iter()
            .filter(|row| !row.accepted)
            .count(),
        1
    );
    let mut limited = state.clone();
    *limited.containers.last_mut().unwrap() =
        Container::new(Place::Person(1000), 127, vec![]).unwrap();
    let contested = prepare(&limited, 1, 17, &actions, true).unwrap().unwrap();
    assert_eq!(
        contested
            .transactions
            .iter()
            .filter(|row| !row.accepted)
            .count(),
        1
    );
    let mut changed = actions;
    let extra = changed[1].movements[0];
    changed[0].movements.push(extra);
    assert!(prepare(&state, 1, 17, &changed, true).unwrap().is_none());
}

#[test]
fn total_incoming_capacity_is_checked_without_overflow() {
    let (mut state, mut actions) = fixture();
    for (index, action) in actions.iter_mut().enumerate().take(2) {
        state.containers[index] = Container::new(
            Place::Person(index as u64 + 1),
            u64::MAX,
            vec![Lot {
                item: None,
                good: 1,
                quantity: u64::MAX,
                expires_at: None,
            }],
        )
        .unwrap();
        action.movements[0].quantity = u64::MAX;
    }
    *state.containers.last_mut().unwrap() =
        Container::new(Place::Person(1000), u64::MAX, vec![]).unwrap();
    let result = prepare(&state, 1, 17, &actions, true).unwrap().unwrap();
    assert!(result.transactions.iter().any(|row| !row.accepted));
}

#[test]
fn interleaved_but_disjoint_source_and_destination_ranges_are_supported() {
    let (mut state, mut actions) = fixture();
    state.containers[63] = Container::new(Place::Person(64), 128, vec![]).unwrap();
    state.containers.push(
        Container::new(
            Place::Person(2000),
            10,
            vec![Lot {
                item: None,
                good: 1,
                quantity: 1,
                expires_at: None,
            }],
        )
        .unwrap(),
    );
    for action in &mut actions {
        action.movements[0].to = Some(Place::Person(64));
    }
    actions[63].movements[0].from = Place::Person(2000);
    assert!(prepare(&state, 1, 17, &actions, true).unwrap().is_some());
}
