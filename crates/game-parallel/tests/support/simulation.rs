use game_parallel::{Person, Region, World};

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap()
}

#[test]
fn outcomes_do_not_depend_on_worker_count_or_region_order() {
    let mut one = world();
    for n in 4..200 {
        one.regions.push(Region {
            id: n,
            food: 5,
            people: vec![Person {
                id: n,
                hunger: n as u32 % 9,
                health: 100,
            }],
        });
    }
    let mut many = one.clone();
    many.regions.reverse();
    let single = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let parallel = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    single.install(|| {
        for _ in 0..100 {
            one.step().unwrap();
        }
    });
    parallel.install(|| {
        for _ in 0..100 {
            many.step().unwrap();
        }
    });
    many.regions.reverse();
    assert_eq!(one, many);
}

#[test]
fn competing_claims_conserve_food_and_save_resume_is_exact() {
    let mut original = world();
    original.rules.food_growth = 0;
    original.regions[0].food = 2;
    for p in &mut original.regions[0].people {
        p.hunger = 4;
    }
    original.step().unwrap();
    assert_eq!(original.regions[0].food, 0);
    assert_eq!(
        original.regions[0]
            .people
            .iter()
            .map(|p| p.hunger)
            .sum::<u32>(),
        6
    );
    let save = serde_json::to_string(&original).unwrap();
    let mut resumed = World::from_json(&save).unwrap();
    for _ in 0..20 {
        original.step().unwrap();
        resumed.step().unwrap();
    }
    assert_eq!(original, resumed);
}

