use game_economy::core_interval::CoreHost;

#[test]
fn exposure_only_credits_clothing_the_person_chooses_and_carries() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = r#"
seed = 7
tick_millis = 1000
decision_period_millis = 700
stock_capacity = 100
food_relief_divisor = 20

[rules]
hunger_per_tick = 1
meal_threshold = 10
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 100
max_work_hunger = 20

[[regions]]
id = 1
stock = { grain = 1 }

[[people]]
id = 1
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false
goods = { quilted-coat = 1 }

[[people]]
id = 2
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false
goods = { quilted-coat = 1 }

[[people]]
id = 3
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[environment]
exposure_period_millis = 83
clothing = [{ good = "quilted-coat", insulation = 4 }]
clothing_choices = [
    { person = 3, good = "quilted-coat", protection_weight = 1 },
    { person = 2, good = "quilted-coat", protection_weight = -1 },
    { person = 1, good = "quilted-coat", protection_weight = 1 },
]

[[environment.regions]]
region = 1
damage = 6
shelter = 0
shelter_reduction = 0
"#;
    let path = std::env::temp_dir().join(format!(
        "legacy-personal-clothing-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let opening = loaded.unwrap().snapshot();
    let run = |workers, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(opening.clone()).unwrap();
                for &millis in steps {
                    let result = host.advance_continuous(millis, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                host.snapshot()
            })
    };
    let state = run(1, &[166]);
    assert_eq!(state, run(4, &[64, 19, 82, 1]));
    assert_eq!(
        state
            .core
            .people
            .iter()
            .map(|row| row.health)
            .collect::<Vec<_>>(),
        [96, 88, 88]
    );
    assert_eq!(state.core.physical.custody, opening.core.physical.custody);
}
