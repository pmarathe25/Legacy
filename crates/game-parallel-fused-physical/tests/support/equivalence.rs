use game_parallel::{Amount, Job, JobPhase, Person, World};
use game_parallel_blocks::Engine;
use game_parallel_fused_physical::Kernel;
fn fixture(scarce: bool) -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    world.rules.food_growth = 0;
    world.rules.food_capacity = 20_000;
    world.regions[0].food = if scarce { 4_000 } else { 20_000 };
    world.regions[0].people = (0..6_000)
        .map(|id| Person {
            id: id + 100,
            health: if id % 19 == 0 { 0 } else { 100 },
            hunger: if id % 3 == 0 { 3 } else { 0 },
        })
        .collect();
    let production = world.production.as_mut().unwrap();
    production.inventories[0].stocks[1].capacity = if scarce { 6_000 } else { 20_000 };
    production.jobs = (0..6_000)
        .map(|id| Job {
            id,
            worker: id + 100,
            region: 1,
            recipe: 1,
            remaining: 1 + id % 2,
            phase: JobPhase::Working,
            reserved: vec![Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
            outputs: vec![Amount {
                good: 2,
                quantity: 2,
            }]
            .into(),
        })
        .collect();
    production.jobs.extend((0..100).map(|id| {
        Job {
            id: 6_000 + id,
            worker: 100 + id,
            region: 1,
            recipe: 1,
            remaining: 0,
            phase: JobPhase::Waiting,
            reserved: vec![].into(),
            outputs: vec![Amount {
                good: 2,
                quantity: 2,
            }]
            .into(),
        }
    }));
    world.validate().unwrap();
    world
}
#[test]
fn fused_physics_and_jobs_match_persistent_blocks_with_abundant_and_contested_resources() {
    for scarce in [false, true] {
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let world = fixture(scarce);
            let mut reference = Engine::compile(world.clone()).unwrap();
            let mut fused = Kernel::compile(world).unwrap();
            pool.install(|| {
                for _ in 0..5 {
                    reference.step().unwrap();
                    fused.step().unwrap();
                    assert_eq!(fused.export(), reference.export());
                }
            });
        }
    }
}
#[test]
fn unsupported_owners_and_overflow_are_rejected() {
    let mut world = fixture(false);
    world.tick = u64::MAX;
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
    let mut world = fixture(false);
    world.guardians = World::from_toml(include_str!("../../../../content/parallel/guardians.toml"))
        .unwrap()
        .guardians;
    assert!(Kernel::compile(world).is_err());
}

#[test]
fn prepared_generation_is_private_and_stale_commit_is_rejected() {
    let world = fixture(true);
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    let first = kernel.prepare().unwrap();
    assert_eq!(kernel.export(), before);
    let stale = kernel.prepare().unwrap();
    assert_eq!(kernel.export(), before);
    assert!(kernel.commit(first).is_err());
    kernel.commit(stale).unwrap();
    let after = kernel.export();
    assert_eq!(kernel.export(), after);
}
#[test]
fn waiting_goods_deliver_after_reload_and_capacity_becomes_available() {
    let mut world = fixture(true);
    world.regions[0].people.truncate(3);
    world.regions[0].people.iter_mut().for_each(|person| {
        person.health = 100;
        person.hunger = 0;
    });
    let production = world.production.as_mut().unwrap();
    production.jobs.truncate(3);
    for job in &mut production.jobs {
        job.remaining = 1;
    }
    production.inventories[0].stocks[1].capacity = 2;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    reference.step().unwrap();
    fused.step().unwrap();
    let mut snapshot = fused.export();
    assert_eq!(snapshot, reference.export());
    assert_eq!(snapshot.production.as_ref().unwrap().jobs.len(), 2);
    snapshot.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 0;
    let snapshot = World::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap();
    reference = Engine::compile(snapshot.clone()).unwrap();
    fused = Kernel::compile(snapshot).unwrap();
    reference.step().unwrap();
    fused.step().unwrap();
    let result = fused.export();
    assert_eq!(result, reference.export());
    assert_eq!(result.production.as_ref().unwrap().jobs.len(), 1);
    assert!(
        result
            .production
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|event| event.kind == game_parallel::ProductionEventKind::Delivered)
    );
}

#[test]
fn fused_handles_arbitrary_job_order_multiple_outputs_and_unavailable_workers() {
    let mut world = fixture(true);
    let production = world.production.as_mut().unwrap();
    for job in &mut production.jobs {
        if job.id % 2 == 0 {
            job.region = 2;
        }
        if job.id % 3 == 0 {
            job.outputs.push(Amount {
                good: 1,
                quantity: 1,
            });
        }
    }
    production.jobs.reverse();
    world.validate().unwrap();
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        for _ in 0..4 {
            reference.step().unwrap();
            fused.step().unwrap();
            assert_eq!(fused.export(), reference.export());
        }
    });
}

#[test]
fn direct_bulk_admission_reserves_atomically_and_invalidates_prepared_generations() {
    use game_parallel::StartWork;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let stale = fused.prepare().unwrap();
    let requests = [
        StartWork {
            id: 2,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        },
        StartWork {
            id: 4,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        },
    ];
    reference.start_work(&requests).unwrap();
    fused.start_work(&requests).unwrap();
    let before = fused.export();
    assert!(fused.commit(stale).is_err());
    assert_eq!(fused.export(), before);
    assert_eq!(fused.export(), reference.export());
    assert!(fused.start_work(&requests).is_err());
    assert_eq!(fused.export(), before);
    for _ in 0..2 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(fused.export(), reference.export());
    }
    let reuse = StartWork {
        id: 2,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    reference.start_work(&[reuse]).unwrap();
    fused.start_work(&[reuse]).unwrap();
    assert_eq!(fused.export(), reference.export());
}
#[test]
fn interleaved_job_ids_preserve_existing_work_and_stock() {
    use game_parallel::StartWork;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    for request in [
        StartWork {
            id: 10,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        },
        StartWork {
            id: 1,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        },
    ] {
        reference.start_work(&[request]).unwrap();
        fused.start_work(&[request]).unwrap();
        assert_eq!(fused.export(), reference.export());
    }
    for _ in 0..2 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(fused.export(), reference.export());
    }
}

