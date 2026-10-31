use game_parallel::{Person, Traveler, World};
use game_parallel_fused_physical::Kernel;
#[test]
fn held_food_is_eaten_locally_before_competing_for_regional_food() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    let mut reference = state.clone();
    assert!(reference.step().is_err());
    assert_eq!(reference, state);
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let result = kernel.export();
                assert_eq!(result.regions[0].people[0].hunger, 5);
                assert_eq!(result.regions[0].people[1].hunger, 0);
                assert_eq!(result.regions[0].food, 0);
                assert!(result.cargo.as_ref().unwrap().holdings.is_empty());
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn carried_meals_find_people_with_sparse_identifiers() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    state.regions[0].people[0].id = 100;
    state.regions[0].people[1].id = 300;
    state.regions[1].people[0].id = 500;
    state.cargo.as_mut().unwrap().holdings[0].person = 100;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].people[0].hunger, 5);
    assert_eq!(result.regions[0].people[1].hunger, 0);
    assert!(result.cargo.unwrap().holdings.is_empty());
}

#[test]
fn an_incomplete_carried_meal_is_kept_and_regional_food_is_used() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    state.cargo.as_mut().unwrap().holdings[0].food = 1;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].people[0].hunger, 5);
    assert_eq!(result.regions[0].people[1].hunger, 4);
    assert_eq!(result.regions[0].food, 0);
    assert_eq!(result.cargo.unwrap().holdings[0].food, 1);
}

#[test]
fn local_carried_meals_are_excluded_from_overlapping_regional_arbitration() {
    use game_parallel::Holding;
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    let count = 6000u64;
    let resident_count = count / 2;
    state.rules.hunger_per_tick = 4;
    state.rules.hunger_satisfied = 4;
    state.rules.starvation_threshold = 100;
    state.rules.food_growth = (resident_count * 2) as u32;
    state.rules.food_capacity = 20_000;
    state.regions[0].food = 0;
    state.regions[0].people = (1..=count)
        .filter(|id| id % 2 == 0)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.travelers = (1..=count)
        .filter(|id| id % 2 == 1)
        .map(|id| Traveler {
            person: Person {
                id,
                health: 100,
                hunger: 0,
            },
            source: 2,
            destination: 1,
            ticks_remaining: 1,
        })
        .collect();
    state.cargo.as_mut().unwrap().holdings = (1..=count)
        .filter(|id| id % 2 == 1 && id % 3 == 0)
        .map(|person| Holding { person, food: 2 })
        .collect();
    let granted: std::collections::HashSet<_> = (1..=count)
        .filter(|id| !(id % 2 == 1 && id % 3 == 0))
        .take(resident_count as usize)
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.regions[0].food, 0);
                assert!(result.cargo.as_ref().unwrap().holdings.is_empty());
                for person in &result.regions[0].people {
                    let is_carrier = person.id % 2 == 1 && person.id % 3 == 0;
                    let hungry = if person.id % 2 == 1 { 8 } else { 4 };
                    let expected = if is_carrier || granted.contains(&person.id) {
                        hungry - 4
                    } else {
                        hungry
                    };
                    assert_eq!(person.hunger, expected, "person {}", person.id);
                }
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn failed_tick_does_not_consume_a_carried_or_regional_meal() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    state.cargo = World::from_toml(include_str!("../../../../content/parallel/local_food.toml"))
        .unwrap()
        .cargo;
    state.regions[0].people[0].hunger = 8;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn overlapping_arrival_blocks_feed_lowest_eligible_identities_across_workers() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    let count = 6000u64;
    let meals = 1700u32;
    let identity = |n: u64| {
        if n <= count / 2 {
            n * 3
        } else {
            (1u64 << 60) + n * 7
        }
    };
    state.regions[0].people = (1..=count)
        .filter(|id| id % 2 == 0)
        .map(|n| Person {
            id: identity(n),
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.travelers = (1..=count)
        .filter(|id| id % 2 == 1)
        .map(|n| Traveler {
            person: Person {
                id: identity(n),
                health: 100,
                hunger: 0,
            },
            source: 2,
            destination: 1,
            ticks_remaining: 1,
        })
        .collect();
    state.rules.hunger_per_tick = 4;
    state.rules.hunger_satisfied = 4;
    state.rules.starvation_threshold = 9;
    state.rules.food_growth = 0;
    state.rules.food_per_meal = 2;
    state.rules.recovery = 0;
    state.rules.food_capacity = 20000;
    state.regions[0].food = ((count / 2) as u32 + meals) * 2;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for _ in 0..2 {
                    kernel.step().unwrap();
                }
                kernel.export()
            })
    };
    let one = run(1);
    let four = run(4);
    assert_eq!(one, four);
    for (index, person) in one.regions[0].people.iter().enumerate() {
        let n = index as u64 + 1;
        assert_eq!(person.id, identity(n));
        let hungry = if n.is_multiple_of(2) { 4 } else { 8 };
        let expected = if n <= u64::from(meals) {
            hungry - 4
        } else {
            hungry
        };
        assert_eq!(person.hunger, expected, "person {n}");
        assert_eq!(person.health, 100);
    }
    assert_eq!(one.regions[0].food, 0);
}

#[test]
fn independent_overlapping_regions_resolve_their_own_food_cutoffs() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    state.rules.hunger_per_tick = 4;
    state.rules.hunger_satisfied = 4;
    state.rules.starvation_threshold = 9;
    state.rules.food_growth = 0;
    state.rules.food_per_meal = 2;
    state.rules.recovery = 0;
    state.rules.food_capacity = 1000;
    state.travelers.clear();
    for region in 0..2 {
        let base = (region as u64 + 1) * 100000;
        state.regions[region].people = (1..=100u64)
            .filter(|n| n.is_multiple_of(2))
            .map(|n| Person {
                id: base + n * 11,
                health: 100,
                hunger: 0,
            })
            .collect();
        state.regions[region].food = (50 + if region == 0 { 23 } else { 31 }) * 2;
        for n in (1..=100u64).filter(|n| n % 2 == 1) {
            state.travelers.push(Traveler {
                person: Person {
                    id: base + n * 11,
                    health: 100,
                    hunger: 0,
                },
                source: if region == 0 { 2 } else { 1 },
                destination: region as u64 + 1,
                ticks_remaining: 1,
            });
        }
    }
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    for (region, meals) in [23u64, 31].into_iter().enumerate() {
        assert_eq!(result.regions[region].food, 0);
        for (index, person) in result.regions[region].people.iter().enumerate() {
            let n = index as u64 + 1;
            let hungry = if n.is_multiple_of(2) { 4 } else { 8 };
            assert_eq!(
                person.hunger,
                if n <= meals { hungry - 4 } else { hungry },
                "region {region} person {n}"
            );
        }
    }
}

#[test]
fn individual_food_acquisition_is_conserved_and_eaten_on_a_later_tick() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    state.regions[0].food = 2;
    state.regions[0].people[0].hunger = 3;
    state.regions[0].people[1].hunger = 3;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.personal_food_access = true;
    cargo.acquire_when_hungry = vec![2];
    let mut reference = state.clone();
    assert!(reference.step().is_err());
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let first = kernel.export();
                assert_eq!(first.regions[0].people[0].hunger, 4);
                assert_eq!(first.regions[0].people[1].hunger, 4);
                assert_eq!(first.regions[0].food, 0);
                assert_eq!(
                    first.cargo.as_ref().unwrap().holdings,
                    vec![game_parallel::Holding { person: 2, food: 2 }]
                );
                let restored = World::from_json(&serde_json::to_string(&first).unwrap()).unwrap();
                let mut restored = Kernel::compile(restored).unwrap();
                kernel.step().unwrap();
                restored.step().unwrap();
                let result = kernel.export();
                assert_eq!(result, restored.export());
                assert_eq!(result.regions[0].people[0].hunger, 5);
                assert_eq!(result.regions[0].people[1].hunger, 1);
                assert!(result.cargo.as_ref().unwrap().holdings.is_empty());
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn acquisition_respects_capacity_and_rolls_back_on_tick_failure() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    state.cargo = World::from_toml(include_str!("../../../../content/parallel/local_food.toml"))
        .unwrap()
        .cargo;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.personal_food_access = true;
    cargo.acquire_when_hungry = vec![1];
    cargo.holdings.clear();
    state.regions[0].people[0].hunger = 8;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);

    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.personal_food_access = true;
    cargo.acquire_when_hungry = vec![1, 2];
    cargo.capacity = 2;
    cargo.holdings[0].food = 1;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].food, 0);
    assert_eq!(
        result.cargo.unwrap().holdings,
        vec![
            game_parallel::Holding { person: 1, food: 1 },
            game_parallel::Holding { person: 2, food: 2 }
        ]
    );
}

