use game_economy::core_interval::CoreHost;
use std::path::Path;

#[test]
fn the_same_blocked_work_goal_reconsiders_after_the_same_elapsed_delay() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scene = content.join("scenarios/economy-elapsed-goal-review.toml");
    let run = |workers, quantum, opening: &[u64], closing: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = game_economy_cli::core_scenario(&scene, &content)
                    .unwrap()
                    .snapshot();
                state.core.physical.tick_millis = quantum;
                let mut host = CoreHost::resume(state).unwrap();
                let advance = |host: &mut CoreHost, steps: &[u64]| {
                    for &step in steps {
                        let result = host.advance_continuous(step, &[]).unwrap();
                        assert_eq!(result.failure, None);
                        let save = serde_json::to_vec(&host.snapshot()).unwrap();
                        *host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                    }
                };
                let deadline = |host: &CoreHost| {
                    let saved = host.snapshot();
                    let goals = saved.goals.unwrap();
                    assert_eq!(goals.people[0].progress, 0);
                    assert!(saved.core.physical.work.is_empty());
                    goals.review_clock.unwrap().deadlines[0].at_millis
                };
                advance(&mut host, opening);
                assert_eq!(deadline(&host), 146);
                advance(&mut host, closing);
                assert_eq!(deadline(&host), 299);
                deadline(&host)
            })
    };
    assert_eq!(run(1, 37, &[145], &[10]), run(6, 159, &[17, 128], &[5, 5]));
}