#[test]
fn dense_bulk_admission_matches_compiled_host_and_rejects_missing_workers() {
    use game_parallel::StartWork;
    let mut world = fixture(false);
    world.production.as_mut().unwrap().jobs.clear();
    world.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 18_000;
    world.production.as_mut().unwrap().inventories[0].stocks[0].capacity = 18_000;
    world.regions[0]
        .people
        .iter_mut()
        .for_each(|person| person.health = 100);
    let requests: Vec<_> = (0..6_000u64)
        .map(|id| StartWork {
            id,
            worker: id + 100,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut reference = Engine::compile(world.clone()).unwrap();
        let mut fused = Kernel::compile(world.clone()).unwrap();
        let before = fused.export();
        let missing: Vec<_> = requests
            .iter()
            .map(|request| StartWork {
                worker: request.worker + 1,
                ..*request
            })
            .collect();
        assert!(pool.install(|| fused.start_work(&missing)).is_err());
        assert_eq!(fused.export(), before);
        pool.install(|| {
            reference.start_work(&requests).unwrap();
            fused.start_work(&requests).unwrap();
        });
        assert_eq!(fused.export(), reference.export());
        pool.install(|| {
            for _ in 0..2 {
                reference.step().unwrap();
                fused.step().unwrap();
                assert_eq!(fused.export(), reference.export());
            }
        });
    }
}

#[test]
fn waiting_output_does_not_hold_the_worker_or_block_new_admission() {
    use game_parallel::StartWork;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    world.production.as_mut().unwrap().inventories[0].stocks[1].capacity = 2;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
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
    reference.start_work(&requests).unwrap();
    fused.start_work(&requests).unwrap();
    for _ in 0..2 {
        reference.step().unwrap();
        fused.step().unwrap();
    }
    assert_eq!(fused.export(), reference.export());
    assert_eq!(
        fused.export().production.as_ref().unwrap().jobs[0].phase,
        JobPhase::Waiting
    );
    let next = StartWork {
        id: 3,
        worker: 2,
        region: 1,
        recipe: 1,
        units: 1,
    };
    reference.start_work(&[next]).unwrap();
    fused.start_work(&[next]).unwrap();
    assert_eq!(fused.export(), reference.export());
    reference.step().unwrap();
    fused.step().unwrap();
    assert_eq!(fused.export(), reference.export());
}

#[test]
fn fused_cancellation_and_reassignment_match_checked_work_owner() {
    use game_parallel::StartWork;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let request = StartWork {
        id: 1,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    reference.start_work(&[request]).unwrap();
    fused.start_work(&[request]).unwrap();
    reference.reassign_work(1, 2).unwrap();
    fused.reassign_work(1, 2).unwrap();
    assert_eq!(fused.export(), reference.export());
    let before = fused.export();
    assert!(fused.reassign_work(1, 3).is_err());
    assert_eq!(fused.export(), before);
    reference.cancel_work(1).unwrap();
    fused.cancel_work(1).unwrap();
    assert_eq!(fused.export(), reference.export());
    assert_eq!(
        fused.export().production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        9
    );
}
#[test]
fn fused_cancellation_is_atomic_when_reserved_inputs_cannot_fit() {
    use game_parallel::StartWork;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    let mut fused = Kernel::compile(world).unwrap();
    fused
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    let mut snapshot = fused.export();
    snapshot.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 20;
    fused = Kernel::compile(snapshot).unwrap();
    let before = fused.export();
    assert!(fused.cancel_work(1).is_err());
    assert_eq!(fused.export(), before);
}

#[test]
fn segmented_job_arena_appends_across_partial_block_boundaries() {
    use game_parallel::StartWork;
    let mut world = fixture(false);
    world.regions[0].people = (0..10_000)
        .map(|id| Person {
            id: id + 100,
            health: 100,
            hunger: 0,
        })
        .collect();
    let production = world.production.as_mut().unwrap();
    production.jobs.clear();
    production.inventories[0].stocks[0].quantity = 30_000;
    production.inventories[0].stocks[0].capacity = 30_000;
    production.inventories[0].stocks[1].capacity = 20_000;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let requests: Vec<_> = (0..10_000u64)
        .map(|id| StartWork {
            id,
            worker: id + 100,
            region: 1,
            recipe: 1,
            units: 1,
        })
        .collect();
    for batch in [&requests[..4_000], &requests[4_000..]] {
        reference.start_work(batch).unwrap();
        fused.start_work(batch).unwrap();
        assert_eq!(fused.export(), reference.export());
    }
    for _ in 0..2 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(fused.export(), reference.export());
    }
}

fn sorted(mut world: World) -> World {
    for region in &mut world.regions {
        region.people.sort_unstable_by_key(|p| p.id);
    }
    world
}
#[test]
fn travel_departure_arrival_and_food_priority_match_reference_across_reload() {
    use game_parallel::Move;
    let mut world = fixture(true);
    world.production.as_mut().unwrap().jobs.clear();
    world.regions[0].people = vec![Person {
        id: 5,
        health: 100,
        hunger: 2,
    }];
    world.regions[1].people = [1, 10]
        .into_iter()
        .map(|id| Person {
            id,
            health: 100,
            hunger: 2,
        })
        .collect();
    world.regions[0].food = 0;
    world.regions[1].food = 0;
    world.rules.food_growth = 2;
    world.routes[0].ticks = 1;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let bad = [
        Move {
            person: 5,
            destination: 2,
        },
        Move {
            person: 999,
            destination: 2,
        },
    ];
    let before = fused.export();
    assert!(fused.move_people(&bad).is_err());
    assert_eq!(fused.export(), before);
    let draft = fused.prepare().unwrap();
    let batch = [Move {
        person: 5,
        destination: 2,
    }];
    reference.move_people(&batch).unwrap();
    fused.move_people(&batch).unwrap();
    assert!(fused.commit(draft).is_err());
    for tick in 0..3 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
        if tick == 0 {
            let save = serde_json::to_string(&fused.export()).unwrap();
            let mut reloaded = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
            reloaded.step().unwrap();
            let mut uninterrupted = Kernel::compile(fused.export()).unwrap();
            uninterrupted.step().unwrap();
            assert_eq!(sorted(reloaded.export()), sorted(uninterrupted.export()));
        }
    }
}
#[test]
fn travel_split_preserves_work_and_rejects_busy_workers() {
    use game_parallel::Move;
    let mut world = fixture(false);
    world.regions[0].people.truncate(3);
    world.regions[0].people.iter_mut().for_each(|person| {
        person.health = 100;
        person.hunger = 0;
    });
    world.production.as_mut().unwrap().jobs.truncate(1);
    world.production.as_mut().unwrap().jobs[0].remaining = 3;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    reference.step().unwrap();
    fused.step().unwrap();
    let before = fused.export();
    assert!(
        fused
            .move_people(&[
                Move {
                    person: 100,
                    destination: 2
                },
                Move {
                    person: 101,
                    destination: 2
                }
            ])
            .is_err()
    );
    assert_eq!(fused.export(), before);
    let batch = [Move {
        person: 101,
        destination: 2,
    }];
    reference.move_people(&batch).unwrap();
    fused.move_people(&batch).unwrap();
    for _ in 0..3 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
}
#[test]
fn prepared_draft_cannot_be_committed_to_another_kernel() {
    let world = fixture(true);
    let mut one = Kernel::compile(world.clone()).unwrap();
    let mut two = Kernel::compile(world).unwrap();
    let before = two.export();
    let draft = one.prepare().unwrap();
    assert!(two.commit(draft).is_err());
    assert_eq!(two.export(), before);
}

