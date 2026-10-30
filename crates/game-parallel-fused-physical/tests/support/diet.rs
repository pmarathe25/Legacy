use game_parallel::{Amount, FoodProfile, Good, GoodsHolding, World};
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/store_building.toml"
    ))
    .unwrap();
    world.production.as_mut().unwrap().goods.push(Good {
        id: 9,
        name: "dates".into(),
        food: Some(FoodProfile {
            portion: 2,
            hunger_relief: 7,
            nutrients: Default::default(),
            health_effect: 0,
            shelf_days: None,
        }),
    });
    world.goods_cargo.as_mut().unwrap().holdings = vec![GoodsHolding {
        person: 1,
        items: vec![Amount {
            good: 9,
            quantity: 4,
        }]
        .into(),
    }];
    world.cargo.as_mut().unwrap().acquire_when_hungry.clear();
    world.regions[0].people[0].hunger = 5;
    world
}

fn nutrition_rules() -> game_parallel::NutritionRules {
    game_parallel::NutritionRules {
        interval: 10,
        capacity: [100; 6],
        initial: [10; 6],
        metabolism: [1; 6],
        minimum: [1; 6],
        deficiency_damage: 3,
        scalar_meal: [5; 6],
    }
}

#[test]
fn nutrition_digest_actual_food_and_survives_save_and_worker_changes() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = world();
                state.market = None;
                state.nutrition = Some(game_parallel::Nutrition {
                    rules: nutrition_rules(),
                    people: vec![],
                });
                let food = state
                    .production
                    .as_mut()
                    .unwrap()
                    .goods
                    .iter_mut()
                    .find(|good| good.id == 9)
                    .unwrap()
                    .food
                    .as_mut()
                    .unwrap();
                food.nutrients.insert("protein".into(), 7);
                food.nutrients.insert("vitamins".into(), 3);
                let mut kernel = Kernel::compile(state).unwrap();
                kernel.step().unwrap();
                let saved = kernel.export();
                assert_eq!(
                    saved.food_effects.as_ref().unwrap().pending[0].nutrients,
                    [0, 7, 0, 0, 3, 0]
                );
                let restored = World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap();
                let mut reloaded = Kernel::compile(restored).unwrap();
                kernel.step().unwrap();
                reloaded.step().unwrap();
                let result = kernel.export();
                assert_eq!(result, reloaded.export());
                let reserves = result
                    .nutrition
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .find(|p| p.person == 1)
                    .unwrap()
                    .reserves;
                assert_eq!(reserves, [10, 17, 10, 10, 13, 10]);
                assert!(result.food_effects.is_none());
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn scalar_meals_nourish_the_eater_and_deficiency_affects_health() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = world();
                state.market = None;
                state.goods_cargo.as_mut().unwrap().holdings.clear();
                state.rules.recovery = 0;
                state.regions[0].people[0].hunger = 5;
                state.cargo.as_mut().unwrap().holdings =
                    vec![game_parallel::Holding { person: 1, food: 2 }];
                let mut rules = nutrition_rules();
                rules.interval = 1;
                rules.initial = [0; 6];
                rules.metabolism = [2; 6];
                rules.minimum = [1; 6];
                state.nutrition = Some(game_parallel::Nutrition {
                    rules,
                    people: vec![],
                });
                let mut kernel = Kernel::compile(state).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let reserves = &result.nutrition.as_ref().unwrap().people;
                assert_eq!(
                    reserves.iter().find(|p| p.person == 1).unwrap().reserves,
                    [3; 6]
                );
                assert_eq!(result.regions[0].people[0].health, 100);
                assert_eq!(result.regions[0].people[1].health, 97);
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn nutritional_deficiency_can_kill_and_updates_lineage() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    state.lineage = World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
        .unwrap()
        .lineage;
    state.regions[0].people[0].health = 1;
    state.rules.recovery = 0;
    let mut rules = nutrition_rules();
    rules.interval = 1;
    rules.initial = [0; 6];
    rules.minimum = [1; 6];
    state.nutrition = Some(game_parallel::Nutrition {
        rules,
        people: vec![],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].people[0].health, 0);
    assert!(
        !result
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .alive
    );
    assert_eq!(result.lineage.as_ref().unwrap().current, None);
    result.validate().unwrap();
}

