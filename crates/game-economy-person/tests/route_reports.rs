use game_economy_person::travel::{RouteObservation, Travel};

fn report(person: u64, source: u64, tick: u64, open: bool) -> RouteObservation {
    RouteObservation {
        person,
        source,
        destination: source + 1,
        open,
        observed_at: tick,
        estimated_ticks: Some(2),
        witness: Some(person),
        learned_from: None,
    }
}

#[test]
fn batched_route_reports_keep_newest_evidence_and_a_bounded_personal_history() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut travel = Travel {
                    observation_limit: 2,
                    ..Travel::default()
                };
                travel.observations = vec![report(1, 1, 1, false), report(2, 1, 1, true)];
                travel.receive_reports(&[
                    report(1, 1, 1, true),
                    report(1, 2, 2, true),
                    report(1, 3, 3, false),
                    report(2, 1, 0, false),
                ]);
                assert_eq!(
                    travel.observations,
                    vec![
                        report(1, 2, 2, true),
                        report(1, 3, 3, false),
                        report(2, 1, 1, true)
                    ]
                );
                travel
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn independent_replacements_keep_newer_evidence_and_deterministic_ties() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut travel = Travel {
                    observations: vec![
                        report(1, 1, 2, false),
                        report(2, 1, 1, false),
                        report(3, 1, 1, false),
                    ],
                    ..Travel::default()
                };
                travel.receive_reports(&[
                    report(1, 1, 1, true),
                    report(2, 1, 1, true),
                    report(3, 1, 2, true),
                ]);
                assert_eq!(
                    travel.observations,
                    vec![
                        report(1, 1, 2, false),
                        report(2, 1, 1, true),
                        report(3, 1, 2, true)
                    ]
                );
                travel
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn route_report_batch_benchmark() {
    use std::hash::{Hash, Hasher};
    let count: u64 = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let hot = std::env::var_os("LEGACY_HOT").is_some();
    let incoming: Vec<_> = (1..=count)
        .map(|id| report(if hot { 1 } else { id }, 1, 0, id % 2 == 0))
        .collect();
    rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut samples = Vec::new();
        let mut fingerprint = None;
        for _ in 0..4 {
            let mut travel = Travel::default();
            let start = std::time::Instant::now();
            travel.receive_reports(&incoming);
            samples.push(start.elapsed().as_secs_f64());
            assert_eq!(travel.observations.len(), if hot { 1 } else { count as usize });
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            for row in &travel.observations { (row.person, row.source, row.destination, row.open, row.observed_at).hash(&mut hash); }
            let actual = hash.finish();
            if let Some(expected) = fingerprint { assert_eq!(expected, actual); }
            fingerprint = Some(actual);
        }
        samples.remove(0);
        samples.sort_by(f64::total_cmp);
        eprintln!("workers={workers} reports={count} hot={hot} elapsed_median={:.6} fingerprint={:016x}", samples[1], fingerprint.unwrap());
    });
}
