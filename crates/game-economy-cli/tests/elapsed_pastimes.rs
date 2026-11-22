use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_lots::Place;
use game_pastimes::Outcome;
use std::path::Path;

fn scene(quantum: u64) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("content/scenarios/economy-pastimes.toml"))
        .unwrap()
        .replace("tick_millis = 3600000", &format!("tick_millis = {quantum}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-pastimes-{}-{quantum}.toml",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

fn carry(host: &CoreHost, person: u64, name: &str) -> u64 {
    let state = host.snapshot();
    let good = state
        .core
        .physical
        .goods
        .iter()
        .find(|row| row.name == name)
        .unwrap()
        .id;
    state
        .core
        .physical
        .custody
        .containers
        .iter()
        .find(|row| row.place == Place::Person(person))
        .unwrap()
        .lots
        .iter()
        .filter(|row| row.good == good)
        .map(|row| row.quantity)
        .sum()
}

#[test]
fn personally_chosen_creative_practice_finishes_at_an_elapsed_deadline() {
    let run = |workers, quantum, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene(quantum);
                let mut events = Vec::new();
                for &chunk in chunks {
                    let run = host.advance_continuous(chunk, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    events.extend(
                        run.events
                            .into_iter()
                            .filter_map(|event| {
                                if let CoreRunEvent::Pastimes(events) = event {
                                    Some(events)
                                } else {
                                    None
                                }
                            })
                            .flatten(),
                    );
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let state = host.snapshot();
                let needs: Vec<_> = state
                    .purpose
                    .unwrap()
                    .owner
                    .people
                    .into_iter()
                    .map(|row| row.deprivation)
                    .collect();
                assert!(state.pastimes.as_ref().unwrap().active.is_empty());
                (events, needs, state.pastimes.unwrap().minds)
            })
    };
    let single = run(1, 3600000, &[1800001]);
    assert_eq!(single, run(6, 733333, &[115, 99999, 640000, 1059887]));
    assert_eq!(
        single
            .0
            .iter()
            .filter(|row| row.outcome == Outcome::Started)
            .count(),
        2
    );
    let completions: Vec<_> = single
        .0
        .iter()
        .filter(|row| row.outcome == Outcome::Completed)
        .map(|row| (row.person, row.at_millis))
        .collect();
    assert_eq!(completions, [(1, 1800000), (2, 1800000)]);
    assert_eq!(single.1, [6, 13, 10]);
}

#[test]
fn creative_materials_are_atomic_and_reusable_instruments_stay_in_custody() {
    let mut host = scene(3600001);
    assert!(host.start_pastime_controlled(3, 3).unwrap().is_none());
    assert_eq!(carry(&host, 3, "pigment"), 1);

    assert!(host.start_pastime_controlled(4, 3).unwrap().is_some());
    assert_eq!(carry(&host, 4, "pigment"), 0);
    assert_eq!(carry(&host, 4, "bark"), 0);

    assert!(host.start_pastime_controlled(3, 2).unwrap().is_some());
    assert_eq!(carry(&host, 3, "reed-pipe"), 1);
    let first = host.advance_continuous(120000, &[]).unwrap();
    assert_eq!(first.failure, None);
    assert_eq!(
        host.stop_pastime_controlled(3).unwrap().unwrap().at_millis,
        120000
    );
    assert_eq!(host.advance_continuous(500000, &[]).unwrap().failure, None);
    assert_eq!(carry(&host, 3, "reed-pipe"), 1);
    let state = host.snapshot();
    assert_eq!(state.purpose.unwrap().owner.people[2].deprivation, 10);
}

#[test]
fn a_person_reuses_practice_across_activities_with_the_same_skill() {
    use game_economy_player::interval::Player;

    let run = |workers, quantum, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene(quantum);
                assert!(Player::new(&mut host, 3).unwrap().start_pastime(2).unwrap());
                assert_eq!(host.advance_continuous(600001, &[]).unwrap().failure, None);
                assert!(Player::new(&mut host, 3).unwrap().start_pastime(1).unwrap());
                for &chunk in chunks {
                    assert_eq!(host.advance_continuous(chunk, &[]).unwrap().failure, None);
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let mut player = Player::new(&mut host, 3).unwrap();
                assert!(player.start_pastime(2).unwrap());
                assert_eq!(player.advance(100000).unwrap().failure, None);
                assert!(player.stop_pastime().unwrap());
                let records = player.pastime_practice().unwrap();
                assert_eq!(records.len(), 1);
                assert_eq!(
                    (
                        records[0].skill.as_str(),
                        records[0].completed,
                        records[0].millis
                    ),
                    ("music", 2, 2400000)
                );
                records.to_vec()
            })
    };
    assert_eq!(
        run(1, 3700001, &[1800001]),
        run(6, 541003, &[121, 540000, 1259880])
    );
}
