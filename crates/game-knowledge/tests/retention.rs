use game_knowledge::{Knowledge, Mind, Observation, RecallTrace, Record, Retention};
use game_social::Encounter;
use std::collections::HashSet;

fn mind(person: u64, records: Vec<Record>) -> Mind {
    Mind {
        person,
        remember_people: false,
        people_seen: vec![],
        people_reporting: None,
        people_reports: vec![],
        reporting: None,
        misreport: None,
        records,
    }
}
fn record() -> Record {
    Record {
        region: 2,
        food: 17,
        observed_at: 0,
        witness: 1,
        learned_from: None,
    }
}
fn observations() -> Vec<Observation> {
    (1..=2)
        .map(|person| Observation {
            person,
            health: 100,
            local: Some((1, 3)),
        })
        .collect()
}
fn meeting() -> Encounter {
    Encounter {
        tick: 2,
        region: 1,
        a: 1,
        b: 2,
        pressured: None,
        also_pressured: None,
    }
}

#[test]
fn passive_recall_expires_and_accepted_reading_reinforces_without_refreshing_evidence() {
    let claim = record();
    let mut owner = Knowledge {
        max_records: 4,
        minds: vec![mind(1, vec![]), mind(2, vec![claim.clone()])],
        retention: vec![Retention {
            person: 2,
            ticks: 2,
            traces: vec![],
        }],
    };
    let before = owner.clone();
    assert_eq!(owner.recall_at(2, 1).unwrap(), std::slice::from_ref(&claim));
    assert!(owner.recall_at(2, 2).unwrap().is_empty());
    assert_eq!(owner, before);
    owner.receive_report_at(2, claim.clone(), None, 2);
    assert_eq!(owner.recall_at(2, 3).unwrap(), std::slice::from_ref(&claim));
    assert_eq!(owner.minds[1].records[0].observed_at, 0);
    assert_eq!(owner.minds[1].records[0].witness, 1);
    owner.receive_report_at(2, claim, None, 2);
    assert!(owner.recall_at(2, 4).unwrap().is_empty());
    let saved = serde_json::to_string(&owner).unwrap();
    let restored: Knowledge = serde_json::from_str(&saved).unwrap();
    assert_eq!(owner, restored);
}

#[test]
fn an_old_page_can_restore_a_claim_after_a_newer_memory_has_faded() {
    let mut newer = record();
    newer.food = 50;
    newer.observed_at = 1;
    let mut owner = Knowledge {
        max_records: 4,
        minds: vec![mind(1, vec![]), mind(2, vec![newer])],
        retention: vec![Retention {
            person: 2,
            ticks: 2,
            traces: vec![],
        }],
    };
    let old_page = record();
    owner.receive_report_at(2, old_page.clone(), None, 3);
    assert_eq!(
        owner.recall_at(2, 3).unwrap(),
        std::slice::from_ref(&old_page)
    );
    assert_eq!(owner.recall_at(2, 4).unwrap(), [old_page]);
    assert!(owner.recall_at(2, 5).unwrap().is_empty());
}

#[test]
fn superseded_reports_do_not_reinforce_and_invalid_batches_are_atomic() {
    let mut newer = record();
    newer.food = 50;
    newer.observed_at = 1;
    let mut owner = Knowledge {
        max_records: 4,
        minds: vec![mind(1, vec![]), mind(2, vec![newer.clone()])],
        retention: vec![Retention {
            person: 2,
            ticks: 3,
            traces: vec![],
        }],
    };
    owner.receive_report_at(2, record(), None, 2);
    assert_eq!(owner.recall_at(2, 2).unwrap(), [newer]);
    assert!(owner.retention[0].traces.is_empty());
    let before = owner.clone();
    assert!(
        owner
            .receive_reports_at(&[(2, record(), None), (2, record(), None)], 3)
            .is_err()
    );
    assert_eq!(owner, before);
    assert!(owner.recall_at(2, 4).unwrap().is_empty());
}

