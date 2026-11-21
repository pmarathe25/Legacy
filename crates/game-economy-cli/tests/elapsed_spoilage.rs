use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_lots::{Container, Place, Spoilage};

const DAY: u64 = 86_400_000;
const FINISH: u64 = 1_200_000;

const SCENARIO: &str = r#"
seed = 7
tick_millis = QUANTUM
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

[[regions]]
id = 1
stock = { bread = 7, flour = 12, timber = 2 }

[[people]]
id = 1
region = 1
health = 100
hunger = 0
controlled = true
willing_to_work = false
seek_meals = false
"#;

#[derive(Debug, PartialEq, Eq)]
enum Receipt {
    Spoiled(u128, Vec<Spoilage>),
    Work { finished: bool, interrupted: bool },
}

#[test]
fn shelf_life_starts_when_work_finishes_across_steps_and_load() {
    let whole = simulate(1, 7 * DAY, false, &[5 * DAY + FINISH]);
    let split = simulate(
        4,
        3 * DAY,
        false,
        &[59, FINISH - 59, 5 * DAY - FINISH, FINISH],
    );
    assert_eq!(whole, split);
    let (events, bread) = whole;
    assert_eq!(bread, 0);
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0],
        Receipt::Work {
            finished: true,
            interrupted: false
        }
    );
    assert!(matches!(
        &events[1], Receipt::Spoiled(at, lots)
        if *at == u128::from(5 * DAY) && lots.iter().map(|lot| lot.quantity).sum::<u64>() == 7
    ));
    assert!(matches!(
        &events[2], Receipt::Spoiled(at, lots)
        if *at == u128::from(5 * DAY + FINISH) && lots.iter().map(|lot| lot.quantity).sum::<u64>() == 12
    ));
}

#[test]
fn reserved_input_expires_before_work_can_consume_it_at_the_same_instant() {
    let (events, bread) = simulate(4, 7 * DAY, true, &[FINISH - 1, 1]);
    assert_eq!(bread, 7);
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0], Receipt::Spoiled(at, lots)
        if *at == u128::from(FINISH)
            && lots.len() == 1
            && lots[0].place == Place::JobInput(1)
            && lots[0].quantity == 12
    ));
    assert_eq!(
        events[1],
        Receipt::Work {
            finished: false,
            interrupted: true
        }
    );
}

fn simulate(
    workers: usize,
    quantum: u64,
    expire_flour: bool,
    parts: &[u64],
) -> (Vec<Receipt>, u64) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = std::env::temp_dir().join(format!(
        "legacy-elapsed-spoilage-{}-{workers}-{quantum}-{expire_flour}.toml",
        std::process::id()
    ));
    std::fs::write(&path, SCENARIO.replace("QUANTUM", &quantum.to_string())).unwrap();
    let opening = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let mut opening = opening.unwrap().snapshot();
    let id = |name| {
        opening
            .core
            .physical
            .goods
            .iter()
            .find(|entry| entry.name == name)
            .unwrap()
            .id
    };
    let bread = id("bread");
    let flour = id("flour");
    if expire_flour {
        let holder = opening
            .core
            .physical
            .custody
            .containers
            .iter_mut()
            .find(|row| {
                row.place
                    == Place::Stock {
                        region: 1,
                        good: flour,
                    }
            })
            .unwrap();
        let mut lots = holder.lots.clone();
        lots[0].expires_at = Some(FINISH);
        *holder = Container::new(holder.place, holder.capacity, lots).unwrap();
    }
    let recipe = opening
        .core
        .physical
        .recipes
        .iter()
        .find(|row| row.name == "bake-bread")
        .unwrap()
        .id;
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut host = CoreHost::resume(opening).unwrap();
            assert_eq!(host.start_controlled_work(1, recipe, 1).unwrap(), (1, true));
            let mut receipts = Vec::new();
            for &part in parts {
                let run = host.advance_continuous(part, &[]).unwrap();
                assert_eq!(run.failure, None);
                assert_eq!(run.elapsed_millis, part);
                receipts.extend(run.events.into_iter().filter_map(|event| match event {
                    CoreRunEvent::Spoilage { at_millis, lots } => {
                        Some(Receipt::Spoiled(at_millis, lots))
                    }
                    CoreRunEvent::Work(work) => Some(Receipt::Work {
                        finished: work.finished.contains(&1),
                        interrupted: work.interrupted.contains(&1),
                    }),
                    _ => None,
                }));
                let archive = serde_json::to_string(&host.snapshot()).unwrap();
                host = CoreHost::resume(serde_json::from_str(&archive).unwrap()).unwrap();
            }
            let bread = host
                .snapshot()
                .core
                .physical
                .custody
                .containers
                .into_iter()
                .flat_map(|container| container.lots)
                .filter(|lot| lot.good == bread)
                .map(|lot| lot.quantity)
                .sum();
            (receipts, bread)
        })
}