#[test]
fn newborn_nutrition_starts_from_the_authored_baseline() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state =
                    World::from_toml(include_str!("../../../../content/parallel/families.toml"))
                        .unwrap();
                let first = state.families.as_ref().unwrap().next_id;
                let mut rules = nutrition_rules();
                rules.interval = 1000;
                rules.scalar_meal = [0; 6];
                state.nutrition = Some(game_parallel::Nutrition {
                    rules,
                    people: vec![],
                });
                let mut kernel = Kernel::compile(state).unwrap();
                for _ in 0..8 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let child = result
                    .families
                    .as_ref()
                    .unwrap()
                    .members
                    .iter()
                    .find(|member| member.id >= first)
                    .unwrap();
                let reserves = result
                    .nutrition
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .find(|person| person.person == child.id)
                    .unwrap();
                assert_eq!(reserves.reserves, [10; 6]);
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn nutrition_invalid_state_and_stale_drafts_do_not_mutate_world() {
    let mut state = world();
    state.nutrition = Some(game_parallel::Nutrition {
        rules: nutrition_rules(),
        people: vec![
            game_parallel::NutritionPerson {
                person: 1,
                reserves: [10; 6],
            },
            game_parallel::NutritionPerson {
                person: 1,
                reserves: [10; 6],
            },
        ],
    });
    assert!(state.validate().is_err());
    state.nutrition.as_mut().unwrap().people.pop();
    let mut kernel = Kernel::compile(state).unwrap();
    let draft = kernel.prepare().unwrap();
    kernel.eat_good(1, 9).unwrap();
    let before = kernel.export();
    assert!(kernel.commit(draft).is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn comparison_hosts_reject_nutrition_instead_of_ignoring_it() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    state.nutrition = Some(game_parallel::Nutrition {
        rules: nutrition_rules(),
        people: vec![],
    });
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    let before = state.clone();
    assert!(state.step().is_err());
    assert_eq!(state, before);
}

#[test]
fn acquiring_food_does_not_nourish_a_person_until_they_eat_it() {
    let mut state = world();
    state.market = None;
    state.goods_cargo.as_mut().unwrap().holdings.clear();
    state.cargo.as_mut().unwrap().acquire_when_hungry = vec![1];
    let mut rules = nutrition_rules();
    rules.interval = 1000;
    state.nutrition = Some(game_parallel::Nutrition {
        rules,
        people: vec![],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let acquired = kernel.export();
    assert!(
        acquired
            .cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .any(|row| row.person == 1)
    );
    assert_eq!(
        acquired
            .nutrition
            .as_ref()
            .unwrap()
            .people
            .iter()
            .find(|p| p.person == 1)
            .unwrap()
            .reserves,
        [10; 6]
    );
    kernel.step().unwrap();
    let eaten = kernel.export();
    assert_eq!(
        eaten
            .nutrition
            .as_ref()
            .unwrap()
            .people
            .iter()
            .find(|p| p.person == 1)
            .unwrap()
            .reserves,
        [15; 6]
    );
}

#[test]
fn manually_eaten_scalar_food_digests_and_overflow_is_atomic() {
    let mut state = world();
    state.market = None;
    state.cargo.as_mut().unwrap().holdings = vec![game_parallel::Holding { person: 1, food: 2 }];
    let mut rules = nutrition_rules();
    rules.interval = 1000;
    state.nutrition = Some(game_parallel::Nutrition {
        rules,
        people: vec![],
    });
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    kernel.eat_carried_food(1).unwrap();
    let eaten = kernel.export();
    assert_eq!(
        eaten.food_effects.as_ref().unwrap().pending[0].nutrients,
        [5; 6]
    );
    let mut restored =
        Kernel::compile(World::from_json(&serde_json::to_string(&eaten).unwrap()).unwrap())
            .unwrap();
    kernel.step().unwrap();
    restored.step().unwrap();
    let result = kernel.export();
    assert_eq!(result, restored.export());
    assert_eq!(
        result
            .nutrition
            .as_ref()
            .unwrap()
            .people
            .iter()
            .find(|p| p.person == 1)
            .unwrap()
            .reserves,
        [15; 6]
    );
    state.tick = u64::MAX;
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    assert!(kernel.eat_carried_food(1).is_err());
    assert_eq!(kernel.export(), state);
}

#[test]
fn nutrition_columns_survive_split_travel_and_preserve_initial_save_order() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state =
                    World::from_toml(include_str!("../../../../content/parallel/example.toml"))
                        .unwrap();
                let mut rules = nutrition_rules();
                rules.interval = 1;
                rules.metabolism = [2; 6];
                state.nutrition = Some(game_parallel::Nutrition {
                    rules,
                    people: vec![
                        game_parallel::NutritionPerson {
                            person: 2,
                            reserves: [30; 6],
                        },
                        game_parallel::NutritionPerson {
                            person: 1,
                            reserves: [20; 6],
                        },
                    ],
                });
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                assert_eq!(kernel.export(), state);
                kernel
                    .move_people(&[game_parallel::Move {
                        person: 1,
                        destination: 2,
                    }])
                    .unwrap();
                assert_eq!(kernel.export().nutrition, state.nutrition);
                kernel.step().unwrap();
                let saved = kernel.export();
                let people = &saved.nutrition.as_ref().unwrap().people;
                assert_eq!(
                    people.iter().find(|p| p.person == 1).unwrap().reserves,
                    [18; 6]
                );
                assert_eq!(
                    people.iter().find(|p| p.person == 2).unwrap().reserves,
                    [28; 6]
                );
                let mut restored = Kernel::compile(
                    World::from_json(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                restored.step().unwrap();
                assert_eq!(kernel.export(), restored.export());
                kernel.export()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn simultaneous_nutrients_combine_per_person_without_overflow() {
    use game_parallel::{FoodEffect, FoodEffects};
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    state.nutrition = Some(game_parallel::Nutrition {
        rules: nutrition_rules(),
        people: vec![],
    });
    state.food_effects = Some(FoodEffects {
        pending: vec![
            FoodEffect {
                person: 2,
                at: 1,
                health: 0,
                nutrients: [3, 0, 0, 0, 0, 0],
            },
            FoodEffect {
                person: 1,
                at: 1,
                health: 0,
                nutrients: [u32::MAX, 2, 0, 0, 0, 0],
            },
            FoodEffect {
                person: 1,
                at: 1,
                health: 0,
                nutrients: [2, 3, 0, 0, 0, 0],
            },
        ],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    let people = &result.nutrition.as_ref().unwrap().people;
    assert_eq!(
        people.iter().find(|p| p.person == 1).unwrap().reserves,
        [100, 15, 10, 10, 10, 10]
    );
    assert_eq!(
        people.iter().find(|p| p.person == 2).unwrap().reserves,
        [13, 10, 10, 10, 10, 10]
    );
}

#[test]
fn manual_edible_good_consumes_a_real_portion_and_failure_is_atomic() {
    let mut kernel = Kernel::compile(world()).unwrap();
    let before = kernel.export();
    assert!(kernel.eat_good(1, 1).is_err());
    assert!(kernel.eat_good(2, 9).is_err());
    assert_eq!(kernel.export(), before);
    let draft = kernel.prepare().unwrap();
    kernel.eat_good(1, 9).unwrap();
    assert!(kernel.commit(draft).is_err());
    let after = kernel.export();
    assert_eq!(after.regions[0].people[0].hunger, 0);
    assert_eq!(
        after.goods_cargo.as_ref().unwrap().holdings[0].items[0].quantity,
        2
    );
    let restored = World::from_json(&serde_json::to_string(&after).unwrap()).unwrap();
    assert_eq!(Kernel::compile(restored).unwrap().export(), after);
}

#[test]
fn automatic_eating_uses_personal_goods_and_keeps_scalar_food_separate() {
    let run = |workers, held_scalar| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = world();
                state.market = None;
                if held_scalar {
                    state.cargo.as_mut().unwrap().holdings =
                        vec![game_parallel::Holding { person: 1, food: 2 }];
                }
                let mut kernel = Kernel::compile(state).unwrap();
                kernel.step().unwrap();
                let after = kernel.export();
                let remaining = after.goods_cargo.as_ref().unwrap().holdings[0].items[0].quantity;
                assert_eq!(remaining, if held_scalar { 4 } else { 2 });
                assert_eq!(
                    after.regions[0].people[0].hunger,
                    if held_scalar { 2 } else { 0 }
                );
                assert!(after.cargo.as_ref().unwrap().holdings.is_empty());
                let restored = World::from_json(&serde_json::to_string(&after).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), after);
                after
            })
    };
    for held_scalar in [false, true] {
        assert_eq!(run(1, held_scalar), run(4, held_scalar));
    }
}

#[test]
fn eating_and_disjoint_trade_preserve_both_physical_changes() {
    use game_parallel::{Person, TradePolicy, TradeProposal, TradeValue};
    let mut state = world();
    let third = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(third);
    state.regions[0].people.push(Person {
        id: 4,
        hunger: 0,
        health: 100,
    });
    state
        .goods_cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(GoodsHolding {
            person: 3,
            items: vec![Amount {
                good: 1,
                quantity: 2,
            }]
            .into(),
        });
    state.market.as_mut().unwrap().policies.push(TradePolicy {
        person: 4,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        influences: vec![],
        values: vec![TradeValue { good: 1, value: 1 }],
    });
    state
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
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    let goods = &result.goods_cargo.as_ref().unwrap().holdings;
    assert_eq!(
        goods
            .iter()
            .map(|row| (row.person, row.items[0].quantity))
            .collect::<Vec<_>>(),
        vec![(1, 2), (3, 1), (4, 1)]
    );
    assert_eq!(result.regions[0].people[0].hunger, 0);
    assert!(result.market.unwrap().offers.is_empty());
}

#[test]
fn consumed_food_health_effects_are_delayed_saved_and_applied_once() {
    let run = |workers, automatic| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = world();
                state
                    .production
                    .as_mut()
                    .unwrap()
                    .goods
                    .iter_mut()
                    .find(|good| good.id == 9)
                    .unwrap()
                    .food
                    .as_mut()
                    .unwrap()
                    .health_effect = -9;
                let mut kernel = Kernel::compile(state).unwrap();
                if automatic {
                    kernel.step().unwrap();
                } else {
                    kernel.eat_good(1, 9).unwrap();
                }
                let first = kernel.export();
                assert_eq!(first.regions[0].people[0].health, 100);
                let event = &first.food_effects.as_ref().unwrap().pending[0];
                assert_eq!(
                    (event.person, event.at, event.health),
                    (1, first.tick + 1, -9)
                );
                let mut kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(result.regions[0].people[0].health, 91);
                assert!(result.food_effects.is_none());
                result
            })
    };
    for automatic in [false, true] {
        assert_eq!(run(1, automatic), run(4, automatic));
    }
}

