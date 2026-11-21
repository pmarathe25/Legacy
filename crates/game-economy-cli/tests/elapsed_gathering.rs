use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_lots::Place;
use std::path::Path;

fn scenario(quantum: u64, profiled: bool) -> CoreHost {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut source = r#"
seed = 7
tick_millis = QUANTUM
decision_period_millis = 250
stock_capacity = 100
food_relief_divisor = 20
elapsed_spoilage = true

[rules]
hunger_per_tick = 1
hunger_rate = { units = 1, per_millis = 30000 }
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 10
max_work_hunger = 20

[[regions]]
id = 1
stock = { bread = 0 }

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

[gathering]
elapsed_renewal = true
elapsed_actions = true
max_hunger = 20
policies = [{ person = 1, node = 1, willing = true }, { person = 2, node = 1, willing = true }]

[[gathering.nodes]]
id = 1
region = 1
good = "bread"
capacity = 1
available = 1
yield_per_action = 1
renewal_rate = { units = 1, per_millis = 1000 }
"#
    .replace("QUANTUM", &format!("{quantum}"));
    if profiled {
        source.push_str(
            r#"
[[personal_choices]]
id = 1
idle = 0
preferences = [{ domain = 20, enabled = true, bias = 2 }]

[[personal_choices]]
id = 2
idle = 0
preferences = [{ domain = 20, enabled = true, bias = 2 }]
"#,
        );
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-elapsed-gathering-{}-{}.toml",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, source).unwrap();
    let host = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    host
}

#[test]
fn finite_gathering_uses_decision_time_and_exact_renewal_across_physical_quanta() {
    let run = |workers, quantum, steps: &[u64], profiled| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scenario(quantum, profiled);
                let mut harvests = Vec::new();
                for &step in steps {
                    let advanced = host.advance_continuous(step, &[]).unwrap();
                    assert_eq!(advanced.failure, None);
                    harvests.extend(advanced.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::ElapsedGathering { at_millis, events } => {
                            Some((at_millis, events))
                        }
                        _ => None,
                    }));
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                assert_eq!(
                    harvests.iter().map(|row| row.0).collect::<Vec<_>>(),
                    [0, 1000, 2000]
                );
                assert!(
                    harvests.iter().all(|row| row.1.len() == 1
                        && row.1[0].quantity == 1
                        && row.1[0].node == 1)
                );
                let state = host.snapshot();
                let holdings: Vec<_> = state
                    .core
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|row| matches!(row.place, Place::Person(_)))
                    .map(|row| (row.place, row.lots.clone()))
                    .collect();
                assert_eq!(
                    holdings
                        .iter()
                        .flat_map(|row| &row.1)
                        .map(|lot| lot.quantity)
                        .sum::<u64>(),
                    3
                );
                (harvests, holdings, state.gathering.unwrap().nodes)
            })
    };
    for profiles in [false, true] {
        assert_eq!(
            run(1, 5000, &[2403], profiles),
            run(6, 1000, &[83, 921, 292, 1107], profiles)
        );
    }
}

#[test]
fn a_controlled_gatherer_takes_one_real_yield_and_leaves_other_actors_alone() {
    use game_economy_player::interval::Player;

    let mut state = scenario(5000, false).snapshot();
    state.core.people[0].controlled = true;
    let mut host = CoreHost::resume(state).unwrap();
    let mut player = Player::new(&mut host, 1).unwrap();
    assert_eq!(player.view().unwrap().accessible_resources[0].available, 1);
    assert!(player.gather(987).is_err());
    let receipt = player.gather(1).unwrap().unwrap();
    assert_eq!((receipt.person, receipt.node, receipt.quantity), (1, 1, 1));
    assert_eq!(player.view().unwrap().accessible_resources[0].available, 0);
    assert!(player.gather(1).unwrap().is_none());
    let state = host.snapshot();
    let other = state
        .core
        .physical
        .custody
        .containers
        .iter()
        .find(|row| row.place == Place::Person(2))
        .unwrap();
    assert_eq!(other.used, 0);
    let mut host =
        CoreHost::resume(serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap())
            .unwrap();
    assert!(
        Player::new(&mut host, 1)
            .unwrap()
            .gather(1)
            .unwrap()
            .is_none()
    );
}
