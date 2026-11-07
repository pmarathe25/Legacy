use game_practices::{Decision, Mind, Practices};

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

fn fixture() -> Practices {
    use game_practices::work::{Mind as WorkMind, Proposal, Received, outcome::Policy};
    let mut worker = mind(2);
    worker.work = Some(std::sync::Arc::new(WorkMind {
        invent: false,
        approve: true,
        evaluation: None,
        outcome_policy: Some(Policy {
            wait_ticks: 2,
            missing_trust: -3,
            missing_affinity: 2,
            ..Default::default()
        }),
        willing: false,
        ask_willing: false,
        review_interval: 1,
        proposal_lifetime: 20,
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
                author: 1,
                sequence: 1,
                worker: 2,
                recipe: 7,
                good: 4,
                quantity: 2,
                reserve: 0,
                created: 1,
                expires: 20,
            },
            delivered: 1,
            decision: Decision::Adopted,
            approved: true,
        }],
    }));
    Practices {
        minds: vec![mind(1), worker, mind(3)],
    }
}

fn advance(
    prior: &Practices,
    starts: &[game_practices::work::outcome::Start],
    finished: &[u64],
    gifts: &[game_practices::work::GiftObservation],
    tick: u64,
) -> (Practices, Vec<game_practices::work::outcome::Reaction>) {
    let mut next = prior.clone();
    let reactions = game_practices::work::outcome::advance(
        prior,
        &mut next,
        starts,
        finished,
        &[],
        gifts,
        tick,
    );
    (next, reactions)
}

