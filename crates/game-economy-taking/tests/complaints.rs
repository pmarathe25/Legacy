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
fn inquiry_assessments_follow_individual_subjective_preferences() {
    use game_economy_taking::AssessmentPreference;
    let account = ComplaintAccount {
        actor: 4,
        target: 1,
        good: 1,
        quantity: 2,
        observed_at: 2,
        outcome: Outcome::Transferred,
        source: None,
    };
    let run = |workers, preference: Option<AssessmentPreference>, belief, claimed_outcome| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let previous = Taking {
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
                    assessment_preferences: preference.into_iter().collect::<Vec<_>>().into(),
                    ..Default::default()
                };
                let mut next = previous.clone();
                next.reports = vec![Report {
                    speaker: 3,
                    recipient: 2,
                    actor: 4,
                    target: 1,
                    good: 1,
                    quantity: 2,
                    observed_at: 2,
                    received_at: 5,
                    claimed_outcome,
                    belief,
                    source_speaker: None,
                }]
                .into();
                next.advance_complaints(&previous, &[(2, 3)], 5, &|_, _| 0);
                next.complaints[0].answer.clone().unwrap()
            })
    };
    let preference = AssessmentPreference {
        person: 2,
        matching_believed: Some(-9),
        different_doubted: Some(7),
        different_unassessed: Some(0),
        ..Default::default()
    };
    assert_eq!(
        run(
            1,
            Some(preference),
            ReportBelief::Believes,
            Outcome::Transferred
        )
        .assessment,
        Some(-9)
    );
    let answer = run(1, Some(preference), ReportBelief::Doubts, Outcome::Resisted);
    assert_eq!(answer.assessment, Some(7));
    assert_eq!(
        answer,
        run(4, Some(preference), ReportBelief::Doubts, Outcome::Resisted)
    );
    assert_eq!(
        run(
            1,
            Some(preference),
            ReportBelief::Unassessed,
            Outcome::Resisted
        )
        .assessment,
        Some(0)
    );
    assert_eq!(
        run(
            1,
            Some(preference),
            ReportBelief::Believes,
            Outcome::Resisted
        )
        .assessment,
        None
    );
    assert_eq!(
        run(1, None, ReportBelief::Doubts, Outcome::Resisted).assessment,
        None
    );
}