#[test]
fn farming_and_shipments_are_staged_around_meals_and_work() {
    use game_parallel::Shipment;
    let mut world = fixture(false);
    world.regions[0].people.truncate(4);
    world.regions[0].people.iter_mut().for_each(|person| {
        person.health = 100;
        person.hunger = 2;
    });
    world.production.as_mut().unwrap().jobs.truncate(1);
    world.production.as_mut().unwrap().jobs[0].remaining = 3;
    world.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    world.regions[0].food = 10;
    world.regions[1].food = 4;
    world.rules.food_capacity = 100;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let shipment = [Shipment {
        id: 9,
        source: 1,
        destination: 2,
        food: 4,
        ticks_remaining: 2,
    }];
    let draft = fused.prepare().unwrap();
    reference.launch_shipments(&shipment).unwrap();
    fused.launch_shipments(&shipment).unwrap();
    assert!(fused.commit(draft).is_err());
    for _ in 0..8 {
        let before = fused.export();
        let draft = fused.prepare().unwrap();
        assert_eq!(fused.export(), before);
        fused.commit(draft).unwrap();
        reference.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
}
#[test]
fn imported_traveler_resumes_work_only_after_arrival_at_workplace() {
    use game_parallel::Traveler;
    let mut world = fixture(false);
    world.regions[0].people.truncate(1);
    world.regions[0].people[0].health = 100;
    world.regions[0].people[0].hunger = 0;
    world.production.as_mut().unwrap().jobs.truncate(1);
    world.production.as_mut().unwrap().jobs[0].remaining = 2;
    world.production.as_mut().unwrap().jobs[0].region = 2;
    let person = world.regions[0].people.pop().unwrap();
    world.travelers.push(Traveler {
        person,
        source: 1,
        destination: 2,
        ticks_remaining: 1,
    });
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    for _ in 0..4 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
}

#[test]
fn provision_packing_and_travel_meals_are_atomic_and_match_reference() {
    use game_parallel::Move;
    let mut world = fixture(false);
    world.production.as_mut().unwrap().jobs.clear();
    world.regions[0].people.truncate(3);
    world.regions[0].people.iter_mut().for_each(|p| {
        p.health = 100;
        p.hunger = 3;
    });
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/cargo_auto.toml"))
        .unwrap()
        .cargo;
    world.routes[0].ticks = 5;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let before = fused.export();
    assert!(
        fused
            .move_people(&[
                Move {
                    person: 100,
                    destination: 2
                },
                Move {
                    person: 999,
                    destination: 2
                }
            ])
            .is_err()
    );
    assert_eq!(fused.export(), before);
    let requests = [
        Move {
            person: 100,
            destination: 2,
        },
        Move {
            person: 101,
            destination: 2,
        },
    ];
    reference.move_people(&requests).unwrap();
    fused.move_people(&requests).unwrap();
    for _ in 0..6 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
}
#[test]
fn manual_food_custody_edits_invalidate_prepared_drafts() {
    let mut world = fixture(false);
    world.production.as_mut().unwrap().jobs.clear();
    world.regions[0].people.truncate(2);
    world.regions[0].people.iter_mut().for_each(|p| {
        p.health = 100;
        p.hunger = 3;
    });
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/cargo_auto.toml"))
        .unwrap()
        .cargo;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let draft = fused.prepare().unwrap();
    reference.pickup_food(100, 4).unwrap();
    fused.pickup_food(100, 4).unwrap();
    assert!(fused.commit(draft).is_err());
    reference.eat_carried_food(100).unwrap();
    fused.eat_carried_food(100).unwrap();
    reference.deposit_food(100, 2).unwrap();
    fused.deposit_food(100, 2).unwrap();
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
}

fn family_world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.production = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))
        .unwrap()
        .production;
    world.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    world.cargo = World::from_toml(include_str!("../../../../content/parallel/cargo_auto.toml"))
        .unwrap()
        .cargo;
    world
}
#[test]
fn families_births_deaths_and_travel_match_across_reload() {
    use game_parallel::Move;
    let world = family_world();
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    reference.step().unwrap();
    fused.step().unwrap();
    let mother = fused
        .export()
        .families
        .unwrap()
        .members
        .into_iter()
        .find(|m| m.id <= 4 && m.pregnancy.is_some())
        .unwrap()
        .id;
    let moves = [Move {
        person: mother,
        destination: 2,
    }];
    reference.move_people(&moves).unwrap();
    fused.move_people(&moves).unwrap();
    for tick in 2..=24 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
        if tick == 4 {
            let state = fused.export();
            let child = state
                .families
                .as_ref()
                .unwrap()
                .members
                .iter()
                .find(|m| m.parents.first() == Some(&mother))
                .unwrap()
                .id;
            let mother_travel = state
                .travelers
                .iter()
                .find(|t| t.person.id == mother)
                .unwrap();
            let child_travel = state
                .travelers
                .iter()
                .find(|t| t.person.id == child)
                .unwrap();
            assert_eq!(
                (mother_travel.destination, mother_travel.ticks_remaining),
                (child_travel.destination, child_travel.ticks_remaining)
            );
            fused =
                Kernel::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
                    .unwrap();
        }
    }
    assert!(fused.export().families.unwrap().totals.births > 0);
}
#[test]
fn late_lifecycle_failure_does_not_publish_other_owners() {
    let mut world = family_world();
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut fused = Kernel::compile(world).unwrap();
    let before = fused.export();
    assert!(fused.step().is_err());
    assert_eq!(fused.export(), before);
}

#[test]
fn property_commands_and_work_permissions_match_reference() {
    use game_parallel::StartWork;
    use game_parallel_fused_physical::PropertyCommand;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/property.toml")).unwrap();
    world.autowork = None;
    world.market = None;
    world.goods_cargo = None;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let request = [StartWork {
        id: 5,
        worker: 2,
        region: 1,
        recipe: 1,
        units: 1,
    }];
    assert!(fused.start_work(&request).is_err());
    assert!(reference.start_work(&request).is_err());
    reference.property().grant(1, 2, 1, 1).unwrap();
    fused
        .property_command(PropertyCommand::Grant {
            owner: 1,
            recipient: 2,
            region: 1,
            good: 1,
        })
        .unwrap();
    reference.start_work(&request).unwrap();
    fused.start_work(&request).unwrap();
    for _ in 0..3 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
    let offer = reference.property().offer(1, 2, 1, 1).unwrap();
    let fused_offer = fused
        .property_command(PropertyCommand::Offer {
            owner: 1,
            recipient: 2,
            region: 1,
            good: 1,
        })
        .unwrap()
        .unwrap();
    assert_eq!(offer, fused_offer);
    reference.property().decide(offer, 2, true).unwrap();
    fused
        .property_command(PropertyCommand::Decide {
            id: offer,
            actor: 2,
            accept: true,
        })
        .unwrap();
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
}

