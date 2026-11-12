use game_economy::core_interval::CoreHost;
use std::fmt::Write as _;
use std::path::PathBuf;

#[test]
#[ignore = "composed signal-learning scaling"]
fn physical_gifts_and_signal_learning_benchmark() {
    let count: u64 = std::env::var("LEGACY_LEARNING_COUNT")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(10_000);
    assert!((1..=100_000).contains(&count));
    let concentrated = std::env::var_os("LEGACY_LEARNING_CONCENTRATED").is_some();
    let baseline = std::env::var_os("LEGACY_LEARNING_BASELINE").is_some();
    let observers = if concentrated { 1 } else { count };
    let mut text = format!(
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
carrying_capacity = {}
max_work_hunger = 20
[[regions]]
id = 1
stock = {{}}
[dynamics.social]
participants = []
[dynamics.social.rules]
interval = 1
min_health = 1
max_hunger = 100
familiarity_gain = 1
familiarity_max = 10
[signals]
catalog = "signals/basic.toml"
"#,
        count + 10
    );
    for id in 1..=count + observers {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false").unwrap();
        if id <= count {
            writeln!(text, "goods = {{ bread = 1 }}").unwrap();
        }
    }
    for actor in 1..=count {
        let observer = if concentrated {
            count + 1
        } else {
            count + actor
        };
        writeln!(text, "[[signals.minds]]\nperson = {actor}\nidle = 0\nattention = 0\nexpressions = [{{ form = 1, intended = 1, recipient = {observer}, enabled = true, value = 10 }}]").unwrap();
    }
    for observer in count + 1..=count + observers {
        writeln!(
            text,
            "[[signals.minds]]\nperson = {observer}\nidle = 0\nattention = 1000"
        )
        .unwrap();
        writeln!(text, "[[signals.learning]]\nperson = {observer}\ngood = \"bread\"\nmeaning = \"comfort\"\nenabled = true\nvalue = 10").unwrap();
    }
    text.push_str("[giving]\n");
    for actor in 1..=count {
        let observer = if concentrated {
            count + 1
        } else {
            count + actor
        };
        writeln!(text, "[[giving.policies]]\nperson = {actor}\nrecipient = {observer}\ngood = \"bread\"\nquantity = 1\nrecipient_hunger_at_least = 2\nwilling = true").unwrap();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = std::env::temp_dir().join(format!(
        "legacy-learning-{}-{}.toml",
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
        initial.advance_continuous(1_799_999, &[]).unwrap().failure,
        None
    );
    let mut opening = initial.snapshot();
    if baseline {
        for mind in &mut opening
            .social
            .as_mut()
            .unwrap()
            .signals
            .as_mut()
            .unwrap()
            .minds
        {
            mind.learning_rules.clear();
        }
    }
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut host = pool.install(|| CoreHost::resume(opening.clone()).unwrap());
            let start = std::time::Instant::now();
            let result = pool.install(|| host.advance_continuous(1, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.failure, None);
            assert_eq!(result.boundaries, 1);
            let state = pool.install(|| host.snapshot());
            let received: u64 = state
                .core
                .physical
                .custody
                .containers
                .iter()
                .filter_map(|row| match row.place {
                    game_lots::Place::Person(id) if id > count => Some(row.used),
                    _ => None,
                })
                .sum();
            assert_eq!(received, count);
            let signals = state.social.as_ref().unwrap().signals.as_ref().unwrap();
            assert_eq!(
                signals.learning_events.len() as u64,
                if baseline { 0 } else { observers }
            );
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} {} gifts ({})",
            times[1],
            if concentrated {
                "concentrated"
            } else {
                "distributed"
            },
            if baseline { "baseline" } else { "learning" }
        );
    }
}