#[test]
fn only_a_personally_known_earlier_report_can_influence_an_assessment() {
    use game_economy_taking::AssessmentPreference;
    let account = ComplaintAccount {
        actor: 4,
        target: 1,
        good: 1,
        quantity: 2,
        observed_at: 4,
        outcome: Outcome::Transferred,
        source: None,
    };
    let prior = Report {
        speaker: 3,
        recipient: 2,
        actor: 4,
        target: 1,
        good: 1,
        quantity: 1,
        observed_at: 1,
        received_at: 2,
        claimed_outcome: Outcome::Resisted,
        belief: ReportBelief::Doubts,
        source_speaker: None,
    };
    let run = |workers, prior: Option<Report>, belief: Option<ReportBelief>| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let previous = Taking {
                    complaints: vec![Complaint {
                        author: 1,
                        recipient: 2,
                        witness: 3,
                        account,
                        created_at: 5,
                        expires_at: 10,
                        decision: ComplaintDecision::Accepted,
                        decided_at: Some(6),
                        answer: None,
                    }]
                    .into(),
                    assessment_preferences: vec![AssessmentPreference {
                        person: 2,
                        matching_unassessed: Some(3),
                        prior_actor_adjustment: Some(-8),
                        prior_belief: belief,
                        ..Default::default()
                    }]
                    .into(),
                    reports: prior.into_iter().collect::<Vec<_>>().into(),
                    ..Default::default()
                };
                let mut next = previous.clone();
                next.reports = vec![Report {
                    speaker: 3,
                    recipient: 2,
                    actor: 4,
                    target: 1,
                    good: 1,
                    quantity: 2,
                    observed_at: 4,
                    received_at: 7,
                    claimed_outcome: Outcome::Transferred,
                    belief: ReportBelief::Unassessed,
                    source_speaker: None,
                }]
                .into();
                next.advance_complaints(&previous, &[(2, 3)], 7, &|_, _| 0);
                let restored: Taking =
                    serde_json::from_str(&serde_json::to_string(&next).unwrap()).unwrap();
                restored
                    .validate(
                        &[1, 2, 3, 4].into_iter().collect(),
                        &[1].into_iter().collect(),
                        7,
                    )
                    .unwrap();
                assert_eq!(next, restored);
                next.complaints[0].answer.clone().unwrap()
            })
    };
    let result = run(1, Some(prior.clone()), Some(ReportBelief::Doubts));
    assert_eq!(result.assessment, Some(-5));
    assert_eq!(result.precedent, Some(prior.clone()));
    assert_eq!(
        result,
        run(4, Some(prior.clone()), Some(ReportBelief::Doubts))
    );
    assert_eq!(run(1, None, None).assessment, Some(3));
    assert_eq!(
        run(1, Some(prior.clone()), Some(ReportBelief::Believes)).assessment,
        Some(3)
    );
    let mut unknown = prior.clone();
    unknown.recipient = 1;
    assert_eq!(run(1, Some(unknown), None).assessment, Some(3));
    let mut same_case = prior;
    same_case.observed_at = 4;
    same_case.received_at = 4;
    assert_eq!(run(1, Some(same_case), None).assessment, Some(3));
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
    let unchanged = run(1, false, 9);
    assert!(std::sync::Arc::ptr_eq(
        &initial.complaints,
        &unchanged.complaints
    ));
    assert_eq!(
        run(1, true, 2).complaints[0].decision,
        ComplaintDecision::Declined
    );
    let accepted = run(1, true, 3);
    assert!(!std::sync::Arc::ptr_eq(
        &initial.complaints,
        &accepted.complaints
    ));
    assert_eq!(accepted.complaints[0].decision, ComplaintDecision::Accepted);
    assert_eq!(accepted.complaints[0].account, account);
    assert_eq!(accepted, run(4, true, 3));
    let mut expired = initial.clone();
    expired.advance_complaints(&initial, &[(1, 2)], 6, &|_, _| 9);
    assert_eq!(expired.complaints[0].decision, ComplaintDecision::Expired);
    assert!(initial.inquiry_meetings(4).is_empty());
    let mut seeking = initial.clone();
    seeking.inquiry_pursuit = vec![1, 2].into();
    assert_eq!(seeking.inquiry_meetings(4), vec![(1, 2)]);
    seeking.inquiry_pursuit = vec![2, 1].into();
    assert_eq!(seeking.inquiry_meetings(4), vec![(1, 2)]);
    let mut accepted_seeking = accepted.clone();
    accepted_seeking.inquiry_pursuit = vec![1, 2].into();
    assert_eq!(accepted_seeking.inquiry_meetings(5), vec![(2, 3)]);
    accepted_seeking.inquiry_pursuit = vec![1].into();
    assert!(accepted_seeking.inquiry_meetings(5).is_empty());
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
    let mut unordered = initial.clone();
    unordered.complaint_preferences = initial
        .complaint_preferences
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>()
        .into();
    let mut unordered_next = unordered.clone();
    unordered_next.advance_complaints(&unordered, &contacts, 3, &|_, _| 0);
    assert_eq!(one.complaints, unordered_next.complaints);
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
    let mut judging = one.clone();
    judging.assessment_preferences = one
        .complaints
        .iter()
        .rev()
        .map(|c| game_economy_taking::AssessmentPreference {
            person: c.recipient,
            matching_unassessed: Some((c.recipient % 7) as i32 - 3),
            ..Default::default()
        })
        .collect::<Vec<_>>()
        .into();
    let judge = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = judging.clone();
                next.reports = reports.clone().into();
                next.advance_complaints(&judging, &[], 4, &|_, _| 0);
                next
            })
    };
    let judged = judge(1);
    assert!(
        judged
            .complaints
            .iter()
            .filter_map(|c| c.answer.as_ref().map(|a| (c, a)))
            .all(|(c, a)| a.assessment == Some((c.recipient % 7) as i32 - 3))
    );
    assert_eq!(judged, judge(4));
    assert_eq!(judged, judge(6));
    let mut precedent_cases = judging.clone();
    precedent_cases
        .assessment_preferences_mut()
        .iter_mut()
        .for_each(|p| {
            p.prior_actor_adjustment = Some(2);
        });
    precedent_cases.reports = reports
        .iter()
        .cloned()
        .map(|mut report| {
            report.observed_at = 0;
            report.received_at = 1;
            report
        })
        .collect::<Vec<_>>()
        .into();
    let judge_with_precedent = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = precedent_cases.clone();
                next.reports = reports.clone().into();
                next.advance_complaints(&precedent_cases, &[], 4, &|_, _| 0);
                next
            })
    };
    let precedent_result = judge_with_precedent(1);
    let precedent_count = precedent_result
        .complaints
        .iter()
        .filter_map(|c| c.answer.as_ref().map(|a| (c, a)))
        .filter(|(c, a)| {
            a.precedent.is_some() && a.assessment == Some((c.recipient % 7) as i32 - 1)
        })
        .count();
    assert_eq!(precedent_count, count as usize / 2);
    assert_eq!(precedent_result, judge_with_precedent(4));
    assert_eq!(precedent_result, judge_with_precedent(6));
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

