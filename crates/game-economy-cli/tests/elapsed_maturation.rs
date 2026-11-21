use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_lot_runtime::{MaturationEvent, MaturationOutcome, Runtime, Snapshot};
use game_lots::{Movement, Place, Transaction};

const DAY: u64 = 86_400_000;
const WORK_TIME: u64 = 1_800_000;
const QUANTUM: u64 = 7 * DAY;

const SCENARIO: &str = r#"
seed = 7
tick_millis = 604800000
decision_period_millis = 604800000
work_rate_period_millis = 1000
elapsed_spoilage = true
stock_capacity = 100
food_relief_divisor = 20

[rules]
meal_threshold = 5
starvation_threshold = 50
max_health = 100
carrying_capacity = 100
max_work_hunger = 20

[storage_climate]
ambient_celsius = 20
places = [{ place = { kind = "person", id = 1 }, celsius = -4 }]

[[regions]]
id = 1
stock = { roots = 24, salt = 4 }

[[people]]
id = 1
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false
goods = { curing-root-batch = 4 }

[[people]]
id = 2
region = 1
health = 100
hunger = 0
controlled = true
willing_to_work = false
seek_meals = false
goods = { curing-root-batch = 5 }
"#;

fn scenario(label: &str) -> CoreHost {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = std::env::temp_dir().join(format!(
        "legacy-maturation-{}-{label}.toml",
        std::process::id()
    ));
    std::fs::write(&path, SCENARIO).unwrap();
    let opening = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    opening.unwrap()
}

fn good(state: &Snapshot, name: &str) -> u32 {
    state.goods.iter().find(|row| row.name == name).unwrap().id
}

#[test]
fn a_character_can_make_a_preparation_that_finishes_independently_of_work() {
    let opening = scenario("craft").snapshot();
    let physical = &opening.core.physical;
    let edible = good(physical, "cured-roots");
    let recipe = physical
        .recipes
        .iter()
        .find(|row| row.name == "salt-roots")
        .unwrap()
        .id;
    let simulate = |workers, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(opening.clone()).unwrap();
                assert_eq!(host.start_controlled_work(2, recipe, 1).unwrap(), (1, true));
                let mut maturity = Vec::new();
                for &step in steps {
                    let run = host.advance_continuous(step, &[]).unwrap();
                    assert_eq!(run.elapsed_millis, step);
                    assert_eq!(run.failure, None);
                    maturity.extend(run.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::Maturation { at_millis, events } => Some((at_millis, events)),
                        _ => None,
                    }));
                    host = CoreHost::resume(
                        serde_json::from_str(&serde_json::to_string(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                (maturity, host.snapshot().core.physical.custody)
            })
    };
    let whole = simulate(1, &[5 * DAY + WORK_TIME]);
    assert_eq!(
        whole,
        simulate(4, &[1, WORK_TIME - 1, 5 * DAY - WORK_TIME, WORK_TIME])
    );
    assert_eq!(whole.0.len(), 2);
    assert_eq!(whole.0[0].0, u128::from(5 * DAY));
    assert_eq!(whole.0[0].1.len(), 2);
    assert_eq!(
        whole.0[0].1[0].outcome,
        MaturationOutcome::UnsuitableTemperature
    );
    assert_eq!(whole.0[0].1[1].outcome, MaturationOutcome::Matured);
    assert_eq!(whole.0[1].0, u128::from(5 * DAY + WORK_TIME));
    assert_eq!(whole.0[1].1[0].outcome, MaturationOutcome::Matured);
    let mut dated: Vec<_> = whole
        .1
        .containers
        .iter()
        .flat_map(|row| &row.lots)
        .filter(|lot| lot.good == edible)
        .map(|lot| (lot.quantity, lot.expires_at))
        .collect();
    dated.sort_unstable_by_key(|row| row.1);
    assert_eq!(
        dated,
        [(5, Some(185 * DAY)), (24, Some(185 * DAY + WORK_TIME))]
    );
}

#[test]
fn moving_a_preparation_changes_its_conditions_and_preserves_its_deadline() {
    let physical = scenario("transfer").snapshot().core.physical;
    let preparation = good(&physical, "curing-root-batch");
    let mut runtime = Runtime::compile(physical).unwrap();
    assert_eq!(runtime.tick_millis(), QUANTUM);
    assert_eq!(runtime.work_interval().unwrap().elapsed_millis, 0);
    runtime.progress_work_interval(2 * DAY).unwrap();
    let moving = [Transaction {
        id: 1,
        movements: vec![Movement {
            id: 1,
            from: Place::Person(1),
            to: Some(Place::Person(2)),
            good: preparation,
            quantity: 3,
            batch: None,
            item: None,
        }],
        creations: vec![],
    }];
    let draft = runtime.prepare_interval_transfer(&moving, 7).unwrap();
    assert!(
        runtime
            .commit_interval_consumption(draft)
            .unwrap()
            .transactions[0]
            .accepted
    );
    assert_eq!(runtime.next_expiry_after(), Some(3 * DAY));
    assert_eq!(runtime.progress_work_interval(3 * DAY).unwrap(), 3 * DAY);
    let output: Vec<(Place, u64, MaturationOutcome)> = runtime
        .take_elapsed_expiry()
        .maturation
        .into_iter()
        .map(
            |MaturationEvent {
                 place,
                 quantity,
                 outcome,
                 ..
             }| (place, quantity, outcome),
        )
        .collect();
    assert_eq!(
        output,
        [
            (
                Place::Person(1),
                1,
                MaturationOutcome::UnsuitableTemperature
            ),
            (Place::Person(2), 8, MaturationOutcome::Matured),
        ]
    );
}