#[test]
fn autonomous_work_reserves_inputs_and_actor_time_before_physics() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/property.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    world.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    for tick in 0..8 {
        let before = fused.export();
        let draft = fused.prepare().unwrap();
        assert_eq!(before, fused.export());
        fused.commit(draft).unwrap();
        reference.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
        if tick == 3 {
            fused = Kernel::compile(
                World::from_json(&serde_json::to_string(&fused.export()).unwrap()).unwrap(),
            )
            .unwrap();
        }
    }
}
#[test]
fn failed_lifecycle_discards_automatic_work_reservations() {
    let mut world = family_world();
    world.autowork = World::from_toml(include_str!("../../../../content/parallel/autowork.toml"))
        .unwrap()
        .autowork;
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn trade_custody_and_automatic_work_share_actor_reservations() {
    use game_parallel::{GoodsDirection, GoodsTransfer, TradeProposal};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    world.autowork = World::from_toml(include_str!("../../../../content/parallel/autowork.toml"))
        .unwrap()
        .autowork;
    world.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let transfers = [GoodsTransfer {
        person: 1,
        good: 1,
        quantity: 2,
        direction: GoodsDirection::Pickup,
    }];
    reference.transfer_goods(&transfers).unwrap();
    fused.transfer_goods(&transfers).unwrap();
    let proposal = TradeProposal {
        proposer: 1,
        recipient: 2,
        give: vec![Amount {
            good: 1,
            quantity: 2,
        }],
        receive: vec![],
    };
    let id = reference.propose_trade(proposal.clone()).unwrap();
    assert_eq!(fused.propose_trade(proposal).unwrap(), id);
    for _ in 0..5 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
}

#[test]
fn lineage_guardians_and_invocation_warning_match_reference() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/invocation.toml")).unwrap();
    world.production = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))
        .unwrap()
        .production;
    world.guardians = World::from_toml(include_str!("../../../../content/parallel/guardians.toml"))
        .unwrap()
        .guardians;
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap()
        .age = 69;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    for _ in 0..7 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
    reference.invoke_lives(&[1]).unwrap();
    fused.invoke_lives(&[1]).unwrap();
    reference.step().unwrap();
    fused.step().unwrap();
    assert!(reference.step().is_err());
    assert!(fused.step().is_err());
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
    let warning = fused
        .export()
        .lineage
        .unwrap()
        .invocation
        .unwrap()
        .pending
        .unwrap();
    reference
        .accept_invocation_warning(warning.revision)
        .unwrap();
    fused.accept_invocation_warning(warning.revision).unwrap();
    reference.step().unwrap();
    fused.step().unwrap();
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
    reference.dismiss_invocation().unwrap();
    fused.dismiss_invocation().unwrap();
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
}

#[test]
fn physical_world_without_production_supports_other_owners() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    for _ in 0..12 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
    assert!(fused.export().production.is_none());
    assert!(
        fused
            .start_work(&[game_parallel::StartWork {
                id: 1,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
}

#[test]
fn accepted_control_loss_blocks_manual_commands_and_allows_autonomy() {
    use game_parallel::Move;
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
    let mut fused = Kernel::compile(world).unwrap();
    for _ in 0..7 {
        fused.step().unwrap();
    }
    fused.invoke_lives(&[1]).unwrap();
    for _ in 0..15 {
        if fused.step().is_err() {
            let warning = fused
                .export()
                .lineage
                .unwrap()
                .invocation
                .unwrap()
                .pending
                .unwrap();
            fused.accept_invocation_warning(warning.revision).unwrap();
        }
        if fused
            .export()
            .lineage
            .unwrap()
            .invocation
            .unwrap()
            .control_until
            .is_some()
        {
            break;
        }
    }
    let mut state = fused.export();
    let person = state.lineage.as_ref().unwrap().current.unwrap();
    assert!(
        state
            .lineage
            .as_ref()
            .unwrap()
            .invocation
            .as_ref()
            .unwrap()
            .control_until
            .is_some()
    );
    let source = state
        .regions
        .iter()
        .position(|region| region.people.iter().any(|p| p.id == person))
        .unwrap();
    let dest = 1 - source;
    state.regions[source].food = 0;
    state.regions[dest].food = 100;
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
                    region: state.regions[dest].id,
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
    let mut reference = Engine::compile(state.clone()).unwrap();
    let mut fused = Kernel::compile(state).unwrap();
    let before = fused.export();
    assert!(
        fused
            .move_people(&[Move {
                person,
                destination: before.regions[dest].id
            }])
            .is_err()
    );
    assert_eq!(before, fused.export());
    assert!(fused.dismiss_invocation().is_err());
    reference.step().unwrap();
    fused.step().unwrap();
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
    assert!(
        fused
            .export()
            .travelers
            .iter()
            .any(|traveler| traveler.person.id == person)
    );
}

#[test]
fn packed_goods_follow_people_across_births_and_travel() {
    use game_parallel::{GoodsDirection, GoodsTransfer, Move};
    let mut world = family_world();
    world.goods_cargo =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))
            .unwrap()
            .goods_cargo;
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let take = [GoodsTransfer {
        person: 1,
        good: 1,
        quantity: 1,
        direction: GoodsDirection::Pickup,
    }];
    reference.transfer_goods(&take).unwrap();
    fused.transfer_goods(&take).unwrap();
    for _ in 0..7 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
    let moves = [Move {
        person: 1,
        destination: 2,
    }];
    reference.move_people(&moves).unwrap();
    fused.move_people(&moves).unwrap();
    for _ in 0..5 {
        reference.step().unwrap();
        fused.step().unwrap();
        assert_eq!(sorted(fused.export()), sorted(reference.export()));
    }
    let put = [GoodsTransfer {
        person: 1,
        good: 1,
        quantity: 1,
        direction: GoodsDirection::Deposit,
    }];
    reference.transfer_goods(&put).unwrap();
    fused.transfer_goods(&put).unwrap();
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
}

#[test]
fn mixed_world_is_deterministic_across_workers_and_reload() {
    let mut world = family_world();
    let market = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    world.market = market.market;
    world.goods_cargo = market.goods_cargo;
    world.autowork = World::from_toml(include_str!("../../../../content/parallel/autowork.toml"))
        .unwrap()
        .autowork;
    world.property = World::from_toml(include_str!("../../../../content/parallel/property.toml"))
        .unwrap()
        .property;
    let minds =
        World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap();
    world.social = minds.social;
    world.knowledge = minds.knowledge;
    world.autonomy = minds.autonomy;
    let rulers =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    world.lineage = rulers.lineage;
    world.guardians = rulers.guardians;
    world.validate().unwrap();
    let pools: Vec<_> = [1, 2, 4, 6]
        .into_iter()
        .map(|workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
        })
        .collect();
    let mut engines: Vec<_> = pools
        .iter()
        .map(|pool| pool.install(|| Kernel::compile(world.clone()).unwrap()))
        .collect();
    for tick in 1..=18 {
        for (pool, engine) in pools.iter().zip(&mut engines) {
            pool.install(|| engine.step().unwrap());
        }
        let expected = sorted(engines[0].export());
        expected.validate().unwrap();
        for engine in &engines[1..] {
            assert_eq!(sorted(engine.export()), expected);
        }
        if tick == 8 {
            let save = serde_json::to_string(&engines[2].export()).unwrap();
            engines[2] = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
        }
    }
}

