use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let people: u64 = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_000);
    let ticks: usize = std::env::var("LEGACY_TICKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);
    let reproduction: u8 = std::env::var("LEGACY_REPRODUCTION")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(80);
    let grain_per_person: u64 = std::env::var("LEGACY_GRAIN_PER_PERSON")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    assert!(people > 4 && ticks > 0 && reproduction <= 100);
    let profile =
        std::fs::read_to_string(root.join("content/scenarios/campaign-population.toml")).unwrap();
    assert!(profile.contains("count = 128"));
    let profile = profile.replacen("count = 128", &format!("count = {}", people - 4), 1);
    assert!(profile.contains("reproduction_percent = 80"));
    let profile = profile.replacen(
        "reproduction_percent = 80",
        &format!("reproduction_percent = {reproduction}"),
        1,
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-mixed-benchmark-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, profile).unwrap();
    let mut scenario_source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    if grain_per_person > 0 {
        let grain = people
            .checked_mul(grain_per_person)
            .expect("grain quantity overflow");
        assert!(scenario_source.contains("stock_capacity = 1000"));
        assert!(scenario_source.contains("grain = 100, grain-seed"));
        assert!(scenario_source.contains("stock = { bread = 20 }"));
        scenario_source = scenario_source
            .replacen(
                "stock_capacity = 1000",
                &format!("stock_capacity = {}", grain.max(1000)),
                1,
            )
            .replacen(
                "grain = 100, grain-seed",
                &format!("grain = {grain}, grain-seed"),
                1,
            )
            .replacen(
                "stock = { bread = 20 }",
                &format!("stock = {{ bread = 20, grain = {grain} }}"),
                1,
            );
    }
    let scenario_path =
        std::env::temp_dir().join(format!("legacy-mixed-scenario-{}.toml", std::process::id()));
    std::fs::write(&scenario_path, scenario_source).unwrap();
    rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let initial = game_economy_cli::scenario_with_options(
            &scenario_path, &root.join("content"), Some(42), Some(&path),
        ).unwrap().snapshot();
        assert_eq!(initial.people.len(), people as usize);
        let mut samples = Vec::new();
        let mut expected = None;
        let mut final_population = 0;
        let mut started_jobs = 0;
        let mut delivered_jobs = 0;
        for round in 0..4 {
            let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
            let started = std::time::Instant::now();
            started_jobs = 0;
            delivered_jobs = 0;
            for _ in 0..ticks {
                let events = world.advance().unwrap();
                started_jobs += events.started.len();
                delivered_jobs += events.delivered.len();
            }
            let elapsed = started.elapsed().as_secs_f64();
            let state = world.snapshot();
            final_population = state.people.len();
            let bytes = serde_json::to_vec(&state).unwrap();
            let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3));
            if let Some(expected) = expected { assert_eq!(hash, expected, "repeated runs must match"); } else { expected = Some(hash); }
            if round > 0 { samples.push(elapsed); }
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} initial_people={people} final_people={final_population} reproduction_percent={reproduction} grain_per_person={grain_per_person} ticks={ticks} started_jobs={started_jobs} delivered_jobs={delivered_jobs} elapsed_min={:.6} elapsed_median={:.6} elapsed_max={:.6} state_fingerprint={:016x}", samples[0], samples[1], samples[2], expected.unwrap());
        #[cfg(feature = "profile")]
        if std::env::var_os("LEGACY_PROFILE").is_some() {
            for (name, measurement) in game_diagnostics::snapshot() {
                println!("{name}: {:.3} ms/call", measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0);
            }
        }
    });
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(scenario_path).unwrap();
}