#[test]
fn lethal_food_effects_flow_through_death_and_player_lineage() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let food_world = world();
    state.production = food_world.production;
    state.goods_cargo = food_world.goods_cargo;
    state.cargo = food_world.cargo;
    state.lineage = World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
        .unwrap()
        .lineage;
    state
        .production
        .as_mut()
        .unwrap()
        .goods
        .iter_mut()
        .find(|good| good.id == 9)
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .health_effect = -10;
    state.regions[0].people[0].health = 1;
    state.regions[0].people[0].hunger = 5;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.eat_good(1, 9).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.regions[0]
            .people
            .iter()
            .find(|person| person.id == 1)
            .unwrap()
            .health,
        0
    );
    assert!(
        !result
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|member| member.id == 1)
            .unwrap()
            .alive
    );
    assert_eq!(result.families.as_ref().unwrap().totals.deaths, 1);
    assert_eq!(result.lineage.as_ref().unwrap().current, None);
    assert!(result.food_effects.is_none());
    result.validate().unwrap();
}

#[test]
fn effect_tick_overflow_rolls_back_manual_and_automatic_consumption() {
    let mut state = world();
    state
        .production
        .as_mut()
        .unwrap()
        .goods
        .iter_mut()
        .find(|good| good.id == 9)
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .health_effect = -1;
    state.tick = u64::MAX;
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    assert!(kernel.eat_good(1, 9).is_err());
    assert_eq!(kernel.export(), state);
    state.tick = u64::MAX - 1;
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), state);
}

