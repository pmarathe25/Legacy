use game_economy::core_interval::{CoreHost, CoreRunEvent, EpisodeCue};
use game_economy_player::interval::Player;
use std::path::Path;

fn scene(quantum: u64, case: &str) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("content/scenarios/economy-episodic-memory.toml"))
        .unwrap()
        .replace("tick_millis = 3600000", &format!("tick_millis = {quantum}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-episodic-{}-{case}-{quantum}.toml",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

fn resume(host: &CoreHost) -> CoreHost {
    CoreHost::resume(
        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap()).unwrap(),
    )
    .unwrap()
}

const MUSIC: EpisodeCue = EpisodeCue::CreativePractice { activity: 1 };

#[test]
fn an_actual_meeting_produces_different_private_memories() {
    let mut host = scene(479009, "encounter");
    assert!(host.recall_episodes_controlled(1, None).is_err());
    let run = host.advance_continuous(200001, &[]).unwrap();
    assert_eq!(run.failure, None);
    assert!(run.events.iter().any(|event| matches!(event,
        CoreRunEvent::ElapsedEncounters { at_millis: 200000, meetings, .. }
        if meetings.iter().any(|row| row.a.min(row.b) == 1 && row.a.max(row.b) == 2)
    )));
    let memories = host.snapshot().knowledge.unwrap().episodes.unwrap();
    assert_eq!(
        memories.recall(1, Some(2), 200001)[0].precise_time,
        Some(200000)
    );
    assert!(memories.recall(2, None, 200001).is_empty());
    assert!(host.recall_episodes_controlled(3, None).unwrap().is_empty());
}

#[test]
fn elapsed_memory_survives_save_and_caller_partitions_under_different_worker_counts() {
    let run = |workers, quantum, practice: &[u64], rest: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene(quantum, "rest");
                assert!(Player::new(&mut host, 3).unwrap().start_pastime(1).unwrap());
                assert!(host.recall_episodes_controlled(3, None).unwrap().is_empty());
                for &chunk in practice {
                    assert_eq!(host.advance_continuous(chunk, &[]).unwrap().failure, None);
                    host = resume(&host);
                }
                let recalled = host.recall_episodes_controlled(3, None).unwrap();
                assert_eq!(recalled[0].cue, MUSIC);
                assert_eq!(recalled[0].precise_time, Some(600000));
                assert!(
                    Player::new(&mut host, 3)
                        .unwrap()
                        .reflect_on_memory(MUSIC)
                        .unwrap()
                );
                host.set_controlled_rest(3, true).unwrap();
                for &chunk in rest {
                    assert_eq!(host.advance_continuous(chunk, &[]).unwrap().failure, None);
                    host = resume(&host);
                }
                host.set_controlled_rest(3, false).unwrap();
                let memory = host.snapshot().knowledge.unwrap().episodes.unwrap();
                let episode = memory.minds[2].memories[0];
                assert_eq!(episode.observed_at, 600000);
                assert_eq!(episode.accessible_until, 1950000);
                assert!(memory.recall(3, None, 1950000).is_empty());
                memory
            })
    };
    assert_eq!(
        run(1, 3600000, &[600001], &[300000]),
        run(6, 479009, &[39, 479100, 120862], &[8600, 291400])
    );
}
