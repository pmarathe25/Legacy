use game_economy::core_interval::{CoreHost, CoreRunEvent, MechanicalExperiment};
use game_economy_player::interval::Player;
use std::path::Path;

const TRIAL: MechanicalExperiment = MechanicalExperiment {
    actor: 1,
    apparatus: 1,
    output_part: 1002,
    input_force: 100,
    input_distance: 100,
    load_force: 20,
};

fn scene(quantum: u64) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text =
        std::fs::read_to_string(root.join("content/scenarios/economy-empirical-mechanics.toml"))
            .unwrap()
            .replace("tick_millis = 900000", &format!("tick_millis = {quantum}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-empirical-{}-{quantum}.toml",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

#[test]
fn a_completed_chosen_reflection_concludes_only_from_observed_comparable_trials() {
    let run = |workers, quantum, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene(quantum);
                let first = host.experiment_controlled_apparatus(TRIAL).unwrap();
                assert!(first.travel > 0);
                assert_eq!(host.advance_continuous(2000, &[]).unwrap().failure, None);
                let second = host
                    .experiment_controlled_apparatus(MechanicalExperiment {
                        load_force: 500,
                        ..TRIAL
                    })
                    .unwrap();
                assert_eq!(second.travel, 0);
                assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
                assert!(
                    host.mechanical_comparisons_controlled(1)
                        .unwrap()
                        .is_empty()
                );

                assert!(Player::new(&mut host, 1).unwrap().start_pastime(1).unwrap());
                assert_eq!(host.advance_continuous(1000, &[]).unwrap().failure, None);
                assert!(Player::new(&mut host, 1).unwrap().stop_pastime().unwrap());
                assert!(
                    host.mechanical_comparisons_controlled(1)
                        .unwrap()
                        .is_empty()
                );
                assert!(Player::new(&mut host, 1).unwrap().start_pastime(1).unwrap());

                let mut conclusions = Vec::new();
                for &chunk in chunks {
                    let advance = host.advance_continuous(chunk, &[]).unwrap();
                    assert_eq!(advance.failure, None);
                    conclusions.extend(
                        advance
                            .events
                            .into_iter()
                            .filter_map(|event| match event {
                                CoreRunEvent::MechanicalConclusions(rows) => Some(rows),
                                _ => None,
                            })
                            .flatten(),
                    );
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                assert_eq!(
                    host.mechanical_comparisons_controlled(1).unwrap(),
                    vec![conclusions[0].evidence]
                );
                conclusions
            })
    };
    let single = run(1, 900000, &[60001]);
    assert_eq!(single, run(6, 29009, &[733, 10117, 49151]));
    assert_eq!(single.len(), 1);
    let conclusion = single[0];
    assert_eq!((conclusion.person, conclusion.at_millis), (1, 64000));
    assert_eq!(
        (
            conclusion.evidence.largest_load_moved,
            conclusion.evidence.smallest_load_stalled
        ),
        (20, 500)
    );
    assert_eq!(
        (
            conclusion.evidence.first_evidence_millis,
            conclusion.evidence.latest_evidence_millis
        ),
        (0, 2000)
    );
}
