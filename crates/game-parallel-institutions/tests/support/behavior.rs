use crate::{Action, Body, Cast, Institutions};
use std::collections::HashSet;
#[test]
fn concentrated_ballot_is_atomic_deterministic_and_changes_law_at_deadline() {
    let count = 10_000u64;
    let people: Vec<_> = (0..count).map(|id| (id + 100, Some(1), 100, 0)).collect();
    let initial = Institutions {
        next_id: 1,
        bodies: vec![Body {
            id: 1,
            region: 1,
            members: (100..100 + count).collect(),
            quorum: 5_000,
            chair: Some(100),
            trade_open: true,
        }],
        ballots: vec![],
        events: vec![],
        requests: vec![],
        admissions: vec![],
    };
    let requests: Vec<_> = (0..count)
        .rev()
        .map(|id| Cast {
            ballot: 1,
            voter: id + 100,
            yes: id < 6_000,
        })
        .collect();
    let mut results = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut state = initial.clone();
        state
            .validate(
                &people.iter().map(|person| person.0).collect::<HashSet<_>>(),
                &HashSet::from([1]),
                0,
            )
            .unwrap();
        assert_eq!(
            state
                .open(&people, 1, 100, Action::Trade { open: false }, 2, 0)
                .unwrap(),
            1
        );
        let before = state.clone();
        assert!(state.vote(&people, &[requests[0], requests[0]]).is_err());
        assert_eq!(state, before);
        pool.install(|| state.vote(&people, &requests)).unwrap();
        let staged = pool.install(|| state.prepare(&people, 1));
        staged.commit(&mut state);
        assert!(state.bodies[0].trade_open);
        let staged = pool.install(|| state.prepare(&people, 2));
        assert!(state.bodies[0].trade_open);
        staged.commit(&mut state);
        assert!(!state.bodies[0].trade_open);
        assert!(state.ballots.is_empty());
        assert!(state.events[0].adopted);
        results.push(state);
    }
    assert_eq!(results[0], results[1]);
}
#[test]
fn office_elections_require_actual_votes_and_do_not_transfer_automatically() {
    let people = vec![(1, Some(1), 100, 0), (2, Some(1), 100, 0)];
    let mut state = Institutions {
        next_id: 1,
        bodies: vec![Body {
            id: 1,
            region: 1,
            members: vec![1, 2],
            quorum: 2,
            chair: Some(1),
            trade_open: true,
        }],
        ballots: vec![],
        events: vec![],
        requests: vec![],
        admissions: vec![],
    };
    let id = state
        .open(&people, 1, 1, Action::Elect { candidate: 2 }, 1, 0)
        .unwrap();
    state
        .vote(
            &people,
            &[Cast {
                ballot: id,
                voter: 1,
                yes: true,
            }],
        )
        .unwrap();
    state.prepare(&people, 1).commit(&mut state);
    assert_eq!(state.bodies[0].chair, Some(1));
    assert!(!state.events[0].adopted);
    let mut deceased = people.clone();
    deceased[0].2 = 0;
    state.prepare(&deceased, 2).commit(&mut state);
    assert_eq!(state.bodies[0].chair, None);
}
#[test]
fn opening_independent_ballots_is_atomic_and_preserves_request_order() {
    use crate::Open;
    let people = vec![
        (1, Some(1), 100, 0),
        (2, Some(1), 100, 0),
        (3, Some(2), 100, 0),
    ];
    let mut state = Institutions {
        next_id: 10,
        bodies: vec![
            Body {
                id: 1,
                region: 1,
                members: vec![1, 2],
                quorum: 1,
                chair: Some(1),
                trade_open: true,
            },
            Body {
                id: 2,
                region: 2,
                members: vec![3],
                quorum: 1,
                chair: Some(3),
                trade_open: true,
            },
        ],
        ballots: vec![],
        events: vec![],
        requests: vec![],
        admissions: vec![],
    };
    let rows = [
        Open {
            body: 2,
            proposer: 3,
            action: Action::Trade { open: false },
            duration: 3,
        },
        Open {
            body: 1,
            proposer: 1,
            action: Action::Elect { candidate: 2 },
            duration: 2,
        },
    ];
    let before = state.clone();
    assert!(
        state
            .open_batch(&people, &[rows[0].clone(), rows[0].clone()], 0)
            .is_err()
    );
    assert_eq!(state, before);
    assert_eq!(state.open_batch(&people, &rows, 0).unwrap(), vec![10, 11]);
    assert_eq!(
        state
            .ballots
            .iter()
            .map(|ballot| ballot.body)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
    let before = state.clone();
    assert!(state.open_batch(&people, &rows, 0).is_err());
    assert_eq!(state, before);
}
