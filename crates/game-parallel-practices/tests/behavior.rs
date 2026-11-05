use game_parallel_practices::{
    Case, Decision, Mind, Observation, Practices, Proposal, Received, ValueLearning,
};
use std::collections::HashSet;

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
        acceptance_threshold: 0,
        value_learning: None,
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

#[test]
fn personally_interpreted_experience_changes_later_practice_judgment() {
    let existing = Proposal {
        author: 1,
        sequence: 1,
        recipient: 2,
        good: 4,
        quantity: 1,
        reserve: 0,
        ask_hunger: 5,
        created: 1,
        expires: 10,
    };
    let offered = Proposal {
        author: 3,
        sequence: 1,
        recipient: 1,
        good: 4,
        quantity: 1,
        reserve: 0,
        ask_hunger: 5,
        created: 1,
        expires: 10,
    };
    let mut person = mind(1);
    person.invent = false;
    person.sequence = 1;
    person.authored = vec![existing].into();
    person.inbox = vec![Received {
        proposal: offered,
        decision: Decision::Pending,
        approved: false,
        delivered: 2,
    }]
    .into();
    person.acceptance_threshold = 2;
    let experiences = vec![
        Observation {
            person: 1,
            case: Case::Received {
                tick: 2,
                helper: 2,
                good: 4,
                quantity: 1,
            },
        },
        Observation {
            person: 1,
            case: Case::Received {
                tick: 2,
                helper: 3,
                good: 4,
                quantity: 1,
            },
        },
    ];
    let run = |workers, rate| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut learner = person.clone();
                learner.value_learning = Some(std::sync::Arc::new(ValueLearning {
                    value: 0,
                    minimum: -4,
                    maximum: 4,
                    received: rate,
                    gave: 0,
                    unanswered: 0,
                }));
                let initial = Practices {
                    minds: vec![learner, mind(2), mind(3)],
                };
                let after = initial.prepare(&[], &experiences, 2, |_, _| 0).unwrap();
                assert_eq!(after.minds[0].value_learning.as_ref().unwrap().value, rate);
                assert_eq!(initial.minds[0].value_learning.as_ref().unwrap().value, 0);
                assert_eq!(after.minds[0].inbox[0].decision, Decision::Pending);
                after
                    .validate(&HashSet::from([1, 2, 3]), &HashSet::from([4]), 2)
                    .unwrap();
                let restored: Practices =
                    serde_json::from_str(&serde_json::to_string(&after).unwrap()).unwrap();
                let next = after.prepare(&[], &[], 3, |_, _| 0).unwrap();
                assert_eq!(next, restored.prepare(&[], &[], 3, |_, _| 0).unwrap());
                next
            })
    };
    let positive = run(1, 3);
    assert_eq!(positive, run(4, 3));
    assert_eq!(positive.minds[0].inbox[0].decision, Decision::Adopted);
    let negative = run(1, -3);
    assert_eq!(negative, run(4, -3));
    assert_eq!(negative.minds[0].inbox[0].decision, Decision::Declined);
}

