use game_work_requests::{Request, Requests, SuccessReceipt, WorkLink};

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
                assert_eq!(
                    requests.memories[0].last_success,
                    Some(SuccessReceipt { request: 2, job: 2 })
                );
                requests
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn only_personally_witnessed_successes_replace_completion_identity() {
    let mut requests = state(2);
    let link = |request, job| WorkLink {
        request,
        requester: 1,
        worker: 2,
        job,
        region: 1,
        recipe: 1,
    };
    requests.work_links.push(link(3, 11));
    requests.resolve(&[(11, true)], 1, |_| true);
    assert_eq!(
        requests.memories[0].last_success,
        Some(SuccessReceipt {
            request: 3,
            job: 11
        })
    );
    requests.work_links.push(link(4, 12));
    requests.resolve(&[(12, true)], 2, |_| false);
    requests.work_links.push(link(5, 13));
    requests.resolve(&[(13, false)], 3, |_| true);
    assert_eq!(
        requests.memories[0].last_success,
        Some(SuccessReceipt {
            request: 3,
            job: 11
        })
    );
    requests.work_links.push(link(6, 14));
    requests.resolve(&[(14, true)], 4, |_| true);
    assert_eq!(
        requests.memories[0].last_success,
        Some(SuccessReceipt {
            request: 6,
            job: 14
        })
    );
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

#[test]
fn a_witness_distinguishes_refusal_from_a_failed_job() {
    let mut requests = state(2);
    requests.work_links.push(WorkLink {
        request: 1,
        requester: 1,
        worker: 2,
        job: 1,
        region: 1,
        recipe: 1,
    });
    requests.resolve(&[(1, false)], 1, |_| true);
    assert_eq!(requests.memories[0].last_refusal_at, None);
    let request = Request {
        id: 2,
        requester: 1,
        worker: 2,
        region: 1,
        recipe: 1,
        created_at: 1,
        expires: 4,
    };
    requests.observe_refusal(&request, 2);
    assert_eq!(requests.memories[0].last_refusal_at, Some(2));
    assert_eq!(requests.memories[0].failures, 2);
    requests.work_links.push(WorkLink {
        request: 3,
        requester: 1,
        worker: 2,
        job: 3,
        region: 1,
        recipe: 1,
    });
    requests.resolve(&[(3, false)], 3, |_| false);
    assert_eq!(requests.memories[0].last_refusal_at, Some(2));
}
