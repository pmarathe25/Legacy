use game_practices::{Decision, Mind, Practices, Proposal, Received, Reporting};

fn mind(person: u64) -> Mind {
    Mind {
        person,
        invent: true,
        approve: true,
        willing: true,
        ask_willing: true,
        adjust_after_failure: false,
        pressure_threshold: None,
        shortage_threshold: 5,
        minimum_shortages: 2,
        review_interval: 2,
        proposal_lifetime: 8,
        reserve_portions: 1,
        quantity_weight: 0,
        reserve_weight: 0,
        giving_experience_weight: 0,
        shortage_experience_weight: 0,
        proposal_affinity_weight: 0,
        acceptance_threshold: 0,
        value_learning: None,
        precedent: None,
        work: None,
        reporting: Default::default(),
        report_attitude: Default::default(),
        unanswered_report_belief: None,
        require_believed_ready: false,
        next_review: 0,
        sequence: 0,
        cases: vec![].into(),
        authored: vec![].into(),
        inbox: vec![].into(),
        sent_reports: vec![].into(),
        reports: vec![].into(),
    }
}

fn workload(count: u64, reporters: u64) -> (Practices, Vec<(u64, u64)>) {
    assert!(count >= 2);
    let mut people: Vec<_> = (1..=count)
        .map(|person| {
            let mut individual = mind(person);
            individual.invent = false;
            if person > 1 {
                if person - 1 <= reporters {
                    individual.reporting = Reporting::Ready;
                }
                individual.inbox = vec![Received {
                    proposal: Proposal {
                        author: 1,
                        recipient: person,
                        sequence: person,
                        good: 4,
                        quantity: 1,
                        reserve: 0,
                        ask_hunger: 0,
                        created: 1,
                        expires: 8,
                    },
                    delivered: 1,
                    decision: Decision::Adopted,
                    approved: true,
                }]
                .into();
            }
            individual
        })
        .collect();
    people[0].invent = false;
    let contacts = (2..=count).map(|person| (1, person)).collect();
    (Practices { minds: people }, contacts)
}

fn run(initial: &Practices, contacts: &[(u64, u64)]) -> Practices {
    initial
        .prepare_with_evidence(contacts, &[], 2, |_, _| 0, |_| true, |_, _, _| false)
        .unwrap()
}

#[test]
fn concentrated_readiness_delivery_is_worker_invariant() {
    let (initial, contacts) = workload(4096, 4095);
    let execute = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| run(&initial, &contacts))
    };
    let result = execute(1);
    assert_eq!(result, execute(4));
    assert_eq!(result.minds[0].reports.len(), game_practices::MAX_PROPOSALS);
    assert_eq!(
        result
            .minds
            .iter()
            .filter(|person| !person.sent_reports.is_empty())
            .count(),
        game_practices::MAX_PROPOSALS
    );
}

#[test]
#[ignore = "performance measurement"]
fn concentrated_readiness_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let reporters = std::env::var("LEGACY_REPORTERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(count - 1)
        .min(count - 1);
    let (initial, contacts) = workload(count, reporters);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let started = std::time::Instant::now();
                let result = run(&initial, &contacts);
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
                assert_eq!(
                    result.minds[0].reports.len(),
                    game_practices::MAX_PROPOSALS.min(reporters as usize)
                );
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} reporters={reporters} boundaries=1 elapsed={:.6}",
                samples[1]
            );
        });
}
