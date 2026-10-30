use game_parallel::{
    CombatCommand, CombatEncounter, CombatFighter, CombatOrder, Conflict, Person, World,
};
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    world.rules.hunger_per_tick = 0;
    world.rules.recovery = 0;
    world.regions[0].people = [
        (10, 100),
        (50, 5),
        (100, 15),
        (1000, 100),
        (2000, 100),
        (3000, 100),
    ]
    .into_iter()
    .map(|(id, health)| Person {
        id,
        health,
        hunger: 0,
    })
    .collect();
    world.regions[1].people.clear();
    world.conflict = Some(Conflict {
        damage: 10,
        next_id: 2,
        events: vec![],
        encounters: vec![CombatEncounter {
            id: 1,
            region: world.regions[0].id,
            fighters: [
                (10, 1, 50),
                (50, 2, 10),
                (100, 2, 10),
                (1000, 1, 100),
                (2000, 2, 10),
                (3000, 1, 2000),
            ]
            .into_iter()
            .map(|(person, side, target)| CombatFighter {
                person,
                side,
                order: CombatOrder::Attack { target },
            })
            .collect(),
        }],
    });
    world
}

fn health(world: &World, id: u64) -> u32 {
    world
        .regions
        .iter()
        .flat_map(|region| &region.people)
        .find(|p| p.id == id)
        .unwrap()
        .health
}

#[test]
fn overlay_orders_keep_failed_commands_and_old_receipts_atomic() {
    let mut initial = world();
    for person in &mut initial.regions[0].people {
        person.health = 1000;
    }
    initial.rules.max_health = 1000;
    let mut kernel = Kernel::compile(initial).unwrap();
    kernel.step().unwrap();
    let old = kernel.export();
    let draft = kernel.prepare().unwrap();
    assert!(
        kernel
            .combat_orders(&[CombatCommand {
                encounter: 1,
                person: 10,
                order: CombatOrder::Attack { target: 1000 }
            }])
            .is_err()
    );
    assert_eq!(kernel.export(), old);
    kernel.commit(draft).unwrap();
    for command in [
        CombatOrder::Attack { target: 2000 },
        CombatOrder::Defend,
        CombatOrder::Attack { target: 50 },
    ] {
        let stale = kernel.prepare().unwrap();
        let receipts = kernel.export().conflict.unwrap().events;
        kernel
            .combat_orders(&[CombatCommand {
                encounter: 1,
                person: 10,
                order: command,
            }])
            .unwrap();
        assert!(kernel.commit(stale).is_err());
        assert_eq!(kernel.export().conflict.unwrap().events, receipts);
        let mut rebuilt = Kernel::compile(kernel.export()).unwrap();
        kernel.step().unwrap();
        rebuilt.step().unwrap();
        assert_eq!(kernel.export(), rebuilt.export());
    }
}

#[test]
fn filtered_casualty_generations_preserve_cascading_defense_and_reload() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    assert_eq!(health(&first, 10), 70);
    assert_eq!(health(&first, 50), 0);
    let old_receipts = first.conflict.as_ref().unwrap().events.clone();
    // Preparing and discarding a new generation cannot overwrite prior receipts.
    drop(kernel.prepare().unwrap());
    assert_eq!(kernel.export(), first);
    let saved = serde_json::to_string(&first).unwrap();
    let mut reloaded = Kernel::compile(World::from_json(&saved).unwrap()).unwrap();
    for expected in [60, 55] {
        kernel.step().unwrap();
        reloaded.step().unwrap();
        let current = kernel.export();
        current.validate().unwrap();
        assert_eq!(health(&current, 10), expected);
        assert_eq!(current, reloaded.export());
    }
    assert!(
        old_receipts
            .iter()
            .filter(|e| e.target == 10)
            .all(|e| e.damage == 10)
    );
    assert!(
        kernel
            .export()
            .conflict
            .unwrap()
            .events
            .iter()
            .filter(|e| e.target == 10)
            .all(|e| e.damage == 5)
    );
}

#[test]
fn combat_commands_invalidate_plans_but_preserve_published_receipts() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let old_receipts = kernel.export().conflict.unwrap().events;
    let stale = kernel.prepare().unwrap();
    kernel
        .combat_orders(&[CombatCommand {
            encounter: 1,
            person: 2000,
            order: CombatOrder::Defend,
        }])
        .unwrap();
    assert_eq!(kernel.export().conflict.unwrap().events, old_receipts);
    let before = kernel.export();
    assert!(kernel.commit(stale).is_err());
    assert_eq!(kernel.export(), before);
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(health(&result, 2000), 85);
    assert!(
        !result
            .conflict
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.attacker == 2000)
    );
    assert!(
        result
            .conflict
            .unwrap()
            .events
            .iter()
            .filter(|e| e.target == 2000)
            .all(|e| e.damage == 5)
    );
}

#[test]
fn unrelated_arrivals_do_not_leave_stale_combat_block_locations() {
    let mut world = world();
    world.regions[1].people.push(Person {
        id: 42,
        health: 100,
        hunger: 0,
    });
    world.routes = vec![game_parallel::Route {
        source: world.regions[1].id,
        destination: world.regions[0].id,
        ticks: 1,
    }];
    let destination = world.regions[0].id;
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .move_people(&[game_parallel::Move {
            person: 42,
            destination,
        }])
        .unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    result.validate().unwrap();
    assert_eq!(health(&result, 42), 100);
    assert_eq!(health(&result, 10), 60);
}

#[test]
fn sparse_multiple_encounters_and_discarded_drafts_are_worker_invariant() {
    let mut world = world();
    world.regions[1].people = vec![
        Person {
            id: 9000,
            health: 100,
            hunger: 0,
        },
        Person {
            id: 90_000,
            health: 100,
            hunger: 0,
        },
    ];
    let conflict = world.conflict.as_mut().unwrap();
    conflict.next_id = 3;
    conflict.encounters.push(CombatEncounter {
        id: 2,
        region: world.regions[1].id,
        fighters: vec![
            CombatFighter {
                person: 9000,
                side: 1,
                order: CombatOrder::Attack { target: 90_000 },
            },
            CombatFighter {
                person: 90_000,
                side: 2,
                order: CombatOrder::Defend,
            },
        ],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                for _ in 0..4 {
                    let first = kernel.prepare().unwrap();
                    let second = kernel.prepare().unwrap();
                    assert!(kernel.commit(first).is_err());
                    kernel.commit(second).unwrap();
                }
                kernel.export()
            })
    };
    let serial = run(1);
    assert_eq!(serial, run(4));
    assert_eq!(health(&serial, 90_000), 80);
    serial.validate().unwrap();
}
