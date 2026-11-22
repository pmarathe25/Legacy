use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;

#[test]
fn unanswered_work_requests_expire_at_their_exact_elapsed_deadline() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scenario = content.join("scenarios/economy-elapsed-work-requests.toml");
    let execute = |workers, quantum, advance: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = game_economy_cli::core_scenario(&scenario, &content)
                    .unwrap()
                    .snapshot();
                state.core.physical.tick_millis = quantum;
                let mut host = CoreHost::resume(state).unwrap();
                let mut expiries = Vec::new();
                let mut now = 0;
                for &step in advance {
                    let run = host.advance_continuous(step, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    expiries.extend(run.events.into_iter().filter_map(|event| {
                        if let CoreRunEvent::WorkRequestExpiry { at_millis, events } = event {
                            assert_eq!(events.len(), 1);
                            Some((at_millis, events[0].request, events[0].worker))
                        } else {
                            None
                        }
                    }));
                    now += step;
                    let state = host.snapshot();
                    let owner = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                    if now == 49 {
                        assert_eq!(owner.pending.len(), 1);
                        let clock = owner.pending[0].elapsed.unwrap();
                        assert_eq!((clock.posted_at, clock.ends_at), (17, 50));
                        assert_eq!(owner.elapsed.unwrap().next_expiry_millis, Some(50));
                    }
                    if now == 50 {
                        assert!(owner.pending.is_empty());
                        assert_eq!(owner.elapsed.unwrap().next_expiry_millis, None);
                    }
                    assert!(state.core.physical.work.is_empty());
                    let bytes = serde_json::to_vec(&state).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&bytes).unwrap()).unwrap();
                }
                assert_eq!(expiries, [(50, 1, 2)]);
                let saved = host.snapshot().autowork.unwrap().requests.unwrap();
                let next = saved.pending[0].elapsed.unwrap();
                assert_eq!((next.posted_at, next.ends_at), (68, 101));
                expiries
            })
    };
    assert_eq!(
        execute(1, 37, &[49, 1, 19]),
        execute(6, 50, &[13, 3, 33, 1, 7, 12])
    );
}
