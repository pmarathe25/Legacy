use std::path::PathBuf;

#[test]
fn walking_salvage_and_wound_scenarios_use_the_elapsed_host() {
    let content = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    pool.install(|| {
        for scenario in [
            "economy-active-tool-repair.toml",
            "economy-autonomous-wound-care.toml",
            "economy-goal-multistore.toml",
            "economy-goal-search.toml",
            "economy-local-food-search.toml",
            "economy-local-walking.toml",
            "economy-salvage.toml",
        ] {
            let path = content.join("scenarios").join(scenario);
            let mut host = game_economy_cli::core_scenario(&path, &content)
                .unwrap_or_else(|error| panic!("{scenario}: {error}"));
            host.detach_player_control().unwrap();
            let result = host.advance_continuous(60_000, &[]).unwrap();
            assert_eq!(result.failure, None, "{scenario}");
            assert_eq!(result.elapsed_millis, 60_000, "{scenario}");
        }
    });
}
