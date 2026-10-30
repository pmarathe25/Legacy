use game_parallel::{MissionPhase, MissionStart, Region, Route, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/mission_start.toml"
    ))
    .unwrap();
    state.rules.hunger_per_tick = 0;
    for id in 3..=5 {
        state.regions.push(Region {
            id,
            food: 10,
            people: vec![],
        });
    }
    state.routes = [(1, 2), (2, 4), (1, 5), (5, 4), (4, 3), (3, 1)]
        .into_iter()
        .map(|(source, destination)| Route {
            source,
            destination,
            ticks: 1,
        })
        .collect();
    state
}
#[test]
fn party_follows_deterministic_shortest_routes_there_and_back_through_real_regions() {
    let state = world();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let first = kernel
                    .start_mission(MissionStart {
                        person: 1,
                        destination: 4,
                        visit_ticks: 1,
                    })
                    .unwrap();
                let second = kernel
                    .start_mission(MissionStart {
                        person: 2,
                        destination: 4,
                        visit_ticks: 1,
                    })
                    .unwrap();
                kernel.set_mission_party_consent(1, first, true).unwrap();
                kernel.set_mission_party_consent(2, second, true).unwrap();
                kernel.form_mission_party(&[first, second]).unwrap();
                let mut legs = Vec::new();
                for tick in 1..=15 {
                    kernel.step().unwrap();
                    let snapshot = kernel.export();
                    if let Some(row) = snapshot.travelers.iter().find(|row| row.person.id == 1) {
                        let leg = (row.source, row.destination);
                        if legs.last() != Some(&leg) {
                            legs.push(leg);
                        }
                        assert!(snapshot.travelers.iter().any(|other| other.person.id == 2
                            && (other.source, other.destination) == leg));
                    }
                    if tick == 4 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&snapshot).unwrap()).unwrap(),
                        )
                        .unwrap();
                    }
                }
                (legs, kernel.export())
            })
    };
    let (legs, result) = run(1);
    assert_eq!(legs, vec![(1, 2), (2, 4), (4, 3), (3, 1)]);
    assert_eq!((legs, result.clone()), run(4));
    assert!(
        result
            .missions
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .all(|row| row.phase == MissionPhase::Completed)
    );
    result.validate().unwrap();
}
#[test]
fn missing_return_path_is_rejected_without_mutation() {
    let mut state = world();
    state.routes.retain(|route| route.source != 4);
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(
        kernel
            .start_mission(MissionStart {
                person: 1,
                destination: 4,
                visit_ticks: 1
            })
            .is_err()
    );
    assert_eq!(kernel.export(), before);
}

#[test]
fn failed_generation_does_not_publish_a_sparse_visit_phase_edit() {
    use game_parallel::{Mission, Missions};
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let person = state.regions[0].people.remove(0);
    state.regions[1].people.push(person);
    state.missions = Some(Missions {
        next_id: 2,
        pressure_threshold: None,
        events: vec![],
        entries: vec![Mission {
            id: 1,
            person: 1,
            party: None,
            party_opt_in: false,
            party_compliant: false,
            objective: None,
            objective_met: false,
            origin: 1,
            destination: 2,
            visit_ticks: 1,
            remaining: 1,
            phase: MissionPhase::Visiting,
        }],
    });
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
