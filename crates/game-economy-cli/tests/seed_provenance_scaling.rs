use std::fmt::Write as _;
use std::path::PathBuf;

#[test]
#[ignore = "seed provenance cultivation scaling measurement"]
fn concentrated_seed_provenance_scaling() {
    let count = std::env::var("LEGACY_SEED_FIELDS")
        .ok()
        .and_then(|row| row.parse::<u64>().ok())
        .unwrap_or(10_000);
    let tracking = std::env::var_os("LEGACY_SEED_BASELINE").is_none();
    let personal = std::env::var_os("LEGACY_SEED_PERSONAL").is_some();
    assert!(!personal || tracking);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let capacity = count * 10 + 100;
    let mut text = format!(
        r#"seed = 7
tick_millis = 900000
stock_capacity = {capacity}
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
track_seed_batches = {tracking}
"#,
        count * 10
    );
    for id in 1..=count {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = true\nseek_meals = false").unwrap();
        writeln!(
            text,
            "[[cultivation.fields]]\nid = {id}\nregion = 1\ncrop = \"grain\""
        )
        .unwrap();
        writeln!(
            text,
            "[[cultivation.policies]]\nperson = {id}\nwilling = true"
        )
        .unwrap();
        if personal {
            writeln!(text, "[[cultivation.seed_profiles]]\nperson = {id}\nidle = 0\npreferences = [{{ batch = {id}, enabled = true, value = 10 }}]").unwrap();
        }
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-seed-provenance-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, text).unwrap();
    let host = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut opening = host.snapshot();
    let seed = opening
        .agriculture
        .as_ref()
        .unwrap()
        .crops
        .iter()
        .find(|crop| crop.id == opening.agriculture.as_ref().unwrap().fields[0].crop)
        .unwrap()
        .seed;
    let stock = opening
        .core
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|row| {
            row.place
                == game_lots::Place::Stock {
                    region: 1,
                    good: seed,
                }
        })
        .unwrap();
    let lots = (1..=count)
        .map(|batch| game_lots::Lot {
            good: seed,
            quantity: 10,
            batch: Some(batch),
            item: None,
            expires_at: None,
        })
        .collect();
    *stock = game_lots::Container::new(stock.place, stock.capacity, lots).unwrap();
    let mut host = game_economy::core_interval::CoreHost::resume(opening).unwrap();
    assert_eq!(host.advance_continuous(899_999, &[]).unwrap().failure, None);
    let snapshot = host.snapshot();
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
            let state = pool.install(|| host.snapshot());
            let farm = state.agriculture.as_ref().unwrap();
            assert_eq!(
                farm.fields
                    .iter()
                    .filter(|field| field.phase == game_economy_agriculture::Phase::Growing)
                    .count(),
                count as usize
            );
            if tracking {
                assert_eq!(
                    farm.provenance.as_ref().unwrap().plantings.len(),
                    count as usize
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
            "{workers} workers: {:?} median for {count} fields; tracking={tracking}; personal={personal}",
            times[1]
        );
    }
}