#[test]
fn learned_value_is_bounded_and_can_change_after_unanswered_or_pressured_help() {
    let proposal = Proposal {
        author: 1,
        sequence: 1,
        recipient: 2,
        good: 4,
        quantity: 1,
        reserve: 0,
        ask_hunger: 5,
        created: 1,
        expires: 10,
    };
    let mut author = mind(1);
    author.invent = false;
    author.sequence = 1;
    author.authored = vec![proposal.clone()].into();
    author.value_learning = Some(std::sync::Arc::new(ValueLearning {
        value: 0,
        minimum: -4,
        maximum: 4,
        received: 0,
        gave: 0,
        unanswered: 3,
    }));
    let mut helper = mind(2);
    helper.invent = false;
    helper.inbox = vec![Received {
        proposal,
        decision: Decision::Declined,
        approved: false,
        delivered: 1,
    }]
    .into();
    helper.value_learning = Some(std::sync::Arc::new(ValueLearning {
        value: 0,
        minimum: -4,
        maximum: 4,
        received: 0,
        gave: -2,
        unanswered: 0,
    }));
    let state = Practices {
        minds: vec![author, helper, mind(3)],
    };
    let observations = vec![
        Observation {
            person: 1,
            case: Case::Unanswered { tick: 2, target: 2 },
        },
        Observation {
            person: 1,
            case: Case::Unanswered { tick: 2, target: 2 },
        },
        Observation {
            person: 1,
            case: Case::Unanswered { tick: 2, target: 3 },
        },
        Observation {
            person: 2,
            case: Case::Gave {
                tick: 2,
                recipient: 1,
                good: 4,
                quantity: 1,
            },
        },
    ];
    let next = state.prepare(&[], &observations, 2, |_, _| 0).unwrap();
    assert_eq!(next.minds[0].value_learning.as_ref().unwrap().value, 4);
    assert_eq!(next.minds[1].value_learning.as_ref().unwrap().value, -2);
    assert_eq!(next.minds[1].inbox[0].decision, Decision::Declined);
    let later = next.prepare(&[], &[], 3, |_, _| 0).unwrap();
    assert_eq!(later.minds[0].value_learning.as_ref().unwrap().value, 4);
    assert_eq!(later.minds[1].value_learning.as_ref().unwrap().value, -2);
    let mut invalid = state;
    std::sync::Arc::make_mut(invalid.minds[0].value_learning.as_mut().unwrap()).minimum = 5;
    assert!(invalid.prepare(&[], &observations, 2, |_, _| 0).is_err());
}

