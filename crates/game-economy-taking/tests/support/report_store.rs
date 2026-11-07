use super::*;

fn report(recipient: u64, speaker: u64, actor: u64, received_at: u64, quantity: u64) -> Report {
    Report {
        recipient,
        speaker,
        actor,
        target: 7,
        good: 1,
        quantity,
        observed_at: 0,
        received_at,
        claimed_outcome: Outcome::Transferred,
        belief: ReportBelief::Unassessed,
        source_speaker: None,
    }
}

fn reference(old: &[Report], mut incoming: Vec<Report>) -> Vec<Report> {
    incoming.sort_unstable_by_key(winner_key);
    let incoming: Vec<_> = incoming
        .chunk_by(|a, b| identity(a) == identity(b))
        .map(|group| group[0].clone())
        .collect();
    let mut all: Vec<_> = old
        .iter()
        .cloned()
        .map(|row| (row, false))
        .chain(incoming.into_iter().map(|row| (row, true)))
        .collect();
    all.sort_unstable_by_key(|(row, new)| (identity(row), Reverse(row.received_at), *new));
    let mut latest: Vec<_> = all
        .chunk_by(|a, b| identity(&a.0) == identity(&b.0))
        .map(|group| group[0].0.clone())
        .collect();
    latest.sort_unstable_by_key(|r| (r.recipient, Reverse(r.received_at), r.speaker, r.actor));
    let mut retained: Vec<_> = latest
        .chunk_by(|a, b| a.recipient == b.recipient)
        .flat_map(|group| group.iter().take(MAX_REPORTS).cloned())
        .collect();
    retained.sort_unstable_by_key(identity);
    retained
}

fn leaf(store: &ReportStore, recipient: u64) -> Option<&Arc<Node>> {
    let mut node = store.root.as_ref()?;
    for depth in 0..DEPTH {
        let Node::Branch(children) = node.as_ref() else {
            return None;
        };
        node = &children[children
            .binary_search_by_key(&digit(recipient, depth), |c| c.0)
            .ok()?]
        .1;
    }
    Some(node)
}

#[test]
fn paged_updates_match_flat_retention_across_workers_and_boundaries() {
    let recipients = [0, 1, 31, 32, 255, 256, 4096, u64::MAX];
    let old: Vec<_> = recipients
        .into_iter()
        .flat_map(|recipient| (0..64).map(move |speaker| report(recipient, speaker, 1, 2, 1)))
        .collect();
    let mut incoming = Vec::new();
    let mut random = 0x42u64;
    for _ in 0..12_000 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let recipient = recipients[(random as usize) % recipients.len()];
        let speaker = (random >> 8) % 150;
        let actor = 1 + (random >> 24) % 2;
        let received = (random >> 32) % 5;
        let quantity = 1 + (random >> 40) % 5;
        incoming.push(report(recipient, speaker, actor, received, quantity));
    }
    let expected = reference(&old, incoming.clone());
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let store = ReportStore::from(old.clone());
                let before = serde_json::to_string(&store).unwrap();
                let mut actions = incoming.clone();
                if reverse {
                    actions.reverse();
                }
                let (next, _) = store.updated(actions);
                assert_eq!(next.iter().cloned().collect::<Vec<_>>(), expected);
                assert_eq!(serde_json::to_string(&store).unwrap(), before);
                assert_eq!(next.len(), expected.len());
                for recipient in recipients {
                    assert_eq!(
                        next.get(recipient),
                        expected
                            .iter()
                            .filter(|r| r.recipient == recipient)
                            .cloned()
                            .collect::<Vec<_>>()
                    );
                    assert!(next.get(recipient).len() <= 64);
                }
                let encoded = serde_json::to_string(&next).unwrap();
                assert_eq!(encoded, serde_json::to_string(&expected).unwrap());
                let restored: ReportStore = serde_json::from_str(&encoded).unwrap();
                assert_eq!(restored, next);
                next
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
fn sparse_updates_share_untouched_pages_and_dense_lookup_checks_gaps() {
    let rows: Vec<_> = (1..=200)
        .map(|recipient| report(recipient, 1, 2, 1, 1))
        .collect();
    let original = ReportStore::from(rows);
    let dense = original.dense_lookup().unwrap();
    assert_eq!(dense.get(73), original.get(73));
    assert!(dense.get(999).is_empty());
    let (next, _) = original.updated(vec![report(1, 2, 2, 2, 1)]);
    assert!(!Arc::ptr_eq(
        leaf(&original, 1).unwrap(),
        leaf(&next, 1).unwrap()
    ));
    assert!(Arc::ptr_eq(
        leaf(&original, 200).unwrap(),
        leaf(&next, 200).unwrap()
    ));
    assert_eq!(original.get(1).len(), 1);
    assert_eq!(next.get(1).len(), 2);
    let (unchanged, _) = original.updated(vec![]);
    assert!(Arc::ptr_eq(
        original.root.as_ref().unwrap(),
        unchanged.root.as_ref().unwrap()
    ));
    let sparse = ReportStore::from(vec![report(1, 1, 2, 1, 1), report(u64::MAX, 1, 2, 1, 1)]);
    assert!(sparse.dense_lookup().is_none());
    assert_eq!(sparse.get(u64::MAX).len(), 1);
    assert!(sparse.get(0).is_empty());
}

