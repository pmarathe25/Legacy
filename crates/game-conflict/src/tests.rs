use crate::{Conflict, Encounter, Fighter, Order, finish};

#[test]
#[ignore = "performance measurement"]
fn many_small_encounter_actor_index_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let count: u64 = std::env::var("LEGACY_FIGHTS")
        .unwrap_or_else(|_| "50000".into())
        .parse()
        .unwrap();
    let conflict = Conflict {
        damage: 1,
        next_id: count + 1,
        events: vec![],
        encounters: (1..=count)
            .map(|id| Encounter {
                id,
                region: 1,
                fighters: vec![
                    Fighter {
                        person: id * 2 - 1,
                        side: 1,
                        order: Order::Defend,
                    },
                    Fighter {
                        person: id * 2,
                        side: 2,
                        order: Order::Defend,
                    },
                ],
            })
            .collect(),
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for _ in 0..5 {
                let now = std::time::Instant::now();
                let actors = std::hint::black_box(conflict.actors());
                samples.push(now.elapsed().as_secs_f64());
                drop(actors);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} fights={count} actors_seconds={:.6}",
                samples[2]
            );
        });
}

#[test]
fn actor_index_is_sorted_for_many_small_and_large_encounters() {
    let fight = |id, people: Vec<u64>| Encounter {
        id,
        region: 1,
        fighters: people
            .into_iter()
            .map(|person| Fighter {
                person,
                side: 1,
                order: Order::Defend,
            })
            .collect(),
    };
    for count in [32, 4095, 4096, 5000] {
        let conflict = Conflict {
            damage: 1,
            next_id: 3,
            events: vec![],
            encounters: vec![
                fight(2, (10000..10000 + count).rev().collect()),
                fight(1, vec![4, 1, 3, 2]),
            ],
        };
        let expected: Vec<_> = [1, 2, 3, 4]
            .into_iter()
            .chain(10000..10000 + count)
            .collect();
        for workers in [1, 4] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    assert_eq!(conflict.actors(), expected);
                });
        }
    }
}

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