#[test]
fn actual_experience_and_contact_construct_a_personal_proposal() {
    let initial = Practices {
        minds: vec![mind(1), mind(2), mind(3)],
    };
    let cases = vec![
        Observation {
            person: 1,
            case: Case::Shortage { tick: 1, hunger: 6 },
        },
        Observation {
            person: 1,
            case: Case::Received {
                tick: 1,
                helper: 2,
                good: 4,
                quantity: 3,
            },
        },
        Observation {
            person: 1,
            case: Case::Shortage { tick: 2, hunger: 7 },
        },
    ];
    let remembered = initial.prepare(&[], &cases, 2, |_, _| 0).unwrap();
    assert!(
        remembered
            .minds
            .iter()
            .all(|m| m.authored.is_empty() && m.inbox.is_empty())
    );
    let absent = remembered.prepare(&[(1, 3)], &[], 3, |_, _| 0).unwrap();
    assert!(absent.minds[0].authored.is_empty());
    let delivered = absent.prepare(&[(1, 2)], &[], 4, |_, _| 0).unwrap();
    let proposal = &delivered.minds[0].authored[0];
    assert_eq!(
        (
            proposal.author,
            proposal.recipient,
            proposal.good,
            proposal.quantity,
            proposal.reserve
        ),
        (1, 2, 4, 3, 3)
    );
    assert_eq!(delivered.minds[1].inbox[0].decision, Decision::Pending);
    assert!(delivered.minds[2].inbox.is_empty());
    delivered
        .validate(&HashSet::from([1, 2, 3]), &HashSet::from([4]), 4)
        .unwrap();
    let saved: Practices =
        serde_json::from_str(&serde_json::to_string(&delivered).unwrap()).unwrap();
    let next = delivered.prepare(&[], &[], 5, |_, _| 0).unwrap();
    assert_eq!(next, saved.prepare(&[], &[], 5, |_, _| 0).unwrap());
    assert_eq!(next.minds[1].inbox[0].decision, Decision::Adopted);
    assert!(next.minds[1].inbox[0].approved);
    let mut full_history = next.clone();
    full_history.minds[0].reports = (100..108)
        .map(|sequence| game_parallel_practices::ReceivedReport {
            statement: game_parallel_practices::ReadinessReport {
                speaker: 2,
                listener: 1,
                sequence,
                claimed_ready: false,
                tick: 1,
            },
            belief: game_parallel_practices::Belief::Unassessed,
        })
        .collect::<Vec<_>>()
        .into();
    full_history.speak(2, 1, 1, true, 6).unwrap();
    assert_eq!(full_history.minds[0].reports.len(), 8);
    assert!(
        !full_history.minds[0]
            .reports
            .iter()
            .any(|r| r.statement.sequence == 100)
    );
    assert!(
        full_history.minds[0]
            .reports
            .iter()
            .any(|r| r.statement.sequence == 1 && r.statement.claimed_ready)
    );
    let mut liar = next.clone();
    liar.minds[1].reporting = game_parallel_practices::Reporting::Lie;
    liar.minds[0].report_attitude = game_parallel_practices::ReportAttitude::Trust;
    let no_contact = liar
        .prepare_with_evidence(&[], &[], 6, |_, _| 0, |_| true, |_, _, _| false)
        .unwrap();
    assert!(no_contact.minds[0].reports.is_empty());
    let statement = liar
        .prepare_with_evidence(&[(1, 2)], &[], 6, |_, _| 0, |_| true, |_, _, _| false)
        .unwrap();
    assert!(statement.minds[0].reports[0].statement.claimed_ready);
    assert_eq!(
        statement.minds[0].reports[0].belief,
        game_parallel_practices::Belief::Unassessed
    );
    assert!(statement.minds[2].reports.is_empty());
    let concealment = liar
        .prepare_with_evidence(&[(1, 2)], &[], 6, |_, _| 0, |_| true, |_, _, _| true)
        .unwrap();
    assert!(!concealment.minds[0].reports[0].statement.claimed_ready);
    let believed = statement
        .prepare_with_evidence(&[], &[], 7, |_, _| 0, |_| true, |_, _, _| false)
        .unwrap();
    assert_eq!(
        believed.minds[0].reports[0].belief,
        game_parallel_practices::Belief::Believes
    );
    let unanswered = [Observation {
        person: 1,
        case: Case::Unanswered {
            tick: 13,
            target: 2,
        },
    }];
    let unchanged = believed.prepare(&[], &unanswered, 13, |_, _| 0).unwrap();
    assert_eq!(
        unchanged.minds[0].reports[0].belief,
        game_parallel_practices::Belief::Believes
    );
    let mut skeptical = believed.clone();
    skeptical.minds[0].unanswered_report_belief =
        Some(game_parallel_practices::Belief::Disbelieves);
    let skeptical = skeptical.prepare(&[], &unanswered, 13, |_, _| 0).unwrap();
    assert_eq!(
        skeptical.minds[0].reports[0].belief,
        game_parallel_practices::Belief::Disbelieves
    );
    assert!(skeptical.minds[0].authored.is_empty());
    skeptical
        .validate(&HashSet::from([1, 2, 3]), &HashSet::from([4]), 13)
        .unwrap();
    liar.minds[1].reporting = game_parallel_practices::Reporting::Truthful;
    liar.minds[0].report_attitude = game_parallel_practices::ReportAttitude::Doubt;
    let truthful = liar
        .prepare_with_evidence(&[(1, 2)], &[], 6, |_, _| 0, |_| true, |_, _, _| false)
        .unwrap();
    assert!(!truthful.minds[0].reports[0].statement.claimed_ready);
    let doubted = truthful
        .prepare_with_evidence(&[], &[], 7, |_, _| 0, |_| true, |_, _, _| false)
        .unwrap();
    assert_eq!(
        doubted.minds[0].reports[0].belief,
        game_parallel_practices::Belief::Disbelieves
    );

    let mut changed = remembered.clone();
    if let Case::Received {
        ref mut quantity, ..
    } = std::sync::Arc::make_mut(&mut changed.minds[0].cases)[1]
    {
        *quantity = 2;
    }
    let changed = changed.prepare(&[(1, 2)], &[], 4, |_, _| 0).unwrap();
    assert_eq!(changed.minds[0].authored[0].quantity, 2);
    assert_eq!(changed.minds[0].authored[0].reserve, 2);
    let mut cautious = remembered.clone();
    cautious.minds[0].quantity_weight = -1;
    cautious.minds[0].reserve_weight = -1;
    let cautious = cautious.prepare(&[(1, 2)], &[], 4, |_, _| 0).unwrap();
    assert_eq!(
        (
            cautious.minds[0].authored[0].quantity,
            cautious.minds[0].authored[0].reserve
        ),
        (1, 0)
    );
    let mut revising = remembered;
    revising.minds[0].adjust_after_failure = true;
    let revising = revising
        .prepare(
            &[],
            &[Observation {
                person: 1,
                case: Case::Unanswered { tick: 3, target: 2 },
            }],
            3,
            |_, _| 0,
        )
        .unwrap();
    let revising = revising.prepare(&[(1, 2)], &[], 4, |_, _| 0).unwrap();
    assert_eq!(revising.minds[0].authored[0].quantity, 2);
}

