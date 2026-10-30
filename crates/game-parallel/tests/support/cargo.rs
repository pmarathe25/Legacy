use game_parallel::{Move, World};

fn world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/cargo.toml")).unwrap();
    world.rules.food_growth = 0;
    for region in &mut world.regions {
        for person in &mut region.people {
            person.hunger = 0;
        }
    }
    world
}
fn total_food(w: &World) -> u64 {
    w.regions.iter().map(|r| u64::from(r.food)).sum::<u64>()
        + w.shipments.iter().map(|s| u64::from(s.food)).sum::<u64>()
        + w.cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .map(|h| u64::from(h.food))
            .sum::<u64>()
}

#[test]
fn carried_food_follows_travel_can_be_eaten_and_deposited() {
    let mut w = world();
    let initial = total_food(&w);
    w.pickup_food(1, 4).unwrap();
    assert_eq!(total_food(&w), initial);
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.step().unwrap();
    assert_eq!(total_food(&w), initial);
    assert!(w.deposit_food(1, 1).is_err());
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.eat_carried_food(1).unwrap();
    assert_eq!(total_food(&w), initial - u64::from(w.rules.food_per_meal));
    w.step().unwrap();
    w.deposit_food(1, 2).unwrap();
    assert!(w.cargo.as_ref().unwrap().holdings.is_empty());
    assert_eq!(total_food(&w), initial - 2);
    w.validate().unwrap();
}

#[test]
fn transfers_reject_invalid_or_unavailable_resources_atomically() {
    let mut w = world();
    for (person, amount) in [(1, 11), (1, 0), (999, 1)] {
        let before = w.clone();
        assert!(w.pickup_food(person, amount).is_err());
        assert_eq!(w, before);
    }
    w.pickup_food(1, 4).unwrap();
    w.regions[0].food = w.rules.food_capacity;
    let before = w.clone();
    assert!(w.deposit_food(1, 1).is_err());
    assert_eq!(w, before);
    w.cargo.as_mut().unwrap().holdings[0].person = 999;
    assert!(w.validate().is_err());
    assert!(World::from_json(&serde_json::to_string(&w).unwrap()).is_err());
}

#[test]
fn deceased_travelers_keep_cargo_until_recovered_locally() {
    let mut w = world();
    w.regions[0].people[0].health = 1;
    w.rules.starvation_threshold = 1;
    w.rules.starvation_damage = 1;
    w.pickup_food(1, 4).unwrap();
    let total = total_food(&w);
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.step().unwrap();
    assert_eq!(w.travelers[0].person.health, 0);
    let before = w.clone();
    assert!(w.recover_food(3, 1, 4).is_err());
    assert_eq!(w, before);
    w.step().unwrap();
    assert_eq!(total_food(&w), total);
    w.recover_food(3, 1, 4).unwrap();
    assert_eq!(total_food(&w), total);
    assert!(w.cargo.as_ref().unwrap().holdings.is_empty());
    w.validate().unwrap();
}

#[test]
fn family_death_during_travel_preserves_cargo_through_save_and_recovery() {
    use game_parallel::Cargo;
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.cargo = Some(Cargo {
        capacity: 10,
        auto_eat_travel: false,
        auto_eat_local: false,
        personal_food_access: false,
        acquire_when_hungry: vec![],
        food_aid: vec![],
        food_stores: vec![],
        food_store_claims: vec![],
        food_store_choices: vec![],
        food_depositors: vec![],
        food_store_blueprints: vec![],
        food_store_builders: vec![],
        inherit_store_building: false,
        food_sharing: vec![],
        inherit_food_sharing: false,
        food_requests: vec![],
        next_food_request: 0,
        food_askers: vec![],
        auto_seek_age: None,
        food_enrollments: vec![],
        auto_pack_travel: false,
        holdings: Vec::new(),
    });
    w.rules.starvation_threshold = 1;
    w.rules.starvation_damage = 1;
    w.regions[0].people[0].health = 1;
    w.pickup_food(1, 4).unwrap();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.step().unwrap();
    assert_eq!(w.travelers[0].person.health, 0);
    assert!(
        !w.families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|m| m.id == 1)
            .unwrap()
            .alive
    );
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    for _ in 0..4 {
        w.step().unwrap();
    }
    let prior = w.regions[1].food;
    w.recover_food(5, 1, 4).unwrap();
    assert_eq!(w.regions[1].food, prior + 4);
    assert!(w.cargo.as_ref().unwrap().holdings.is_empty());
    w.validate().unwrap();
}