#[test]
fn crowded_individual_acquisition_conserves_stock_and_is_worker_independent() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    state.rules.food_capacity = 20000;
    state.regions[0].food = 6000;
    state.regions[0].people = (1..=6000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 3,
        })
        .collect();
    state.regions[1].people.clear();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.personal_food_access = true;
    cargo.acquire_when_hungry = (1..=6000).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.regions[0].food, 0);
                let holdings = &result.cargo.as_ref().unwrap().holdings;
                assert_eq!(holdings.len(), 3000);
                assert_eq!(
                    holdings.iter().map(|h| u64::from(h.food)).sum::<u64>(),
                    6000
                );
                assert!(result.regions[0].people.iter().all(|p| p.hunger == 4));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn changing_a_persons_food_choice_invalidates_prepared_attempts() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap();
    state.cargo.as_mut().unwrap().holdings.clear();
    let mut kernel = Kernel::compile(state).unwrap();
    let draft = kernel.prepare().unwrap();
    kernel.set_food_seeking(2, false).unwrap();
    let before = kernel.export();
    assert!(kernel.commit(draft).is_err());
    assert_eq!(kernel.export(), before);
    kernel.step().unwrap();
    let after = kernel.export();
    assert_eq!(after.regions[0].food, 2);
    assert!(after.cargo.unwrap().holdings.iter().all(|h| h.person != 2));
}

#[test]
fn seeking_food_occupies_the_same_person_as_farm_labor() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers.toml"
    ))
    .unwrap();
    state.cargo = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap()
    .cargo;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = vec![1];
    state.regions[0].people[0].hunger = 3;
    let previous_farm = state.agriculture.clone();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.agriculture, previous_farm);
    assert_eq!(result.cargo.unwrap().holdings[0].person, 1);
}

#[test]
fn gifts_move_real_food_and_only_participants_learn_from_success() {
    let text = format!(
        "{}\n[opinions.rules]\nminimum = -5\nmaximum = 5\ninitial = 0\nfulfilled_gain = 0\ndefault_loss = 0\ngratitude_gain = 3\n",
        include_str!("../../../../content/parallel/local_food.toml")
    );
    let state = World::from_toml(&text).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    let draft = kernel.prepare().unwrap();
    let gift = game_parallel::FoodGift {
        giver: 1,
        recipient: 2,
        amount: 2,
    };
    kernel.give_food_batch(&[gift]).unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].food, 2);
    assert_eq!(
        result.cargo.as_ref().unwrap().holdings,
        vec![game_parallel::Holding { person: 2, food: 2 }]
    );
    let opinions = result.opinions.as_ref().unwrap();
    assert_eq!(opinions.index().obligation(2, 1), 3);
    assert_eq!(opinions.index().obligation(1, 2), 0);
    assert_eq!(opinions.index().obligation(3, 1), 0);
    assert!(kernel.commit(draft).is_err());
    assert_eq!(kernel.export(), result);
    assert!(kernel.give_food_batch(&[gift]).is_err());
    assert_eq!(kernel.export(), result);
    let saved = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(Kernel::compile(saved).unwrap().export(), result);
}

#[test]
fn gift_batches_reject_remote_duplicate_and_over_capacity_participants_atomically() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    for gifts in [
        vec![game_parallel::FoodGift {
            giver: 1,
            recipient: 3,
            amount: 1,
        }],
        vec![
            game_parallel::FoodGift {
                giver: 1,
                recipient: 2,
                amount: 1,
            },
            game_parallel::FoodGift {
                giver: 1,
                recipient: 2,
                amount: 1,
            },
        ],
        vec![game_parallel::FoodGift {
            giver: 1,
            recipient: 2,
            amount: 3,
        }],
    ] {
        assert!(kernel.give_food_batch(&gifts).is_err());
        assert_eq!(kernel.export(), before);
    }
    let mut full = before.clone();
    full.cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(game_parallel::Holding {
            person: 2,
            food: 10,
        });
    let mut kernel = Kernel::compile(full).unwrap();
    let before = kernel.export();
    assert!(
        kernel
            .give_food_batch(&[game_parallel::FoodGift {
                giver: 1,
                recipient: 2,
                amount: 1
            }])
            .is_err()
    );
    assert_eq!(kernel.export(), before);
}