#[test]
fn private_disapproval_can_coexist_with_outward_adoption_under_pressure() {
    let mut recipient = mind(2);
    recipient.approve = false;
    recipient.pressure_threshold = Some(3);
    let state = Practices {
        minds: vec![mind(1), recipient],
    }
    .prepare(
        &[],
        &[
            Observation {
                person: 1,
                case: Case::Shortage { tick: 1, hunger: 5 },
            },
            Observation {
                person: 1,
                case: Case::Shortage { tick: 1, hunger: 6 },
            },
            Observation {
                person: 1,
                case: Case::Received {
                    tick: 1,
                    helper: 2,
                    good: 4,
                    quantity: 1,
                },
            },
        ],
        1,
        |_, _| 0,
    )
    .unwrap()
    .prepare(&[(1, 2)], &[], 2, |_, _| 0)
    .unwrap();
    let declined = state.prepare(&[], &[], 3, |_, _| 0).unwrap();
    assert_eq!(declined.minds[1].inbox[0].decision, Decision::Declined);
    let pressured = state
        .prepare(&[], &[], 3, |observer, target| {
            if (observer, target) == (2, 1) { 3 } else { 0 }
        })
        .unwrap();
    assert_eq!(pressured.minds[1].inbox[0].decision, Decision::Adopted);
    assert!(!pressured.minds[1].inbox[0].approved);
    assert_eq!(pressured.minds[0].authored, declined.minds[0].authored);
    let mut controlled = state
        .prepare_with_agency(&[], &[], 3, |_, _| 0, |person| person != 2)
        .unwrap();
    assert_eq!(controlled.minds[1].inbox[0].decision, Decision::Pending);
    controlled.minds[1].decide(1, 1, true, 3).unwrap();
    assert_eq!(controlled.minds[1].inbox[0].decision, Decision::Adopted);
    controlled.minds[1].withdraw(1, 1, 3).unwrap();
    assert_eq!(controlled.minds[1].inbox[0].decision, Decision::Declined);
    assert_eq!(controlled.minds[0].authored, state.minds[0].authored);
    let mut experienced = state.clone();
    experienced.minds[1].approve = true;
    experienced.minds[1].pressure_threshold = None;
    experienced.minds[1].giving_experience_weight = 3;
    experienced.minds[1].acceptance_threshold = 2;
    std::sync::Arc::make_mut(&mut experienced.minds[1].cases).push(Case::Gave {
        tick: 1,
        recipient: 1,
        good: 4,
        quantity: 1,
    });
    assert_eq!(
        experienced.prepare(&[], &[], 3, |_, _| 0).unwrap().minds[1].inbox[0].decision,
        Decision::Adopted
    );
    experienced.minds[1].giving_experience_weight = -3;
    assert_eq!(
        experienced.prepare(&[], &[], 3, |_, _| 0).unwrap().minds[1].inbox[0].decision,
        Decision::Declined
    );
}

#[test]
fn a_crowded_inbox_accepts_only_actually_delivered_proposals_across_workers() {
    let mut target = mind(1);
    target.invent = false;
    let mut minds = vec![target];
    for person in 2..=1025 {
        let mut actor = mind(person);
        actor.cases = vec![
            Case::Shortage { tick: 1, hunger: 5 },
            Case::Shortage { tick: 1, hunger: 6 },
            Case::Received {
                tick: 1,
                helper: 1,
                good: 4,
                quantity: 1,
            },
        ]
        .into();
        minds.push(actor);
    }
    let initial = Practices { minds };
    let contacts: Vec<_> = (2..=1025).map(|id| (id, 1)).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| initial.prepare(&contacts, &[], 2, |_, _| 0).unwrap())
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(result.minds[0].inbox.len(), 8);
    assert_eq!(
        result.minds.iter().map(|m| m.authored.len()).sum::<usize>(),
        8
    );
}

