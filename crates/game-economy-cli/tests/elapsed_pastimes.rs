use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_lots::Place;
use game_pastimes::Outcome;
use std::num::NonZeroU64;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

fn scene(quantum: u64) -> CoreHost {
    load("economy-pastimes.toml", quantum)
}

fn load(scenario: &str, quantum: u64) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("content/scenarios").join(scenario))
        .unwrap()
        .replace("tick_millis = 3600000", &format!("tick_millis = {quantum}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-pastimes-{}-{}-{quantum}.toml",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, text).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

#[test]
fn listeners_can_replay_vary_or_ignore_a_personally_composed_phrase() {
    use game_pastimes::music::Choice;
    let run = |workers, quantum, intervals: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = load("economy-musical-phrases.toml", quantum);
                let mut phrases = Vec::new();
                for &elapsed in intervals {
                    let run = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    phrases.extend(
                        run.events
                            .into_iter()
                            .filter_map(|event| match event {
                                CoreRunEvent::MusicalPhrases(rows) => Some(rows),
                                _ => None,
                            })
                            .flatten(),
                    );
                    let checkpoint = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&checkpoint).unwrap()).unwrap();
                }
                assert_eq!(phrases.len(), 4);
                let original = phrases[0];
                let exact = phrases[1];
                let variation = phrases[2];
                let replay = phrases[3];
                assert_eq!(
                    (original.person, original.choice, original.at_millis),
                    (1, Choice::Composed, 600000)
                );
                assert_eq!(
                    (exact.person, exact.choice, exact.from),
                    (2, Choice::Adopted, Some(1))
                );
                assert_eq!(exact.phrase, original.phrase);
                assert_eq!(
                    (variation.person, variation.choice, variation.from),
                    (3, Choice::Varied, Some(1))
                );
                assert_eq!(
                    variation
                        .phrase
                        .iter()
                        .zip(&original.phrase)
                        .filter(|(left, right)| left != right)
                        .count(),
                    1
                );
                assert_eq!(
                    (replay.person, replay.choice, replay.from, replay.at_millis),
                    (2, Choice::Replayed, Some(1), 1200000)
                );
                assert_eq!(replay.phrase, original.phrase);
                let state = host.snapshot();
                let culture = state.pastimes.unwrap().culture.unwrap();
                assert_eq!(culture.remembered(4).count(), 1);
                assert!(culture.music.as_ref().unwrap().remembered(4).is_empty());
                phrases
            })
    };
    assert_eq!(
        run(1, 3600000, &[1200000]),
        run(6, 473003, &[115000, 358002, 234000, 492998])
    );
}