#[test]
fn recurring_autowork_reclaims_metadata_while_a_job_remains_stalled() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    world.regions[0].people[0].hunger = 1;
    world.regions[0].people[1].hunger = 0;
    world.rules.hunger_per_tick = 0;
    let production = world.production.as_mut().unwrap();
    production.max_work_hunger = 0;
    production.inventories[0].stocks[0].quantity = 10000;
    production.inventories[0].stocks[0].capacity = 10000;
    production.inventories[0].stocks[1].capacity = 10000;
    production.jobs.push(Job {
        id: 1,
        worker: 1,
        region: 1,
        recipe: 1,
        remaining: 1,
        phase: JobPhase::Working,
        reserved: vec![Amount {
            good: 1,
            quantity: 3,
        }]
        .into(),
        outputs: vec![Amount {
            good: 2,
            quantity: 2,
        }]
        .into(),
    });
    world
        .autowork
        .as_mut()
        .unwrap()
        .policies
        .retain(|policy| policy.person == 2);
    let mut reference = Engine::compile(world.clone()).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    let mut previous = 0;
    let mut compacted = false;
    pool.install(|| {
        for tick in 0..800 {
            reference.step().unwrap();
            fused.step().unwrap();
            if tick % 100 == 0 {
                assert_eq!(sorted(fused.export()), sorted(reference.export()));
            }
            let size = fused.job_storage_len();
            compacted |= size < previous;
            previous = size;
            assert!(size <= 258);
        }
    });
    assert!(compacted);
    assert_eq!(sorted(fused.export()), sorted(reference.export()));
    let production = fused.export().production.unwrap();
    assert!(production.jobs.iter().any(|job| job.worker == 1));
    assert!(production.inventories[0].stocks[1].quantity > 500);
}

#[test]
fn combat_rounds_are_simultaneous_reserved_and_cause_family_deaths() {
    use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, Move};
    let world =
        World::from_toml(include_str!("../../../../content/parallel/conflict.toml")).unwrap();
    let mut fused = Kernel::compile(world).unwrap();
    let fight = CombatEncounter {
        id: 1,
        region: 1,
        fighters: vec![
            CombatFighter {
                person: 1,
                side: 1,
                order: CombatOrder::Attack { target: 2 },
            },
            CombatFighter {
                person: 2,
                side: 2,
                order: CombatOrder::Attack { target: 1 },
            },
        ],
    };
    let draft = fused.prepare().unwrap();
    assert_eq!(fused.begin_combat(fight).unwrap(), 1);
    assert!(fused.commit(draft).is_err());
    assert!(
        fused
            .move_people(&[Move {
                person: 1,
                destination: 2
            }])
            .is_err()
    );
    let snapshot = fused.export();
    snapshot.validate().unwrap();
    fused = Kernel::compile(World::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap())
        .unwrap();
    fused.step().unwrap();
    let state = fused.export();
    state.validate().unwrap();
    assert_eq!(state.conflict.as_ref().unwrap().events.len(), 2);
    assert!(state.conflict.as_ref().unwrap().encounters.is_empty());
    for id in [1, 2] {
        assert_eq!(
            state.regions[0]
                .people
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .health,
            0
        );
        assert!(
            !state
                .families
                .as_ref()
                .unwrap()
                .members
                .iter()
                .find(|m| m.id == id)
                .unwrap()
                .alive
        );
    }
}
#[test]
fn failed_lifecycle_rolls_back_a_prepared_combat_round() {
    use game_parallel::{CombatEncounter, CombatFighter, CombatOrder};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/conflict.toml")).unwrap();
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut fused = Kernel::compile(world).unwrap();
    fused
        .begin_combat(CombatEncounter {
            id: 1,
            region: 1,
            fighters: vec![
                CombatFighter {
                    person: 1,
                    side: 1,
                    order: CombatOrder::Attack { target: 2 },
                },
                CombatFighter {
                    person: 2,
                    side: 2,
                    order: CombatOrder::Defend,
                },
            ],
        })
        .unwrap();
    let before = fused.export();
    assert!(fused.step().is_err());
    assert_eq!(fused.export(), before);
}