#[test]
fn many_people_eat_and_digest_in_parallel_with_deterministic_results() {
    use game_parallel::Person;
    let count = 5_000u64;
    let mut state = world();
    state.market = None;
    state.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 5,
            health: 100,
        })
        .collect();
    state.regions[1].people.clear();
    state
        .production
        .as_mut()
        .unwrap()
        .goods
        .iter_mut()
        .find(|good| good.id == 9)
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .health_effect = -2;
    state.goods_cargo.as_mut().unwrap().holdings = (1..=count)
        .map(|person| GoodsHolding {
            person,
            items: vec![Amount {
                good: 9,
                quantity: 2,
            }]
            .into(),
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
                let first = kernel.export();
                assert_eq!(
                    first.food_effects.as_ref().unwrap().pending.len(),
                    count as usize
                );
                assert!(first.goods_cargo.as_ref().unwrap().holdings.is_empty());
                let mut kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert!(
                    result.regions[0]
                        .people
                        .iter()
                        .all(|person| person.hunger == 1 && person.health == 98)
                );
                assert!(result.food_effects.is_none());
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn sparse_interleaved_regions_preserve_eating_and_digestion() {
    use game_parallel::Person;
    let mut state = world();
    state.market = None;
    state.autowork = None;
    state.regions[0].people = [1, 3, 5]
        .into_iter()
        .map(|id| Person {
            id,
            hunger: 5,
            health: 100,
        })
        .collect();
    state.regions[1].people = [2, 4, 6]
        .into_iter()
        .map(|id| Person {
            id,
            hunger: 5,
            health: 100,
        })
        .collect();
    state
        .production
        .as_mut()
        .unwrap()
        .goods
        .iter_mut()
        .find(|good| good.id == 9)
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .health_effect = -2;
    state.goods_cargo.as_mut().unwrap().holdings = (1..=6)
        .map(|person| GoodsHolding {
            person,
            items: vec![Amount {
                good: 9,
                quantity: 2,
            }]
            .into(),
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
                assert!(
                    result
                        .regions
                        .iter()
                        .flat_map(|region| &region.people)
                        .all(|person| person.hunger == 1 && person.health == 98)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn multiple_simultaneous_effects_for_one_person_are_combined() {
    use game_parallel::{FoodEffect, FoodEffects};
    let mut state = world();
    state.goods_cargo.as_mut().unwrap().holdings.clear();
    state.regions[0].people[0].health = 90;
    state.food_effects = Some(FoodEffects {
        pending: vec![
            FoodEffect {
                person: 3,
                at: 1,
                health: -1,
                nutrients: [0; 6],
            },
            FoodEffect {
                person: 1,
                at: 1,
                health: -3,
                nutrients: [0; 6],
            },
            FoodEffect {
                person: 1,
                at: 1,
                health: 2,
                nutrients: [0; 6],
            },
        ],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].people[0].health, 90);
    assert_eq!(result.regions[1].people[0].health, 99);
    assert!(result.food_effects.is_none());
}