fn paired_invention(count: u64) -> (Practices, Vec<(u64, u64)>) {
    let mut minds = Vec::new();
    let mut contacts = Vec::new();
    for author in (1..count).step_by(2) {
        let helper = author + 1;
        let mut proposer = mind(author);
        proposer.cases = vec![
            Case::Shortage { tick: 1, hunger: 5 },
            Case::Shortage { tick: 1, hunger: 6 },
            Case::Received {
                tick: 1,
                helper,
                good: 4,
                quantity: 1,
            },
        ]
        .into();
        proposer.report_attitude = game_parallel_practices::ReportAttitude::Trust;
        let mut recipient = mind(helper);
        recipient.invent = false;
        recipient.reporting = game_parallel_practices::Reporting::Lie;
        minds.push(proposer);
        minds.push(recipient);
        contacts.push((author, helper));
    }
    (Practices { minds }, contacts)
}

fn complete_invention(initial: &Practices, contacts: &[(u64, u64)]) -> Practices {
    let mut state = initial.clone();
    for tick in 2..=5 {
        state = state
            .prepare_with_evidence(contacts, &[], tick, |_, _| 0, |_| true, |_, _, _| false)
            .unwrap();
    }
    state
}

fn paired_value_learning(count: u64) -> (Practices, Vec<Observation>) {
    let (initial, contacts) = paired_invention(count);
    let mut state = complete_invention(&initial, &contacts);
    for mind in &mut state.minds {
        mind.value_learning = Some(std::sync::Arc::new(ValueLearning {
            value: 0,
            minimum: -10,
            maximum: 10,
            received: 1,
            gave: 1,
            unanswered: 0,
        }));
    }
    let observations = (1..count)
        .step_by(2)
        .flat_map(|author| {
            [
                Observation {
                    person: author,
                    case: Case::Received {
                        tick: 6,
                        helper: author + 1,
                        good: 4,
                        quantity: 1,
                    },
                },
                Observation {
                    person: author + 1,
                    case: Case::Gave {
                        tick: 6,
                        recipient: author,
                        good: 4,
                        quantity: 1,
                    },
                },
            ]
        })
        .collect();
    (state, observations)
}

#[test]
fn independent_learned_values_are_deterministic_across_workers() {
    let (initial, observations) = paired_value_learning(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| initial.prepare(&[], &observations, 6, |_, _| 0).unwrap())
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert!(
        result
            .minds
            .iter()
            .all(|m| m.value_learning.as_ref().unwrap().value == 1)
    );
    assert!(
        initial
            .minds
            .iter()
            .all(|m| m.value_learning.as_ref().unwrap().value == 0)
    );
}

#[test]
#[ignore = "performance measurement"]
fn paired_value_learning_benchmark() {
    use rayon::prelude::*;
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let (initial, observations) = paired_value_learning(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let start = std::time::Instant::now();
                let result = initial.prepare(&[], &observations, 6, |_, _| 0).unwrap();
                assert!(
                    result
                        .minds
                        .par_iter()
                        .all(|m| m.value_learning.as_ref().unwrap().value == 1)
                );
                result.minds.into_par_iter().for_each(drop);
                if round > 0 {
                    samples.push(start.elapsed().as_secs_f64());
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} boundaries=1 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn independent_people_propose_report_and_believe_across_worker_counts() {
    let (initial, contacts) = paired_invention(4096);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| complete_invention(&initial, &contacts))
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(
        result
            .minds
            .iter()
            .map(|mind| mind.reports.len())
            .sum::<usize>(),
        2048
    );
    assert!(
        result
            .minds
            .iter()
            .flat_map(|mind| mind.reports.iter())
            .all(|report| report.statement.claimed_ready
                && report.belief == game_parallel_practices::Belief::Believes)
    );
}

#[test]
#[ignore = "performance measurement"]
fn paired_invention_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000u64);
    let (initial, contacts) = paired_invention(count);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let start = std::time::Instant::now();
                let result = complete_invention(&initial, &contacts);
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(
                    result
                        .minds
                        .iter()
                        .map(|mind| mind.reports.len())
                        .sum::<usize>(),
                    count as usize / 2
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} people={count} boundaries=4 elapsed={:.6}",
                samples[1]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
}