#[test]
fn combat_orders_and_shared_actor_reservations_are_atomic() {
    use game_parallel::{
        CombatCommand, CombatEncounter, CombatFighter, CombatOrder, GoodsDirection, GoodsTransfer,
        StartWork,
    };
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/conflict.toml")).unwrap();
    let workshop =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.production = workshop.production;
    world.goods_cargo = workshop.goods_cargo;
    world.conflict.as_mut().unwrap().damage = 60;
    let mut kernel = Kernel::compile(world).unwrap();
    let fight = CombatEncounter {
        id: 1,
        region: 1,
        fighters: vec![
            CombatFighter {
                person: 1,
                side: 1,
                order: CombatOrder::Attack { target: 2 },
            },
            CombatFighter {
                person: 2,
                side: 2,
                order: CombatOrder::Attack { target: 1 },
            },
            CombatFighter {
                person: 3,
                side: 1,
                order: CombatOrder::Attack { target: 2 },
            },
        ],
    };
    kernel.begin_combat(fight).unwrap();
    let before = kernel.export();
    assert!(
        kernel
            .start_work(&[StartWork {
                id: 5,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    assert!(
        kernel
            .transfer_goods(&[GoodsTransfer {
                person: 1,
                good: 1,
                quantity: 1,
                direction: GoodsDirection::Pickup
            }])
            .is_err()
    );
    assert!(
        kernel
            .combat_orders(&[
                CombatCommand {
                    encounter: 1,
                    person: 1,
                    order: CombatOrder::Defend
                },
                CombatCommand {
                    encounter: 1,
                    person: 3,
                    order: CombatOrder::Attack { target: 999 }
                }
            ])
            .is_err()
    );
    assert_eq!(kernel.export(), before);
    kernel.step().unwrap();
    let result = kernel.export();
    result.validate().unwrap();
    assert_eq!(
        result.regions[0]
            .people
            .iter()
            .find(|person| person.id == 2)
            .unwrap()
            .health,
        0
    );
    assert_eq!(
        result.regions[0]
            .people
            .iter()
            .find(|person| person.id == 1)
            .unwrap()
            .health,
        40
    );
    assert_eq!(result.conflict.unwrap().events.len(), 3);
}

#[test]
fn large_contested_encounter_is_worker_invariant_and_cleans_up_targets() {
    use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, Conflict};
    for hot in [false, true] {
        let count = 6000usize;
        let mut world =
            World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
        world.rules.hunger_per_tick = 0;
        world.rules.recovery = 0;
        world.regions[0].people = (0..count)
            .map(|i| Person {
                id: i as u64 + 1,
                health: 100,
                hunger: 0,
            })
            .collect();
        world.regions[1].people.clear();
        let fighters = (0..count)
            .map(|i| {
                let side = if i < count / 2 { 1 } else { 2 };
                let target = if side == 1 {
                    if hot {
                        count / 2 + 1
                    } else {
                        i + count / 2 + 1
                    }
                } else if hot {
                    1
                } else {
                    i - count / 2 + 1
                };
                CombatFighter {
                    person: i as u64 + 1,
                    side,
                    order: CombatOrder::Attack {
                        target: target as u64,
                    },
                }
            })
            .collect();
        world.conflict = Some(Conflict {
            damage: 1,
            next_id: 2,
            encounters: vec![CombatEncounter {
                id: 1,
                region: 1,
                fighters,
            }],
            events: vec![],
        });
        let mut expected = None;
        for workers in [1, 2, 4, 6] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let mut kernel = Kernel::compile(world.clone()).unwrap();
            pool.install(|| kernel.step().unwrap());
            let result = sorted(kernel.export());
            result.validate().unwrap();
            assert_eq!(result.conflict.as_ref().unwrap().events.len(), count);
            if let Some(expected) = &expected {
                assert_eq!(&result, expected);
            } else {
                expected = Some(result);
            }
        }
    }
}

#[test]
fn retreat_takes_simultaneous_damage_then_releases_the_actor() {
    use game_parallel::{CombatCommand, CombatEncounter, CombatFighter, CombatOrder, Move};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/conflict.toml")).unwrap();
    world.conflict.as_mut().unwrap().damage = 20;
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .begin_combat(CombatEncounter {
            id: 1,
            region: 1,
            fighters: vec![
                CombatFighter {
                    person: 1,
                    side: 1,
                    order: CombatOrder::Attack { target: 2 },
                },
                CombatFighter {
                    person: 2,
                    side: 2,
                    order: CombatOrder::Attack { target: 1 },
                },
                CombatFighter {
                    person: 3,
                    side: 2,
                    order: CombatOrder::Attack { target: 1 },
                },
            ],
        })
        .unwrap();
    kernel
        .combat_orders(&[CombatCommand {
            encounter: 1,
            person: 2,
            order: CombatOrder::Retreat,
        }])
        .unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    state.validate().unwrap();
    assert_eq!(
        state.regions[0]
            .people
            .iter()
            .find(|person| person.id == 2)
            .unwrap()
            .health,
        80
    );
    let fight = &state.conflict.as_ref().unwrap().encounters[0];
    assert!(!fight.fighters.iter().any(|fighter| fighter.person == 2));
    assert!(matches!(
        fight
            .fighters
            .iter()
            .find(|fighter| fighter.person == 1)
            .unwrap()
            .order,
        CombatOrder::Defend
    ));
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
        .unwrap();
    kernel
        .move_people(&[Move {
            person: 2,
            destination: 2,
        }])
        .unwrap();
}

#[test]
fn treatment_reserves_medicine_and_caregiver_then_heals() {
    use game_parallel::{CareStart, Move, StartWork};
    let world = World::from_toml(include_str!("../../../../content/parallel/care.toml")).unwrap();
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    let bad = [
        CareStart {
            id: 1,
            caregiver: 1,
            patient: 2,
        },
        CareStart {
            id: 2,
            caregiver: 999,
            patient: 2,
        },
    ];
    assert!(kernel.start_care(&bad).is_err());
    assert_eq!(kernel.export(), before);
    kernel.start_care(&[bad[0]]).unwrap();
    assert_eq!(
        kernel.export().production.unwrap().inventories[0].stocks[0].quantity,
        6
    );
    assert!(
        kernel
            .move_people(&[Move {
                person: 1,
                destination: 2
            }])
            .is_err()
    );
    assert!(
        kernel
            .start_work(&[StartWork {
                id: 9,
                worker: 1,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    kernel.step().unwrap();
    let saved = serde_json::to_string(&kernel.export()).unwrap();
    kernel = Kernel::compile(World::from_json(&saved).unwrap()).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    state.validate().unwrap();
    assert_eq!(
        state.regions[0]
            .people
            .iter()
            .find(|person| person.id == 2)
            .unwrap()
            .health,
        92
    );
    let event = &state.care.unwrap().events[0];
    assert_eq!(event.healed, 30);
    assert!(!event.interrupted);
}
#[test]
fn departing_patient_interrupts_care_without_refunding_consumed_medicine() {
    use game_parallel::{CareStart, Move};
    let world = World::from_toml(include_str!("../../../../content/parallel/care.toml")).unwrap();
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 1,
            caregiver: 1,
            patient: 2,
        }])
        .unwrap();
    kernel
        .move_people(&[Move {
            person: 2,
            destination: 2,
        }])
        .unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert!(state.care.as_ref().unwrap().jobs.is_empty());
    assert!(state.care.unwrap().events[0].interrupted);
    assert_eq!(
        state.production.unwrap().inventories[0].stocks[0].quantity,
        6
    );
}

#[test]
fn many_caregivers_share_one_patient_with_stable_bounded_healing() {
    use game_parallel::{Care, CareStart};
    let count = 6000usize;
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    world.rules.hunger_per_tick = 0;
    world.rules.recovery = 0;
    world.regions[1].people.clear();
    world.regions[0].people = (0..=count)
        .map(|i| Person {
            id: i as u64 + 1,
            health: if i == count { 1 } else { 100 },
            hunger: 0,
        })
        .collect();
    let stock = &mut world.production.as_mut().unwrap().inventories[0].stocks[0];
    stock.quantity = count as u64;
    stock.capacity = count as u64;
    world.care = Some(Care {
        medicine: 1,
        dose: 1,
        duration: 1,
        healing: 1,
        next_id: 1,
        jobs: vec![],
        events: vec![],
    });
    let starts: Vec<_> = (0..count)
        .map(|i| CareStart {
            id: i as u64 + 1,
            caregiver: i as u64 + 1,
            patient: count as u64 + 1,
        })
        .collect();
    let mut expected = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut kernel = Kernel::compile(world.clone()).unwrap();
        pool.install(|| {
            kernel.start_care(&starts).unwrap();
            kernel.step().unwrap();
        });
        let result = sorted(kernel.export());
        result.validate().unwrap();
        assert_eq!(result.regions[0].people.last().unwrap().health, 100);
        let events = &result.care.as_ref().unwrap().events;
        assert_eq!(events.len(), count);
        assert_eq!(
            events
                .iter()
                .map(|event| u64::from(event.healed))
                .sum::<u64>(),
            99
        );
        assert!(events.iter().take(99).all(|event| event.healed == 1));
        if let Some(expected) = &expected {
            assert_eq!(&result, expected);
        } else {
            expected = Some(result);
        }
    }
}

#[test]
fn fatal_combat_prevents_treatment_from_resurrecting_patient() {
    use game_parallel::{Care, CareStart, CombatEncounter, CombatFighter, CombatOrder};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/conflict.toml")).unwrap();
    world.production = World::from_toml(include_str!("../../../../content/parallel/workshop.toml"))
        .unwrap()
        .production;
    world.regions[0]
        .people
        .iter_mut()
        .find(|person| person.id == 2)
        .unwrap()
        .health = 40;
    world.care = Some(Care {
        medicine: 1,
        dose: 1,
        duration: 1,
        healing: 60,
        next_id: 1,
        jobs: vec![],
        events: vec![],
    });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 1,
            caregiver: 1,
            patient: 2,
        }])
        .unwrap();
    kernel
        .begin_combat(CombatEncounter {
            id: 1,
            region: 1,
            fighters: vec![
                CombatFighter {
                    person: 2,
                    side: 1,
                    order: CombatOrder::Defend,
                },
                CombatFighter {
                    person: 3,
                    side: 2,
                    order: CombatOrder::Attack { target: 2 },
                },
            ],
        })
        .unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    state.validate().unwrap();
    assert_eq!(
        state.regions[0]
            .people
            .iter()
            .find(|person| person.id == 2)
            .unwrap()
            .health,
        0
    );
    assert!(
        !state
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|member| member.id == 2)
            .unwrap()
            .alive
    );
    assert!(state.care.unwrap().events[0].interrupted);
}
#[test]
fn failed_lifecycle_leaves_active_treatment_unchanged() {
    use game_parallel::{Care, CareStart};
    let mut world = family_world();
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    world.care = Some(Care {
        medicine: 1,
        dose: 1,
        duration: 1,
        healing: 10,
        next_id: 1,
        jobs: vec![],
        events: vec![],
    });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 1,
            caregiver: 1,
            patient: 2,
        }])
        .unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn simultaneous_care_uses_pre_healing_health_and_reports_zero_healing() {
    use game_parallel::{Care, CareStart};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    world.rules.recovery = 0;
    world.regions[0].people[0].health = 50;
    world.regions[0].people[1].health = 60;
    world.care = Some(Care {
        medicine: 1,
        dose: 1,
        duration: 1,
        healing: 10,
        next_id: 1,
        jobs: vec![],
        events: vec![],
    });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .start_care(&[
            CareStart {
                id: 1,
                caregiver: 1,
                patient: 2,
            },
            CareStart {
                id: 2,
                caregiver: 2,
                patient: 1,
            },
        ])
        .unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert_eq!(
        state.regions[0]
            .people
            .iter()
            .map(|person| person.health)
            .collect::<Vec<_>>(),
        vec![60, 70]
    );
    assert!(
        state
            .care
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|event| event.healed == 10 && !event.interrupted)
    );
    let mut state = state;
    state.regions[0].people[0].health = 100;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 3,
            caregiver: 2,
            patient: 1,
        }])
        .unwrap();
    kernel.step().unwrap();
    let care = kernel.export().care.unwrap();
    assert_eq!(care.events[0].healed, 0);
    assert!(!care.events[0].interrupted);
}
#[test]
fn aging_death_interrupts_a_continuing_care_job() {
    use game_parallel::{Care, CareStart};
    let mut world = family_world();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = 69;
    world.care = Some(Care {
        medicine: 1,
        dose: 1,
        duration: 3,
        healing: 10,
        next_id: 1,
        jobs: vec![],
        events: vec![],
    });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 1,
            caregiver: 1,
            patient: 2,
        }])
        .unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    state.validate().unwrap();
    assert!(
        !state
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|member| member.id == 1)
            .unwrap()
            .alive
    );
    assert!(state.care.as_ref().unwrap().jobs.is_empty());
    assert!(state.care.unwrap().events[0].interrupted);
}

