use game_parallel_work_requests::{Requests, WorkLink};

fn state(limit: usize) -> Requests {
    Requests {
        inherit_births: false,
        next_id: 100_002,
        memory_limit: limit,
        work_links: vec![],
        memories: vec![],
        preferences: vec![],
        askers: vec![],
        pending: vec![],
        events: vec![],
    }
}

#[test]
fn concentrated_outcomes_are_bounded_and_deterministic() {
    let mut input = state(2);
    input.work_links = (2..=10_001)
        .map(|worker| WorkLink {
            request: worker,
            requester: 1,
            worker,
            job: worker,
            region: 1,
            recipe: 1,
        })
        .collect();
    let completed: Vec<_> = (2..=10_001).map(|job| (job, true)).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut requests = input.clone();
                requests.resolve(&completed, 1, |_| true);
                assert!(requests.work_links.is_empty());
                assert_eq!(requests.memories.len(), 1);
                assert_eq!(requests.memories[0].worker, 2);
                requests
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn personal_attention_and_retention_are_bounded() {
    let mut requests = state(2);
    for worker in 2..=5 {
        requests.work_links.push(WorkLink {
            request: worker,
            requester: 1,
            worker,
            job: worker,
            region: 1,
            recipe: 1,
        });
        requests.resolve(
            &[(worker, true)],
            if worker == 3 { 2 } else { worker },
            |_| true,
        );
    }
    assert_eq!(requests.memories.len(), 2);
    assert_eq!(
        requests
            .memories
            .iter()
            .map(|r| r.worker)
            .collect::<Vec<_>>(),
        vec![4, 5]
    );
}