#[test]
fn many_local_food_gifts_are_conserved_and_worker_independent() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/local_food.toml")).unwrap();
    state.regions[0].people = (1..=10000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.cargo.as_mut().unwrap().holdings = (1..=5000)
        .map(|person| game_parallel::Holding { person, food: 2 })
        .collect();
    let gifts: Vec<_> = (1..=5000)
        .map(|giver| game_parallel::FoodGift {
            giver,
            recipient: giver + 5000,
            amount: 1,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.give_food_batch(&gifts).unwrap();
                let result = kernel.export();
                let holdings = &result.cargo.as_ref().unwrap().holdings;
                assert_eq!(holdings.len(), 10000);
                assert!(holdings.iter().all(|h| h.food == 1));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn fear_can_motivate_food_help_and_the_person_can_stop_after_fear_fades() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let first = kernel.export();
                assert_eq!(first.regions[0].food, 2);
                assert_eq!(first.regions[0].people[0].hunger, 9);
                assert_eq!(first.regions[0].people[1].hunger, 4);
                assert_eq!(
                    first.cargo.as_ref().unwrap().holdings,
                    vec![
                        game_parallel::Holding { person: 2, food: 2 },
                        game_parallel::Holding { person: 3, food: 2 }
                    ]
                );
                let opinions = first.opinions.as_ref().unwrap().index();
                assert_eq!(opinions.fear(1, 2), 2);
                assert_eq!(opinions.obligation(2, 1), 1);
                let restored = World::from_json(&serde_json::to_string(&first).unwrap()).unwrap();
                let mut restored = Kernel::compile(restored).unwrap();
                kernel.pickup_food(1, 2).unwrap();
                restored.pickup_food(1, 2).unwrap();
                kernel.step().unwrap();
                restored.step().unwrap();
                let result = kernel.export();
                assert_eq!(result, restored.export());
                assert_eq!(result.regions[0].people[1].hunger, 1);
                assert!(
                    result
                        .cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .all(|h| h.person != 2)
                );
                assert_eq!(result.opinions.as_ref().unwrap().index().fear(1, 2), 0);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn food_aid_rolls_back_when_an_unrelated_lifecycle_phase_fails() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let source =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.cargo = source.cargo;
    state.opinions = source.opinions;
    state.regions[0].people[1].hunger = 3;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn simultaneous_food_aid_never_uses_one_participant_twice() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.regions[0].people = (1..=5001)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 3,
        })
        .collect();
    state.regions[1].people.clear();
    state.opinions = None;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = (1..=5000)
        .map(|person| game_parallel::Holding { person, food: 2 })
        .collect();
    cargo.food_aid = (1..=5000)
        .map(|giver| game_parallel::FoodAidPolicy {
            giver,
            recipient: 5001,
            amount: 2,
            retain: 0,
            help_value: 1,
            threshold: 1,
            interval: 1,
            requires_request: false,
            enabled: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let holdings = &result.cargo.as_ref().unwrap().holdings;
                assert_eq!(holdings.iter().find(|h| h.person == 5001).unwrap().food, 2);
                assert_eq!(holdings.iter().map(|h| u64::from(h.food)).sum::<u64>(), 2);
                assert_eq!(result.regions[0].people[5000].hunger, 4);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_can_change_their_food_help_choice_before_commit() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    let draft = kernel.prepare().unwrap();
    kernel.set_food_aid(1, false).unwrap();
    let before = kernel.export();
    assert!(kernel.commit(draft).is_err());
    assert_eq!(kernel.export(), before);
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result
            .cargo
            .unwrap()
            .holdings
            .iter()
            .find(|h| h.person == 2)
            .unwrap()
            .food,
        2
    );
    assert_eq!(
        result.opinions.as_ref().unwrap().index().obligation(2, 1),
        0
    );
}

#[test]
fn a_worker_stalled_by_hunger_can_seek_and_eat_before_resuming_work() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    state.cargo = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap()
    .cargo;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = vec![1];
    state.rules.food_growth = 0;
    state.regions[0].people[0].hunger = 11;
    state
        .production
        .as_mut()
        .unwrap()
        .jobs
        .push(game_parallel::Job {
            id: 10,
            worker: 1,
            region: 1,
            recipe: 1,
            remaining: 2,
            phase: game_parallel::JobPhase::Working,
            reserved: vec![game_parallel::Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
            outputs: vec![game_parallel::Amount {
                good: 2,
                quantity: 2,
            }]
            .into(),
        });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    assert_eq!(first.production.as_ref().unwrap().jobs[0].remaining, 2);
    assert_eq!(first.cargo.as_ref().unwrap().holdings[0].food, 2);
    kernel.step().unwrap();
    let second = kernel.export();
    assert_eq!(second.production.as_ref().unwrap().jobs[0].remaining, 2);
    assert_eq!(second.regions[0].people[0].hunger, 9);
    kernel.step().unwrap();
    let third = kernel.export();
    assert_eq!(third.production.as_ref().unwrap().jobs[0].remaining, 1);
}

#[test]
fn automatic_aid_merges_sparse_and_partially_retained_holdings() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.regions[0].people = vec![
        Person {
            id: 10,
            health: 100,
            hunger: 0,
        },
        Person {
            id: 20,
            health: 100,
            hunger: 3,
        },
        Person {
            id: 40,
            health: 100,
            hunger: 0,
        },
        Person {
            id: 80,
            health: 100,
            hunger: 3,
        },
        Person {
            id: 90,
            health: 100,
            hunger: 0,
        },
    ];
    state.regions[1].people.clear();
    state.opinions = None;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![
        game_parallel::Holding {
            person: 10,
            food: 4,
        },
        game_parallel::Holding {
            person: 20,
            food: 1,
        },
        game_parallel::Holding {
            person: 40,
            food: 3,
        },
        game_parallel::Holding {
            person: 90,
            food: 1,
        },
    ];
    cargo.food_aid = vec![
        game_parallel::FoodAidPolicy {
            giver: 10,
            recipient: 20,
            amount: 2,
            retain: 2,
            help_value: 1,
            threshold: 1,
            interval: 1,
            requires_request: false,
            enabled: true,
        },
        game_parallel::FoodAidPolicy {
            giver: 40,
            recipient: 80,
            amount: 2,
            retain: 1,
            help_value: 1,
            threshold: 1,
            interval: 1,
            requires_request: false,
            enabled: true,
        },
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.cargo.as_ref().unwrap().holdings,
                    vec![
                        game_parallel::Holding {
                            person: 10,
                            food: 2
                        },
                        game_parallel::Holding {
                            person: 20,
                            food: 3
                        },
                        game_parallel::Holding {
                            person: 40,
                            food: 1
                        },
                        game_parallel::Holding {
                            person: 80,
                            food: 2
                        },
                        game_parallel::Holding {
                            person: 90,
                            food: 1
                        }
                    ]
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn food_actions_use_the_participants_fatigue_budget() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.fatigue = World::from_toml(include_str!("../../../../content/parallel/fatigue.toml"))
        .unwrap()
        .fatigue;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    for person in [1, 2, 3] {
        assert_eq!(
            result
                .fatigue
                .as_ref()
                .unwrap()
                .people
                .iter()
                .find(|row| row.id == person)
                .unwrap()
                .fatigue,
            3
        );
    }
}

#[test]
fn a_food_request_waits_for_the_givers_choice_and_is_resolved_only_by_delivery() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.regions[0].people[0].hunger = 0;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry = vec![3];
    cargo.food_aid[0].requires_request = true;
    cargo.food_aid[0].threshold = 0;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                assert_eq!(
                    kernel.export().cargo.as_ref().unwrap().holdings[0].person,
                    1
                );
                let stale = kernel.prepare().unwrap();
                let request = game_parallel::FoodRequest {
                    id: 71,
                    requester: 2,
                    giver: 1,
                    amount: 2,
                    expires_at: 5,
                };
                kernel.request_food_batch(&[request]).unwrap();
                let before = kernel.export();
                assert!(kernel.commit(stale).is_err());
                assert_eq!(kernel.export(), before);
                kernel.set_food_aid(1, false).unwrap();
                kernel.step().unwrap();
                let waiting = kernel.export();
                assert_eq!(waiting.cargo.as_ref().unwrap().food_requests, vec![request]);
                assert_eq!(waiting.cargo.as_ref().unwrap().holdings[0].person, 1);
                let restored = World::from_json(&serde_json::to_string(&waiting).unwrap()).unwrap();
                let mut restored = Kernel::compile(restored).unwrap();
                kernel.set_food_aid(1, true).unwrap();
                restored.set_food_aid(1, true).unwrap();
                kernel.step().unwrap();
                restored.step().unwrap();
                let result = kernel.export();
                assert_eq!(result, restored.export());
                assert!(result.cargo.as_ref().unwrap().food_requests.is_empty());
                assert!(
                    result
                        .cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .any(|h| h.person == 2 && h.food == 2)
                );
                assert_eq!(
                    result.opinions.as_ref().unwrap().index().obligation(2, 1),
                    1
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn ignored_food_requests_expire_without_compelling_a_delivery() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.regions[0].people[0].hunger = 0;
    state.cargo.as_mut().unwrap().food_aid[0].enabled = false;
    let mut kernel = Kernel::compile(state).unwrap();
    let request = game_parallel::FoodRequest {
        id: 4,
        requester: 2,
        giver: 1,
        amount: 2,
        expires_at: 1,
    };
    kernel.request_food_batch(&[request]).unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().cargo.as_ref().unwrap().food_requests,
        vec![request]
    );
    kernel.step().unwrap();
    let result = kernel.export();
    assert!(result.cargo.as_ref().unwrap().food_requests.is_empty());
    assert_eq!(
        result.opinions.as_ref().unwrap().index().obligation(2, 1),
        0
    );
}

