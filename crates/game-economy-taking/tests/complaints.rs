use game_economy_taking::{
    Complaint, ComplaintAccount, ComplaintDecision, ComplaintPreference, Outcome, Report,
    ReportBelief, Taking,
};

#[test]
fn accepted_request_records_only_a_later_matching_event_statement() {
    let account = ComplaintAccount {
        actor: 4,
        target: 1,
        good: 1,
        quantity: 2,
        observed_at: 2,
        outcome: Outcome::Transferred,
        source: None,
    };
    let mut previous = Taking {
        complaints: vec![Complaint {
            author: 1,
            recipient: 2,
            witness: 3,
            account,
            created_at: 3,
            expires_at: 8,
            decision: ComplaintDecision::Accepted,
            decided_at: Some(4),
            answer: None,
        }]
        .into(),
        ..Default::default()
    };
    let statement = Report {
        speaker: 3,
        recipient: 2,
        actor: 4,
        target: 1,
        good: 1,
        quantity: 2,
        observed_at: 2,
        received_at: 5,
        claimed_outcome: Outcome::Resisted,
        belief: ReportBelief::Doubts,
        source_speaker: None,
    };
    let mut next = previous.clone();
    let mut wrong = statement.clone();
    wrong.observed_at = 3;
    next.reports = vec![wrong].into();
    next.advance_complaints(&previous, &[(2, 3)], 5, &|_, _| 0);
    assert!(next.complaints[0].answer.is_none());
    previous = next;
    let mut next = previous.clone();
    next.reports = vec![statement.clone()].into();
    next.advance_complaints(&previous, &[(2, 3)], 5, &|_, _| 0);
    let answer = next.complaints[0].answer.as_ref().unwrap();
    assert!(!answer.matches_account);
    assert_eq!(answer.report, statement);
    assert_eq!(answer.report.belief, ReportBelief::Doubts);
}

#[test]
fn complaints_require_contact_and_personal_choice_and_expire() {
    let account = ComplaintAccount {
        actor: 4,
        target: 1,
        good: 1,
        quantity: 2,
        observed_at: 2,
        outcome: Outcome::Transferred,
        source: None,
    };
    let initial = Taking {
        complaints: vec![Complaint {
            author: 1,
            recipient: 2,
            witness: 3,
            account,
            created_at: 3,
            expires_at: 6,
            decision: ComplaintDecision::Pending,
            decided_at: None,
            answer: None,
        }]
        .into(),
        complaint_preferences: vec![ComplaintPreference {
            person: 2,
            willing: false,
            pressure_threshold: Some(3),
        }]
        .into(),
        ..Default::default()
    };
    let run = |workers, contact: bool, pressure| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = initial.clone();
                next.advance_complaints(
                    &initial,
                    if contact { &[(1, 2)] } else { &[] },
                    4,
                    &|recipient, author| {
                        if (recipient, author) == (2, 1) {
                            pressure
                        } else {
                            0
                        }
                    },
                );
                next
            })
    };
    assert_eq!(
        run(1, false, 9).complaints[0].decision,
        ComplaintDecision::Pending
    );
    assert_eq!(
        run(1, true, 2).complaints[0].decision,
        ComplaintDecision::Declined
    );
    let accepted = run(1, true, 3);
    assert_eq!(accepted.complaints[0].decision, ComplaintDecision::Accepted);
    assert_eq!(accepted.complaints[0].account, account);
    assert_eq!(accepted, run(4, true, 3));
    let mut expired = initial.clone();
    expired.advance_complaints(&initial, &[(1, 2)], 6, &|_, _| 9);
    assert_eq!(expired.complaints[0].decision, ComplaintDecision::Expired);
    let encoded = serde_json::to_string(&accepted).unwrap();
    assert_eq!(serde_json::from_str::<Taking>(&encoded).unwrap(), accepted);
}

