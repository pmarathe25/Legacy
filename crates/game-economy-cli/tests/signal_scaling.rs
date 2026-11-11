use std::fmt::Write as _;
use std::path::PathBuf;

/// Includes a boundary and many independent people signaling one nearby observer.
#[test]
#[ignore = "host directed-signal scaling measurement"]
fn hosted_concentrated_signals_benchmark() {
    let count: u64 = std::env::var("LEGACY_SIGNAL_PEOPLE")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(20_000);
    assert!(count > 0 && count < u64::MAX);
    let observer = count + 1;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut text = String::from(
        r#"seed = 7
tick_millis = 900000
stock_capacity = 100
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
stock = {}
[signals]
catalog = "signals/basic.toml"
[dynamics.social]
[dynamics.social.rules]
interval = 1
min_health = 1
max_hunger = 100
familiarity_gain = 1
familiarity_max = 10
"#,
    );
    for id in 1..=observer {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false").unwrap();
        writeln!(
            text,
            "[[dynamics.social.participants]]\nperson = {id}\nwilling = false"
        )
        .unwrap();
        if id != observer {
            writeln!(text, "[[signals.minds]]\nperson = {id}\nidle = 0\nattention = 0\nexpressions = [{{ form = 1, intended = 1, recipient = {observer}, enabled = true, value = 10 }}]").unwrap();
        }
    }
    writeln!(text, "[[signals.minds]]\nperson = {observer}\nidle = 0\nattention = 1000\nremember = true\ncapacity = 16\ninterpretations = [{{ form = 1, meaning = 2 }}]").unwrap();
    let path = std::env::temp_dir().join(format!(
        "legacy-signals-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, text).unwrap();
    let mut initial = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        initial.advance_continuous(899_999, &[]).unwrap().failure,
        None
    );
    let mut snapshot = initial.snapshot();
    assert!(
        snapshot
            .social
            .as_ref()
            .unwrap()
            .signals
            .as_ref()
            .unwrap()
            .events
            .is_empty()
    );
    let baseline = std::env::var_os("LEGACY_SIGNAL_BASELINE").is_some();
    if baseline {
        snapshot.social.as_mut().unwrap().signals = None;
    }
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
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
            assert_eq!(result.boundaries, 1);
            let state = pool.install(|| host.snapshot());
            if !baseline {
                let signals = state.social.as_ref().unwrap().signals.as_ref().unwrap();
                assert_eq!(signals.events.len() as u64, count);
                assert_eq!(
                    signals.minds.last().unwrap().observations.len(),
                    16.min(count as usize)
                );
            }
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} people ({})",
            times[1],
            if baseline {
                "boundary only"
            } else {
                "directed signals"
            }
        );
    }
}
