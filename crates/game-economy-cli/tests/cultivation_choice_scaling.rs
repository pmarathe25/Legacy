use std::{fmt::Write as _, path::PathBuf};

#[test]
#[ignore = "concentrated personal cultivation scaling measurement"]
fn concentrated_personal_cultivation_scaling() {
    let count = std::env::var("LEGACY_CHOICE_FIELDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10_000);
    let mut text = format!(
        r#"seed = 7
tick_millis = 900000
stock_capacity = {}
food_relief_divisor = 20
[rules]
hunger_per_tick = 0
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 100
max_work_hunger = 20
[[regions]]
id = 1
stock = {{ grain-seed = {} }}
[cultivation]
max_hunger = 20
policies = []
"#,
        count * 10 + 100,
        count * 10
    );
    for id in 1..=count {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = true\nseek_meals = false").unwrap();
        writeln!(
            text,
            "[[cultivation.fields]]\nid = {id}\nregion = 1\ncrop = \"grain\""
        )
        .unwrap();
        writeln!(text, "[[personal_choices]]\nid = {id}\nidle = 0\npreferences = [{{ domain = 11, enabled = true, bias = 10, weight = 0 }}]").unwrap();
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-farm-choice-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, text).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let opening = game_economy_cli::core_scenario(&path, &root.join("content"))
        .unwrap()
        .snapshot();
    std::fs::remove_file(path).unwrap();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut host = pool.install(|| {
                game_economy::core_interval::CoreHost::resume(opening.clone()).unwrap()
            });
            let start = std::time::Instant::now();
            let result = pool.install(|| host.advance_continuous(900_000, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.failure, None);
            let state = pool.install(|| host.snapshot());
            let growing = state
                .agriculture
                .as_ref()
                .unwrap()
                .fields
                .iter()
                .filter(|field| field.phase == game_economy_agriculture::Phase::Growing)
                .count();
            assert!(growing > 0 && growing <= count as usize);
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} personal cultivation choices",
            times[1]
        );
    }
}