#[test]
fn food_request_batches_are_atomic_with_a_concentrated_recipient() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap();
    state.regions[0].people = (1..=5001)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry.clear();
    let requests: Vec<_> = (2..=5001)
        .map(|requester| game_parallel::FoodRequest {
            id: requester,
            requester,
            giver: 1,
            amount: 2,
            expires_at: 5,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let mut invalid = requests.clone();
                invalid[4999].id = invalid[0].id;
                let before = kernel.export();
                assert!(kernel.request_food_batch(&invalid).is_err());
                assert_eq!(kernel.export(), before);
                kernel.request_food_batch(&requests).unwrap();
                let result = kernel.export();
                assert_eq!(result.cargo.as_ref().unwrap().food_requests, requests);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn food_requester_can_withdraw_and_a_manual_delivery_satisfies_their_request() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    let request = game_parallel::FoodRequest {
        id: 7,
        requester: 2,
        giver: 1,
        amount: 2,
        expires_at: 5,
    };
    kernel.request_food_batch(&[request]).unwrap();
    let before = kernel.export();
    assert!(kernel.withdraw_food_request(1, 7).is_err());
    assert_eq!(kernel.export(), before);
    let draft = kernel.prepare().unwrap();
    kernel.withdraw_food_request(2, 7).unwrap();
    assert!(kernel.commit(draft).is_err());
    assert!(
        kernel
            .export()
            .cargo
            .as_ref()
            .unwrap()
            .food_requests
            .is_empty()
    );
    kernel.request_food_batch(&[request]).unwrap();
    kernel
        .give_food_batch(&[game_parallel::FoodGift {
            giver: 1,
            recipient: 2,
            amount: 2,
        }])
        .unwrap();
    let result = kernel.export();
    assert!(result.cargo.as_ref().unwrap().food_requests.is_empty());
    assert!(
        result
            .cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .any(|h| h.person == 2 && h.food == 2)
    );
}

#[test]
fn hungry_person_asks_a_known_giver_then_receives_only_after_their_later_decision() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.regions[0].people[0].hunger = 0;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.food_aid[0].requires_request = true;
    cargo.food_aid[0].help_value = 1;
    cargo.food_askers = vec![game_parallel::FoodAsker {
        person: 2,
        giver: 1,
        choose_known: false,
        amount: 2,
        interval: 1,
        lifetime: 3,
        enabled: true,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let first = kernel.export();
                let cargo = first.cargo.as_ref().unwrap();
                assert_eq!(
                    cargo.food_requests,
                    vec![game_parallel::FoodRequest {
                        id: 0,
                        requester: 2,
                        giver: 1,
                        amount: 2,
                        expires_at: 4
                    }]
                );
                assert_eq!(cargo.next_food_request, 1);
                assert_eq!(first.regions[0].food, 2);
                assert_eq!(first.regions[0].people[1].hunger, 4);
                assert!(cargo.holdings.iter().any(|h| h.person == 1 && h.food == 2));
                let restored = World::from_json(&serde_json::to_string(&first).unwrap()).unwrap();
                let mut restored = Kernel::compile(restored).unwrap();
                kernel.step().unwrap();
                restored.step().unwrap();
                let second = kernel.export();
                assert_eq!(second, restored.export());
                assert!(second.cargo.as_ref().unwrap().food_requests.is_empty());
                assert!(
                    second
                        .cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .any(|h| h.person == 2 && h.food == 2)
                );
                assert_eq!(
                    second.opinions.as_ref().unwrap().index().obligation(2, 1),
                    1
                );
                kernel.step().unwrap();
                let third = kernel.export();
                assert_eq!(third.regions[0].people[1].hunger, 2);
                assert!(third.cargo.as_ref().unwrap().food_requests.is_empty());
                third
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn automatic_request_overflow_aborts_the_whole_tick() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.next_food_request = u64::MAX;
    cargo.food_askers = vec![game_parallel::FoodAsker {
        person: 2,
        giver: 1,
        choose_known: false,
        amount: 2,
        interval: 1,
        lifetime: 3,
        enabled: true,
    }];
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn many_automatic_food_requests_to_one_person_are_bounded_and_worker_independent() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap();
    state.regions[0].people = (1..=5001)
        .map(|id| Person {
            id,
            health: 100,
            hunger: if id == 1 { 0 } else { 3 },
        })
        .collect();
    state.regions[1].people.clear();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry.clear();
    cargo.food_askers = (2..=5001)
        .map(|person| game_parallel::FoodAsker {
            person,
            giver: 1,
            choose_known: false,
            amount: 2,
            interval: 1,
            lifetime: 3,
            enabled: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let cargo = result.cargo.as_ref().unwrap();
                assert_eq!(cargo.food_requests.len(), 5000);
                assert_eq!(cargo.next_food_request, 5000);
                assert_eq!(result.regions[0].food, 2);
                assert!(result.regions[0].people[1..].iter().all(|p| p.hunger == 4));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_can_stop_automatically_asking_for_food() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.cargo.as_mut().unwrap().food_askers = vec![game_parallel::FoodAsker {
        person: 2,
        giver: 1,
        choose_known: false,
        amount: 2,
        interval: 1,
        lifetime: 3,
        enabled: true,
    }];
    let mut kernel = Kernel::compile(state).unwrap();
    let draft = kernel.prepare().unwrap();
    kernel.set_food_asking(2, false).unwrap();
    assert!(kernel.commit(draft).is_err());
    kernel.step().unwrap();
    assert!(
        kernel
            .export()
            .cargo
            .as_ref()
            .unwrap()
            .food_requests
            .is_empty()
    );
}

#[test]
fn one_giver_can_choose_between_thousands_of_personal_food_requests() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    state.regions[0].people = (1..=5001)
        .map(|id| Person {
            id,
            health: 100,
            hunger: if id == 1 { 0 } else { 3 },
        })
        .collect();
    state.regions[1].people.clear();
    state.opinions = None;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![game_parallel::Holding { person: 1, food: 2 }];
    cargo.food_askers = (2..=5001)
        .map(|person| game_parallel::FoodAsker {
            person,
            giver: 1,
            choose_known: false,
            amount: 2,
            interval: 1,
            lifetime: 5,
            enabled: true,
        })
        .collect();
    cargo.food_aid = (2..=5001)
        .map(|recipient| game_parallel::FoodAidPolicy {
            giver: 1,
            recipient,
            amount: 2,
            retain: 0,
            help_value: 1,
            threshold: 1,
            interval: 1,
            requires_request: true,
            enabled: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let cargo = result.cargo.as_ref().unwrap();
                assert_eq!(cargo.food_requests.len(), 4999);
                assert_eq!(
                    cargo.holdings,
                    vec![game_parallel::Holding { person: 2, food: 2 }]
                );
                assert!(cargo.food_requests.iter().all(|r| r.requester != 2));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn hungry_person_can_choose_a_past_helper_instead_of_the_initial_contact() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let person = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(person);
    state.regions[0].people[1].hunger = 8;
    let cargo = state.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![game_parallel::Holding { person: 3, food: 2 }];
    cargo.food_askers = vec![game_parallel::FoodAsker {
        person: 2,
        giver: 1,
        choose_known: true,
        amount: 2,
        interval: 1,
        lifetime: 3,
        enabled: true,
    }];
    let run = |workers, leave| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel
                    .give_food_batch(&[game_parallel::FoodGift {
                        giver: 3,
                        recipient: 2,
                        amount: 2,
                    }])
                    .unwrap();
                kernel.eat_carried_food(2).unwrap();
                let saved = kernel.export();
                assert_eq!(saved.opinions.as_ref().unwrap().index().obligation(2, 3), 1);
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                if leave {
                    kernel
                        .move_people(&[game_parallel::Move {
                            person: 3,
                            destination: 2,
                        }])
                        .unwrap();
                }
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.cargo.as_ref().unwrap().food_requests[0].giver,
                    if leave { 1 } else { 3 }
                );
                result
            })
    };
    assert_eq!(run(1, false), run(4, false));
    assert_eq!(run(1, true), run(4, true));
}

#[test]
fn people_take_from_separate_physical_stores_and_claimants_can_object() {
    use game_parallel::{FoodStore, FoodStoreChoice};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.regions[0].people[0].hunger = 3;
    world.opinions.as_mut().unwrap().records.clear();
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = vec![1, 2];
    cargo.food_stores = vec![
        FoodStore {
            id: 10,
            region: 1,
            food: 2,
            capacity: 10,
            claimant: Some(1),
            object_to_taking: true,
        },
        FoodStore {
            id: 20,
            region: 1,
            food: 2,
            capacity: 10,
            claimant: Some(3),
            object_to_taking: true,
        },
    ];
    cargo.food_store_choices = vec![
        FoodStoreChoice {
            person: 1,
            store: 20,
            reconsider_on_failure: false,
        },
        FoodStoreChoice {
            person: 2,
            store: 10,
            reconsider_on_failure: false,
        },
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let result = kernel.export();
                let cargo = result.cargo.as_ref().unwrap();
                assert_eq!(
                    cargo.food_stores.iter().map(|s| s.food).collect::<Vec<_>>(),
                    vec![0, 0]
                );
                assert_eq!(
                    cargo
                        .holdings
                        .iter()
                        .map(|h| (h.person, h.food))
                        .collect::<Vec<_>>(),
                    vec![(1, 2), (2, 2)]
                );
                assert_eq!(result.regions[0].food, before.regions[0].food);
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 2), -2);
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(3, 1), 0);
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn concentrated_store_withdrawals_conserve_food_across_workers() {
    use game_parallel::{FoodStore, FoodStoreChoice, Person};
    let count = 5_000u64;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.opinions = None;
    world.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 3,
            health: 100,
        })
        .collect();
    world.regions[1].people.clear();
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = (1..=count).collect();
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: count as u32,
        capacity: count as u32,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_store_choices = (1..=count)
        .map(|person| FoodStoreChoice {
            person,
            store: 10,
            reconsider_on_failure: false,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let cargo = result.cargo.as_ref().unwrap();
                assert_eq!(cargo.food_stores[0].food, 0);
                assert_eq!(cargo.holdings.len(), count as usize / 2);
                assert!(cargo.holdings.iter().all(|h| h.food == 2));
                assert_eq!(result.regions[0].food, world.regions[0].food);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn changing_source_is_checked_and_invalidates_an_old_draft() {
    use game_parallel::FoodStore;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.cargo.as_mut().unwrap().food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 2,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.set_food_source(3, Some(10)).is_err());
    assert_eq!(kernel.export(), before);
    let draft = kernel.prepare().unwrap();
    kernel.set_food_source(2, Some(10)).unwrap();
    assert!(kernel.commit(draft).is_err());
    assert_eq!(
        kernel.export().cargo.unwrap().food_store_choices[0].person,
        2
    );
    kernel.set_food_source(2, None).unwrap();
    assert!(kernel.export().cargo.unwrap().food_store_choices.is_empty());
}

#[test]
fn manual_store_transfers_are_local_conserved_and_not_blocked_by_a_claim() {
    use game_parallel::FoodStore;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.opinions.as_mut().unwrap().records.clear();
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    world.cargo.as_mut().unwrap().food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 2,
        capacity: 2,
        claimant: Some(1),
        object_to_taking: true,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.transfer_food_store(3, 10, 1, false).is_err());
    assert_eq!(kernel.export(), before);
    let draft = kernel.prepare().unwrap();
    kernel.transfer_food_store(2, 10, 2, false).unwrap();
    assert!(kernel.commit(draft).is_err());
    let result = kernel.export();
    assert_eq!(result.cargo.as_ref().unwrap().food_stores[0].food, 0);
    assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 2), -2);
    kernel.transfer_food_store(2, 10, 1, true).unwrap();
    let result = kernel.export();
    assert_eq!(result.cargo.as_ref().unwrap().food_stores[0].food, 1);
    assert_eq!(
        result
            .cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .find(|h| h.person == 2)
            .unwrap()
            .food,
        1
    );
    assert!(kernel.transfer_food_store(2, 10, 2, true).is_err());
    assert_eq!(kernel.export(), result);
}

