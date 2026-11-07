use game_economy_taking::{
    Complaint, ComplaintAccount, ComplaintDecision, ComplaintPreference, Outcome, Taking,
};

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
}
