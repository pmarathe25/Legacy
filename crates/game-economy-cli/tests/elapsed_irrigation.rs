use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_agriculture::Phase;
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

fn farm(quantum: u64) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = r#"
seed = 7
tick_millis = QUANTUM
decision_period_millis = 250
stock_capacity = 100
food_relief_divisor = 20
elapsed_spoilage = true

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
stock = { fresh-water = 4 }

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
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[cultivation]
growth_unit_millis = 1000
max_hunger = 20
policies = []

[[cultivation.fields]]
id = 1
region = 1
crop = "grain"

[irrigation]
water = "fresh-water"
capacity = 3
minimum_growth = 1
evaporation = 1
evaporation_period_millis = 100
elapsed_actions = true
moisture = [{ field = 1, amount = 0 }]
choices = [
    { person = 1, field = 1, quantity = 2, willing = true },
    { person = 2, field = 1, quantity = 2, willing = true },
    { person = 3, field = 1, quantity = 2, willing = false }
]
"#
    .replace("QUANTUM", &quantum.to_string());
    let file = std::env::temp_dir().join(format!(
        "legacy-elapsed-irrigation-{}-{quantum}.toml",
        std::process::id(),
    ));
    std::fs::write(&file, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&file, &root.join("content"));
    std::fs::remove_file(file).unwrap();
    let mut state = loaded.unwrap().snapshot();
    let field = &mut state.agriculture.as_mut().unwrap().fields[0];
    field.phase = Phase::Growing;
    field.remaining = 5;
    CoreHost::resume(state).unwrap()
}

fn water_left(host: &CoreHost) -> u64 {
    let state = host.snapshot();
    let good = state.irrigation.unwrap().water;
    state
        .core
        .physical
        .custody
        .containers
        .iter()
        .find(|row| row.place == Place::Stock { region: 1, good })
        .unwrap()
        .used
}

#[test]
fn chosen_watering_arbitrates_real_fields_at_personal_decision_times() {
    let run = |workers, quantum, pieces: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = farm(quantum);
                let mut observed = Vec::new();
                for &piece in pieces {
                    let result = host.advance_continuous(piece, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    for event in result.events {
                        if let CoreRunEvent::ElapsedWatering { at_millis, events } = event {
                            observed.push((
                                at_millis,
                                events
                                    .into_iter()
                                    .map(|row| (row.person, row.quantity, row.accepted))
                                    .collect::<Vec<_>>(),
                            ));
                        }
                    }
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                assert_eq!(
                    observed.iter().map(|row| row.0).collect::<Vec<_>>(),
                    [0, 250, 500]
                );
                assert_eq!(
                    observed
                        .iter()
                        .map(|row| row.1.iter().filter(|row| row.2).count())
                        .collect::<Vec<_>>(),
                    [1, 1, 0]
                );
                assert!(observed.iter().flat_map(|row| &row.1).all(|row| row.0 != 3));
                assert_eq!(water_left(&host), 0);
                let owner = host.snapshot().irrigation.unwrap();
                assert_eq!(owner.moisture[0].amount, 0);
                (observed, owner.moisture)
            })
    };
    assert_eq!(run(1, 900, &[543]), run(6, 173, &[55, 208, 117, 163]));
}

#[test]
fn a_player_can_water_a_growing_field_without_causing_other_attempts() {
    let mut state = farm(800).snapshot();
    state.core.people[0].controlled = true;
    let mut host = CoreHost::resume(state).unwrap();
    let mut player = Player::new(&mut host, 1).unwrap();
    assert_eq!(
        player.view().unwrap().accessible_fields[0].moisture,
        Some(0)
    );
    let first = player.water(1, 4).unwrap().unwrap();
    assert!(first.accepted);
    assert_eq!(first.quantity, 3);
    assert!(player.water(137, 1).unwrap().is_none());
    assert!(!player.water(1, 1).unwrap().unwrap().accepted);
    assert_eq!(
        player.view().unwrap().accessible_fields[0].moisture,
        Some(3)
    );
    assert_eq!(water_left(&host), 1);
}
