use game_parallel_practices::{Decision, Mind, Practices};

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

fn workload(
    count: u64,
) -> (
    Practices,
    Vec<game_parallel_practices::work::outcome::Start>,
) {
    use game_parallel_practices::work::{
        Mind as WorkMind, Proposal, Received,
        outcome::{Policy, Start},
    };
    assert_eq!(count % 2, 0);
    let mut people: Vec<_> = (1..=count).map(mind).collect();
    let starts = (1..count)
        .step_by(2)
        .map(|author| {
            let worker = author + 1;
            people[(worker - 1) as usize].work = Some(std::sync::Arc::new(WorkMind {
                invent: false,
                approve: true,
                evaluation: None,
                outcome_policy: Some(Policy {
                    wait_ticks: 1,
                    missing_trust: -1,
                    missing_affinity: 1,
                }),
                willing: false,
                ask_willing: false,
                review_interval: 1,
                proposal_lifetime: 8,
                reserve: 0,
                pressure_threshold: None,
                next_review: 0,
                sequence: 0,
                authored: None,
                engagement: None,
                gifts: vec![],
                experiences: vec![],
                inbox: vec![Received {
                    proposal: Proposal {
                        author,
                        sequence: 1,
                        worker,
                        recipe: 7,
                        good: 4,
                        quantity: 1,
                        reserve: 0,
                        created: 1,
                        expires: 8,
                    },
                    delivered: 1,
                    decision: Decision::Adopted,
                    approved: true,
                }],
            }));
            Start {
                requester: author,
                worker,
                recipe: 7,
                request: author,
                job: author,
            }
        })
        .collect();
    (Practices { minds: people }, starts)
}

fn run(
    initial: &Practices,
    starts: &[game_parallel_practices::work::outcome::Start],
) -> (
    Practices,
    Vec<game_parallel_practices::work::outcome::Reaction>,
) {
    use game_parallel_practices::work::outcome::advance;
    let mut next = initial.clone();
    assert!(advance(initial, &mut next, starts, &[], &[], &[], 2).is_empty());
    let previous = next;
    let mut next = previous.clone();
    let finished: Vec<_> = starts.iter().map(|start| start.job).collect();
    assert!(advance(&previous, &mut next, &[], &finished, &[], &[], 3).is_empty());
    let previous = next;
    let mut next = previous.clone();
    let reactions = advance(&previous, &mut next, &[], &[], &[], &[], 4);
    (next, reactions)
}

#[test]
fn many_independent_worker_expectations_are_worker_invariant() {
    let (initial, starts) = workload(4096);
    let execute = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| run(&initial, &starts))
    };
    let (result, reactions) = execute(1);
    assert_eq!((result.clone(), reactions.clone()), execute(4));
    assert_eq!(reactions.len(), 2048);
    assert!(
        result
            .minds
            .iter()
            .filter_map(|person| person.work.as_ref())
            .all(|work| work.experiences[0].assessed)
    );
}

#[test]
#[ignore = "performance measurement"]
fn worker_expectations_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let (initial, starts) = workload(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let started = std::time::Instant::now();
                let (result, reactions) = run(&initial, &starts);
                if round > 0 {
                    samples.push(started.elapsed().as_secs_f64());
                }
                assert_eq!(reactions.len(), count as usize / 2);
                assert!(
                    result
                        .minds
                        .iter()
                        .filter_map(|person| person.work.as_ref())
                        .all(|work| work.experiences[0].assessed)
                );
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} boundaries=3 elapsed={:.6}",
                samples[1]
            );
        });
}
