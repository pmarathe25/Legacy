use std::{fmt::Write as _, path::PathBuf};

#[test]
#[ignore = "mechanical load scaling measurement"]
fn mechanical_load_scaling() {
    use game_economy::core_interval::{MechanicalLoadOutcome, MechanicalLoadRequest};
    use game_lots::{Movement, Place};
    let count = std::env::var("LEGACY_MECHANICAL_LOADS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10_000);
    let contended = std::env::var_os("LEGACY_LOAD_CONTENDED").is_some();
    let available = if contended { count / 2 } else { count };
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
carrying_capacity = 10
max_work_hunger = 20
[[regions]]
id = 1
stock = {{ timber = {available} }}
[local_transport]
[[local_transport.stores]]
id = 1
region = 1
name = "load store"
capacity = {}
[mechanics]
catalog = "mechanics/basic.toml"
[[mechanics.materials]]
component = 1
good = "timber"
[[mechanics.materials]]
component = 2
good = "tool"
[[mechanics.load_materials]]
good = "timber"
force_per_unit = 10
"#,
        count + 100,
        count + 100
    );
    for id in 1..=count {
        let first = 2 * id + 1000;
        let second = first + 1;
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false\nitems = [{{ id = {first}, good = \"timber\" }}, {{ id = {second}, good = \"tool\" }}]").unwrap();
        writeln!(text, "[[mechanics.apparatus]]\nid = {id}\nparts = [{{ id = {first}, component = 1, share_per_mille = 1000 }}, {{ id = {second}, component = 2, parent = {first}, share_per_mille = 1000 }}]").unwrap();
        writeln!(
            text,
            "[[mechanics.capabilities]]\nperson = {id}\nmax_force = 200\nmax_distance = 200"
        )
        .unwrap();
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-mechanical-load-{}-{}.toml",
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
    let good = opening
        .core
        .physical
        .goods
        .iter()
        .find(|row| row.name == "timber")
        .unwrap()
        .id;
    let requests: Vec<_> = (1..=count)
        .map(|actor| MechanicalLoadRequest {
            actor,
            apparatus: actor,
            output_part: 2 * actor + 1001,
            input_force: 100,
            input_distance: 100,
            required_travel: 10,
            movement: Movement {
                id: 0,
                from: Place::Stock { region: 1, good },
                to: Some(Place::Inventory(1)),
                good,
                quantity: 1,
                item: None,
                batch: None,
            },
        })
        .collect();
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
            let results = pool.install(|| host.move_mechanical_loads(&requests, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(
                results
                    .iter()
                    .filter(|row| row.outcome == MechanicalLoadOutcome::Moved)
                    .count(),
                available as usize
            );
            let state = pool.install(|| host.snapshot());
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} mechanical loads; contended={contended}",
            times[1]
        );
    }
}