#[test]
fn store_withdrawals_and_claimant_reactions_roll_back_with_failed_tick() {
    use game_parallel::{FoodStore, FoodStoreChoice};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let source =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.cargo = source.cargo;
    world.opinions = source.opinions;
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    world.regions[0].people[1].hunger = 3;
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = vec![2];
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 2,
        capacity: 2,
        claimant: Some(1),
        object_to_taking: true,
    }];
    cargo.food_store_choices = vec![FoodStoreChoice {
        person: 2,
        store: 10,
        reconsider_on_failure: false,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn voluntary_store_restock_and_later_withdrawal_are_separate_actions() {
    use game_parallel::{FoodDepositPolicy, FoodStore, FoodStoreChoice};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.acquire_when_hungry = vec![2];
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 0,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_store_choices = vec![FoodStoreChoice {
        person: 2,
        store: 10,
        reconsider_on_failure: false,
    }];
    cargo.food_depositors = vec![FoodDepositPolicy {
        person: 1,
        store: 10,
        retain: 0,
        claim_after_deposit: None,
        interval: 1,
        enabled: true,
    }];
    let run = |workers, refuse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                if refuse {
                    kernel.set_food_deposit(1, false).unwrap();
                }
                kernel.step().unwrap();
                let first = kernel.export();
                assert_eq!(
                    first.cargo.as_ref().unwrap().food_stores[0].food,
                    if refuse { 0 } else { 2 }
                );
                assert!(
                    first
                        .cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .all(|h| h.person != 2)
                );
                if !refuse {
                    let mut loaded = Kernel::compile(
                        World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                    )
                    .unwrap();
                    kernel.step().unwrap();
                    loaded.step().unwrap();
                    assert_eq!(kernel.export(), loaded.export());
                    assert!(
                        kernel
                            .export()
                            .cargo
                            .unwrap()
                            .holdings
                            .iter()
                            .any(|h| h.person == 2 && h.food == 2)
                    );
                }
                kernel.export()
            })
    };
    assert_eq!(run(1, false), run(4, false));
    assert_eq!(run(1, true), run(4, true));
}

#[test]
fn concentrated_voluntary_deposits_share_capacity_and_conserve_food() {
    use game_parallel::{FoodDepositPolicy, FoodStore, Holding, Person};
    let count = 5_000u64;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.opinions = None;
    world.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = (1..=count)
        .map(|person| Holding { person, food: 2 })
        .collect();
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 0,
        capacity: count as u32,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_depositors = (1..=count)
        .map(|person| FoodDepositPolicy {
            person,
            store: 10,
            retain: 0,
            claim_after_deposit: None,
            interval: 1,
            enabled: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let cargo = result.cargo.as_ref().unwrap();
                assert_eq!(cargo.food_stores[0].food, count as u32);
                assert_eq!(cargo.holdings.len(), count as usize / 2);
                assert!(cargo.holdings.iter().all(|holding| holding.food == 2));
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_newly_started_job_prevents_a_second_automatic_deposit_action() {
    use game_parallel::{FoodDepositPolicy, FoodStore, Holding, WorkPolicy};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/work_requests.toml"
    ))
    .unwrap();
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))
        .unwrap()
        .cargo;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![Holding { person: 2, food: 2 }];
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 0,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_depositors = vec![FoodDepositPolicy {
        person: 2,
        store: 10,
        retain: 0,
        claim_after_deposit: None,
        interval: 1,
        enabled: true,
    }];
    world.autowork.as_mut().unwrap().policies.push(WorkPolicy {
        person: 2,
        recipe: 1,
        enabled: true,
        disregard_claims: false,
    });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.production.unwrap().jobs[0].worker, 2);
    assert_eq!(result.cargo.as_ref().unwrap().food_stores[0].food, 0);
    assert_eq!(result.cargo.unwrap().holdings[0].food, 2);
}

#[test]
fn autonomous_store_deposits_roll_back_with_failed_tick() {
    use game_parallel::{FoodDepositPolicy, FoodStore};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))
        .unwrap()
        .cargo;
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 0,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_depositors = vec![FoodDepositPolicy {
        person: 1,
        store: 10,
        retain: 0,
        claim_after_deposit: None,
        interval: 1,
        enabled: true,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn building_a_store_consumes_carried_material_and_does_not_create_a_lock() {
    use game_parallel::{FoodStoreBlueprint, GoodsDirection, GoodsTransfer};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/work_requests.toml"
    ))
    .unwrap();
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/food_aid.toml"))
        .unwrap()
        .cargo;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.food_store_blueprints = vec![FoodStoreBlueprint {
        id: 7,
        good: 1,
        quantity: 3,
        capacity: 20,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.build_food_store(1, 10, 7).is_err());
    assert_eq!(kernel.export(), before);
    kernel
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 1,
            quantity: 3,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    let draft = kernel.prepare().unwrap();
    kernel.build_food_store(1, 10, 7).unwrap();
    assert!(kernel.commit(draft).is_err());
    let result = kernel.export();
    assert!(
        result
            .goods_cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .all(|holder| holder.person != 1)
    );
    let store = &result.cargo.as_ref().unwrap().food_stores[0];
    assert_eq!(
        (
            store.id,
            store.region,
            store.capacity,
            store.food,
            store.claimant,
            store.object_to_taking
        ),
        (10, 1, 20, 0, Some(1), false)
    );
    assert_eq!(
        result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        6
    );
    let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(Kernel::compile(restored).unwrap().export(), result);
    assert!(kernel.build_food_store(1, 10, 7).is_err());
    assert_eq!(kernel.export(), result);
}

#[test]
fn a_claimant_can_change_their_own_response_without_preventing_taking() {
    use game_parallel::FoodStore;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.opinions.as_mut().unwrap().records.clear();
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    world.cargo.as_mut().unwrap().food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 4,
        capacity: 4,
        claimant: Some(1),
        object_to_taking: false,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.set_store_objection(2, 10, true).is_err());
    assert_eq!(kernel.export(), before);
    kernel.transfer_food_store(2, 10, 2, false).unwrap();
    assert_eq!(
        kernel
            .export()
            .opinions
            .as_ref()
            .unwrap()
            .index()
            .trust(1, 2),
        0
    );
    kernel.set_store_objection(1, 10, true).unwrap();
    kernel.transfer_food_store(2, 10, 2, false).unwrap();
    let result = kernel.export();
    assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 2), -2);
    assert_eq!(result.cargo.unwrap().food_stores[0].food, 0);
}