#[test]
fn automatic_travel_meal_precedes_starvation_and_persists() {
    let mut w = world();
    w.cargo.as_mut().unwrap().auto_eat_travel = true;
    w.rules.starvation_threshold = 4;
    w.rules.starvation_damage = 5;
    w.regions[0].people[0].health = 1;
    w.regions[0].people[0].hunger = 3;
    w.pickup_food(1, 2).unwrap();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.step().unwrap();
    assert_eq!(w.travelers[0].person.hunger, 0);
    assert_eq!(w.travelers[0].person.health, 2);
    assert!(w.cargo.as_ref().unwrap().holdings.is_empty());
    w.validate().unwrap();
}

#[test]
fn failed_family_step_rolls_back_automatic_travel_consumption() {
    use game_parallel::Cargo;
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.cargo = Some(Cargo {
        capacity: 10,
        auto_eat_travel: true,
        auto_eat_local: false,
        personal_food_access: false,
        acquire_when_hungry: vec![],
        food_aid: vec![],
        food_stores: vec![],
        food_store_claims: vec![],
        food_store_choices: vec![],
        food_depositors: vec![],
        food_store_blueprints: vec![],
        food_store_builders: vec![],
        inherit_store_building: false,
        food_sharing: vec![],
        inherit_food_sharing: false,
        food_requests: vec![],
        next_food_request: 0,
        food_askers: vec![],
        auto_seek_age: None,
        food_enrollments: vec![],
        auto_pack_travel: false,
        holdings: Vec::new(),
    });
    w.regions[0].people[0].hunger = 3;
    w.pickup_food(1, 4).unwrap();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 2)
        .unwrap()
        .age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn duplicate_cargo_is_rejected_before_automatic_consumption() {
    let mut w = world();
    w.cargo.as_mut().unwrap().auto_eat_travel = true;
    w.pickup_food(1, 4).unwrap();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    let holding = w.cargo.as_ref().unwrap().holdings[0].clone();
    w.cargo.as_mut().unwrap().holdings.push(holding);
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn automatic_meals_are_deterministic_for_large_travel_batches() {
    use game_parallel::{Cargo, Holding, Person, Traveler};
    let mut base = world();
    for r in &mut base.regions {
        r.people.clear();
    }
    base.rules.starvation_threshold = 4;
    base.travelers = (1..=10000)
        .rev()
        .map(|id| Traveler {
            person: Person {
                id,
                health: 10,
                hunger: 3,
            },
            source: 1,
            destination: 2,
            ticks_remaining: 2,
        })
        .collect();
    base.cargo = Some(Cargo {
        capacity: 4,
        auto_eat_travel: true,
        auto_eat_local: false,
        personal_food_access: false,
        acquire_when_hungry: vec![],
        food_aid: vec![],
        food_stores: vec![],
        food_store_claims: vec![],
        food_store_choices: vec![],
        food_depositors: vec![],
        food_store_blueprints: vec![],
        food_store_builders: vec![],
        inherit_store_building: false,
        food_sharing: vec![],
        inherit_food_sharing: false,
        food_requests: vec![],
        next_food_request: 0,
        food_askers: vec![],
        auto_seek_age: None,
        food_enrollments: vec![],
        auto_pack_travel: false,
        holdings: (1..=10000)
            .filter(|id| id % 2 == 0)
            .map(|person| Holding { person, food: 2 })
            .collect(),
    });
    base.validate().unwrap();
    let mut worlds = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = base.clone();
        pool.install(|| w.step().unwrap());
        assert!(w.cargo.as_ref().unwrap().holdings.is_empty());
        assert_eq!(
            w.travelers.iter().filter(|t| t.person.hunger == 0).count(),
            5000
        );
        w.validate().unwrap();
        worlds.push(w);
    }
    assert_eq!(worlds[0], worlds[1]);
}

