use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_agriculture::{Phase, requests::Outcome};
use std::path::Path;

fn world(quantum: u64, help_value: i64) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scene = r#"
seed = 7
tick_millis = QUANTUM
decision_period_millis = 70
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
stock = { grain-seed = 20 }

[[regions]]
id = 2
stock = { grain-seed = 20 }

[[regions]]
id = 3
stock = { grain-seed = 20 }

[[people]]
id = 1
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[[people]]
id = 2
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[[people]]
id = 3
region = 2
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[[people]]
id = 4
region = 3
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[cultivation]
max_hunger = 20
growth_unit_millis = 1000
action_clock = { period_millis = 70, request_unit_millis = 10 }
policies = [{ person = 3, willing = true }, { person = 4, willing = false }]

[[cultivation.fields]]
id = 1
region = 1
crop = "grain"

[[cultivation.fields]]
id = 2
region = 2
crop = "grain"

[[cultivation.fields]]
id = 3
region = 3
crop = "grain"

[cultivation.requests]
next_id = 1
askers = [{ person = 1, worker = 2, field = 1, interval = 11, lifetime = 15 }]
preferences = [{ person = 2, help_value = HELP, effort = 1 }]
"#
    .replace("QUANTUM", &quantum.to_string())
    .replace("HELP", &help_value.to_string());
    let path = std::env::temp_dir().join(format!(
        "legacy-elapsed-field-requests-{}-{quantum}-{help_value}.toml",
        std::process::id()
    ));
    std::fs::write(&path, scene).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

#[test]
fn ordinary_farming_and_requests_reconsider_on_their_elapsed_clock() {
    let run = |workers, quantum, help_value, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = world(quantum, help_value);
                let mut receipts = Vec::new();
                let mut record = |run: game_economy::core_interval::CoreRun| {
                    assert_eq!(run.failure, None);
                    receipts.extend(run.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::FarmReview { at_millis, review } => Some((at_millis, review)),
                        _ => None,
                    }));
                };
                record(host.advance_continuous(1, &[]).unwrap());
                let farm = host.snapshot().agriculture.unwrap();
                let opening = farm.requests.unwrap().pending;
                assert_eq!((opening[0].created_at, opening[0].expires), (0, 150));
                assert_eq!(farm.fields[0].phase, Phase::Fallow);
                assert_eq!(farm.fields[1].phase, Phase::Growing);
                assert_eq!(farm.fields[2].phase, Phase::Fallow);
                for &millis in steps {
                    record(host.advance_continuous(millis, &[]).unwrap());
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let farm = host.snapshot().agriculture.unwrap();
                assert_eq!(
                    farm.action_clock.unwrap().last_review_millis, Some(70),
                    "physical quantum {quantum} and help value {help_value}"
                );
                let phases: Vec<_> = farm.fields.into_iter().map(|field| field.phase).collect();
                (phases, farm.requests.unwrap().events, receipts)
            })
    };
    let willing = run(1, 1000, 4, &[70]);
    assert_eq!(willing, run(6, 37, 4, &[25, 11, 12, 22]));
    assert_eq!(willing.0, [Phase::Growing, Phase::Growing, Phase::Fallow]);
    assert_eq!(willing.1[0].outcome, Outcome::Fulfilled);
    assert_eq!((willing.2[0].0, willing.2[0].1.work[0].person), (0, 3));
    assert_eq!((willing.2[1].0, willing.2[1].1.work[0].person), (70, 2));
    let unwilling = run(6, 37, -4, &[70]);
    assert_eq!(unwilling.0, [Phase::Fallow, Phase::Growing, Phase::Fallow]);
    assert_eq!(unwilling.1[0].outcome, Outcome::Refused);
    assert!(unwilling.2[1].1.work.is_empty());
}