#[test]
fn invalid_input_and_overflow_are_rejected() {
    let mut w = world();
    w.regions[1].people[0].id = w.regions[0].people[0].id;
    assert!(w.validate().is_err());
    let mut w = world();
    w.tick = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn travel_is_atomic_and_simultaneous() {
    use game_parallel::Move;
    let mut w = world();
    let before = w.clone();
    assert!(
        w.move_people(&[
            Move {
                person: 1,
                destination: 2
            },
            Move {
                person: 999,
                destination: 1
            }
        ])
        .is_err()
    );
    assert_eq!(w, before);
    w.move_people(&[
        Move {
            person: 1,
            destination: 2,
        },
        Move {
            person: 3,
            destination: 1,
        },
    ])
    .unwrap();
    assert_eq!(w.travelers.len(), 2);
    w.step().unwrap();
    let saved = serde_json::to_string(&w).unwrap();
    w = World::from_json(&saved).unwrap();
    w.step().unwrap();
    assert_eq!(
        w.regions[0].people.iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(
        w.regions[1].people.iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![1]
    );
    w.validate().unwrap();
    let saved = serde_json::to_string(&w).unwrap();
    assert_eq!(World::from_json(&saved).unwrap(), w);
}

#[test]
fn travel_order_does_not_change_arrivals() {
    use game_parallel::Move;
    let mut a = world();
    let mut b = a.clone();
    let moves = [
        Move {
            person: 1,
            destination: 2,
        },
        Move {
            person: 2,
            destination: 2,
        },
    ];
    a.move_people(&moves).unwrap();
    b.move_people(&[moves[1], moves[0]]).unwrap();
    assert_eq!(a, b);
}

#[test]
fn shipments_reserve_stock_travel_and_wait_for_space() {
    use game_parallel::Shipment;
    let mut w = world();
    w.rules.food_growth = 0;
    w.rules.hunger_per_tick = 0;
    w.regions[0].food = 10;
    w.regions[1].food = 99;
    let cargo = Shipment {
        id: 5,
        source: 1,
        destination: 2,
        food: 4,
        ticks_remaining: 2,
    };
    let before = w.clone();
    let mut too_much = cargo.clone();
    too_much.id = 6;
    too_much.food = 7;
    assert!(w.launch_shipments(&[cargo.clone(), too_much]).is_err());
    assert_eq!(w, before);
    w.launch_shipments(&[cargo]).unwrap();
    assert_eq!(w.regions[0].food, 6);
    w.step().unwrap();
    assert_eq!(w.regions[1].food, 99);
    let saved = serde_json::to_string(&w).unwrap();
    w = World::from_json(&saved).unwrap();
    w.step().unwrap();
    assert_eq!(w.regions[1].food, 100);
    assert_eq!(w.shipments[0].food, 3);
    w.regions[1].food = 97;
    w.step().unwrap();
    assert_eq!(w.regions[1].food, 100);
    assert!(w.shipments.is_empty());
}

#[test]
fn large_contended_region_is_deterministic_across_workers() {
    let mut a = world();
    a.regions[0].people = (100..5100)
        .map(|id| Person {
            id,
            hunger: 4,
            health: 100,
        })
        .collect();
    a.rules.food_capacity = 500;
    a.regions[0].food = 500;
    a.rules.food_growth = 0;
    let mut b = a.clone();
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let four = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    one.install(|| a.step().unwrap());
    four.install(|| b.step().unwrap());
    assert_eq!(a, b);
    assert_eq!(a.regions[0].food, 0);
    assert_eq!(
        a.regions[0].people.iter().filter(|p| p.hunger == 1).count(),
        250
    );
}

#[test]
fn bulk_arbitration_matches_reference_order_and_person_layout() {
    fn priority(seed: u64, tick: u64, id: u64) -> u64 {
        let mut x = seed ^ tick.wrapping_mul(0x9e3779b97f4a7c15) ^ id;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    let mut w = world();
    w.regions[0].people = (100..5100)
        .map(|id| Person {
            id,
            hunger: 4,
            health: 100,
        })
        .collect();
    w.rules.food_capacity = 10000;
    w.rules.food_growth = 0;
    w.regions[0].food = 2468;
    let mut expected: Vec<_> = w.regions[0]
        .people
        .iter()
        .map(|p| (priority(w.seed, w.tick, p.id), p.id))
        .collect();
    expected.sort_unstable();
    let mut winners: Vec<_> = expected.iter().take(1234).map(|(_, id)| *id).collect();
    winners.sort_unstable();
    w.regions[0].people.reverse();
    w.step().unwrap();
    let mut actual: Vec<_> = w.regions[0]
        .people
        .iter()
        .filter(|p| p.hunger == 1)
        .map(|p| p.id)
        .collect();
    actual.sort_unstable();
    assert_eq!(actual, winners);
}

#[test]
fn bulk_grants_match_sorted_reference_across_scarcity_and_layouts() {
    fn priority(seed: u64, tick: u64, id: u64) -> u64 {
        let mut x = seed ^ tick.wrapping_mul(0x9e3779b97f4a7c15) ^ id;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        for amount in [0, 2, 3999, 8000, 10000] {
            let mut w = world();
            w.tick = 17;
            w.seed = 456;
            w.rules.food_capacity = 10000;
            w.rules.food_growth = 0;
            w.regions[0].food = amount;
            w.regions[0].people = (0..5000)
                .rev()
                .map(|id| Person {
                    id: id * 1234567 + 10,
                    hunger: if id % 3 == 0 { 0 } else { 4 },
                    health: if id % 7 == 0 { 0 } else { 100 },
                })
                .collect();
            let mut expected = w.regions[0].clone();
            for p in &mut expected.people {
                if p.health > 0 {
                    p.hunger = p.hunger.saturating_add(w.rules.hunger_per_tick);
                }
            }
            let mut claims: Vec<_> = expected
                .people
                .iter()
                .enumerate()
                .filter(|(_, p)| p.health > 0 && p.hunger >= w.rules.hunger_satisfied)
                .map(|(i, p)| (priority(w.seed, w.tick, p.id), p.id, i))
                .collect();
            claims.sort_unstable();
            for (_, _, i) in claims {
                if expected.food < w.rules.food_per_meal {
                    break;
                }
                expected.food -= w.rules.food_per_meal;
                expected.people[i].hunger = expected.people[i]
                    .hunger
                    .saturating_sub(w.rules.hunger_satisfied);
            }
            for p in &mut expected.people {
                if p.health == 0 {
                    continue;
                }
                if p.hunger >= w.rules.starvation_threshold {
                    p.health = p.health.saturating_sub(w.rules.starvation_damage);
                } else {
                    p.health = p
                        .health
                        .saturating_add(w.rules.recovery)
                        .min(w.rules.max_health);
                }
            }
            pool.install(|| w.step().unwrap());
            assert_eq!(w.regions[0], expected, "workers={workers}, food={amount}");
        }
    }
}

#[test]
fn concurrent_deliveries_match_serial_priority_and_conserve_cargo() {
    use game_parallel::{Region, Route, Shipment};
    let mut initial = world();
    initial.rules.food_growth = 0;
    initial.rules.food_capacity = 1000;
    for region in &mut initial.regions {
        region.people.clear();
    }
    initial.regions[0].food = 5;
    initial.regions[1].food = 700;
    initial.regions.push(Region {
        id: 3,
        food: 997,
        people: Vec::new(),
    });
    initial.routes.push(Route {
        source: 1,
        destination: 3,
        ticks: 3,
    });
    initial.shipments = (1..=8000)
        .rev()
        .map(|id| Shipment {
            id,
            source: 1,
            destination: if id % 2 == 0 { 2 } else { 3 },
            food: (id % 7 + 1) as u32,
            ticks_remaining: if id % 4 == 0 { 2 } else { 1 },
        })
        .collect();
    initial.validate().unwrap();
    let total = |w: &World| -> u64 {
        w.regions.iter().map(|r| u64::from(r.food)).sum::<u64>()
            + w.shipments.iter().map(|s| u64::from(s.food)).sum::<u64>()
    };
    let initial_total = total(&initial);
    let mut expected = initial.clone();
    expected.shipments.sort_unstable_by_key(|s| s.id);
    for shipment in &mut expected.shipments {
        shipment.ticks_remaining = shipment.ticks_remaining.saturating_sub(1);
        if shipment.ticks_remaining == 0 {
            let region = expected
                .regions
                .iter_mut()
                .find(|r| r.id == shipment.destination)
                .unwrap();
            let delivered = shipment
                .food
                .min(expected.rules.food_capacity - region.food);
            region.food += delivered;
            shipment.food -= delivered;
        }
    }
    expected.shipments.retain(|s| s.food > 0);
    expected.tick += 1;
    for workers in [1, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut actual = initial.clone();
        pool.install(|| actual.step().unwrap());
        assert_eq!(actual, expected);
        assert_eq!(total(&actual), initial_total);
        assert_eq!(
            World::from_json(&serde_json::to_string(&actual).unwrap()).unwrap(),
            actual
        );
    }
}

#[test]
fn full_storage_keeps_waiting_cargo_and_advances_other_shipments() {
    use game_parallel::Shipment;
    let mut w = world();
    w.rules.food_growth = 0;
    for r in &mut w.regions {
        r.people.clear();
    }
    w.regions[1].food = w.rules.food_capacity;
    w.shipments = vec![
        Shipment {
            id: 9,
            source: 1,
            destination: 2,
            food: 3,
            ticks_remaining: 1,
        },
        Shipment {
            id: 2,
            source: 1,
            destination: 2,
            food: 4,
            ticks_remaining: 3,
        },
    ];
    w.step().unwrap();
    assert_eq!(
        w.shipments
            .iter()
            .map(|s| (s.id, s.food, s.ticks_remaining))
            .collect::<Vec<_>>(),
        vec![(2, 4, 2), (9, 3, 0)]
    );
    w.validate().unwrap();
}

#[test]
fn concentrated_shipments_match_serial_priority_across_chunks() {
    use game_parallel::Shipment;
    let mut initial = world();
    initial.rules.food_growth = 0;
    initial.rules.food_capacity = 4321;
    for region in &mut initial.regions {
        region.people.clear();
        region.food = 0;
    }
    initial.shipments = (0..9000)
        .map(|id| Shipment {
            id,
            source: 1,
            destination: 2,
            food: (id % 9 + 1) as u32,
            ticks_remaining: if id % 5 == 0 { 2 } else { 1 },
        })
        .collect();
    let mut expected = initial.clone();
    for s in &mut expected.shipments {
        s.ticks_remaining = s.ticks_remaining.saturating_sub(1);
        if s.ticks_remaining == 0 {
            let food = &mut expected.regions[1].food;
            let delivered = s.food.min(expected.rules.food_capacity - *food);
            *food += delivered;
            s.food -= delivered;
        }
    }
    expected.shipments.retain(|s| s.food > 0);
    expected.tick += 1;
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut actual = initial.clone();
        pool.install(|| actual.step().unwrap());
        assert_eq!(actual, expected);
    }
}

#[test]
fn invalid_traveler_destination_rejects_tick_without_losing_person() {
    use game_parallel::Move;
    let mut w = world();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.travelers[0].destination = 999;
    w.travelers[0].ticks_remaining = 1;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn large_travel_batches_are_deterministic_and_preserve_every_person() {
    use game_parallel::Traveler;
    let mut initial = world();
    initial.rules.food_growth = 0;
    for r in &mut initial.regions {
        r.people.clear();
    }
    initial.travelers = (1..=10000)
        .rev()
        .map(|id| Traveler {
            person: Person {
                id,
                hunger: (id % 7) as u32,
                health: 100,
            },
            source: if id % 2 == 0 { 1 } else { 2 },
            destination: if id % 2 == 0 { 2 } else { 1 },
            ticks_remaining: if id % 3 == 0 { 2 } else { 1 },
        })
        .collect();
    initial.validate().unwrap();
    let mut outcomes = Vec::new();
    for threads in [1, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        pool.install(|| w.step().unwrap());
        assert_eq!(
            w.regions.iter().map(|r| r.people.len()).sum::<usize>() + w.travelers.len(),
            10000
        );
        w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
        pool.install(|| w.step().unwrap());
        assert!(w.travelers.is_empty());
        assert_eq!(
            w.regions.iter().map(|r| r.people.len()).sum::<usize>(),
            10000
        );
        w.validate().unwrap();
        outcomes.push(w);
    }
    assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
}