#[test]
fn retiring_generations_preserves_shared_and_updated_pages() {
    let original = ReportStore::from(
        (1..=100_000)
            .map(|recipient| report(recipient, 1, 2, 1, 1))
            .collect::<Vec<_>>(),
    );
    let shared = original.clone();
    let incoming = (1..=100_000)
        .step_by(3)
        .map(|recipient| report(recipient, 1, 2, 2, 2))
        .collect();
    let (next, _) = original.updated(incoming);
    drop(original);
    assert_eq!(shared.get(1)[0].quantity, 1);
    assert_eq!(next.get(1)[0].quantity, 2);
    drop(shared);
    assert_eq!(next.get(1)[0].quantity, 2);
    assert_eq!(next.get(2)[0].quantity, 1);
    assert_eq!(next.len(), 100_000);
}

#[test]
fn single_updates_match_retention_at_full_capacity_and_reuse_noop_roots() {
    for count in [0, 1, 63, 64] {
        let mut old: Vec<_> = (1..=count)
            .map(|speaker| report(15, speaker, 1, 1, 1))
            .collect();
        old.push(report(16, 1, 1, 1, 1));
        for candidate in [
            report(15, 65, 1, 0, 1),
            report(15, 65, 1, 1, 1),
            report(15, 0, 1, 1, 1),
            report(15, 1, 1, 2, 2),
            report(15, 65, 1, 2, 1),
        ] {
            let store = ReportStore::from(old.clone());
            let expected = reference(&old, vec![candidate.clone()]);
            let (next, _) = store.updated(vec![candidate]);
            assert_eq!(next.iter().cloned().collect::<Vec<_>>(), expected);
            if expected == store.iter().cloned().collect::<Vec<_>>() {
                assert!(Arc::ptr_eq(
                    store.root.as_ref().unwrap(),
                    next.root.as_ref().unwrap()
                ));
            }
        }
    }
}

#[test]
fn ordered_hot_batch_matches_general_retention_with_newer_existing_entries() {
    let old: Vec<_> = (50..114)
        .map(|speaker| report(42, speaker, 1, if speaker % 2 == 0 { 3 } else { 1 }, 1))
        .collect();
    let incoming: Vec<_> = (1..=500)
        .map(|speaker| report(42, speaker, 1, 2, 1))
        .collect();
    let expected = reference(&old, incoming.clone());
    let run = |workers, reverse| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let store = ReportStore::from(old.clone());
                let mut actions = incoming.clone();
                if reverse {
                    actions.reverse();
                }
                let (next, _) = store.updated(actions);
                assert_eq!(next.iter().cloned().collect::<Vec<_>>(), expected);
                next
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
#[ignore = "manual sparse report-page benchmark"]
fn sparse_report_pages_benchmark() {
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let rows: Vec<_> = (1..=count)
        .map(|id| report(id * 1024 + 100, 1, 2, 1, 1))
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let original = ReportStore::from(rows);
            let mut samples = Vec::new();
            for round in 0..11 {
                let start = std::time::Instant::now();
                let (next, _) =
                    original.updated(vec![report((count / 2) * 1024 + 100, 2, 2, 2, 1)]);
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(next.len(), original.len() + 1);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} sparse_recipients={count} update={:.9}",
                samples[5]
            );
        });
}

#[test]
#[ignore = "manual full report-page benchmark"]
fn full_report_page_benchmark() {
    let rows: Vec<_> = (0..32)
        .flat_map(|recipient| (1..=64).map(move |speaker| report(recipient, speaker, 1, 1, 1)))
        .collect();
    let original = ReportStore::from(rows);
    let mut samples = Vec::new();
    for round in 0..51 {
        let start = std::time::Instant::now();
        let (next, _) = original.updated(vec![report(15, 65, 1, 2, 1)]);
        let elapsed = start.elapsed().as_secs_f64();
        assert_eq!(next.len(), original.len());
        assert_eq!(next.get(15).len(), 64);
        if round > 0 {
            samples.push(elapsed);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "full_page_reports={} update={:.9}",
        original.len(),
        samples[25]
    );
}

#[test]
#[ignore = "manual dense report-generation retirement benchmark"]
fn dense_report_retirement_benchmark() {
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..6 {
                let rows: Vec<_> = (1..=count).map(|id| report(id, 1, 2, 1, 1)).collect();
                let original = ReportStore::from(rows);
                let incoming = (1..=count).map(|id| report(id, 1, 2, 2, 2)).collect();
                let (next, candidates) = original.updated(incoming);
                drop(candidates);
                let start = std::time::Instant::now();
                drop(original);
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(next.len(), count as usize);
                assert_eq!(next.get(count)[0].received_at, 2);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} recipients={count} retirement={:.6}",
                samples[2]
            );
        });
}
