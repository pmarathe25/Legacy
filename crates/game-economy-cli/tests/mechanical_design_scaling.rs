use std::{fmt::Write as _, path::PathBuf};

#[test]
#[ignore = "mechanical design scaling measurement"]
fn mechanical_design_scaling() {
    use game_economy::core_interval::CoreRunEvent;
    let count = std::env::var("LEGACY_DESIGNERS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let trials = std::env::var_os("LEGACY_DESIGN_TRIALS").is_some();
    let mut text = String::from(
        r#"seed = 7
tick_millis = 900000
stock_capacity = 100
food_relief_divisor = 20
[rules]
hunger_per_tick = 0
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 10
max_work_hunger = 20
[[regions]]
id = 1
stock = {}
[mechanics]
catalog = "mechanics/basic.toml"
[[mechanics.materials]]
component = 1
good = "timber"
[[mechanics.materials]]
component = 2
good = "tool"
"#,
    );
    for actor in 1..=count {
        let first = actor * 2 + 1000;
        let second = first + 1;
        writeln!(text, "[[people]]\nid = {actor}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false\nitems = [{{ id = {first}, good = \"timber\" }}, {{ id = {second}, good = \"tool\" }}]\n[[personal_choices]]\nid = {actor}\nidle = 0\npreferences = [{{ domain = 13, enabled = true, bias = 10 }}]").unwrap();
        if trials {
            writeln!(
                text,
                "[[mechanics.capabilities]]\nperson = {actor}\nmax_force = 100\nmax_distance = 100"
            )
            .unwrap();
        }
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-design-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, text).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut initial = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    if trials {
        assert_eq!(initial.advance_continuous(1, &[]).unwrap().failure, None);
    }
    let mut opening = initial.snapshot();
    if trials {
        for person in std::sync::Arc::make_mut(
            opening
                .schedule
                .as_mut()
                .unwrap()
                .personal_choices
                .as_mut()
                .unwrap(),
        ) {
            person.preferences.insert(
                0,
                serde_json::from_value(serde_json::json!({"domain":6,"enabled":true,"bias":10}))
                    .unwrap(),
            );
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
            let mut host = pool.install(|| {
                game_economy::core_interval::CoreHost::resume(opening.clone()).unwrap()
            });
            let start = std::time::Instant::now();
            let run = pool.install(|| {
                host.advance_continuous(if trials { 900_000 } else { 1 }, &[])
                    .unwrap()
            });
            times.push(start.elapsed());
            assert_eq!(run.failure, None);
            let successes = run
                .events
                .iter()
                .find_map(|event| match event {
                    CoreRunEvent::MechanicalAssemblies(rows) if !trials => {
                        Some(rows.iter().filter(|row| row.accepted).count())
                    }
                    CoreRunEvent::MechanicalExperiments(rows) if trials => Some(rows.len()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(successes, count as usize);
            let state = pool.install(|| host.snapshot());
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} personal designs; trials={trials}",
            times[1]
        );
    }
}
