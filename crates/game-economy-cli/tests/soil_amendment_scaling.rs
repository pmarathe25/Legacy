use std::{fmt::Write as _, path::PathBuf};

#[test]
#[ignore = "concentrated soil amendment scaling measurement"]
fn concentrated_soil_amendment_scaling() {
    let count = std::env::var("LEGACY_SOIL_FIELDS")
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
        count + 100,
        count
    );
    for id in 1..=count {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = true\nseek_meals = false").unwrap();
        writeln!(
            text,
            "[[cultivation.fields]]\nid = {id}\nregion = 1\ncrop = \"grain\""
        )
        .unwrap();
    }
    writeln!(text, "[cultivation.soil]\ncapacity = 100\nloss_per_harvest = 1\nrecovery_per_fallow = 0\nfields = [").unwrap();
    for id in 1..=count {
        writeln!(text, "{{ field = {id}, fertility = 50 }},").unwrap();
    }
    writeln!(
        text,
        "]\n[soil_amendment]\ngood = \"grain-seed\"\nquantity = 1\nfertility_gain = 1\nchoices = ["
    )
    .unwrap();
    for id in 1..=count {
        writeln!(text, "{{ person = {id}, field = {id}, willing = true }},").unwrap();
    }
    writeln!(text, "]").unwrap();
    let path = std::env::temp_dir().join(format!(
        "legacy-soil-scaling-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, text).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut host = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(host.advance_continuous(899_999, &[]).unwrap().failure, None);
    let opening = host.snapshot();
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
            let result = pool.install(|| host.advance_continuous(1, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.failure, None);
            let state = pool.install(|| host.snapshot());
            let soil = state.agriculture.as_ref().unwrap().soil.as_ref().unwrap();
            assert_eq!(
                soil.amendment
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .filter(|event| event.accepted)
                    .count(),
                count as usize
            );
            assert!(soil.fields.iter().all(|field| field.fertility == 51));
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} soil improvements",
            times[1]
        );
    }
}
