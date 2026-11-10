use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_scenario(workers: usize) -> serde_json::Value {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let root = root();
            let mut host = game_economy_cli::core_scenario(
                &root.join("content/scenarios/economy-core.toml"),
                &root.join("content"),
            )
            .unwrap();
            assert_eq!(
                host.advance_continuous(7_200_000, &[]).unwrap().failure,
                None
            );
            let snapshot = host.snapshot();
            assert_eq!(snapshot.core.people.len(), 3);
            assert_eq!(snapshot.family.as_ref().unwrap().totals.births, 1);
            assert!(
                snapshot
                    .dependent_care
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .any(|event| event.child == 3 && event.caregiver == Some(2))
            );
            assert!(snapshot.gathering.as_ref().unwrap().nodes[0].available < 20);
            assert_eq!(snapshot.fauna.as_ref().unwrap().animals[0].age, 4);
            let saved = serde_json::to_string(&snapshot).unwrap();
            let mut resumed = game_economy::core_interval::CoreHost::resume(
                serde_json::from_str(&saved).unwrap(),
            )
            .unwrap();
            assert_eq!(
                host.advance_continuous(3_600_000, &[]).unwrap().failure,
                None
            );
            assert_eq!(
                resumed.advance_continuous(3_600_000, &[]).unwrap().failure,
                None
            );
            let result = serde_json::to_value(host.snapshot()).unwrap();
            assert_eq!(result, serde_json::to_value(resumed.snapshot()).unwrap());
            result
        })
}

#[test]
fn authored_core_scenario_runs_and_survives_save_resume() {
    assert_eq!(run_scenario(1), run_scenario(6));
}

#[test]
fn unsupported_scenario_owners_are_reported() {
    let root = root();
    let state = game_economy_cli::scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let error = game_economy::core_interval::CoreHostSnapshot::from_scenario(state).unwrap_err();
    assert!(error.contains("campaign"), "{error}");
}
