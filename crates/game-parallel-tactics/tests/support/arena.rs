use super::*;
#[test]
#[ignore = "performance measurement"]
fn many_small_battle_index_benchmark() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let count: u64 = std::env::var("LEGACY_FIGHTS")
        .unwrap_or_else(|_| "50000".into())
        .parse()
        .unwrap();
    let rules = Rules {
        width: 10,
        height: 10,
        obstacle_count: 0,
        max_obstacle_size: 1,
        movement: 4,
        attack_range: 1,
    };
    let tactics = Tactics {
        maps: vec![Map::generate(1, 1, rules).unwrap()],
        battles: (1..=count)
            .map(|id| Battle {
                encounter: id,
                region: 1,
                people: vec![
                    Placement {
                        person: id * 2 - 1,
                        position: Point { x: 0, y: 0 },
                        last_move: None,
                    },
                    Placement {
                        person: id * 2,
                        position: Point { x: 0, y: 1 },
                        last_move: None,
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
                let index = std::hint::black_box(tactics.index().unwrap());
                samples.push(now.elapsed().as_secs_f64());
                drop(index);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} fights={count} index_seconds={:.6}",
                samples[2]
            );
        });
}
#[test]
fn sparse_unordered_battles_keep_their_own_movement_and_membership() {
    let rules = Rules {
        width: 20,
        height: 20,
        obstacle_count: 0,
        max_obstacle_size: 1,
        movement: 10,
        attack_range: 10,
    };
    let battle = |encounter, person| Battle {
        encounter,
        region: 1,
        people: vec![
            Placement {
                person,
                position: Point { x: 0, y: 0 },
                last_move: None,
            },
            Placement {
                person: person + 1,
                position: Point { x: 0, y: 1 },
                last_move: None,
            },
        ],
    };
    let mut state = Tactics {
        maps: vec![Map::generate(1, 1, rules).unwrap()],
        battles: vec![battle(u64::MAX, 9), battle(2, 1), battle(17, 5)],
    };
    let mut index = state.index().unwrap();
    index
        .move_batch(
            &mut state,
            &[
                Move {
                    encounter: 17,
                    person: 5,
                    destination: Point { x: 2, y: 0 },
                },
                Move {
                    encounter: u64::MAX,
                    person: 9,
                    destination: Point { x: 3, y: 0 },
                },
            ],
            1,
        )
        .unwrap();
    assert_eq!(state.battles[2].people[0].position.x, 2);
    assert_eq!(state.battles[0].people[0].position.x, 3);
    state.battles.remove(0);
    index.membership_changed(&state, &[u64::MAX], &[9, 10]);
    assert!(
        index
            .move_batch(
                &mut state,
                &[Move {
                    encounter: u64::MAX,
                    person: 9,
                    destination: Point { x: 1, y: 1 }
                }],
                2
            )
            .is_err()
    );
    index
        .move_batch(
            &mut state,
            &[Move {
                encounter: 2,
                person: 1,
                destination: Point { x: 1, y: 1 },
            }],
            2,
        )
        .unwrap();
    assert_eq!(state.battles[0].people[0].position, Point { x: 1, y: 1 });
}

#[test]
fn movement_is_atomic_and_single_use_per_round() {
    let rules = Rules {
        width: 100,
        height: 100,
        obstacle_count: 0,
        max_obstacle_size: 10,
        movement: 20,
        attack_range: 30,
    };
    let tactics = Tactics {
        maps: vec![Map::generate(1, 1, rules).unwrap()],
        battles: vec![Battle {
            encounter: 3,
            region: 1,
            people: vec![
                Placement {
                    person: 1,
                    position: Point { x: 10, y: 10 },
                    last_move: None,
                },
                Placement {
                    person: 2,
                    position: Point { x: 20, y: 10 },
                    last_move: None,
                },
            ],
        }],
    };
    tactics.validate(0).unwrap();
    assert!(
        tactics
            .move_batch(
                &[Move {
                    encounter: 3,
                    person: 1,
                    destination: Point { x: 20, y: 10 }
                }],
                0
            )
            .is_err()
    );
    let moved = tactics
        .move_batch(
            &[
                Move {
                    encounter: 3,
                    person: 1,
                    destination: Point { x: 20, y: 10 },
                },
                Move {
                    encounter: 3,
                    person: 2,
                    destination: Point { x: 10, y: 10 },
                },
            ],
            0,
        )
        .unwrap();
    assert!(
        moved
            .move_batch(
                &[Move {
                    encounter: 3,
                    person: 1,
                    destination: Point { x: 21, y: 10 }
                }],
                0
            )
            .is_err()
    );
    assert!(
        moved
            .move_batch(
                &[Move {
                    encounter: 3,
                    person: 1,
                    destination: Point { x: 21, y: 10 }
                }],
                1
            )
            .is_ok()
    );
    assert!(moved.index().unwrap().can_attack_between(1, 2));
    assert!(!moved.index().unwrap().can_attack_between(1, 3));
    assert_eq!(
        tactics.battles[0].people[0].position,
        Point { x: 10, y: 10 }
    );
}

