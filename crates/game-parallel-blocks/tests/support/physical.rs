use crate::Engine;
use game_parallel::{Person, World};
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap()
}
#[test]
fn abundant_food_matches_the_existing_physical_engine_and_reload() {
    let mut reference = world();
    let mut blocks = Engine::compile(reference.clone()).unwrap();
    for tick in 1..=20 {
        reference.step().unwrap();
        blocks.step().unwrap();
        assert_eq!(blocks.export(), reference);
        if tick == 7 {
            blocks = Engine::compile(
                World::from_json(&serde_json::to_string(&blocks.export()).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
}
#[test]
fn scarce_food_is_conserved_and_deterministic_across_workers_and_layouts() {
    let mut initial = world();
    initial.rules.food_growth = 0;
    initial.rules.food_capacity = 7_000;
    initial.rules.hunger_per_tick = 1;
    initial.regions[0].food = 7_000;
    initial.regions[0].people = (0..10_000)
        .map(|id| Person {
            id: id + 100,
            health: 100,
            hunger: 3,
        })
        .collect();
    let mut results = Vec::new();
    for workers in [1, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut start = initial.clone();
        if workers > 1 {
            start.regions[0].people.reverse();
        }
        let mut engine = Engine::compile(start).unwrap();
        pool.install(|| engine.step().unwrap());
        let out = engine.export();
        assert_eq!(out.regions[0].food, 0);
        assert_eq!(
            out.regions[0]
                .people
                .iter()
                .filter(|p| p.hunger == 0)
                .count(),
            3500
        );
        assert!(
            out.regions[0]
                .people
                .iter()
                .take(3500)
                .all(|p| p.hunger == 0)
        );
        results.push(out);
    }
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
}
#[test]
fn scarce_food_after_arrival_matches_save_and_recompile() {
    use game_parallel::Move;
    let mut initial = world();
    initial.rules.food_growth = 2;
    initial.regions[0].food = 0;
    initial.regions[1].food = 0;
    initial.regions[0].people = vec![Person {
        id: 5,
        health: 100,
        hunger: 2,
    }];
    initial.regions[1].people = [1, 10]
        .into_iter()
        .map(|id| Person {
            id,
            health: 100,
            hunger: 2,
        })
        .collect();
    initial.routes[0].ticks = 1;
    let mut uninterrupted = Engine::compile(initial).unwrap();
    uninterrupted
        .move_people(&[Move {
            person: 5,
            destination: 2,
        }])
        .unwrap();
    uninterrupted.step().unwrap();
    let mut reloaded = Engine::compile(
        World::from_json(&serde_json::to_string(&uninterrupted.export()).unwrap()).unwrap(),
    )
    .unwrap();
    uninterrupted.step().unwrap();
    reloaded.step().unwrap();
    let mut actual = uninterrupted.export();
    let mut expected = reloaded.export();
    for world in [&mut actual, &mut expected] {
        world.regions[1]
            .people
            .sort_unstable_by_key(|person| person.id);
    }
    assert_eq!(actual, expected);
    assert_eq!(
        actual.regions[1]
            .people
            .iter()
            .map(|p| (p.id, p.hunger))
            .collect::<Vec<_>>(),
        vec![(1, 0), (5, 0), (10, 4)]
    );
}
#[test]
fn unsupported_owners_and_overflow_cannot_silently_mutate_state() {
    let mut w = world();
    w.tick = u64::MAX;
    let mut engine = Engine::compile(w.clone()).unwrap();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), w);
    let mut invalid = world();
    invalid.version = 2;
    assert!(Engine::compile(invalid).is_err());
}

#[test]
fn travel_batches_are_atomic_and_arrivals_join_resident_blocks() {
    use game_parallel::Move;
    let mut engine = Engine::compile(world()).unwrap();
    let before = engine.export();
    assert!(
        engine
            .move_people(&[
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
    assert_eq!(engine.export(), before);
    engine
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    engine.step().unwrap();
    assert_eq!(engine.export().travelers.len(), 1);
    let save = serde_json::to_string(&engine.export()).unwrap();
    engine = Engine::compile(World::from_json(&save).unwrap()).unwrap();
    engine.step().unwrap();
    let arrived = engine.export();
    assert!(arrived.travelers.is_empty());
    assert!(
        arrived.regions[1]
            .people
            .iter()
            .any(|person| person.id == 1)
    );
    engine.step().unwrap();
    engine.export().validate().unwrap();
}
#[test]
fn large_concentrated_arrivals_are_deterministic_across_worker_counts() {
    use game_parallel::Move;
    let mut initial = world();
    initial.regions[0].people = (0..8_000)
        .map(|id| Person {
            id: id + 100,
            health: 100,
            hunger: 0,
        })
        .collect();
    let moves: Vec<_> = (0..8_000)
        .map(|id| Move {
            person: id + 100,
            destination: 2,
        })
        .collect();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut e = Engine::compile(initial.clone()).unwrap();
        pool.install(|| {
            e.move_people(&moves).unwrap();
            e.step().unwrap();
            e.step().unwrap();
        });
        let mut out = e.export();
        for region in &mut out.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        assert!(out.travelers.is_empty());
        assert_eq!(out.regions[1].people.len(), 8_001);
        out.validate().unwrap();
        results.push(out);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn partial_departures_split_blocks_without_losing_residents() {
    use game_parallel::Move;
    let mut initial = world();
    initial.regions[0].people = (0..5000)
        .map(|id| Person {
            id: id + 100,
            health: 100,
            hunger: 0,
        })
        .collect();
    let moves: Vec<_> = (0..5000)
        .filter(|id| id % 2 == 0)
        .rev()
        .map(|id| Move {
            person: id + 100,
            destination: 2,
        })
        .collect();
    let mut engine = Engine::compile(initial).unwrap();
    engine.move_people(&moves).unwrap();
    let state = engine.export();
    assert_eq!(state.regions[0].people.len(), 2500);
    assert_eq!(state.travelers.len(), 2500);
    engine.step().unwrap();
    engine.step().unwrap();
    let state = engine.export();
    assert!(state.travelers.is_empty());
    assert_eq!(state.regions[1].people.len(), 2501);
    state.validate().unwrap();
}

#[test]
fn production_uses_the_persistent_index_and_reserves_inputs_atomically() {
    use game_parallel::{Move, StartWork};
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    w.goods_cargo = None;
    w.market = None;
    let mut reference = w.clone();
    let mut engine = Engine::compile(w).unwrap();
    let requests = [
        StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        },
        StartWork {
            id: 2,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        },
    ];
    engine.start_work(&requests).unwrap();
    reference.start_work(&requests).unwrap();
    let before = engine.export();
    assert!(
        engine
            .move_people(&[Move {
                person: 1,
                destination: 2
            }])
            .is_err()
    );
    assert_eq!(engine.export(), before);
    engine.step().unwrap();
    reference.step().unwrap();
    assert_eq!(engine.export(), reference);
    engine = Engine::compile(
        World::from_json(&serde_json::to_string(&engine.export()).unwrap()).unwrap(),
    )
    .unwrap();
    engine.step().unwrap();
    reference.step().unwrap();
    assert_eq!(engine.export(), reference);
    assert_eq!(
        engine.export().production.as_ref().unwrap().inventories[0].stocks[1].quantity,
        4
    );
}
#[test]
fn persistent_production_index_tracks_departures_and_arrivals() {
    use game_parallel::{Move, StartWork};
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    w.goods_cargo = None;
    w.market = None;
    let mut engine = Engine::compile(w).unwrap();
    engine
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    assert!(
        engine
            .start_work(&[StartWork {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    engine.step().unwrap();
    engine.step().unwrap();
    let state = engine.export();
    assert!(state.regions[1].people.iter().any(|p| p.id == 1));
    state.validate().unwrap();
    engine
        .move_people(&[Move {
            person: 1,
            destination: 1,
        }])
        .unwrap();
    engine.step().unwrap();
    engine.step().unwrap();
    engine
        .start_work(&[StartWork {
            id: 2,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    engine.step().unwrap();
    engine.step().unwrap();
    assert_eq!(
        engine.export().production.as_ref().unwrap().inventories[0].stocks[1].quantity,
        2
    );
}

#[test]
fn concentrated_work_is_deterministic_across_blocks_and_worker_counts() {
    use game_parallel::StartWork;
    let mut initial =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    initial.goods_cargo = None;
    initial.market = None;
    let count = 5_000u64;
    initial.regions[0].people = (100..100 + count)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    let inventory = &mut initial.production.as_mut().unwrap().inventories[0];
    inventory.stocks[0].quantity = count * 3;
    inventory.stocks[0].capacity = count * 3;
    inventory.stocks[1].capacity = count * 2;
    let jobs: Vec<_> = (0..count)
        .map(|id| StartWork {
            id,
            worker: id + 100,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    let mut results = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut w = initial.clone();
        if workers == 4 {
            w.regions[0].people.reverse();
        }
        let mut engine = Engine::compile(w).unwrap();
        pool.install(|| {
            engine.start_work(&jobs).unwrap();
            engine.step().unwrap();
            engine.step().unwrap();
        });
        let out = engine.export();
        assert_eq!(
            out.production.as_ref().unwrap().inventories[0].stocks[1].quantity,
            count * 2
        );
        assert!(out.production.as_ref().unwrap().jobs.is_empty());
        out.validate().unwrap();
        results.push(out);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn compiled_production_goods_and_travel_form_a_complete_physical_chain() {
    use game_parallel::{GoodsDirection, GoodsTransfer, Move, StartWork};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    let mut engine = Engine::compile(world).unwrap();
    engine
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    engine.step().unwrap();
    engine.step().unwrap();
    engine
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 2,
            quantity: 2,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    engine
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    engine.step().unwrap();
    engine = Engine::compile(
        World::from_json(&serde_json::to_string(&engine.export()).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(
        engine
            .transfer_goods(&[GoodsTransfer {
                person: 1,
                good: 2,
                quantity: 2,
                direction: GoodsDirection::Deposit
            }])
            .is_err()
    );
    engine.step().unwrap();
    engine
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 2,
            quantity: 2,
            direction: GoodsDirection::Deposit,
        }])
        .unwrap();
    let world = engine.export();
    assert_eq!(
        world.production.as_ref().unwrap().inventories[1].stocks[1].quantity,
        2
    );
    assert!(world.goods_cargo.as_ref().unwrap().holdings.is_empty());
    world.validate().unwrap();
}

#[test]
fn compiled_market_requires_consent_and_stages_automatic_exchange() {
    use game_parallel::{Amount, GoodsDirection, GoodsTransfer, TradeProposal};
    let world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    let mut engine = Engine::compile(world).unwrap();
    engine
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 1,
            quantity: 2,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    let id = engine
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 2,
            give: vec![Amount {
                good: 1,
                quantity: 2,
            }],
            receive: vec![],
        })
        .unwrap();
    assert_eq!(engine.export().market.as_ref().unwrap().offers[0].id, id);
    engine = Engine::compile(
        World::from_json(&serde_json::to_string(&engine.export()).unwrap()).unwrap(),
    )
    .unwrap();
    engine.step().unwrap();
    let result = engine.export();
    assert!(result.market.as_ref().unwrap().offers.is_empty());
    assert_eq!(result.goods_cargo.as_ref().unwrap().holdings[0].person, 2);
    result.validate().unwrap();
}
#[test]
fn compiled_auto_trade_overflow_rolls_back_every_owner() {
    use game_parallel::{Amount, TradePolicy, TradeProposal, TradeValue};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market.as_mut().unwrap().policies.push(TradePolicy {
        person: 2,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        influences: Vec::new(),
        values: vec![
            TradeValue {
                good: 1,
                value: u64::MAX,
            },
            TradeValue {
                good: 2,
                value: u64::MAX,
            },
        ],
    });
    let mut engine = Engine::compile(world).unwrap();
    engine
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 2,
            give: vec![
                Amount {
                    good: 1,
                    quantity: u64::MAX,
                },
                Amount {
                    good: 2,
                    quantity: u64::MAX,
                },
            ],
            receive: vec![],
        })
        .unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn compiled_autowork_reserves_and_progresses_like_the_owner_runtime() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap();
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..4 {
        engine.step().unwrap();
        reference.step().unwrap();
        assert_eq!(engine.export(), reference);
    }
}
#[test]
fn compiled_autowork_overflow_preserves_published_generation() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap();
    world.autowork.as_mut().unwrap().next_id = u64::MAX;
    let mut engine = Engine::compile(world.clone()).unwrap();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), world);
}

#[test]
fn compiled_farming_uses_physical_workers_seed_and_storage() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/farming.toml")).unwrap();
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..4 {
        engine.step().unwrap();
        reference.step().unwrap();
        assert_eq!(engine.export(), reference);
    }
}
#[test]
fn compiled_farming_excludes_workers_committed_to_autonomous_work() {
    use game_parallel::CropPhase;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap();
    world.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    let mut engine = Engine::compile(world.clone()).unwrap();
    engine.step().unwrap();
    world.step().unwrap();
    assert_eq!(engine.export(), world);
    assert!(
        engine.export().agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Fallow)
    );
}

#[test]
fn compiled_social_encounters_follow_the_shared_labor_snapshot() {
    let world = World::from_toml(include_str!("../../../../content/parallel/social.toml")).unwrap();
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..3 {
        engine.step().unwrap();
        reference.step().unwrap();
        assert_eq!(engine.export(), reference);
    }
}
#[test]
fn compiled_work_and_trade_cannot_also_socialize() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap();
    world.social = World::from_toml(include_str!("../../../../content/parallel/social.toml"))
        .unwrap()
        .social;
    let mut engine = Engine::compile(world).unwrap();
    engine.step().unwrap();
    assert!(engine.export().social.as_ref().unwrap().events.is_empty());
}

#[test]
fn compiled_knowledge_uses_local_evidence_and_encounter_reports() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/knowledge.toml")).unwrap();
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..3 {
        engine.step().unwrap();
        reference.step().unwrap();
        assert_eq!(engine.export(), reference);
    }
}

#[test]
fn compiled_people_choose_travel_from_personal_observations() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap();
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..5 {
        engine.step().unwrap();
        reference.step().unwrap();
        let mut actual = engine.export();
        let mut expected = reference.clone();
        for region in &mut actual.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        for region in &mut expected.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        assert_eq!(actual, expected);
    }
}

#[test]
fn compiled_families_preserve_birth_death_and_save_consistency() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.rules.food_capacity = 100_000;
    world.rules.food_growth = 10_000;
    for region in &mut world.regions {
        region.food = 10_000;
    }
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for tick in 1..=24 {
        engine.step().unwrap();
        reference.step().unwrap();
        let mut actual = engine.export();
        let mut expected = reference.clone();
        for region in &mut actual.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        for region in &mut expected.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        assert_eq!(actual, expected);
        actual.validate().unwrap();
        if tick == 7 {
            engine = Engine::compile(
                World::from_json(&serde_json::to_string(&actual).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
    assert!(engine.export().families.unwrap().totals.births > 0);
}
#[test]
fn family_failure_rolls_back_compiled_farming_and_production() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let workshop =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap();
    world.production = workshop.production;
    world.goods_cargo = workshop.goods_cargo;
    world.market = workshop.market;
    world.autowork = workshop.autowork;
    world.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 3)
        .unwrap()
        .age = u64::MAX;
    let mut engine = Engine::compile(world.clone()).unwrap();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), world);
}

#[test]
fn compiled_birth_during_travel_keeps_the_mothers_remaining_journey() {
    use game_parallel::Move;
    let world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut engine = Engine::compile(world).unwrap();
    engine.step().unwrap();
    let mother = engine
        .export()
        .families
        .unwrap()
        .members
        .iter()
        .find(|member| member.id <= 4 && member.pregnancy.is_some())
        .unwrap()
        .id;
    engine
        .move_people(&[Move {
            person: mother,
            destination: 2,
        }])
        .unwrap();
    while engine.tick() < 4 {
        engine.step().unwrap();
    }
    let state = engine.export();
    let child = state
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .find(|m| m.parents.first() == Some(&mother))
        .unwrap()
        .id;
    let m = state
        .travelers
        .iter()
        .find(|t| t.person.id == mother)
        .unwrap();
    let c = state
        .travelers
        .iter()
        .find(|t| t.person.id == child)
        .unwrap();
    assert_eq!(
        (m.source, m.destination, m.ticks_remaining),
        (c.source, c.destination, c.ticks_remaining)
    );
    engine = Engine::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
        .unwrap();
    while engine.tick() < 6 {
        engine.step().unwrap();
    }
    let state = engine.export();
    assert!(state.regions[1].people.iter().any(|p| p.id == child));
    state.validate().unwrap();
}

#[test]
fn compiled_shipments_reserve_travel_and_deliver_food() {
    use game_parallel::Shipment;
    let mut reference = world();
    let mut engine = Engine::compile(reference.clone()).unwrap();
    let request = [Shipment {
        id: 5,
        source: 1,
        destination: 2,
        food: 4,
        ticks_remaining: 2,
    }];
    engine.launch_shipments(&request).unwrap();
    reference.launch_shipments(&request).unwrap();
    for _ in 0..2 {
        engine.step().unwrap();
        reference.step().unwrap();
        assert_eq!(engine.export(), reference);
    }
    assert!(engine.export().shipments.is_empty());
}
#[test]
fn failed_family_phase_does_not_advance_a_compiled_shipment() {
    use game_parallel::Shipment;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world
        .launch_shipments(&[Shipment {
            id: 3,
            source: 2,
            destination: 1,
            food: 3,
            ticks_remaining: 5,
        }])
        .unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 2)
        .unwrap()
        .age = u64::MAX;
    let mut engine = Engine::compile(world.clone()).unwrap();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), world);
}