#[test]
fn finite_shelter_protects_lowest_living_identity() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/exposure.toml")).unwrap();
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    state.validate().unwrap();
    let people = &state.regions[0].people;
    assert_eq!(
        people.iter().find(|person| person.id == 1).unwrap().health,
        100
    );
    assert_eq!(
        people.iter().find(|person| person.id == 2).unwrap().health,
        90
    );
}
#[test]
fn shelter_priority_is_stable_after_interleaving_arrivals_and_reload() {
    use game_parallel::{Environment, Hazard, Move};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    world.regions[0].people = vec![Person {
        id: 5,
        health: 100,
        hunger: 0,
    }];
    world.regions[1].people = [1, 10]
        .into_iter()
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.routes[0].ticks = 1;
    world.environment = Some(Environment {
        seasons: Vec::new(),
        regions: vec![Hazard {
            region: 2,
            damage: 10,
            shelter: 2,
            shelter_reduction: 10,
            shelter_good: None,
        }],
    });
    let mut uninterrupted = Kernel::compile(world).unwrap();
    uninterrupted
        .move_people(&[Move {
            person: 5,
            destination: 2,
        }])
        .unwrap();
    uninterrupted.step().unwrap();
    let save = serde_json::to_string(&uninterrupted.export()).unwrap();
    let mut reloaded = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
    uninterrupted.step().unwrap();
    reloaded.step().unwrap();
    assert_eq!(sorted(uninterrupted.export()), sorted(reloaded.export()));
    let state = uninterrupted.export();
    let people = &state.regions[1].people;
    assert_eq!(
        people
            .iter()
            .map(|person| (person.id, person.health))
            .collect::<Vec<_>>(),
        vec![(1, 100), (5, 100), (10, 90)]
    );
}

#[test]
fn produced_shelter_goods_protect_people_on_the_following_tick() {
    use game_parallel::{Environment, Good, Hazard, Recipe, StartWork, Stock};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.market = None;
    world.goods_cargo = None;
    world.rules.recovery = 0;
    let production = world.production.as_mut().unwrap();
    production.goods.push(Good {
        id: 3,
        name: "shelter".into(),
        food: None,
    });
    production.recipes.push(Recipe {
        id: 2,
        name: "build shelter".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 3,
            quantity: 1,
        }],
    });
    production.inventories[0].stocks.push(Stock {
        good: 3,
        quantity: 0,
        capacity: 10,
    });
    world.environment = Some(Environment {
        seasons: Vec::new(),
        regions: vec![Hazard {
            region: 1,
            damage: 10,
            shelter: 0,
            shelter_reduction: 10,
            shelter_good: Some(3),
        }],
    });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 2,
            units: 1,
        }])
        .unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    assert_eq!(first.regions[0].people[0].health, 90);
    assert_eq!(
        first.production.unwrap().inventories[0].stocks[2].quantity,
        1
    );
    kernel.step().unwrap();
    let second = kernel.export();
    assert_eq!(second.regions[0].people[0].health, 90);
    assert_eq!(second.regions[0].people[1].health, 80);
}

