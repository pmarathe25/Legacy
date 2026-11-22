use game_economy::core_interval::{CoreHost, CoreRunEvent};

#[test]
fn wildlife_grazes_and_delivers_on_elapsed_deadlines_across_resume() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = r#"
seed = 7
tick_millis = QUANTUM
decision_period_millis = 500
stock_capacity = 100
food_relief_divisor = 20

[rules]
hunger_per_tick = 1
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 100
max_work_hunger = 20

[[regions]]
id = 1
stock = { grain = 0 }

[[people]]
id = 1
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[gathering]
elapsed_renewal = true
max_hunger = 20
policies = []

[[gathering.nodes]]
id = 1
region = 1
good = "grain"
available = 0
capacity = 20
renewal_rate = { units = 1, per_millis = 83 }
yield_per_action = 1

[fauna]
next_id = 4
lifecycle_period_millis = 83

[[fauna.species]]
id = 1
name = "grazer"
food_good = "grain"
intake = 1
hunger_per_tick = 1
relief_per_unit = 1
starvation_threshold = 100
starvation_damage = 1
max_health = 10
adult_age = 1
fertile_until = 6
lifespan = 10
gestation = 1
conception_one_in = 1
breeding_max_hunger = 8
newborn_health = 10

[[fauna.animals]]
id = 1
species = 1
region = 1
age = 1
health = 10
hunger = 1
genome = { sex = "Female", alleles = [[100,100], [100,100], [100,100], [100,100]] }

[[fauna.animals]]
id = 2
species = 1
region = 1
age = 1
health = 10
hunger = 1
genome = { sex = "Male", alleles = [[500,500], [500,500], [500,500], [500,500]] }

[[fauna.animals]]
id = 3
species = 1
region = 1
age = 9
health = 10
hunger = 1
genome = { sex = "Male", alleles = [[700,700], [700,700], [700,700], [700,700]] }
"#;
    let path =
        std::env::temp_dir().join(format!("legacy-elapsed-fauna-{}.toml", std::process::id()));
    let load = |quantum: u64| {
        std::fs::write(&path, source.replace("QUANTUM", &quantum.to_string())).unwrap();
        let result = game_economy_cli::core_scenario(&path, &root.join("content"));
        std::fs::remove_file(&path).unwrap();
        result.unwrap().snapshot()
    };
    let simulate = |workers, quantum, steps: &[u64]| {
        let opening = load(quantum);
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(opening).unwrap();
                assert_eq!(host.next_deadline(None).unwrap().after_millis, 83);
                let mut recorded = Vec::new();
                for &millis in steps {
                    let run = host.advance_continuous(millis, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    assert_eq!(run.elapsed_millis, millis);
                    recorded.extend(run.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::Fauna(events) => Some(events),
                        _ => None,
                    }));
                    let persisted = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&persisted).unwrap()).unwrap();
                }
                let state = host.snapshot();
                (
                    recorded,
                    state.fauna.unwrap(),
                    state.gathering.unwrap().nodes,
                )
            })
    };
    let whole = simulate(1, 1_000, &[166]);
    assert_eq!(whole, simulate(4, 100, &[1, 82, 10, 32, 41]));
    assert_eq!(whole.0.len(), 2);
    assert_eq!(whole.0[0].at_millis, 83);
    assert_eq!(whole.0[0].deaths[0].animal, 3);
    assert_eq!(whole.0[1].at_millis, 166);
    assert_eq!(whole.0[1].births, [4]);
    assert_eq!(whole.1.animals[3].parents, Some([1, 2]));
    assert_eq!(whole.1.last_tick, 2);
    assert_eq!(whole.2[0].available, 0);
}

#[test]
fn hungry_animals_disperse_on_their_own_clock_using_inherited_drive() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = r#"
seed = 7
tick_millis = QUANTUM
decision_period_millis = 500
stock_capacity = 100
food_relief_divisor = 20

[rules]
hunger_per_tick = 1
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 100
max_work_hunger = 20

[[regions]]
id = 1
stock = { grain = 0 }
[[regions]]
id = 2
stock = { grain = 0 }