#[test]
fn personally_expected_gifts_are_reviewed_once_without_asserting_dishonesty() {
    use game_practices::work::{Gift, GiftObservation, outcome::Start};
    use std::collections::HashSet;
    let start = Start {
        requester: 1,
        worker: 2,
        recipe: 7,
        request: 10,
        job: 20,
    };
    let mut unheard = fixture();
    std::sync::Arc::make_mut(unheard.minds[1].work.as_mut().unwrap()).inbox[0].decision =
        Decision::Pending;
    std::sync::Arc::make_mut(unheard.minds[1].work.as_mut().unwrap()).inbox[0].approved = false;
    assert!(
        advance(&unheard, &[start], &[], &[], 2).0.minds[1]
            .work
            .as_ref()
            .unwrap()
            .experiences
            .is_empty()
    );
    let mut same_tick = fixture();
    std::sync::Arc::make_mut(same_tick.minds[1].work.as_mut().unwrap()).inbox[0].delivered = 2;
    assert!(
        advance(&same_tick, &[start], &[], &[], 2).0.minds[1]
            .work
            .as_ref()
            .unwrap()
            .experiences
            .is_empty()
    );
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let (mut started, reactions) = advance(&fixture(), &[start], &[], &[], 2);
                assert!(reactions.is_empty());
                let work = std::sync::Arc::make_mut(started.minds[1].work.as_mut().unwrap());
                let mut revision = work.inbox[0].clone();
                revision.proposal.sequence = 2;
                revision.proposal.good = 9;
                revision.proposal.created = 3;
                revision.delivered = 3;
                work.inbox.push(revision);
                let gift = GiftObservation {
                    person: 2,
                    action: 11,
                    gift: Gift {
                        tick: 3,
                        other: 1,
                        good: 4,
                        quantity: 1,
                        given: false,
                    },
                };
                let (completed, reactions) = advance(&started, &[], &[20], &[gift, gift], 3);
                assert!(reactions.is_empty());
                let record = &completed.minds[1].work.as_ref().unwrap().experiences[0];
                assert_eq!(
                    (record.proposal.good, record.received, record.completed),
                    (4, 1, Some(3))
                );
                completed
                    .validate_with_goods(
                        &HashSet::from([1, 2, 3]),
                        &HashSet::from([4]),
                        &HashSet::from([4, 9]),
                        3,
                    )
                    .unwrap();
                let restored: Practices =
                    serde_json::from_str(&serde_json::to_string(&completed).unwrap()).unwrap();
                let (waiting, reactions) = advance(&restored, &[], &[], &[], 4);
                assert!(reactions.is_empty());
                let (reviewed, reactions) = advance(&waiting, &[], &[], &[], 5);
                assert_eq!(reactions.len(), 1);
                assert_eq!(
                    (
                        reactions[0].observer,
                        reactions[0].target,
                        reactions[0].trust,
                        reactions[0].affinity
                    ),
                    (2, 1, -3, 2)
                );
                assert!(advance(&reviewed, &[], &[], &[], 6).1.is_empty());
                let last = GiftObservation {
                    action: 12,
                    gift: Gift {
                        tick: 5,
                        ..gift.gift
                    },
                    ..gift
                };
                let (satisfied, reactions) = advance(&waiting, &[], &[], &[last], 5);
                assert!(reactions.is_empty());
                assert_eq!(
                    satisfied.minds[1].work.as_ref().unwrap().experiences[0].received,
                    2
                );
                reviewed
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn individuals_interpret_full_partial_and_missing_receipts_independently() {
    use game_practices::work::{Gift, GiftObservation, outcome::Start};
    let mut initial = fixture();
    let work = std::sync::Arc::make_mut(initial.minds[1].work.as_mut().unwrap());
    let policy = work.outcome_policy.as_mut().unwrap();
    policy.received_trust = 4;
    policy.received_affinity = -5;
    policy.partial_trust = Some(6);
    policy.partial_affinity = Some(-7);
    policy.missing_fear = 2;
    policy.missing_obligation = -4;
    policy.received_fear = -2;
    policy.received_obligation = 4;
    policy.partial_fear = Some(3);
    policy.partial_obligation = Some(-5);
    let start = Start {
        requester: 1,
        worker: 2,
        recipe: 7,
        request: 10,
        job: 20,
    };
    let gift = |action, quantity, tick| GiftObservation {
        person: 2,
        action,
        gift: Gift {
            tick,
            other: 1,
            good: 4,
            quantity,
            given: false,
        },
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let (started, _) = advance(&initial, &[start], &[], &[], 2);
                let (completed, _) = advance(&started, &[], &[20], &[], 3);
                let (partial, reactions) = advance(&completed, &[], &[], &[gift(11, 1, 4)], 4);
                assert!(reactions.is_empty());
                let (partial_reviewed, reactions) = advance(&partial, &[], &[], &[], 5);
                assert_eq!(
                    reactions
                        .iter()
                        .map(|r| (
                            r.observer,
                            r.target,
                            r.trust,
                            r.affinity,
                            r.fear,
                            r.obligation
                        ))
                        .collect::<Vec<_>>(),
                    [(2, 1, 6, -7, 3, -5)]
                );
                assert!(advance(&partial_reviewed, &[], &[], &[], 6).1.is_empty());
                let (full_reviewed, reactions) = advance(&partial, &[], &[], &[gift(12, 1, 5)], 5);
                assert_eq!(
                    reactions
                        .iter()
                        .map(|r| (
                            r.observer,
                            r.target,
                            r.trust,
                            r.affinity,
                            r.fear,
                            r.obligation
                        ))
                        .collect::<Vec<_>>(),
                    [(2, 1, 4, -5, -2, 4)]
                );
                assert!(advance(&full_reviewed, &[], &[], &[], 6).1.is_empty());
                let (waiting, _) = advance(&completed, &[], &[], &[], 4);
                let (missing, reactions) = advance(&waiting, &[], &[], &[], 5);
                assert_eq!(
                    reactions
                        .iter()
                        .map(|r| (
                            r.observer,
                            r.target,
                            r.trust,
                            r.affinity,
                            r.fear,
                            r.obligation
                        ))
                        .collect::<Vec<_>>(),
                    [(2, 1, -3, 2, 2, -4)]
                );
                assert!(
                    advance(&missing, &[], &[], &[gift(13, 2, 6)], 6)
                        .1
                        .is_empty()
                );
                let restored: Practices =
                    serde_json::from_str(&serde_json::to_string(&full_reviewed).unwrap()).unwrap();
                assert_eq!(restored, full_reviewed);
                (partial_reviewed, full_reviewed, missing)
            })
    };
    assert_eq!(run(1), run(4));
    let mut old = serde_json::to_value(&initial).unwrap();
    let policy = old["minds"][1]["work"]["outcome_policy"]
        .as_object_mut()
        .unwrap();
    for key in [
        "received_trust",
        "received_affinity",
        "partial_trust",
        "partial_affinity",
        "missing_fear",
        "missing_obligation",
        "received_fear",
        "received_obligation",
        "partial_fear",
        "partial_obligation",
    ] {
        policy.remove(key);
    }
    let loaded: Practices = serde_json::from_value(old).unwrap();
    let policy = loaded.minds[1]
        .work
        .as_ref()
        .unwrap()
        .outcome_policy
        .unwrap();
    assert_eq!(
        (
            policy.missing_fear,
            policy.missing_obligation,
            policy.received_fear,
            policy.received_obligation,
            policy.partial_fear,
            policy.partial_obligation
        ),
        (0, 0, 0, 0, None, None)
    );
    assert_eq!(
        (
            policy.received_trust,
            policy.received_affinity,
            policy.partial_trust,
            policy.partial_affinity
        ),
        (0, 0, None, None)
    );
}