#[test]
#[ignore = "manual synthetic personal precedent benchmark"]
fn precedent_benchmark() {
    use game_economy_taking::AssessmentPreference;
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let complaints: Vec<_> = (0..count)
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
                    observed_at: 3,
                    outcome: Outcome::Transferred,
                    source: None,
                },
                created_at: 4,
                expires_at: 10,
                decision: ComplaintDecision::Accepted,
                decided_at: Some(5),
                answer: None,
            }
        })
        .collect();
    let prior: Vec<_> = complaints
        .iter()
        .map(|c| Report {
            speaker: c.witness,
            recipient: c.recipient,
            actor: c.account.actor,
            target: c.account.target,
            good: 1,
            quantity: 1,
            observed_at: 1,
            received_at: 2,
            claimed_outcome: Outcome::Transferred,
            belief: ReportBelief::Unassessed,
            source_speaker: None,
        })
        .collect();
    let current: game_economy_taking::ReportStore = prior
        .iter()
        .cloned()
        .map(|mut r| {
            r.observed_at = 3;
            r.received_at = 6;
            r
        })
        .collect::<Vec<_>>()
        .into();
    let preferences = complaints
        .iter()
        .map(|c| AssessmentPreference {
            person: c.recipient,
            matching_unassessed: Some(7),
            prior_actor_adjustment: Some(-2),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let previous = Taking {
        complaints: complaints.into(),
        assessment_preferences: preferences.into(),
        reports: prior.into(),
        ..Default::default()
    };
    rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut samples = Vec::new();
        for round in 0..6 {
            let mut next = previous.clone();
            next.reports = current.clone();
            let now = std::time::Instant::now();
            next.advance_complaints(&previous, &[], 6, &|_, _| 0);
            let elapsed = now.elapsed().as_secs_f64();
            assert_eq!(next.complaints.iter().filter(|c| c.answer.as_ref().is_some_and(|a| a.assessment == Some(5) && a.precedent.is_some())).count(), count as usize);
            if round > 0 { samples.push(elapsed); }
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} synthetic_prior_reports={count} answered_cases={count} elapsed_min={:.6} elapsed_median={:.6} elapsed_max={:.6}", samples[0], samples[2], samples[4]);
    });
}