#[test]
fn hearing_a_forgotten_report_again_restores_its_recall() {
    let mut earlier = record();
    earlier.learned_from = Some(1);
    let owner = Knowledge {
        max_records: 4,
        minds: vec![mind(1, vec![record()]), mind(2, vec![earlier])],
        retention: vec![Retention {
            person: 2,
            ticks: 1,
            traces: vec![],
        }],
    };
    let next = owner
        .prepare(&observations(), &HashSet::from([1, 2]), &[meeting()], 1)
        .unwrap();
    let recovered = next.recall_at(2, 2).unwrap();
    let report = recovered.iter().find(|record| record.region == 2).unwrap();
    assert_eq!(
        (report.food, report.observed_at, report.learned_from),
        (17, 0, Some(1))
    );
    assert!(
        next.recall_at(2, 3)
            .unwrap()
            .iter()
            .all(|record| record.region != 2)
    );
}

#[test]
fn forgotten_speaker_cannot_relay_a_memory_but_an_accepted_report_has_its_own_retention() {
    let regions = HashSet::from([1, 2]);
    let owner = Knowledge {
        max_records: 4,
        minds: vec![mind(1, vec![record()]), mind(2, vec![])],
        retention: vec![
            Retention {
                person: 1,
                ticks: 1,
                traces: vec![],
            },
            Retention {
                person: 2,
                ticks: 2,
                traces: vec![],
            },
        ],
    };
    let faded = owner
        .prepare(&observations(), &regions, &[meeting()], 1)
        .unwrap();
    assert!(
        faded.minds[1]
            .records
            .iter()
            .all(|record| record.region != 2)
    );
    assert!(
        faded.minds[0]
            .records
            .iter()
            .all(|record| record.region != 2)
    );
    let mut available = owner.clone();
    available.retention[0].ticks = 3;
    let received = available
        .prepare(&observations(), &regions, &[meeting()], 1)
        .unwrap();
    let remembered = received.recall_at(2, 2).unwrap();
    let report = remembered.iter().find(|record| record.region == 2).unwrap();
    assert_eq!(
        (
            report.food,
            report.observed_at,
            report.witness,
            report.learned_from
        ),
        (17, 0, 1, Some(1))
    );
    assert!(
        received
            .recall_at(2, 3)
            .unwrap()
            .iter()
            .any(|record| record.region == 2)
    );
    assert!(
        received
            .recall_at(2, 4)
            .unwrap()
            .iter()
            .all(|record| record.region != 2)
    );
}

#[test]
fn malformed_retention_is_rejected() {
    let mut owner = Knowledge {
        max_records: 4,
        minds: vec![mind(1, vec![record()])],
        retention: vec![Retention {
            person: 1,
            ticks: 1,
            traces: vec![RecallTrace {
                region: 2,
                reinforced_at: 2,
            }],
        }],
    };
    assert!(
        owner
            .validate(&HashSet::from([1]), &HashSet::from([1, 2]), 1)
            .is_err()
    );
    owner.retention[0].traces[0].reinforced_at = 0;
    assert!(
        owner
            .validate(&HashSet::from([1]), &HashSet::from([1, 2]), 1)
            .is_ok()
    );
}

#[test]
#[ignore = "performance measurement"]
fn personal_memory_retention_benchmark() {
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000u64);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let stride = std::env::var("LEGACY_RETENTION_STRIDE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100u64);
    assert!(stride > 0);
    let owner = Knowledge {
        max_records: 4,
        minds: (1..=count)
            .map(|person| mind(person, vec![record()]))
            .collect(),
        retention: (1..=count)
            .filter(|person| person % stride == 0)
            .map(|person| Retention {
                person,
                ticks: 2,
                traces: vec![],
            })
            .collect(),
    };
    let observations: Vec<_> = (1..=count)
        .map(|person| Observation {
            person,
            health: 100,
            local: Some((1, 3)),
        })
        .collect();
    let regions = HashSet::from([1, 2]);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let start = std::time::Instant::now();
                let next = owner
                    .prepare_validated(&observations, &regions, &[], 2)
                    .unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert!(
                    next.minds
                        .iter()
                        .filter(|mind| mind.person % stride == 0)
                        .all(|mind| mind.records.iter().all(|record| record.region != 2))
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "people={count} stride={stride} workers={workers} seconds={:.6}",
                samples[1]
            );
        });
}