#[test]
fn one_gift_is_not_counted_toward_multiple_personal_expectations() {
    use game_practices::work::{Gift, GiftObservation, outcome::Start};
    let first = Start {
        requester: 1,
        worker: 2,
        recipe: 7,
        request: 10,
        job: 20,
    };
    let second = Start {
        request: 11,
        job: 21,
        ..first
    };
    let (started, _) = advance(&fixture(), &[first], &[], &[], 2);
    let (started, _) = advance(&started, &[second], &[], &[], 3);
    let (completed, _) = advance(&started, &[], &[20, 21], &[], 4);
    let gift = GiftObservation {
        person: 2,
        action: 42,
        gift: Gift {
            tick: 5,
            other: 1,
            good: 4,
            quantity: 2,
            given: false,
        },
    };
    let (paid, reactions) = advance(&completed, &[], &[], &[gift, gift], 5);
    assert!(reactions.is_empty());
    let experiences = &paid.minds[1].work.as_ref().unwrap().experiences;
    assert_eq!((experiences[0].received, experiences[1].received), (2, 0));
    let (_, reactions) = advance(&paid, &[], &[], &[], 6);
    assert_eq!(reactions.len(), 1);
}

#[test]
fn interrupted_work_does_not_create_a_completed_work_expectation() {
    use game_practices::work::outcome::{Start, advance as observe};
    let start = Start {
        requester: 1,
        worker: 2,
        recipe: 7,
        request: 10,
        job: 20,
    };
    let (started, _) = advance(&fixture(), &[start], &[], &[], 2);
    let mut next = started.clone();
    assert!(observe(&started, &mut next, &[], &[], &[20], &[], 3).is_empty());
    assert!(next.minds[1].work.as_ref().unwrap().experiences.is_empty());
    assert!(advance(&next, &[], &[], &[], 10).1.is_empty());
}

#[test]
fn worker_start_observation_handles_reordered_staged_people() {
    use game_practices::work::outcome::{Start, advance as observe};
    let prior = fixture();
    let start = Start {
        requester: 1,
        worker: 2,
        recipe: 7,
        request: 10,
        job: 20,
    };
    let (expected, _) = advance(&prior, &[start], &[], &[], 2);
    let mut reordered = prior.clone();
    reordered.minds.reverse();
    assert!(observe(&prior, &mut reordered, &[start], &[], &[], &[], 2).is_empty());
    reordered.minds.sort_unstable_by_key(|person| person.person);
    assert_eq!(reordered, expected);
}