[[people]]
id = 1
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[gathering]
elapsed_renewal = true
max_hunger = 20

[[gathering.nodes]]
id = 1
region = 2
good = "grain"
available = 4
capacity = 4
yield_per_action = 1

[fauna]
next_id = 3
lifecycle_period_millis = 83

[fauna.dispersal]
corridors = [{ from = 1, to = 2, hunger_cost = 2 }]
policies = [{ species = 1, minimum_hunger = 2, minimum_age = 2, initiative = 500, locus = 0, inherited_drive = 2000, effort_weight = 25 }]

[[fauna.species]]
id = 1
name = "grazer"
food_good = "grain"
intake = 1
hunger_per_tick = 1
relief_per_unit = 2
starvation_threshold = 100
starvation_damage = 1
max_health = 10
adult_age = 1
fertile_until = 6
lifespan = 10
gestation = 1
conception_one_in = 1
breeding_max_hunger = 8
newborn_health = 10

[[fauna.animals]]
id = 1
species = 1
region = 1
age = 1
health = 10
hunger = 1
genome = { sex = "Male", alleles = [[100,100], [500,500], [500,500], [500,500]] }

[[fauna.animals]]
id = 2
species = 1
region = 1
age = 1
health = 10
hunger = 1
genome = { sex = "Male", alleles = [[900,900], [500,500], [500,500], [500,500]] }
"#;
    let simulate = |workers, quantum: u64, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let path = std::env::temp_dir().join(format!(
                    "legacy-dispersal-{}-{quantum}.toml",
                    std::process::id()
                ));
                std::fs::write(&path, source.replace("QUANTUM", &quantum.to_string())).unwrap();
                let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
                std::fs::remove_file(path).unwrap();
                let mut host = loaded.unwrap();
                let mut recorded = Vec::new();
                for &millis in steps {
                    let run = host.advance_continuous(millis, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    recorded.extend(run.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::Fauna(events) if !events.migrations.is_empty() => {
                            Some(events)
                        }
                        _ => None,
                    }));
                    let saved = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&saved).unwrap()).unwrap();
                }
                let state = host.snapshot();
                (
                    recorded,
                    state.fauna.unwrap(),
                    state.gathering.unwrap().nodes,
                )
            })
    };
    let whole = simulate(1, 1000, &[166]);
    assert_eq!(whole, simulate(6, 100, &[80, 3, 32, 51]));
    assert_eq!(whole.0.len(), 1);
    assert_eq!(whole.0[0].at_millis, 83);
    assert_eq!(whole.0[0].migrations[0].animal, 2);
    assert_eq!(whole.0[0].migrations[0].hunger_cost, 2);
    assert_eq!(whole.1.animals[0].region, 1);
    assert_eq!(whole.1.animals[1].region, 2);
    assert_eq!(whole.1.animals[1].hunger, 3);
    assert_eq!(whole.2[0].available, 3);
}

#[test]
fn named_wildlife_traits_survive_elapsed_birth_and_save_resume() {
    use game_economy::core_interval::{CoreHost, CoreRunEvent};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let opening = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-fauna-named-traits.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let run = |workers, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(opening.clone()).unwrap();
                let mut births = Vec::new();
                for elapsed in steps {
                    let result = host.advance_continuous(*elapsed, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    for event in result.events {
                        if let CoreRunEvent::Fauna(receipt) = event {
                            births.extend(receipt.births.iter().map(|id| (receipt.at_millis, *id)));
                        }
                    }
                    let bytes = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&bytes).unwrap()).unwrap();
                }
                let fauna = host.snapshot().fauna.unwrap();
                assert_eq!(births, [(300, 4)]);
                assert!(fauna.animals[2].pregnancy.is_none());
                let offspring = &fauna.animals[3];
                assert_eq!(offspring.parents, Some([1, 2]));
                let traits = fauna.species[0].traits.as_ref().unwrap();
                assert_eq!(traits.expression(&offspring.genome, 12), Some(1000));
                assert_eq!(traits.expression(&offspring.genome, 18), Some(800));
                (fauna, births)
            })
    };
    assert_eq!(run(1, &[300]), run(6, &[53, 168, 79]));
}
