use game_economy::core_interval::CoreHost;
use game_economy_agriculture::{Phase, soil::FieldSoil};
use std::path::Path;

#[test]
fn only_elapsed_fallow_time_recovers_fertility_across_quanta_and_save_reload() {
    let run = |workers, quantum, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
                let authored = std::fs::read_to_string(
                    root.join("content/scenarios/economy-cultivation-soil.toml"),
                )
                .unwrap();
                let authored = authored
                    .replace("tick_millis = 900000", &format!("tick_millis = {quantum}"))
                    .replace(
                        "recovery_per_fallow = 10",
                        "recovery_per_fallow = 10\nrecovery_clock = { period_millis = 100 }",
                    );
                let scene = std::env::temp_dir().join(format!(
                    "legacy-fallow-recovery-{}-{quantum}.toml",
                    std::process::id()
                ));
                std::fs::write(&scene, authored).unwrap();
                let loaded = game_economy_cli::core_scenario(&scene, &root.join("content"));
                std::fs::remove_file(scene).unwrap();
                let mut state = loaded.unwrap().snapshot();
                let farm = state.agriculture.as_mut().unwrap();
                farm.requests = None;
                let mut growing = farm.fields[0].clone();
                growing.id = 2;
                growing.phase = Phase::Growing;
                growing.remaining = 3;
                farm.fields.push(growing);
                farm.soil.as_mut().unwrap().fields.push(FieldSoil {
                    field: 2,
                    fertility: 50,
                });
                let mut host = CoreHost::resume(state).unwrap();
                for &millis in steps {
                    assert_eq!(host.advance_continuous(millis, &[]).unwrap().failure, None);
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let soil = host.snapshot().agriculture.unwrap().soil.unwrap();
                assert_eq!(soil.fields[0].fertility, 84);
                assert_eq!(soil.fields[1].fertility, 50);
                assert_eq!(soil.recovery_clock.as_ref().unwrap().remainders, [(1, 70)]);
                assert_eq!(host.advance_continuous(155, &[]).unwrap().failure, None);
                let saturated = host.snapshot().agriculture.unwrap().soil.unwrap();
                assert_eq!(saturated.fields[0].fertility, 100);
                assert_eq!(saturated.fields[1].fertility, 50);
                assert!(saturated.recovery_clock.unwrap().remainders.is_empty());
                soil
            })
    };
    assert_eq!(run(1, 950, &[347]), run(6, 160, &[17, 149, 181]));
}