#[test]
fn ending_a_performance_early_leaves_no_phrase_to_learn() {
    let mut host = load("economy-musical-phrases.toml", 3_600_000);
    let mut opening = host.snapshot();
    opening.core.people[0].controlled = true;
    host = CoreHost::resume(opening).unwrap();
    assert!(host.start_pastime_controlled(1, 1).unwrap().is_some());
    let first = host.advance_continuous(310_000, &[]).unwrap();
    assert_eq!(first.failure, None);
    host = CoreHost::resume(
        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(host.stop_pastime_controlled(1).unwrap().is_some());
    let remainder = host.advance_continuous(890_000, &[]).unwrap();
    assert_eq!(remainder.failure, None);
    for event in first.events.iter().chain(&remainder.events) {
        assert!(!matches!(event, CoreRunEvent::MusicalPhrases(_)));
    }
}

#[test]
fn listeners_choose_sustained_attention_and_an_interruption_breaks_it() {
    use game_economy::LocalEndpoint;
    use game_pastimes::Heard;

    let run = |workers, quantum, chunks: &[u64], leaves| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = load("economy-pastime-audience.toml", quantum);
                let mut receipts = Vec::new();
                let advance = |host: &mut CoreHost, elapsed, receipts: &mut Vec<Heard>| {
                    let run = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    receipts.extend(run.events.into_iter().flat_map(|event| match event {
                        CoreRunEvent::HeardPerformances(rows) => rows,
                        _ => Vec::new(),
                    }));
                    *host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                };
                let taking_part = |host: &CoreHost| {
                    host.snapshot()
                        .pastimes
                        .unwrap()
                        .culture
                        .unwrap()
                        .attendance
                        .into_iter()
                        .map(|row| row.listener)
                        .collect::<Vec<_>>()
                };
                advance(&mut host, 100000, &mut receipts);
                assert_eq!(taking_part(&host), [2, 5]);
                assert_eq!(host.local_performances_controlled(4).unwrap()[0].person, 1);
                let long = NonZeroU64::new(500001).unwrap();
                let minimum = NonZeroU64::new(250000).unwrap();
                assert!(!host.join_performance_controlled(4, 1, long).unwrap());
                assert!(host.join_performance_controlled(4, 1, minimum).unwrap());
                assert!(host.join_performance_controlled(6, 1, minimum).unwrap());
                assert_eq!(taking_part(&host), [2, 4, 5, 6]);
                advance(&mut host, 100000, &mut receipts);
                assert_eq!(taking_part(&host), [2, 4, 6]);
                if leaves {
                    assert!(host.leave_performance_controlled(6).unwrap());
                }
                host.walk_controlled(4, LocalEndpoint::Store { id: 50 })
                    .unwrap();
                assert!(host.current_performance_controlled(4).unwrap().is_none());
                advance(&mut host, 30000, &mut receipts);
                assert!(host.local_performances_controlled(4).unwrap().is_empty());
                host.walk_controlled(4, LocalEndpoint::Stock).unwrap();
                advance(&mut host, 30000, &mut receipts);
                assert_eq!(host.local_performances_controlled(4).unwrap().len(), 1);
                assert!(host.current_performance_controlled(4).unwrap().is_none());
                for &elapsed in chunks {
                    advance(&mut host, elapsed, &mut receipts);
                }
                assert_eq!(host.core().simulated_millis(), 600000);
                assert!(taking_part(&host).is_empty());
                assert!(host.heard_performances_controlled(4).unwrap().is_empty());
                assert_eq!(
                    host.heard_performances_controlled(6).unwrap().len(),
                    usize::from(!leaves)
                );
                receipts
                    .into_iter()
                    .map(|row| {
                        (
                            row.listener,
                            row.performer,
                            row.at_millis,
                            row.attended_millis,
                        )
                    })
                    .collect::<Vec<_>>()
            })
    };
    let uninterrupted = run(1, 3600000, &[340000], false);
    assert_eq!(
        uninterrupted,
        run(6, 197003, &[13017, 210011, 116972], false)
    );
    assert_eq!(
        uninterrupted,
        [(2, 1, 600000, Some(600000)), (6, 1, 600000, Some(500000))]
    );
    assert_eq!(
        run(4, 260019, &[340000], true),
        [(2, 1, 600000, Some(600000))]
    );
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

#[test]
fn hearing_a_local_performance_is_independent_and_can_change_future_practice() {
    let simulate = |workers, quantum, chunks: &[u64], present| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene(quantum);
                assert_eq!(
                    host.advance_continuous(1_800_001, &[]).unwrap().failure,
                    None
                );
                host.listen_for_controlled(4, 2, 20).unwrap();
                host.present_performance_controlled(3, 2, present).unwrap();
                host.start_pastime_controlled(3, 2).unwrap().unwrap();
                let mut heard = Vec::new();
                for &elapsed in chunks {
                    let run = host.advance_continuous(elapsed, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    heard.extend(
                        run.events
                            .into_iter()
                            .filter_map(|event| {
                                if let CoreRunEvent::HeardPerformances(receipts) = event {
                                    Some(receipts)
                                } else {
                                    None
                                }
                            })
                            .flatten(),
                    );
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                assert_eq!(
                    host.heard_performances_controlled(4).unwrap(),
                    heard
                        .iter()
                        .filter(|row| row.listener == 4)
                        .copied()
                        .collect::<Vec<_>>()
                );
                let until_choice = 3_600_000 - host.core().simulated_millis() as u64;
                let result = host.advance_continuous(until_choice, &[]).unwrap();
                assert_eq!(result.failure, None);
                let took_up_music = result.events.iter().any(|event| {
                    matches!(
                        event, CoreRunEvent::Pastimes(rows) if rows.iter().any(|row|
                        row.person == 1 && row.activity == 2 && row.outcome == Outcome::Started)
                    )
                });
                (heard, took_up_music)
            })
    };
    let presented = simulate(1, 3_600_000, &[600_000], true);
    assert_eq!(presented, simulate(6, 733_333, &[12, 599_987, 1], true));
    assert_eq!(
        presented
            .0
            .iter()
            .map(|row| (row.listener, row.performer, row.activity, row.at_millis))
            .collect::<Vec<_>>(),
        [(1, 3, 2, 2_400_001), (4, 3, 2, 2_400_001)]
    );
    assert!(presented.1);
    assert_eq!(
        simulate(1, 3_600_000, &[600_000], false),
        (Vec::new(), false)
    );
}
