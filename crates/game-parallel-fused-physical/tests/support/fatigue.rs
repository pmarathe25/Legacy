use game_parallel::{StartWork, World};
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/fatigue.toml")).unwrap()
}
#[test]
fn tired_worker_pauses_until_recovered_and_resumes_after_reload() {
    let mut world = world();
    world.production.as_mut().unwrap().recipes[0].ticks = 6;
    world.rules.recovery = 0;
    let mut kernel = Kernel::compile(world).unwrap();
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let exhausted = kernel.export();
    assert_eq!(exhausted.production.as_ref().unwrap().jobs[0].remaining, 4);
    assert_eq!(exhausted.fatigue.as_ref().unwrap().people[0].fatigue, 6);
    assert!(exhausted.fatigue.as_ref().unwrap().people[0].resting);
    assert_eq!(exhausted.regions[0].people[0].health, 98);
    let json = serde_json::to_string(&exhausted).unwrap();
    let mut restored = Kernel::compile(World::from_json(&json).unwrap()).unwrap();
    for _ in 0..3 {
        kernel.step().unwrap();
        restored.step().unwrap();
    }
    let resumed = kernel.export();
    assert_eq!(resumed, restored.export());
    assert_eq!(resumed.production.unwrap().jobs[0].remaining, 3);
}
#[test]
fn fatigue_is_rolled_back_when_later_lifecycle_preparation_fails() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    world.fatigue = world_fixture_fatigue();
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
fn world_fixture_fatigue() -> Option<game_parallel::Fatigue> {
    world().fatigue
}

#[test]
fn fatigue_and_skill_progress_are_worker_invariant_across_reload() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/skills.toml")).unwrap();
    state.fatigue = world_fixture_fatigue();
    state.regions[0].people = (1..=5000)
        .map(|id| game_parallel::Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.rules.food_capacity = 100_000;
    state.rules.food_growth = 20_000;
    state.regions[0].food = 100_000;
    for stock in &mut state.production.as_mut().unwrap().inventories[0].stocks {
        stock.capacity = 100_000;
        stock.quantity = if stock.good == 1 { 100_000 } else { 0 };
    }
    state.autowork.as_mut().unwrap().policies = (1..=5000)
        .map(|person| game_parallel::WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=12 {
                    kernel.step().unwrap();
                    if tick == 6 {
                        let json = serde_json::to_string(&kernel.export()).unwrap();
                        kernel = Kernel::compile(World::from_json(&json).unwrap()).unwrap();
                    }
                }
                kernel.export()
            })
    };
    let serial = run(1);
    let parallel = run(4);
    assert_eq!(serial, parallel);
    parallel.validate().unwrap();
    assert_eq!(parallel.fatigue.unwrap().people.len(), 5000);
}

#[test]
fn fighting_prevents_rest_and_continues_to_cause_fatigue() {
    use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, FatiguePerson};
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/conflict.toml")).unwrap();
    let mut fatigue = world_fixture_fatigue().unwrap();
    fatigue.people.push(FatiguePerson {
        id: 1,
        fatigue: 6,
        resting: true,
    });
    state.fatigue = Some(fatigue);
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .begin_combat(CombatEncounter {
            id: 1,
            region: 1,
            fighters: vec![
                CombatFighter {
                    person: 1,
                    side: 1,
                    order: CombatOrder::Defend,
                },
                CombatFighter {
                    person: 2,
                    side: 2,
                    order: CombatOrder::Defend,
                },
            ],
        })
        .unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    let tired = result
        .fatigue
        .unwrap()
        .people
        .into_iter()
        .find(|person| person.id == 1)
        .unwrap();
    assert_eq!(tired.fatigue, 9);
    assert!(tired.resting);
}

#[test]
fn exhausted_traveler_cannot_recover_until_arrival() {
    use game_parallel::{FatiguePerson, Move};
    let mut state = world();
    state.rules.recovery = 0;
    state.fatigue.as_mut().unwrap().people.push(FatiguePerson {
        id: 1,
        fatigue: 6,
        resting: true,
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().fatigue.unwrap().people[0].fatigue, 6);
    kernel.step().unwrap();
    assert_eq!(kernel.export().fatigue.unwrap().people[0].fatigue, 6);
    kernel.step().unwrap();
    assert_eq!(kernel.export().fatigue.unwrap().people[0].fatigue, 4);
}

#[test]
fn initial_sparse_rows_remain_exact_until_a_tick_normalizes_them() {
    use game_parallel::FatiguePerson;
    let mut state = world();
    state.fatigue.as_mut().unwrap().people = vec![
        FatiguePerson {
            id: 2,
            fatigue: 3,
            resting: false,
        },
        FatiguePerson {
            id: 1,
            fatigue: 0,
            resting: false,
        },
    ];
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    assert_eq!(kernel.export(), state);
    kernel.step().unwrap();
    let rows = kernel.export().fatigue.unwrap().people;
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].id, rows[0].fatigue), (2, 2));
}
#[test]
fn split_travel_preserves_each_persons_fatigue_columns() {
    use game_parallel::{FatiguePerson, Move};
    let mut state = world();
    state.fatigue.as_mut().unwrap().people = vec![
        FatiguePerson {
            id: 1,
            fatigue: 4,
            resting: false,
        },
        FatiguePerson {
            id: 2,
            fatigue: 5,
            resting: false,
        },
    ];
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    kernel.step().unwrap();
    let rows = kernel.export().fatigue.unwrap().people;
    assert_eq!(
        rows.iter().map(|p| (p.id, p.fatigue)).collect::<Vec<_>>(),
        vec![(1, 4), (2, 4)]
    );
    let json = serde_json::to_string(&kernel.export()).unwrap();
    let mut restored = Kernel::compile(World::from_json(&json).unwrap()).unwrap();
    kernel.step().unwrap();
    restored.step().unwrap();
    assert_eq!(kernel.export(), restored.export());
}
#[test]
fn newborn_blocks_get_fatigue_columns_before_the_next_generation() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut fatigue = world_fixture_fatigue().unwrap();
    fatigue.rules.baseline_gain = 1;
    fatigue.rules.idle_recovery = 0;
    state.fatigue = Some(fatigue);
    let initial = state.families.as_ref().unwrap().next_id;
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..10 {
        kernel.step().unwrap();
    }
    let result = kernel.export();
    result.validate().unwrap();
    assert!(
        result
            .fatigue
            .unwrap()
            .people
            .iter()
            .any(|person| person.id >= initial && person.fatigue > 0)
    );
}