#[test]
fn a_person_can_confront_a_witnessed_taking_without_the_targets_consent() {
    use game_parallel::{ConfrontationPreference, FoodStore, OpinionSensitivity};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.conflict = World::from_toml(include_str!("../../../../content/parallel/conflict.toml"))
        .unwrap()
        .conflict;
    world.conflict.as_mut().unwrap().damage = 5;
    world.regions[0].people[0].hunger = 0;
    world.opinions.as_mut().unwrap().records.clear();
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    world
        .opinions
        .as_mut()
        .unwrap()
        .sensitivities
        .push(OpinionSensitivity {
            person: 1,
            fear_per_damage: None,
            gratitude_gain: None,
            obligation_relief: None,
            theft_loss: None,
            confrontation: Some(ConfrontationPreference {
                trust_at_most: -2,
                fear_at_most: 0,
                interval: 1,
                enabled: true,
            }),
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
            affinity_site_use: None,
            affinity_site_destruction: None,
            affinity_site_salvage: None,
            relationship_cases: vec![],
            obligation_development: None,
            promise_obligation_gain: None,
        });
    world.cargo.as_mut().unwrap().food_aid.clear();
    world.cargo.as_mut().unwrap().food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 2,
        capacity: 2,
        claimant: Some(1),
        object_to_taking: true,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.transfer_food_store(2, 10, 2, false).unwrap();
                assert_eq!(
                    kernel
                        .export()
                        .opinions
                        .as_ref()
                        .unwrap()
                        .index()
                        .trust(1, 2),
                    -2
                );
                kernel.step().unwrap();
                let started = kernel.export();
                let fight = &started.conflict.as_ref().unwrap().encounters[0];
                assert_eq!(fight.fighters.len(), 2);
                assert!(fight.fighters.iter().any(|fighter| fighter.person == 1
                    && matches!(
                        fighter.order,
                        game_parallel::CombatOrder::Attack { target: 2 }
                    )));
                assert_eq!(started.regions[0].people[1].health, 100);
                let mut loaded = Kernel::compile(
                    World::from_json(&serde_json::to_string(&started).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                loaded.step().unwrap();
                assert_eq!(kernel.export(), loaded.export());
                assert!(kernel.export().regions[0].people[1].health < 100);
                kernel.export()
            })
    };
    assert_eq!(run(1), run(4));
}

fn confrontation_fixture() -> World {
    use game_parallel::{ConfrontationPreference, Opinion, OpinionSensitivity};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.conflict = World::from_toml(include_str!("../../../../content/parallel/conflict.toml"))
        .unwrap()
        .conflict;
    world.conflict.as_mut().unwrap().damage = 5;
    let opinions = world.opinions.as_mut().unwrap();
    opinions.records = vec![Opinion {
        observer: 1,
        target: 2,
        trust: -2,
        fear: 0,
        obligation: 0,
        affinity: 0,
    }];
    opinions.sensitivities = vec![OpinionSensitivity {
        person: 1,
        fear_per_damage: None,
        gratitude_gain: None,
        obligation_relief: None,
        theft_loss: None,
        confrontation: Some(ConfrontationPreference {
            trust_at_most: -2,
            fear_at_most: 0,
            interval: 1,
            enabled: true,
        }),
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        affinity_site_use: None,
        affinity_site_destruction: None,
        affinity_site_salvage: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    }];
    world.cargo = None;
    world
}

#[test]
fn fear_or_individual_choice_can_prevent_a_confrontation() {
    for afraid in [false, true] {
        let mut world = confrontation_fixture();
        if afraid {
            world.opinions.as_mut().unwrap().records[0].fear = 1;
        }
        let mut kernel = Kernel::compile(world).unwrap();
        if !afraid {
            kernel.set_confrontation(1, false).unwrap();
        }
        kernel.step().unwrap();
        assert!(kernel.export().conflict.unwrap().encounters.is_empty());
    }
}

#[test]
fn a_confronted_person_can_choose_to_retreat() {
    let mut kernel = Kernel::compile(confrontation_fixture()).unwrap();
    kernel.step().unwrap();
    kernel
        .combat_orders(&[game_parallel::CombatCommand {
            encounter: 1,
            person: 2,
            order: game_parallel::CombatOrder::Retreat,
        }])
        .unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert!(result.conflict.unwrap().encounters.is_empty());
}

