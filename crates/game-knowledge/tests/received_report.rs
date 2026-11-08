use game_knowledge::{Knowledge, Mind, Record};

fn record(region: u64, observed_at: u64) -> Record {
    Record {
        region,
        food: 10,
        observed_at,
        witness: 1,
        learned_from: Some(3),
    }
}

#[test]
fn received_reports_respect_recency_capacity_and_local_memory() {
    let mut knowledge = Knowledge {
        retention: vec![],
        max_records: 2,
        minds: vec![Mind {
            person: 2,
            remember_people: false,
            people_seen: vec![],
            people_reporting: None,
            people_reports: vec![],
            reporting: None,
            misreport: None,
            records: vec![record(1, 1), record(2, 5)],
        }],
    };
    let original = knowledge.clone();
    assert!(!knowledge.receive_report(9, record(3, 9), Some(1)));
    assert!(!knowledge.receive_report(2, record(2, 4), Some(1)));
    assert!(!knowledge.receive_report(2, record(3, 4), Some(1)));
    assert_eq!(knowledge, original);
    assert!(knowledge.receive_report(2, record(3, 6), Some(1)));
    assert_eq!(knowledge.minds[0].records, vec![record(1, 1), record(3, 6)]);
}

#[test]
fn batched_reports_are_independent_and_reject_duplicate_recipients_atomically() {
    let mind = |person| Mind {
        person,
        remember_people: false,
        people_seen: vec![],
        people_reporting: None,
        people_reports: vec![],
        reporting: None,
        misreport: None,
        records: vec![],
    };
    let initial = Knowledge {
        retention: vec![],
        max_records: 2,
        minds: vec![mind(2), mind(4)],
    };
    let reports = vec![(2, record(2, 3), Some(1)), (4, record(3, 4), Some(1))];
    let mut batched = initial.clone();
    batched.receive_reports(&reports).unwrap();
    let mut individual = initial;
    for (person, report, local) in &reports {
        individual.receive_report(*person, report.clone(), *local);
    }
    assert_eq!(batched, individual);
    let before = batched.clone();
    assert!(
        batched
            .receive_reports(&[(2, record(3, 5), None), (2, record(4, 6), None)])
            .is_err()
    );
    assert_eq!(batched, before);
}
