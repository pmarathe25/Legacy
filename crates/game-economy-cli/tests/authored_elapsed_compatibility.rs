use std::path::PathBuf;

#[test]
fn a_local_goal_search_expires_between_decisions_and_pauses_only_its_own_walk() {
    use game_economy::core_interval::{CoreHost, CoreRunEvent};
    use game_economy::{LocalEndpoint, LocalLocation, LocalWalkPreference};
    let content = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let authored = std::fs::read_to_string(content.join("scenarios/economy-goal-search.toml"))
        .unwrap()
        .replace("\r\n", "\n");
    let run = |workers, quantum, chunks: &[u64], continues: bool| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let mut source = authored
                .replace("tick_millis = 3600000", &format!("tick_millis = {quantum}\ndecision_period_millis = 11"))
                .replace("hunger_per_tick = 1", "hunger_rate = { units = 0, per_millis = 1 }")
                .replace("retry_millis = 28800000, max_millis = 28800000", "retry_millis = 95, max_millis = 37")
                .replace("ticks = 2", "duration_millis = 80");
            source.push_str(
                "\n[goals]\nretry_interval = 3\n[[goals.people]]\nperson = 2\nrecipe = \"fixture-tool\"\npriority = 1\ntarget = 1\n",
            );
            let path = std::env::temp_dir().join(format!("legacy-local-goal-{}-{workers}-{quantum}.toml", std::process::id()));
            std::fs::write(&path, source).unwrap();
            let loaded = game_economy_cli::core_scenario(&path, &content);
            std::fs::remove_file(&path).unwrap();
            let mut host = loaded.unwrap();
            host.detach_player_control().unwrap();
            assert_eq!(host.advance_continuous(20, &[]).unwrap().failure, None);
            let mut opening = host.snapshot();
            let local = opening.local_transport.as_mut().unwrap();
            let timer = local.goal_fetchers[0].search.as_ref().unwrap().elapsed.unwrap();
            assert!(timer.owns_walk);
            let expiry = timer.expires_at.unwrap();
            assert!(expiry > 20 && !expiry.is_multiple_of(11));
            assert_eq!(timer.next_review, Some(expiry - 37 + 95));
            if continues {
                std::sync::Arc::make_mut(&mut local.walking.as_mut().unwrap().preferences).push(LocalWalkPreference {
                    person: 2, region: 1, destination: LocalEndpoint::Store { id: 1 },
                    willing: true, pressured_by: None, pressure_threshold: None, meal_hunger: None,
                });
            }
            host = CoreHost::resume(serde_json::from_slice(&serde_json::to_vec(&opening).unwrap()).unwrap()).unwrap();
            let mut expirations = Vec::new();
            for &elapsed in chunks {
                let result = host.advance_continuous(elapsed, &[]).unwrap();
                assert_eq!(result.failure, None);
                expirations.extend(result.events.into_iter().filter_map(|event| match event {
                    CoreRunEvent::LocalGoalSearchExpiry { at_millis, people } => Some((at_millis, people)),
                    _ => None,
                }));
                host = CoreHost::resume(serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap()).unwrap()).unwrap();
            }
            assert_eq!(host.core().simulated_millis(), 75);
            assert_eq!(expirations, [(expiry, vec![2])]);
            let ending = host.snapshot();
            let local = ending.local_transport.as_ref().unwrap();
            let walking = local.walking.as_ref().unwrap();
            assert_eq!(walking.elapsed.as_ref().unwrap().paused.contains(&2), !continues);
            assert!(matches!(host.local_location(2), Some(LocalLocation::Walking { remaining, .. })
                if (continues && remaining == u64::try_from(80 - 75 + expiry - 37).unwrap()) || (!continues && remaining == 43)));
            assert!(local.goal_fetchers[0].search.as_ref().unwrap().active_recipe.is_none());
            (host.local_location(2), expirations)
        })
    };
    assert_eq!(
        run(1, 3600000, &[55], false),
        run(6, 29, &[7, 16, 19, 13], false)
    );
    assert_eq!(
        run(1, 3600000, &[55], true),
        run(6, 29, &[7, 16, 19, 13], true)
    );
}

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
