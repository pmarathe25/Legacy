use game_parallel::{MissionEventKind, MissionPhase, MissionStart, World};
use game_parallel_fused_physical::Kernel;

fn scenario(willing: bool) -> World {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/mission_start.toml"
    ))
    .unwrap();
    world.rules.food_growth = 100;
    let mut social = World::from_toml(include_str!("../../../../content/parallel/autonomy.toml"))
        .unwrap()
        .social
        .unwrap();
    social
        .participants
        .iter_mut()
        .find(|p| p.person == 3)
        .unwrap()
        .willing = willing;
    world.social = Some(social);
    world
}

fn expedition(willing: bool, workers: usize) -> World {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut kernel = Kernel::compile(scenario(willing)).unwrap();
            let id = kernel
                .start_mission(MissionStart {
                    person: 1,
                    destination: 2,
                    visit_ticks: 2,
                })
                .unwrap();
            let before = kernel.export();
            assert!(kernel.set_mission_meeting(1, id, 999).is_err());
            assert_eq!(kernel.export(), before);
            kernel.set_mission_meeting(1, id, 3).unwrap();
            let mut saw_meeting = false;
            let mut saw_failure = false;
            for tick in 0..16 {
                kernel.step().unwrap();
                let state = kernel.export();
                let missions = state.missions.as_ref().unwrap();
                saw_meeting |= missions
                    .events
                    .iter()
                    .any(|e| e.kind == MissionEventKind::ObjectiveMet);
                saw_failure |= missions
                    .events
                    .iter()
                    .any(|e| e.kind == MissionEventKind::ObjectiveFailed);
                if tick == 4 {
                    kernel = Kernel::compile(
                        World::from_json(&serde_json::to_string(&state).unwrap()).unwrap(),
                    )
                    .unwrap();
                }
                if matches!(
                    missions.entries[0].phase,
                    MissionPhase::Completed | MissionPhase::Failed
                ) {
                    break;
                }
            }
            let state = kernel.export();
            state.validate().unwrap();
            let mission = &state.missions.as_ref().unwrap().entries[0];
            assert_eq!(mission.objective_met, willing);
            assert_eq!(
                mission.phase,
                if willing {
                    MissionPhase::Completed
                } else {
                    MissionPhase::Failed
                }
            );
            assert_eq!(saw_meeting, willing);
            assert_eq!(saw_failure, !willing);
            assert!(state.regions[0].people.iter().any(|p| p.id == 1));
            state
        })
}

#[test]
fn meeting_objective_depends_on_real_consent_and_survives_reload() {
    for willing in [false, true] {
        assert_eq!(expedition(willing, 1), expedition(willing, 4));
    }
}
