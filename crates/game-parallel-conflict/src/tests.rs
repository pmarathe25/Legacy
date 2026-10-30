use crate::{Conflict, Encounter, Fighter, Order, finish};

#[test]
fn cleanup_uses_original_unsorted_roster_to_remove_retreats_and_dead_targets() {
    let mut conflict = Conflict {
        damage: 1,
        next_id: 2,
        encounters: vec![Encounter {
            id: 1,
            region: 1,
            fighters: vec![
                Fighter {
                    person: 40,
                    side: 1,
                    order: Order::Attack { target: 10 },
                },
                Fighter {
                    person: 10,
                    side: 1,
                    order: Order::Attack { target: 30 },
                },
                Fighter {
                    person: 30,
                    side: 2,
                    order: Order::Retreat,
                },
                Fighter {
                    person: 20,
                    side: 2,
                    order: Order::Attack { target: 40 },
                },
            ],
        }],
        events: vec![],
    };
    finish(&mut conflict, |id| id != 40);
    let fighters = &conflict.encounters[0].fighters;
    assert_eq!(fighters.len(), 2);
    assert_eq!(fighters[0].person, 10);
    assert_eq!(fighters[1].person, 20);
    assert!(
        fighters
            .iter()
            .all(|fighter| matches!(fighter.order, Order::Defend))
    );
}

#[test]
fn events_are_grouped_by_target_across_encounters() {
    let attack = |person, side, target| Fighter {
        person,
        side,
        order: Order::Attack { target },
    };
    let conflict = Conflict {
        damage: 1,
        next_id: 3,
        events: vec![],
        encounters: vec![
            Encounter {
                id: 1,
                region: 1,
                fighters: vec![attack(3, 1, 4), attack(4, 2, 3)],
            },
            Encounter {
                id: 2,
                region: 1,
                fighters: vec![attack(1, 1, 2), attack(2, 2, 1)],
            },
        ],
    };
    let people = vec![
        (1, Some(1), 100, 0),
        (2, Some(1), 100, 0),
        (3, Some(1), 100, 0),
        (4, Some(1), 100, 0),
    ];
    let result = crate::prepare(&conflict, &people, 1);
    assert_eq!(
        result
            .events
            .iter()
            .map(|event| event.target)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}
