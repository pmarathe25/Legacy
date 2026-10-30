use game_parallel::*;
use game_parallel_fused_physical::Kernel;

fn fixture() -> (Kernel, CombatEncounter, TacticalBattle) {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    world.rules.hunger_per_tick = 0;
    world.rules.recovery = 0;
    world.regions[0].people = vec![
        Person {
            id: 1,
            health: 100,
            hunger: 0,
        },
        Person {
            id: 2,
            health: 100,
            hunger: 0,
        },
    ];
    world.regions[1].people.clear();
    let region = world.regions[0].id;
    world.conflict = Some(Conflict {
        damage: 10,
        next_id: 1,
        encounters: vec![],
        events: vec![],
    });
    world.tactics = Some(Tactics {
        maps: vec![
            TacticalMap::generate(
                region,
                1,
                TacticalRules {
                    width: 20,
                    height: 20,
                    obstacle_count: 0,
                    max_obstacle_size: 1,
                    movement: 10,
                    attack_range: 2,
                },
            )
            .unwrap(),
        ],
        battles: vec![],
    });
    let fight = CombatEncounter {
        id: 1,
        region,
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
    let battle = TacticalBattle {
        encounter: 1,
        region,
        people: vec![
            TacticalPlacement {
                person: 1,
                position: TacticalPoint { x: 1, y: 1 },
                last_move: None,
            },
            TacticalPlacement {
                person: 2,
                position: TacticalPoint { x: 10, y: 1 },
                last_move: None,
            },
        ],
    };
    (Kernel::compile(world).unwrap(), fight, battle)
}
fn movement(person: u64, x: u32) -> TacticalMove {
    TacticalMove {
        encounter: 1,
        person,
        destination: TacticalPoint { x, y: 1 },
    }
}
#[test]
fn sparse_casualties_preserve_cached_geometry_for_sorted_and_unsorted_placements() {
    for reverse in [false, true] {
        let (kernel, mut fight, mut battle) = fixture();
        let mut world = kernel.export();
        world.regions[0].people[1].health = 5;
        world.regions[0].people.push(Person {
            id: 3,
            health: 100,
            hunger: 0,
        });
        fight.fighters.push(CombatFighter {
            person: 3,
            side: 2,
            order: CombatOrder::Attack { target: 1 },
        });
        battle.people[1].position.x = 2;
        battle.people.push(TacticalPlacement {
            person: 3,
            position: TacticalPoint { x: 3, y: 1 },
            last_move: None,
        });
        if reverse {
            battle.people.reverse();
        }
        let mut kernel = Kernel::compile(world).unwrap();
        kernel.begin_tactical_combat(fight, battle).unwrap();
        kernel.step().unwrap();
        let first = kernel.export();
        first.validate().unwrap();
        assert_eq!(first.tactics.as_ref().unwrap().battles[0].people.len(), 2);
        let mut reload = Kernel::compile(first).unwrap();
        for _ in 0..2 {
            kernel.step().unwrap();
            reload.step().unwrap();
            assert_eq!(kernel.export(), reload.export());
        }
    }
}

#[test]
fn encounter_placements_and_movement_are_atomic_and_budgeted() {
    let (mut kernel, fight, battle) = fixture();
    let initial = kernel.export();
    assert!(kernel.begin_combat(fight.clone()).is_err());
    let mut bad = battle.clone();
    bad.people[1].position = bad.people[0].position;
    assert!(kernel.begin_tactical_combat(fight.clone(), bad).is_err());
    assert_eq!(initial, kernel.export());
    kernel.begin_tactical_combat(fight, battle).unwrap();
    let before = kernel.export();
    assert!(
        kernel
            .tactical_moves(&[movement(1, 2), movement(2, 2)])
            .is_err()
    );
    assert_eq!(before, kernel.export());
    let draft = kernel.prepare().unwrap();
    kernel.tactical_moves(&[movement(1, 9)]).unwrap();
    let moved = kernel.export();
    assert!(kernel.commit(draft).is_err());
    assert!(kernel.tactical_moves(&[movement(1, 8)]).is_err());
    assert_eq!(moved, kernel.export());
    kernel.step().unwrap();
    assert_eq!(kernel.export().conflict.unwrap().events.len(), 2);
    let receipts = kernel.export().conflict.unwrap().events;
    kernel.tactical_moves(&[movement(1, 8)]).unwrap();
    assert_eq!(kernel.export().conflict.unwrap().events, receipts);
    kernel.export().validate().unwrap();
}

#[test]
fn geometry_cache_reload_and_membership_cleanup_preserve_sites_and_receipts() {
    let (mut kernel, fight, battle) = fixture();
    kernel.begin_tactical_combat(fight, battle).unwrap();
    kernel.step().unwrap();
    assert!(kernel.export().conflict.unwrap().events.is_empty());
    kernel.tactical_moves(&[movement(1, 9)]).unwrap();
    drop(kernel.prepare().unwrap());
    assert!(kernel.export().conflict.unwrap().events.is_empty());
    let mut reload = Kernel::compile(
        World::from_json(&serde_json::to_string(&kernel.export()).unwrap()).unwrap(),
    )
    .unwrap();
    kernel.step().unwrap();
    reload.step().unwrap();
    assert_eq!(kernel.export(), reload.export());
    kernel
        .combat_orders(&[CombatCommand {
            encounter: 1,
            person: 1,
            order: CombatOrder::Retreat,
        }])
        .unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    result.validate().unwrap();
    assert!(result.conflict.as_ref().unwrap().encounters.is_empty());
    assert!(result.tactics.as_ref().unwrap().battles.is_empty());
    assert_eq!(result.tactics.as_ref().unwrap().maps.len(), 1);
    Kernel::compile(World::from_json(&serde_json::to_string(&result).unwrap()).unwrap()).unwrap();
}