#[test]
fn departure_reserves_food_in_stable_order_without_changing_total() {
    let mut a = world();
    a.cargo.as_mut().unwrap().auto_pack_travel = true;
    a.regions[0].food = 3;
    let total = total_food(&a);
    let before = a.clone();
    assert!(
        a.move_people(&[
            Move {
                person: 1,
                destination: 2
            },
            Move {
                person: 999,
                destination: 2
            }
        ])
        .is_err()
    );
    assert_eq!(a, before);
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
    assert_eq!(a.regions[0].food, 0);
    let held = &a.cargo.as_ref().unwrap().holdings;
    assert_eq!(
        held.iter().map(|h| (h.person, h.food)).collect::<Vec<_>>(),
        vec![(1, 2), (2, 1)]
    );
    assert_eq!(total_food(&a), total);
    a.validate().unwrap();
    assert_eq!(
        World::from_json(&serde_json::to_string(&a).unwrap()).unwrap(),
        a
    );
}

#[test]
fn packing_accounts_for_existing_food_and_capacity() {
    let mut w = world();
    w.pickup_food(1, 1).unwrap();
    w.cargo.as_mut().unwrap().auto_pack_travel = true;
    let total = total_food(&w);
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    assert_eq!(w.cargo.as_ref().unwrap().holdings[0].food, 2);
    assert_eq!(total_food(&w), total);
}

#[test]
fn large_departure_reservations_match_priority_across_workers() {
    use game_parallel::Person;
    let mut base = world();
    base.cargo.as_mut().unwrap().auto_pack_travel = true;
    base.rules.food_capacity = 100000;
    base.regions[0].food = 12345;
    base.routes
        .iter_mut()
        .find(|r| r.source == 1 && r.destination == 2)
        .unwrap()
        .ticks = 8;
    base.regions[0].people = (1..=10000)
        .rev()
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    base.regions[1].people.clear();
    let requests: Vec<_> = (1..=10000)
        .rev()
        .map(|person| Move {
            person,
            destination: 2,
        })
        .collect();
    let total = total_food(&base);
    let mut outcomes = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut w = base.clone();
        pool.install(|| w.move_people(&requests).unwrap());
        assert_eq!(w.regions[0].food, 0);
        let holdings = &w.cargo.as_ref().unwrap().holdings;
        assert_eq!(holdings.len(), 3087);
        assert_eq!((holdings[0].person, holdings[0].food), (1, 4));
        assert_eq!((holdings[3086].person, holdings[3086].food), (3087, 1));
        assert_eq!(total_food(&w), total);
        w.validate().unwrap();
        outcomes.push(w);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn reserved_provisions_support_the_complete_journey() {
    let mut w = world();
    let cargo = w.cargo.as_mut().unwrap();
    cargo.auto_eat_travel = true;
    cargo.auto_pack_travel = true;
    w.regions[0].people.truncate(1);
    w.regions[1].people.clear();
    w.rules.starvation_threshold = 4;
    w.routes
        .iter_mut()
        .find(|r| r.source == 1 && r.destination == 2)
        .unwrap()
        .ticks = 8;
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    assert_eq!(w.cargo.as_ref().unwrap().holdings[0].food, 4);
    for _ in 0..4 {
        w.step().unwrap();
    }
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    for _ in 0..4 {
        w.step().unwrap();
    }
    assert_eq!(w.regions[1].people[0].health, 100);
    assert_eq!(w.regions[1].people[0].hunger, 0);
    assert!(w.cargo.as_ref().unwrap().holdings.is_empty());
    assert_eq!(w.regions[0].food, 6);
    w.validate().unwrap();
}

#[test]
fn authored_policy_enables_both_packing_and_eating() {
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/cargo_auto.toml")).unwrap();
    assert!(w.cargo.as_ref().unwrap().auto_pack_travel);
    assert!(w.cargo.as_ref().unwrap().auto_eat_travel);
    w.move_people(&[Move {
        person: 2,
        destination: 2,
    }])
    .unwrap();
    assert_eq!(w.cargo.as_ref().unwrap().holdings[0].food, 2);
    w.step().unwrap();
    assert_eq!(w.cargo.as_ref().unwrap().holdings[0].food, 2);
    w.step().unwrap();
    assert!(w.cargo.as_ref().unwrap().holdings.is_empty());
}
