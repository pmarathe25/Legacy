use std::fmt::Write as _;
use std::path::PathBuf;

/// Includes candidate preparation, personal selection, revalidation and
/// physical arbitration against one shared stock and one shared local store.
#[test]
#[ignore = "mixed-workload performance measurement"]
fn concentrated_personal_choice_benchmark() {
    let count: u64 = std::env::var("LEGACY_CHOICE_PEOPLE")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(5_000);
    assert!((1..=100_000).contains(&count));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut text = String::from(
        r#"seed = 7
tick_millis = 900000
stock_capacity = 100000
food_relief_divisor = 20
[rules]
hunger_per_tick = 1
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 10
max_work_hunger = 20
[[regions]]
id = 1
stock = { grain = 50000 }
"#,
    );
    for id in 1..=count {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false").unwrap();
    }
    text.push_str(
        r#"[work_sites]
sites = []
[construction]
[[construction.blueprints]]
id = 1
name = "Shelter"
kind = "shelter"
capacity = 2
kit = "grain"
quantity = 1
salvage = 1
"#,
    );
    for id in 1..=count {
        writeln!(text, "[[construction.policies]]\nperson = {id}\nblueprint = 1\ntarget_sites = {count}\nwilling = true").unwrap();
    }
    text.push_str(
        r#"[local_transport]
[[local_transport.stores]]
id = 1
region = 1
name = "Granary"
capacity = 10000
"#,
    );
    for id in 1..=count {
        writeln!(text, "[[local_transport.policies]]\nperson = {id}\nregion = 1\nsource = {{ kind = \"stock\" }}\ndestination = {{ kind = \"store\", id = 1 }}\ngood = \"grain\"\nquantity = 1\nwilling = true").unwrap();
    }
    for id in 1..=count {
        let (build, carry) = if id % 2 == 0 { (0, 10) } else { (10, 0) };
        writeln!(text, "[[personal_choices]]\nid = {id}\nidle = 0\npreferences = [{{ domain = 1, enabled = true, bias = {build} }}, {{ domain = 2, enabled = true, bias = {carry} }}]").unwrap();
    }
    text = text
        .replace(
            "stock_capacity = 100000",
            &format!("stock_capacity = {}", count * 20),
        )
        .replace("grain = 50000", &format!("grain = {}", count * 10))
        .replace(
            "\ncapacity = 10000\n",
            &format!("\ncapacity = {}\n", count * 2),
        );
    let token = format!(
        "legacy-choice-benchmark-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(format!("{token}.toml"));
    std::fs::write(&path, text).unwrap();
    let snapshot = game_economy_cli::core_scenario(&path, &root.join("content"))
        .unwrap()
        .snapshot();
    std::fs::remove_file(path).unwrap();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        #[cfg(feature = "profile")]
        let before = game_diagnostics::snapshot();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut host = pool.install(|| {
                game_economy::core_interval::CoreHost::resume(snapshot.clone()).unwrap()
            });
            let start = std::time::Instant::now();
            let result = pool.install(|| host.advance_continuous(1, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.failure, None);
            let state = pool.install(|| host.snapshot());
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} people",
            times[1]
        );
        #[cfg(feature = "profile")]
        for (name, sample) in game_diagnostics::snapshot()
            .iter()
            .filter(|(name, _)| name.starts_with("choice."))
        {
            let earlier = before.get(name);
            let calls = sample.calls - earlier.map_or(0, |row| row.calls);
            let nanos = sample.nanoseconds - earlier.map_or(0, |row| row.nanoseconds);
            if calls > 0 {
                eprintln!(
                    "  {name}: {:.3} ms/call",
                    nanos as f64 / calls as f64 / 1_000_000.0
                );
            }
        }
    }
}