#[test]
fn many_independent_inquiry_requests_replay_across_workers() {
    let count = 10_000u64;
    let complaints = (0..count)
        .map(|i| {
            let author = i * 4 + 1;
            Complaint {
                author,
                recipient: author + 1,
                witness: author + 2,
                account: ComplaintAccount {
                    actor: author + 3,
                    target: author,
                    good: 1,
                    quantity: 1,
                    observed_at: 1,
                    outcome: Outcome::Transferred,
                    source: None,
                },
                created_at: 2,
                expires_at: 8,
                decision: ComplaintDecision::Pending,
                decided_at: None,
                answer: None,
            }
        })
        .collect::<Vec<_>>();
    let preferences = complaints
        .iter()
        .enumerate()
        .map(|(i, c)| ComplaintPreference {
            person: c.recipient,
            willing: i % 2 == 0,
            pressure_threshold: None,
        })
        .collect::<Vec<_>>();
    let contacts = complaints
        .iter()
        .map(|c| (c.recipient, c.author))
        .collect::<Vec<_>>();
    let initial = Taking {
        complaints: complaints.into(),
        complaint_preferences: preferences.into(),
        ..Default::default()
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = initial.clone();
                next.advance_complaints(&initial, &contacts, 3, &|_, _| 0);
                next
            })
    };
    let one = run(1);
    assert_eq!(
        one.complaints
            .iter()
            .filter(|c| c.decision == ComplaintDecision::Accepted)
            .count(),
        count as usize / 2
    );
    assert_eq!(
        one.complaints
            .iter()
            .filter(|c| c.decision == ComplaintDecision::Declined)
            .count(),
        count as usize / 2
    );
    assert_eq!(one, run(4));
    assert_eq!(one, run(6));
    let reports: Vec<_> = one
        .complaints
        .iter()
        .map(|c| Report {
            speaker: c.witness,
            recipient: c.recipient,
            actor: c.account.actor,
            target: c.account.target,
            good: c.account.good,
            quantity: c.account.quantity,
            observed_at: c.account.observed_at,
            received_at: 4,
            claimed_outcome: c.account.outcome,
            belief: ReportBelief::Unassessed,
            source_speaker: None,
        })
        .collect();
    let answer = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = one.clone();
                next.reports = reports.clone().into();
                next.advance_complaints(&one, &[], 4, &|_, _| 0);
                next
            })
    };
    let answered = answer(1);
    assert_eq!(
        answered
            .complaints
            .iter()
            .filter(|c| c.answer.is_some())
            .count(),
        count as usize / 2
    );
    assert_eq!(answered, answer(4));
    assert_eq!(answered, answer(6));
    let mut with_history = one.clone();
    with_history.witnesses = one
        .complaints
        .iter()
        .map(|c| game_economy_taking::Witness {
            person: c.witness,
            actor: c.account.actor,
            notices: true,
            last_seen: Some(game_economy_taking::ObservedTaking {
                tick: c.account.observed_at,
                target: c.account.target,
                good: c.account.good,
                quantity: c.account.quantity,
                outcome: c.account.outcome,
            }),
            ..Default::default()
        })
        .collect::<Vec<_>>()
        .into();
    with_history.report_policies = one
        .complaints
        .iter()
        .map(|c| game_economy_taking::ReportPolicy {
            person: c.witness,
            recipient: c.recipient,
            actor: c.account.actor,
            willing: true,
            pressure_threshold: None,
            claimed_outcome: None,
            source: Default::default(),
            heard_from: None,
            details: None,
        })
        .collect::<Vec<_>>()
        .into();
    with_history.reports = reports
        .iter()
        .cloned()
        .map(|mut report| {
            report.received_at = 2;
            report
        })
        .collect::<Vec<_>>()
        .into();
    let witness_contacts = one
        .complaints
        .iter()
        .map(|c| (c.witness, c.recipient))
        .collect::<Vec<_>>();
    let repeat = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = with_history.clone();
                assert!(
                    next.receive_reports(&with_history, &witness_contacts, 4, &|_, _| (0, 0, 0))
                        .is_empty()
                );
                next.advance_complaints(&with_history, &witness_contacts, 4, &|_, _| 0);
                next
            })
    };
    let repeated = repeat(1);
    assert_eq!(
        repeated
            .complaints
            .iter()
            .filter(|c| c.answer.is_some())
            .count(),
        count as usize / 2
    );
    assert_eq!(repeated, repeat(4));
    assert_eq!(repeated, repeat(6));
}