#[test]
fn accepted_invocation_strain_injures_and_dismissal_stops_the_cost() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/invocation_strain.toml"
    ))
    .unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = 69;
    let mut kernel = Kernel::compile(world).unwrap();
    for _ in 0..7 {
        kernel.step().unwrap();
    }
    kernel.invoke_lives(&[1]).unwrap();
    kernel.step().unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    let warning = kernel
        .export()
        .lineage
        .unwrap()
        .invocation
        .unwrap()
        .pending
        .unwrap();
    assert_eq!(warning.strain_damage_per_tick, 5);
    assert_eq!(kernel.export().regions, before.regions);
    kernel.accept_invocation_warning(warning.revision).unwrap();
    kernel.step().unwrap();
    let strained = kernel.export();
    let current = strained.lineage.as_ref().unwrap().current.unwrap();
    let health = strained
        .regions
        .iter()
        .flat_map(|region| &region.people)
        .find(|person| person.id == current)
        .unwrap()
        .health;
    assert_eq!(health, 95);
    kernel.dismiss_invocation().unwrap();
    kernel.step().unwrap();
    let after = kernel.export();
    assert_eq!(
        after
            .regions
            .iter()
            .flat_map(|region| &region.people)
            .find(|person| person.id == current)
            .unwrap()
            .health,
        96
    );
}
#[test]
fn failed_lifecycle_rolls_back_accepted_invocation_injury() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/invocation_strain.toml"
    ))
    .unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = 69;
    let mut kernel = Kernel::compile(world).unwrap();
    for _ in 0..7 {
        kernel.step().unwrap();
    }
    kernel.invoke_lives(&[1]).unwrap();
    kernel.step().unwrap();
    assert!(kernel.step().is_err());
    let warning = kernel
        .export()
        .lineage
        .unwrap()
        .invocation
        .unwrap()
        .pending
        .unwrap();
    kernel.accept_invocation_warning(warning.revision).unwrap();
    let mut world = kernel.export();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.alive && member.born.is_none())
        .unwrap()
        .age = u64::MAX;
    kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn protracted_invocation_warns_before_permanent_lifespan_cost() {
    use game_parallel::InvocationWarningKind;
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/invocation_protracted.toml"
    ))
    .unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = 69;
    let mut kernel = Kernel::compile(world).unwrap();
    for _ in 0..7 {
        kernel.step().unwrap();
    }
    kernel.invoke_lives(&[1]).unwrap();
    kernel.step().unwrap();
    assert!(kernel.step().is_err());
    let strain = kernel
        .export()
        .lineage
        .unwrap()
        .invocation
        .unwrap()
        .pending
        .unwrap();
    assert_eq!(strain.kind, InvocationWarningKind::Strain);
    kernel.accept_invocation_warning(strain.revision).unwrap();
    kernel.step().unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    let paused = kernel.export();
    let warning = paused
        .lineage
        .as_ref()
        .unwrap()
        .invocation
        .as_ref()
        .unwrap()
        .pending
        .as_ref()
        .unwrap();
    assert_eq!(warning.kind, InvocationWarningKind::Lifespan);
    assert_eq!(warning.lifespan_loss_per_tick, 1);
    assert_eq!(paused.regions, before.regions);
    assert_eq!(paused.families, before.families);
    let revision = warning.revision;
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&paused).unwrap()).unwrap())
        .unwrap();
    kernel.accept_invocation_warning(revision).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    let current = state.lineage.as_ref().unwrap().current.unwrap();
    assert_eq!(
        state
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|member| member.id == current)
            .unwrap()
            .lifespan_loss,
        1
    );
    kernel.dismiss_invocation().unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert_eq!(
        state
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|member| member.id == current)
            .unwrap()
            .lifespan_loss,
        1
    );
}

#[test]
fn accepted_control_loss_allows_uncapped_lifespan_consequences() {
    use game_parallel::InvocationWarningKind;
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/invocation_protracted.toml"
    ))
    .unwrap();
    world
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = 69;
    let rules = &mut world
        .lineage
        .as_mut()
        .unwrap()
        .invocation
        .as_mut()
        .unwrap()
        .rules;
    rules.permanent_exposure_threshold = 5;
    rules.lifespan_loss_per_tick = 100;
    let mut kernel = Kernel::compile(world).unwrap();
    for _ in 0..7 {
        kernel.step().unwrap();
    }
    kernel.invoke_lives(&[1]).unwrap();
    kernel.step().unwrap();
    assert!(kernel.step().is_err());
    let strain = kernel
        .export()
        .lineage
        .unwrap()
        .invocation
        .unwrap()
        .pending
        .unwrap();
    assert_eq!(strain.kind, InvocationWarningKind::Strain);
    kernel.accept_invocation_warning(strain.revision).unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    assert!(kernel.step().is_err());
    let warning = kernel
        .export()
        .lineage
        .unwrap()
        .invocation
        .unwrap()
        .pending
        .unwrap();
    assert_eq!(warning.kind, InvocationWarningKind::ControlLoss);
    assert_eq!(warning.lifespan_loss_per_tick, 100);
    kernel.accept_invocation_warning(warning.revision).unwrap();
    kernel.step().unwrap();
    let current = kernel.export().lineage.unwrap().current.unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    state.validate().unwrap();
    let member = state
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .find(|member| member.id == current)
        .unwrap();
    assert_eq!(member.lifespan_loss, 100);
    assert!(!member.alive);
    assert!(state.lineage.unwrap().current.is_none());
}

#[path = "fatigue.rs"]
mod fatigue;

#[path = "dispositions.rs"]
mod dispositions;
#[path = "skills.rs"]
mod skills;

#[path = "lifecycle_columns.rs"]
mod lifecycle_columns;
#[path = "packed_combat.rs"]
mod packed_combat;
#[path = "tactics.rs"]
mod tactics;

#[path = "social_needs.rs"]
mod social_needs;

#[path = "enrollment.rs"]
mod enrollment;

#[path = "disease.rs"]
mod disease;

#[path = "agreement_bulk.rs"]
mod agreement_bulk;
#[path = "agreements.rs"]
mod agreements;
#[path = "cooperation.rs"]
mod cooperation;
#[path = "food.rs"]
mod food;
#[path = "goals.rs"]
mod goals;
#[path = "knowledge_columns.rs"]
mod knowledge_columns;
#[path = "social_edges.rs"]
mod social_edges;
#[path = "work_requests.rs"]
mod work_requests;

#[path = "purpose.rs"]
mod purpose;

#[path = "caregiving.rs"]
mod caregiving;

#[path = "autonomy_columns.rs"]
mod autonomy_columns;
#[path = "dreams.rs"]
mod dreams;
#[path = "missions.rs"]
mod missions;
#[path = "work_age.rs"]
mod work_age;

#[path = "personality.rs"]
mod personality;

#[path = "mission_parties.rs"]
mod mission_parties;

#[path = "goal_planning.rs"]
mod goal_planning;

#[path = "seasons.rs"]
mod seasons;

#[path = "trust_reports.rs"]
mod trust_reports;

#[path = "campaign.rs"]
mod campaign;

#[path = "mission_routes.rs"]
mod mission_routes;

#[path = "conversation.rs"]
mod conversation;

#[path = "theft.rs"]
mod theft;

#[path = "estates.rs"]
mod estates;

#[path = "action_reservations.rs"]
mod action_reservations;
#[path = "mission_objectives.rs"]
mod mission_objectives;

#[path = "farm_volunteers.rs"]
mod farm_volunteers;

#[path = "family_choice.rs"]
mod family_choice;

#[path = "diet.rs"]
mod diet;
