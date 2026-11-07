use super::*;
use game_parallel_lot_model::Container;

fn fixture() -> (State, Vec<Transaction>) {
    let source = Place::Stock { region: 1, good: 1 };
    let mut containers = vec![
        Container::new(
            source,
            128,
            vec![Lot {
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