#[test]
fn sparse_index_swaps_and_large_batches_match_reference_and_rollback() {
    let mut state = Tactics {
        maps: vec![
            Map::generate(
                1,
                1,
                Rules {
                    width: 20_000,
                    height: 100,
                    obstacle_count: 0,
                    max_obstacle_size: 1,
                    movement: 3,
                    attack_range: 5,
                },
            )
            .unwrap(),
        ],
        battles: vec![Battle {
            encounter: 1,
            region: 1,
            people: (0..10_000)
                .map(|i| Placement {
                    person: i,
                    position: Point { x: i as u32, y: 1 },
                    last_move: None,
                })
                .collect(),
        }],
    };
    let mut index = state.index().unwrap();
    let failed = [Move {
        encounter: 1,
        person: 0,
        destination: Point { x: 1, y: 1 },
    }];
    let original = state.clone();
    assert!(index.move_batch(&mut state, &failed, 0).is_err());
    assert_eq!(state, original);
    let swaps: Vec<_> = (0..10_000)
        .map(|person| Move {
            encounter: 1,
            person,
            destination: Point {
                x: (person ^ 1) as u32,
                y: 1,
            },
        })
        .collect();
    let reference = state.move_batch(&swaps, 0).unwrap();
    index.move_batch(&mut state, &swaps, 0).unwrap();
    assert_eq!(state, reference);
    for (i, person) in state.battles[0].people.iter().enumerate() {
        assert_eq!(
            person.position,
            Point {
                x: (i ^ 1) as u32,
                y: 1
            }
        );
        assert_eq!(person.last_move, Some(0));
    }
    assert!(index.move_batch(&mut state, &swaps, 0).is_err());
    assert_eq!(state, reference);
    index.move_batch(&mut state, &swaps, 1).unwrap();
    assert_eq!(index.can_attack_between(0, 4), state.can_attack(1, 0, 4));
}

#[test]
fn sparse_index_rebuilds_only_dirty_membership_slots_before_movement() {
    let mut state = Tactics {
        maps: vec![
            Map::generate(
                1,
                1,
                Rules {
                    width: 100,
                    height: 100,
                    obstacle_count: 0,
                    max_obstacle_size: 1,
                    movement: 20,
                    attack_range: 30,
                },
            )
            .unwrap(),
        ],
        battles: vec![Battle {
            encounter: 1,
            region: 1,
            people: [3, 1, 2]
                .into_iter()
                .map(|person| Placement {
                    person,
                    position: Point {
                        x: person as u32 * 10,
                        y: 1,
                    },
                    last_move: None,
                })
                .collect(),
        }],
    };
    let mut index = state.index().unwrap();
    state.battles[0].people.retain(|person| person.person != 1);
    index.membership_changed(&state, &[1], &[1]);
    assert!(!index.can_attack_between(1, 2));
    let before = state.clone();
    assert!(
        index
            .move_batch(
                &mut state,
                &[Move {
                    encounter: 1,
                    person: 1,
                    destination: Point { x: 1, y: 1 }
                }],
                1
            )
            .is_err()
    );
    assert_eq!(state, before);
    index
        .move_batch(
            &mut state,
            &[Move {
                encounter: 1,
                person: 2,
                destination: Point { x: 10, y: 1 },
            }],
            1,
        )
        .unwrap();
    assert_eq!(state.battles[0].people[1].person, 2);
    assert_eq!(state.battles[0].people[1].position.x, 10);
    assert_eq!(index.can_attack_between(2, 3), state.can_attack(1, 2, 3));
}

#[test]
fn compact_sparse_geometry_handles_extreme_ids_moves_and_resolved_encounters() {
    let mut state = Tactics {
        maps: vec![
            Map::generate(
                1,
                1,
                Rules {
                    width: 100,
                    height: 100,
                    obstacle_count: 0,
                    max_obstacle_size: 1,
                    movement: 20,
                    attack_range: 2,
                },
            )
            .unwrap(),
        ],
        battles: vec![Battle {
            encounter: 1,
            region: 1,
            people: [(u64::MAX, 3), (10, 1), (u64::MAX - 1, 2)]
                .into_iter()
                .map(|(person, x)| Placement {
                    person,
                    position: Point { x, y: 1 },
                    last_move: None,
                })
                .collect(),
        }],
    };
    let mut index = state.index().unwrap();
    assert!(index.can_attack_between(10, u64::MAX));
    assert!(!index.can_attack_between(0, 10));
    assert!(!index.can_attack_between(11, 10));
    index
        .move_batch(
            &mut state,
            &[Move {
                encounter: 1,
                person: 10,
                destination: Point { x: 10, y: 1 },
            }],
            0,
        )
        .unwrap();
    assert!(!index.can_attack_between(10, u64::MAX));
    assert_eq!(
        index.can_attack_between(10, u64::MAX),
        state.can_attack(1, 10, u64::MAX)
    );
    state.battles.clear();
    index.membership_changed(&state, &[1], &[10, u64::MAX - 1, u64::MAX]);
    assert!(!index.can_attack_between(u64::MAX - 1, u64::MAX));
    assert!(
        index
            .move_batch(
                &mut state,
                &[Move {
                    encounter: 1,
                    person: 10,
                    destination: Point { x: 1, y: 1 }
                }],
                1
            )
            .is_err()
    );
}
