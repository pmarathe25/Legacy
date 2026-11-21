use game_economy::core_interval::CoreHost;

#[test]
fn minute_observations_and_events_match_independent_advances() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut host = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-history-integrated-hourly.toml"),
        &root.join("content"),
    )
    .unwrap();
    host.detach_player_control().unwrap();
    let opening = host.snapshot();
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap()
        .install(|| {
            let mut reference = CoreHost::resume(opening).unwrap();
            let mut steps = host.stepper(&[]);
            for millis in [
                60_000, 1, 124_000, 60_000, 5_399_907, 60_000, 731, 12_879_252,
            ] {
                let expected = reference.advance_continuous(millis, &[]).unwrap();
                let actual = steps.advance(millis).unwrap();
                assert_eq!(actual.failure, None);
                assert_eq!(expected, actual);
                assert_eq!(reference.snapshot(), steps.host().snapshot());
            }
        });
}