#[test]
fn compiled_food_cargo_pack_and_automatic_meals_follow_the_journey() {
    use game_parallel::Move;
    let world =
        World::from_toml(include_str!("../../../../content/parallel/cargo_auto.toml")).unwrap();
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    engine
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    reference
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    for _ in 0..2 {
        engine.step().unwrap();
        reference.step().unwrap();
        let mut a = engine.export();
        let mut b = reference.clone();
        for region in &mut a.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        for region in &mut b.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        assert_eq!(a, b);
    }
}
#[test]
fn compiled_food_pickup_eating_and_deposit_conserve_custody() {
    use game_parallel::Move;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/cargo.toml")).unwrap();
    world.regions[0].people[0].hunger = 3;
    let mut engine = Engine::compile(world.clone()).unwrap();
    engine.pickup_food(1, 4).unwrap();
    world.pickup_food(1, 4).unwrap();
    engine.eat_carried_food(1).unwrap();
    world.eat_carried_food(1).unwrap();
    engine
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    world
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    engine.step().unwrap();
    world.step().unwrap();
    engine.step().unwrap();
    world.step().unwrap();
    engine.deposit_food(1, 2).unwrap();
    world.deposit_food(1, 2).unwrap();
    let mut a = engine.export();
    for region in &mut a.regions {
        region.people.sort_unstable_by_key(|p| p.id);
    }
    for region in &mut world.regions {
        region.people.sort_unstable_by_key(|p| p.id);
    }
    assert_eq!(a, world);
}
#[test]
fn compiled_pack_and_family_failure_are_atomic() {
    use game_parallel::{Holding, Move};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/cargo_auto.toml"))
        .unwrap()
        .cargo;
    world
        .cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(Holding { person: 1, food: 4 });
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 2)
        .unwrap()
        .age = u64::MAX;
    let mut engine = Engine::compile(world).unwrap();
    engine
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn compiled_all_existing_owners_compose_across_generations_and_reload() {
    use game_parallel::Shipment;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let economy =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap();
    world.production = economy.production;
    world.goods_cargo = economy.goods_cargo;
    world.market = economy.market;
    world.autowork = economy.autowork;
    world.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    let mind =
        World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap();
    world.social = mind.social;
    world.knowledge = mind.knowledge;
    world.autonomy = mind.autonomy;
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/cargo_auto.toml"))
        .unwrap()
        .cargo;
    world.rules.food_capacity = 100_000;
    world.rules.food_growth = 10_000;
    for region in &mut world.regions {
        region.food = 10_000;
    }
    let ticks = world
        .routes
        .iter()
        .find(|route| route.source == 1 && route.destination == 2)
        .unwrap()
        .ticks;
    world
        .launch_shipments(&[Shipment {
            id: 1,
            source: 1,
            destination: 2,
            food: 5,
            ticks_remaining: ticks,
        }])
        .unwrap();
    world.validate().unwrap();
    let mut results = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut engine = Engine::compile(world.clone()).unwrap();
        pool.install(|| {
            for tick in 1..=20 {
                engine.step().unwrap();
                if tick == 9 {
                    engine = Engine::compile(
                        World::from_json(&serde_json::to_string(&engine.export()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
            }
        });
        let mut out = engine.export();
        for region in &mut out.regions {
            region.people.sort_unstable_by_key(|p| p.id);
        }
        out.validate().unwrap();
        assert!(out.families.as_ref().unwrap().totals.births > 0);
        results.push(out);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn compiled_lineage_waits_for_real_birth_and_survives_reload() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/lineage.toml")).unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap()
        .age = 69;
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for tick in 1..=7 {
        engine.step().unwrap();
        reference.step().unwrap();
        let actual = engine.export();
        assert_eq!(actual.lineage, reference.lineage);
        actual.validate().unwrap();
        if tick == 1 {
            assert!(actual.lineage.as_ref().unwrap().current.is_none());
            engine = Engine::compile(
                World::from_json(&serde_json::to_string(&actual).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
    let state = engine.export();
    let heir = state.lineage.as_ref().unwrap().current.unwrap();
    assert!(
        state
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|m| m.id == heir)
            .unwrap()
            .born
            .is_some_and(|tick| tick > 1)
    );
}

#[test]
fn guardians_travel_recover_in_confinement_and_survive_reload() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    let mut reference = world.clone();
    let mut engine = Engine::compile(world).unwrap();
    for tick in 1..=10 {
        reference.step().unwrap();
        engine.step().unwrap();
        let actual = engine.export();
        assert_eq!(actual.guardians, reference.guardians);
        actual.validate().unwrap();
        if tick == 3 {
            assert!(
                actual.guardians.as_ref().unwrap().guardians[0]
                    .journey
                    .is_some()
            );
            engine = Engine::compile(
                World::from_json(&serde_json::to_string(&actual).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
    let state = engine.export();
    let guardians = state.guardians.unwrap();
    assert_eq!(guardians.guardians[0].region, 1);
    assert!(guardians.ready(&guardians.guardians[0]));
    assert_eq!(guardians.guardians[1].region, 2);
    assert_eq!(guardians.guardians[1].health, 100);
    assert!(guardians.guardians[1].confined);
    assert!(guardians.guardians[1].journey.is_none());
}

#[test]
fn failed_family_generation_rolls_back_guardian_travel() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap()
        .age = u64::MAX;
    let mut engine = Engine::compile(world).unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn guardian_release_requires_contact_and_mentorship_and_is_saved() {
    use game_parallel::ReleaseDecision;
    let world =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    let mut engine = Engine::compile(world).unwrap();
    assert_eq!(
        engine.request_guardian_release(1).unwrap(),
        ReleaseDecision::NotPresent
    );
    for _ in 0..6 {
        engine.step().unwrap();
    }
    assert_eq!(
        engine.request_guardian_release(1).unwrap(),
        ReleaseDecision::NotReady
    );
    for _ in 0..3 {
        engine.step().unwrap();
    }
    assert_eq!(
        engine.request_guardian_release(1).unwrap(),
        ReleaseDecision::Accepted
    );
    let world = World::from_json(&serde_json::to_string(&engine.export()).unwrap()).unwrap();
    assert!(world.guardians.unwrap().guardians[0].released);
}

#[test]
fn invocation_warns_before_advancing_and_borrows_without_copying_knowledge() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/invocation.toml")).unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap()
        .age = 69;
    world.knowledge = World::from_toml(include_str!("../../../../content/parallel/knowledge.toml"))
        .unwrap()
        .knowledge;
    world.validate().unwrap();
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..7 {
        engine.step().unwrap();
    }
    assert_ne!(engine.export().lineage.as_ref().unwrap().current, Some(1));
    engine.invoke_lives(&[1]).unwrap();
    let memories = engine.borrowed_memories().unwrap();
    assert!(!memories.is_empty());
    assert!(memories.iter().all(|(life, _)| *life == 1));
    engine.step().unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    let pending = engine.export();
    assert_eq!(pending.tick, before.tick);
    assert_eq!(pending.regions, before.regions);
    assert_eq!(pending.knowledge, before.knowledge);
    let warning = pending
        .lineage
        .as_ref()
        .unwrap()
        .invocation
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap();
    assert_eq!(warning.kind, game_parallel::InvocationWarningKind::Strain);
    let revision = warning.revision;
    engine = Engine::compile(World::from_json(&serde_json::to_string(&pending).unwrap()).unwrap())
        .unwrap();
    assert!(engine.accept_invocation_warning(revision + 1).is_err());
    engine.accept_invocation_warning(revision).unwrap();
    engine.step().unwrap();
    let knowledge = engine.export().knowledge;
    engine.dismiss_invocation().unwrap();
    assert!(engine.borrowed_memories().unwrap().is_empty());
    assert_eq!(engine.export().knowledge, knowledge);
}

#[test]
fn late_family_failure_does_not_commit_invocation_burden() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/invocation.toml")).unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap()
        .age = 69;
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..7 {
        engine.step().unwrap();
    }
    let mut world = engine.export();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.alive)
        .unwrap()
        .age = u64::MAX;
    let mut engine = Engine::compile(world).unwrap();
    engine.invoke_lives(&[1]).unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn accepted_control_loss_blocks_dismissal_and_survives_reload() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/invocation.toml")).unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap()
        .age = 69;
    let mut engine = Engine::compile(world).unwrap();
    for _ in 0..7 {
        engine.step().unwrap();
    }
    engine.invoke_lives(&[1]).unwrap();
    for _ in 0..10 {
        if engine.step().is_err() {
            let state = engine.export();
            let warning = state
                .lineage
                .as_ref()
                .unwrap()
                .invocation
                .as_ref()
                .unwrap()
                .pending
                .as_ref()
                .unwrap();
            engine.accept_invocation_warning(warning.revision).unwrap();
        }
        if engine
            .export()
            .lineage
            .as_ref()
            .unwrap()
            .invocation
            .as_ref()
            .unwrap()
            .control_until
            .is_some()
        {
            break;
        }
    }
    let mut state = engine.export();
    let until = state
        .lineage
        .as_ref()
        .unwrap()
        .invocation
        .as_ref()
        .unwrap()
        .control_until
        .unwrap();
    let person = state.lineage.as_ref().unwrap().current.unwrap();
    let source = state
        .regions
        .iter()
        .position(|region| region.people.iter().any(|p| p.id == person))
        .unwrap();
    let destination = 1 - source;
    state.regions[source].food = 0;
    state.regions[destination].food = 100;
    state.regions[source]
        .people
        .iter_mut()
        .find(|p| p.id == person)
        .unwrap()
        .hunger = 7;
    state.knowledge = Some(game_parallel::Knowledge {
        max_records: 4,
        minds: vec![game_parallel::Mind {
            person,
            records: vec![
                game_parallel::KnowledgeRecord {
                    region: state.regions[source].id,
                    food: 0,
                    observed_at: state.tick,
                    witness: person,
                    learned_from: None,
                },
                game_parallel::KnowledgeRecord {
                    region: state.regions[destination].id,
                    food: 100,
                    observed_at: state.tick,
                    witness: person,
                    learned_from: None,
                },
            ],
        }],
    });
    let mut autonomy = World::from_toml(include_str!("../../../../content/parallel/autonomy.toml"))
        .unwrap()
        .autonomy
        .unwrap();
    autonomy.rules.review_interval = 1;
    autonomy.rules.min_hunger = 0;
    autonomy.actors = vec![game_parallel::AutonomousActor {
        person,
        enabled: true,
        next_review: 0,
        travel_caution: 0,
        hunger_urgency: 0,
    }];
    state.autonomy = Some(autonomy);
    let mut editable = World::from_json(&serde_json::to_string(&state).unwrap()).unwrap();
    let editable_before = editable.clone();
    assert!(
        editable
            .move_people(&[game_parallel::Move {
                person,
                destination: state.regions[destination].id
            }])
            .is_err()
    );
    assert!(
        editable
            .start_work(&[game_parallel::StartWork {
                id: 99,
                worker: person,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    assert!(
        editable
            .transfer_goods(&[game_parallel::GoodsTransfer {
                person,
                good: 1,
                quantity: 1,
                direction: game_parallel::GoodsDirection::Pickup
            }])
            .is_err()
    );
    assert!(editable.pickup_food(person, 1).is_err());
    assert_eq!(editable, editable_before);
    editable.step().unwrap();
    assert!(
        editable
            .travelers
            .iter()
            .any(|traveler| traveler.person.id == person)
    );
    engine = Engine::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
        .unwrap();
    assert!(engine.dismiss_invocation().is_err());
    assert!(engine.request_guardian_release(1).is_err());
    let before = engine.export();
    assert!(
        engine
            .move_people(&[game_parallel::Move {
                person,
                destination: 2
            }])
            .is_err()
    );
    assert!(
        engine
            .start_work(&[game_parallel::StartWork {
                id: 99,
                worker: person,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    assert!(
        engine
            .transfer_goods(&[game_parallel::GoodsTransfer {
                person,
                good: 1,
                quantity: 1,
                direction: game_parallel::GoodsDirection::Pickup
            }])
            .is_err()
    );
    assert!(engine.pickup_food(person, 1).is_err());
    assert_eq!(engine.export(), before);
    engine.step().unwrap();
    let after = engine.export();
    assert!(
        after
            .travelers
            .iter()
            .any(|traveler| traveler.person.id == person),
        "{:?}",
        after.autonomy
    );
    while engine.tick() < until {
        engine.step().unwrap();
    }
    engine.dismiss_invocation().unwrap();
}

#[test]
fn packed_cargo_stays_with_its_holder_when_family_births_extend_the_index() {
    use game_parallel::{GoodsDirection, GoodsTransfer};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let economy =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.production = economy.production;
    world.goods_cargo = economy.goods_cargo;
    world.market = None;
    world.validate().unwrap();
    let mut engine = Engine::compile(world).unwrap();
    engine
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 1,
            quantity: 1,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    for _ in 0..10 {
        engine.step().unwrap();
    }
    let state = engine.export();
    assert!(state.families.as_ref().unwrap().totals.births > 0);
    let holder = &state.goods_cargo.as_ref().unwrap().holdings[0];
    assert_eq!((holder.person, holder.items[0].quantity), (1, 1));
    state.validate().unwrap();
    let engine =
        Engine::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
            .unwrap();
    assert_eq!(engine.export().goods_cargo, state.goods_cargo);
}

#[test]
fn packed_workers_remain_busy_for_automatic_market_acceptance() {
    use game_parallel::{Amount, GoodsDirection, GoodsTransfer, StartWork, TradeProposal};
    let world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    let mut engine = Engine::compile(world).unwrap();
    engine
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 1,
            quantity: 2,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    engine
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 2,
            give: vec![Amount {
                good: 1,
                quantity: 2,
            }],
            receive: vec![],
        })
        .unwrap();
    engine
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    engine.step().unwrap();
    engine.step().unwrap();
    assert_eq!(engine.export().market.as_ref().unwrap().offers.len(), 1);
    engine.step().unwrap();
    let world = engine.export();
    assert!(world.market.as_ref().unwrap().offers.is_empty());
    assert_eq!(world.goods_cargo.as_ref().unwrap().holdings[0].person, 2);
}

#[test]
fn player_control_prevents_autonomous_travel_until_control_is_surrendered() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/lineage.toml")).unwrap();
    let mind =
        World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap();
    world.knowledge = mind.knowledge;
    let mut autonomy = mind.autonomy.unwrap();
    autonomy.actors[0].person = 1;
    autonomy.rules.min_hunger = 0;
    world.autonomy = Some(autonomy);
    world.regions[0].food = 0;
    world.regions[1].food = 100;
    world
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|mind| mind.person == 1)
        .unwrap()
        .records = vec![
        game_parallel::KnowledgeRecord {
            region: 1,
            food: 0,
            observed_at: 0,
            witness: 1,
            learned_from: None,
        },
        game_parallel::KnowledgeRecord {
            region: 2,
            food: 100,
            observed_at: 0,
            witness: 1,
            learned_from: None,
        },
    ];
    world.validate().unwrap();
    let mut engine = Engine::compile(world).unwrap();
    engine.step().unwrap();
    assert!(
        engine
            .export()
            .travelers
            .iter()
            .all(|traveler| traveler.person.id != 1)
    );
}

#[path = "institutions.rs"]
mod institutions;
#[path = "property.rs"]
mod property;