#[test]
fn concentrated_confrontations_select_disjoint_people_deterministically() {
    use game_parallel::{ConfrontationPreference, Opinion, OpinionSensitivity, Person};
    let count = 5_000u64;
    let mut world = confrontation_fixture();
    world.regions[0].people = (1..=count + 1)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    let opinions = world.opinions.as_mut().unwrap();
    opinions.records = (1..=count)
        .map(|observer| Opinion {
            observer,
            target: count + 1,
            trust: -2,
            fear: 0,
            obligation: 0,
            affinity: 0,
        })
        .collect();
    opinions.sensitivities = (1..=count)
        .map(|person| OpinionSensitivity {
            person,
            fear_per_damage: None,
            gratitude_gain: None,
            obligation_relief: None,
            theft_loss: None,
            confrontation: Some(ConfrontationPreference {
                trust_at_most: -2,
                fear_at_most: 0,
                interval: 1,
                enabled: true,
            }),
            affinity_help: None,
            affinity_harm_per_damage: None,
            affinity_encounter: None,
            affinity_pressured_encounter: None,
            affinity_site_use: None,
            affinity_site_destruction: None,
            affinity_site_salvage: None,
            relationship_cases: vec![],
            obligation_development: None,
            promise_obligation_gain: None,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let fights = &result.conflict.as_ref().unwrap().encounters;
                assert_eq!(fights.len(), 1);
                assert_eq!(
                    fights[0]
                        .fighters
                        .iter()
                        .map(|f| f.person)
                        .collect::<Vec<_>>(),
                    vec![1, count + 1]
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn failed_tick_does_not_start_a_confrontation() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let source = confrontation_fixture();
    world.conflict = source.conflict;
    world.opinions = source.opinions;
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn a_confrontation_does_not_start_when_a_participant_dies_in_the_same_tick() {
    let mut world = confrontation_fixture();
    world.regions[0].food = 0;
    world.regions[0].people[0].health = 1;
    world.regions[0].people[0].hunger = world.rules.starvation_threshold;
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].people[0].health, 0);
    assert!(result.conflict.as_ref().unwrap().encounters.is_empty());
    Kernel::compile(result).unwrap();
}

#[test]
fn a_person_can_reconsider_an_empty_store_after_a_real_failed_attempt() {
    use game_parallel::{FoodStore, FoodStoreChoice};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap();
    let cargo = world.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = vec![2];
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 0,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_store_choices = vec![FoodStoreChoice {
        person: 2,
        store: 10,
        reconsider_on_failure: true,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let first = kernel.export();
                assert!(first.cargo.as_ref().unwrap().food_store_choices.is_empty());
                assert!(first.cargo.as_ref().unwrap().holdings.is_empty());
                assert_eq!(first.regions[0].food, 2);
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                let second = kernel.export();
                assert_eq!(second.cargo.as_ref().unwrap().holdings[0].person, 2);
                assert_eq!(second.regions[0].food, 0);
                second
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn conflicting_personal_store_claims_do_not_prevent_taking_and_are_individually_revocable() {
    use game_parallel::FoodStore;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let person = world.regions[1].people.pop().unwrap();
    world.regions[0].people.push(person);
    world.opinions.as_mut().unwrap().records.clear();
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    world.cargo.as_mut().unwrap().food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 6,
        capacity: 6,
        claimant: Some(1),
        object_to_taking: true,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let draft = kernel.prepare().unwrap();
    kernel.set_food_store_claim(3, 10, true).unwrap();
    assert!(kernel.commit(draft).is_err());
    kernel.set_store_objection(3, 10, true).unwrap();
    kernel.transfer_food_store(2, 10, 2, false).unwrap();
    let result = kernel.export();
    let opinions = result.opinions.as_ref().unwrap().index();
    assert_eq!(opinions.trust(1, 2), -2);
    assert_eq!(opinions.trust(3, 2), -2);
    assert_eq!(result.cargo.as_ref().unwrap().food_stores[0].food, 4);
    let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(Kernel::compile(restored).unwrap().export(), result);
    kernel.set_food_store_claim(1, 10, false).unwrap();
    kernel.set_store_objection(3, 10, false).unwrap();
    kernel.transfer_food_store(2, 10, 2, false).unwrap();
    let result = kernel.export();
    let opinions = result.opinions.as_ref().unwrap().index();
    assert_eq!(opinions.trust(1, 2), -2);
    assert_eq!(opinions.trust(3, 2), -2);
    assert_eq!(result.cargo.unwrap().food_stores[0].food, 2);
}

#[test]
fn automatic_taking_notifies_only_present_objecting_claimants_across_worker_counts() {
    use game_parallel::{FoodStore, FoodStoreChoice, FoodStoreClaim};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.opinions.as_mut().unwrap().records.clear();
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = vec![2];
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 2,
        capacity: 2,
        claimant: Some(1),
        object_to_taking: true,
    }];
    cargo.food_store_claims = vec![FoodStoreClaim {
        person: 3,
        store: 10,
        object_to_taking: true,
    }];
    cargo.food_store_choices = vec![FoodStoreChoice {
        person: 2,
        store: 10,
        reconsider_on_failure: false,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let opinions = result.opinions.as_ref().unwrap().index();
                assert_eq!(opinions.trust(1, 2), -2);
                assert_eq!(opinions.trust(3, 2), 0);
                assert_eq!(result.cargo.as_ref().unwrap().food_stores[0].food, 0);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn crowded_store_claims_bound_observation_work_and_preserve_parallel_determinism() {
    use game_parallel::{FoodStore, FoodStoreChoice, FoodStoreClaim, Person};
    let count = 1_000u64;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.regions[0].people = (1..=count * 2)
        .map(|id| Person {
            id,
            hunger: 3,
            health: 100,
        })
        .collect();
    world.regions[1].people.clear();
    let opinions = world.opinions.as_mut().unwrap();
    opinions.records.clear();
    opinions.rules.theft_loss = 1;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = (count + 1..=count * 2).collect();
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: count as u32 * 2,
        capacity: count as u32 * 2,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_store_claims = (1..=count)
        .map(|person| FoodStoreClaim {
            person,
            store: 10,
            object_to_taking: true,
        })
        .collect();
    cargo.food_store_choices = (count + 1..=count * 2)
        .map(|person| FoodStoreChoice {
            person,
            store: 10,
            reconsider_on_failure: false,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.cargo.as_ref().unwrap().holdings.len(),
                    count as usize
                );
                assert_eq!(
                    result.opinions.as_ref().unwrap().records.len(),
                    count as usize * 4
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn duplicate_or_unknown_store_claims_fail_validation() {
    use game_parallel::{FoodStore, FoodStoreClaim};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 2,
        capacity: 2,
        claimant: Some(1),
        object_to_taking: true,
    }];
    cargo.food_store_claims = vec![FoodStoreClaim {
        person: 1,
        store: 10,
        object_to_taking: true,
    }];
    assert!(Kernel::compile(world.clone()).is_err());
    world.cargo.as_mut().unwrap().food_store_claims[0].person = 999;
    assert!(Kernel::compile(world).is_err());
}

#[test]
fn people_can_form_overlapping_claims_by_actually_depositing_food() {
    use game_parallel::{FoodDepositPolicy, FoodStore, Holding};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let third = world.regions[1].people.pop().unwrap();
    world.regions[0].people.push(third);
    for person in &mut world.regions[0].people {
        person.hunger = 0;
    }
    world.opinions.as_mut().unwrap().records.clear();
    world.opinions.as_mut().unwrap().rules.theft_loss = 2;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![
        Holding { person: 1, food: 2 },
        Holding { person: 2, food: 2 },
    ];
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 0,
        capacity: 4,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_depositors = (1..=2)
        .map(|person| FoodDepositPolicy {
            person,
            store: 10,
            retain: 0,
            interval: 1,
            enabled: true,
            claim_after_deposit: Some(true),
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let after = kernel.export();
                assert_eq!(after.cargo.as_ref().unwrap().food_stores[0].food, 4);
                assert_eq!(after.cargo.as_ref().unwrap().food_store_claims.len(), 2);
                assert!(after.cargo.as_ref().unwrap().holdings.is_empty());
                kernel.transfer_food_store(3, 10, 2, false).unwrap();
                let result = kernel.export();
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 3), -2);
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(2, 3), -2);
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn failed_deposit_does_not_form_a_claim_and_people_can_change_their_tendency() {
    use game_parallel::{FoodDepositPolicy, FoodStore, Holding};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.regions[0].people[0].hunger = 0;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![Holding { person: 1, food: 2 }];
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 2,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_depositors = vec![FoodDepositPolicy {
        person: 1,
        store: 10,
        retain: 0,
        interval: 1,
        enabled: true,
        claim_after_deposit: Some(true),
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    assert!(
        kernel
            .export()
            .cargo
            .as_ref()
            .unwrap()
            .food_store_claims
            .is_empty()
    );
    kernel.set_deposit_claim(1, None).unwrap();
    kernel.transfer_food_store(2, 10, 2, false).unwrap();
    kernel.step().unwrap();
    let cargo = kernel.export().cargo.unwrap();
    assert_eq!(cargo.food_stores[0].food, 2);
    assert!(cargo.food_store_claims.is_empty());
}

#[test]
fn one_observer_has_bounded_attention_even_if_many_people_take_simultaneously() {
    use game_parallel::{FoodStore, FoodStoreChoice, Person};
    let count = 1_000u64;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    world.regions[0].people = (1..=count + 1)
        .map(|id| Person {
            id,
            hunger: 3,
            health: 100,
        })
        .collect();
    world.regions[1].people.clear();
    let opinions = world.opinions.as_mut().unwrap();
    opinions.records.clear();
    opinions.rules.theft_loss = 1;
    let cargo = world.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = (2..=count + 1).collect();
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: count as u32 * 2,
        capacity: count as u32 * 2,
        claimant: Some(1),
        object_to_taking: true,
    }];
    cargo.food_store_choices = (2..=count + 1)
        .map(|person| FoodStoreChoice {
            person,
            store: 10,
            reconsider_on_failure: false,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.cargo.as_ref().unwrap().holdings.len(),
                    count as usize
                );
                assert_eq!(result.opinions.as_ref().unwrap().records.len(), 32);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn autonomous_builders_consume_their_own_material_and_construct_in_parallel() {
    use game_parallel::{Amount, FoodStoreBuilder, GoodsHolding};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))
    .unwrap();
    world.cargo.as_mut().unwrap().acquire_when_hungry.clear();
    world.goods_cargo.as_mut().unwrap().holdings = vec![
        GoodsHolding {
            person: 1,
            items: vec![Amount {
                good: 1,
                quantity: 4,
            }]
            .into(),
        },
        GoodsHolding {
            person: 2,
            items: vec![Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
        },
    ];
    world.cargo.as_mut().unwrap().food_store_builders = (1..=2)
        .map(|person| FoodStoreBuilder {
            person,
            blueprint: 7,
            remaining: 1,
            interval: 1,
            object_to_taking: true,
            use_after_build: person == 1,
            deposit_after_build: person == 1,
            enabled: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let result = kernel.export();
                let cargo = result.cargo.as_ref().unwrap();
                assert_eq!(
                    cargo
                        .food_stores
                        .iter()
                        .map(|s| (s.id, s.claimant, s.capacity))
                        .collect::<Vec<_>>(),
                    vec![(1, Some(1), 20), (2, Some(2), 20)]
                );
                assert!(
                    cargo
                        .food_store_builders
                        .iter()
                        .all(|row| row.remaining == 0)
                );
                assert!(
                    cargo
                        .food_store_choices
                        .iter()
                        .any(|choice| choice.person == 1 && choice.store == 1)
                );
                assert!(
                    cargo
                        .food_depositors
                        .iter()
                        .any(|policy| policy.person == 1 && policy.store == 1)
                );
                let goods = &result.goods_cargo.as_ref().unwrap().holdings;
                assert_eq!(goods.len(), 1);
                assert_eq!((goods[0].person, goods[0].items[0].quantity), (1, 1));
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                kernel.pickup_food(1, 2).unwrap();
                kernel.step().unwrap();
                assert_eq!(
                    kernel.export().cargo.as_ref().unwrap().food_stores[0].food,
                    2
                );
                kernel.transfer_food_store(2, 1, 2, false).unwrap();
                let result = kernel.export();
                assert_eq!(result.cargo.as_ref().unwrap().food_stores[0].food, 0);
                assert!(
                    result
                        .cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .any(|holding| holding.person == 2 && holding.food == 2)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn construction_and_disjoint_trade_preserve_both_material_changes() {
    use game_parallel::{
        Amount, FoodStoreBuilder, GoodsHolding, Person, TradePolicy, TradeProposal, TradeValue,
    };
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))
    .unwrap();
    let third = world.regions[1].people.pop().unwrap();
    world.regions[0].people.push(third);
    world.regions[0].people.push(Person {
        id: 4,
        health: 100,
        hunger: 0,
    });
    world.cargo.as_mut().unwrap().acquire_when_hungry.clear();
    world.cargo.as_mut().unwrap().food_store_builders = vec![FoodStoreBuilder {
        person: 1,
        blueprint: 7,
        remaining: 1,
        interval: 1,
        object_to_taking: false,
        use_after_build: false,
        deposit_after_build: false,
        enabled: true,
    }];
    world.goods_cargo.as_mut().unwrap().holdings = vec![
        GoodsHolding {
            person: 1,
            items: vec![Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
        },
        GoodsHolding {
            person: 3,
            items: vec![Amount {
                good: 1,
                quantity: 2,
            }]
            .into(),
        },
    ];
    world.market.as_mut().unwrap().policies.push(TradePolicy {
        person: 4,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        influences: Vec::new(),
        values: vec![TradeValue { good: 1, value: 1 }],
    });
    world
        .propose_trade(TradeProposal {
            proposer: 3,
            recipient: 4,
            give: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            receive: vec![],
        })
        .unwrap();
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.cargo.as_ref().unwrap().food_stores[0].claimant,
        Some(1)
    );
    let goods = &result.goods_cargo.as_ref().unwrap().holdings;
    assert_eq!(
        goods
            .iter()
            .map(|h| (h.person, h.items[0].quantity))
            .collect::<Vec<_>>(),
        vec![(3, 1), (4, 1)]
    );
    assert!(result.market.unwrap().offers.is_empty());
}

#[test]
fn construction_respects_personal_choice_and_id_overflow_rolls_back() {
    use game_parallel::{Amount, FoodStore, FoodStoreBuilder, GoodsHolding};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))
    .unwrap();
    world.cargo.as_mut().unwrap().acquire_when_hungry.clear();
    world.goods_cargo.as_mut().unwrap().holdings = vec![GoodsHolding {
        person: 1,
        items: vec![Amount {
            good: 1,
            quantity: 3,
        }]
        .into(),
    }];
    world.cargo.as_mut().unwrap().food_store_builders = vec![FoodStoreBuilder {
        person: 1,
        blueprint: 7,
        remaining: 1,
        interval: 1,
        object_to_taking: false,
        use_after_build: false,
        deposit_after_build: false,
        enabled: true,
    }];
    world.cargo.as_mut().unwrap().food_stores = vec![FoodStore {
        id: u64::MAX,
        region: 1,
        food: 0,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
    kernel.set_store_building(1, false).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.cargo.as_ref().unwrap().food_stores.len(), 1);
    assert_eq!(result.goods_cargo.unwrap().holdings[0].items[0].quantity, 3);
}

#[test]
fn repeated_regional_scarcity_rotates_across_sparse_person_ids() {
    use game_parallel::Person;
    use std::collections::BTreeSet;
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap();
    world.rules.food_growth = 2;
    world.rules.starvation_threshold = 1000;
    world.regions[0].food = 0;
    world.regions[0].people = (1..=10)
        .map(|i| Person {
            id: i * i * 10,
            hunger: 100,
            health: 100,
        })
        .collect();
    world.regions[1].people.clear();
    let cargo = world.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = world.regions[0]
        .people
        .iter()
        .map(|person| person.id)
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                let mut winners = BTreeSet::new();
                for tick in 0..80 {
                    kernel.step().unwrap();
                    let result = kernel.export();
                    winners.extend(
                        result
                            .cargo
                            .as_ref()
                            .unwrap()
                            .holdings
                            .iter()
                            .map(|row| row.person),
                    );
                    if tick == 39 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&result).unwrap()).unwrap(),
                        )
                        .unwrap();
                    }
                }
                assert_eq!(
                    winners.len(),
                    10,
                    "every persistent contender should receive food during this run"
                );
                (winners, kernel.export())
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn repeated_store_scarcity_rotates_across_contenders() {
    use game_parallel::{FoodStore, FoodStoreChoice, Person};
    use std::collections::BTreeSet;
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/personal_food.toml"
    ))
    .unwrap();
    world.rules.starvation_threshold = 1000;
    world.regions[0].food = 200;
    world.rules.food_capacity = 200;
    world.regions[0].people = (1..=11)
        .map(|id| Person {
            id,
            hunger: if id == 11 { 0 } else { 100 },
            health: 100,
        })
        .collect();
    world.regions[1].people.clear();
    let cargo = world.cargo.as_mut().unwrap();
    cargo.holdings.clear();
    cargo.acquire_when_hungry = (1..=10).collect();
    cargo.food_stores = vec![FoodStore {
        id: 10,
        region: 1,
        food: 0,
        capacity: 2,
        claimant: None,
        object_to_taking: false,
    }];
    cargo.food_store_choices = (1..=10)
        .map(|person| FoodStoreChoice {
            person,
            store: 10,
            reconsider_on_failure: false,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                let mut winners = BTreeSet::new();
                for _ in 0..80 {
                    kernel.pickup_food(11, 2).unwrap();
                    kernel.transfer_food_store(11, 10, 2, true).unwrap();
                    kernel.step().unwrap();
                    winners.extend(
                        kernel
                            .export()
                            .cargo
                            .unwrap()
                            .holdings
                            .iter()
                            .filter(|row| row.person != 11)
                            .map(|row| row.person),
                    );
                }
                assert_eq!(
                    winners.len(),
                    10,
                    "every persistent contender should receive food during this run"
                );
                (winners, kernel.export())
            })
    };
    assert_eq!(run(1), run(4));
}
