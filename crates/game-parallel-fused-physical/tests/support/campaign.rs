use game_parallel::{MissionPhase, MissionStart, World};
use game_parallel_fused_physical::Kernel;
#[test]
fn composed_campaign_runs_through_births_work_and_a_party_across_reload() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/campaign.toml")).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let player = kernel
                    .start_mission(MissionStart {
                        person: 1,
                        destination: 2,
                        visit_ticks: 2,
                    })
                    .unwrap();
                kernel.set_mission_party_consent(1, player, true).unwrap();
                kernel.form_mission_party(&[1, player]).unwrap();
                for tick in 1..=20 {
                    kernel.step().unwrap();
                    if tick == 10 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    result.validate().unwrap();
    assert!(result.families.as_ref().unwrap().totals.births > 0);
    assert!(
        result
            .goals
            .as_ref()
            .unwrap()
            .people
            .iter()
            .any(|goal| goal.person == 2 && goal.progress > 0)
    );
    assert!(result.institutions.is_none());
    assert!(
        result
            .missions
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .all(|mission| mission.phase == MissionPhase::Completed)
    );
    assert!(
        result
            .knowledge
            .as_ref()
            .unwrap()
            .minds
            .iter()
            .find(|mind| mind.person == 1)
            .unwrap()
            .records
            .iter()
            .any(|record| record.region == 2 && record.witness == 1)
    );
    assert!(
        result
            .personality
            .as_ref()
            .unwrap()
            .people
            .iter()
            .any(|person| person.person >= 9)
    );
}

#[test]
fn parallel_runtime_rejects_preset_institution_bodies() {
    let world = World::from_toml(include_str!(
        "../../../../content/parallel/institutions.toml"
    ))
    .unwrap();
    let error = match Kernel::compile(world) {
        Ok(_) => panic!("preset institutions must be rejected"),
        Err(error) => error,
    };
    assert!(error.contains("preset institutions"));
}
