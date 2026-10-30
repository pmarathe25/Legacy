use super::*;
fn state() -> Agreements {
    Agreements {
        offering: vec![],
        next_id: 1,
        policies: Vec::new(),
        bulk_debtors: Vec::new(),
        contracts: Vec::new(),
        events: Vec::new(),
    }
}
#[test]
fn promises_need_consent_and_only_successful_transfer_can_publish_progress() {
    let people = HashSet::from([1, 2]);
    let goods = HashSet::from([3]);
    let (proposed, ids) = state()
        .propose(
            &[Proposal {
                debtor: 1,
                creditor: 2,
                good: 3,
                quantity: 5,
                due: 5,
            }],
            &people,
            &goods,
            0,
        )
        .unwrap();
    assert!(
        proposed
            .deliver(
                &[Delivery {
                    contract: ids[0],
                    actor: 1,
                    quantity: 2
                }],
                0
            )
            .is_err()
    );
    assert!(
        proposed
            .decide(
                &[Decision {
                    contract: ids[0],
                    actor: 1,
                    accept: true
                }],
                0
            )
            .is_err()
    );
    let active = proposed
        .decide(
            &[Decision {
                contract: ids[0],
                actor: 2,
                accept: true,
            }],
            0,
        )
        .unwrap();
    let staged = active
        .deliver(
            &[Delivery {
                contract: ids[0],
                actor: 1,
                quantity: 2,
            }],
            1,
        )
        .unwrap();
    assert_eq!(active.contracts[0].delivered, 0);
    assert_eq!(
        staged.transfers,
        vec![Transfer {
            debtor: 1,
            creditor: 2,
            good: 3,
            quantity: 2
        }]
    );
    assert_eq!(staged.state.contracts[0].status, Status::Active);
    let complete = staged
        .state
        .deliver(
            &[Delivery {
                contract: ids[0],
                actor: 1,
                quantity: 3,
            }],
            5,
        )
        .unwrap()
        .state;
    assert_eq!(complete.contracts[0].status, Status::Fulfilled);
    assert_eq!(complete.advance(6).contracts[0].status, Status::Fulfilled);
    complete.validate(&people, &goods, 5).unwrap();
}
#[test]
fn missed_deadlines_default_and_bad_batches_leave_input_unchanged() {
    let people = HashSet::from([1, 2, 3]);
    let goods = HashSet::from([4]);
    let proposals = vec![
        Proposal {
            debtor: 1,
            creditor: 2,
            good: 4,
            quantity: 4,
            due: 2,
        },
        Proposal {
            debtor: 3,
            creditor: 2,
            good: 4,
            quantity: 1,
            due: 2,
        },
    ];
    let (proposed, ids) = state().propose(&proposals, &people, &goods, 0).unwrap();
    let before = proposed.clone();
    assert!(
        proposed
            .decide(
                &[
                    Decision {
                        contract: ids[0],
                        actor: 2,
                        accept: true
                    },
                    Decision {
                        contract: ids[1],
                        actor: 1,
                        accept: true
                    }
                ],
                0
            )
            .is_err()
    );
    assert_eq!(proposed, before);
    let active = proposed
        .decide(
            &[Decision {
                contract: ids[0],
                actor: 2,
                accept: true,
            }],
            0,
        )
        .unwrap();
    let after = active.advance(3);
    assert_eq!(after.contracts[0].status, Status::Defaulted);
    assert_eq!(after.contracts[1].status, Status::Withdrawn);
    assert!(
        active
            .deliver(
                &[Delivery {
                    contract: ids[0],
                    actor: 1,
                    quantity: 1
                }],
                3
            )
            .is_err()
    );
    after.validate(&people, &goods, 3).unwrap();
}
#[test]
fn large_batches_are_deterministic_across_worker_counts() {
    let count = 10000u64;
    let people = (1..=count * 2).collect();
    let goods = HashSet::from([1]);
    let proposals: Vec<_> = (1..=count)
        .map(|id| Proposal {
            debtor: id,
            creditor: id + count,
            good: 1,
            quantity: 2,
            due: 9,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let (proposed, ids) = state().propose(&proposals, &people, &goods, 0).unwrap();
                let decisions: Vec<_> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, &contract)| Decision {
                        contract,
                        actor: i as u64 + count + 1,
                        accept: true,
                    })
                    .collect();
                let active = proposed.decide(&decisions, 0).unwrap();
                let requests: Vec<_> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, &contract)| Delivery {
                        contract,
                        actor: i as u64 + 1,
                        quantity: 1,
                    })
                    .collect();
                let draft = active.deliver(&requests, 1).unwrap();
                (draft.state, draft.transfers)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn scheduled_deadlines_match_full_scans_with_shuffled_contracts_and_reload() {
    let people = HashSet::from([1, 2]);
    let goods = HashSet::from([1]);
    let proposals: Vec<_> = (0..9000)
        .map(|i| Proposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 5,
            due: 1 + (i % 8),
        })
        .collect();
    let (mut initial, ids) = state().propose(&proposals, &people, &goods, 0).unwrap();
    let decisions: Vec<_> = ids
        .iter()
        .step_by(2)
        .map(|&contract| Decision {
            contract,
            actor: 2,
            accept: true,
        })
        .collect();
    initial = initial.decide(&decisions, 0).unwrap();
    initial.contracts.reverse();
    let mut expected = initial.clone();
    let mut actual = initial;
    let mut runtime = crate::runtime::Runtime::compile(&actual);
    for tick in 1..=10 {
        expected.advance_in_place(tick);
        runtime.advance(&mut actual, tick);
        assert_eq!(actual, expected);
        if tick == 4 {
            runtime = crate::runtime::Runtime::compile(&actual);
        }
    }
}

#[test]
fn private_draft_decisions_handle_unsorted_contracts_and_fail_atomically() {
    let people = HashSet::from([1, 2, 3]);
    let goods = HashSet::from([4]);
    let proposals = vec![
        Proposal {
            debtor: 1,
            creditor: 2,
            good: 4,
            quantity: 1,
            due: 5,
        },
        Proposal {
            debtor: 3,
            creditor: 2,
            good: 4,
            quantity: 1,
            due: 5,
        },
    ];
    let (mut draft, ids) = state().propose(&proposals, &people, &goods, 0).unwrap();
    draft.contracts.reverse();
    let before = draft.clone();
    assert!(
        draft
            .decide_in_place(
                &[
                    Decision {
                        contract: ids[0],
                        actor: 2,
                        accept: true
                    },
                    Decision {
                        contract: ids[1],
                        actor: 1,
                        accept: true
                    },
                ],
                1
            )
            .is_err()
    );
    assert_eq!(draft, before);
    let choices = [
        Decision {
            contract: ids[0],
            actor: 2,
            accept: true,
        },
        Decision {
            contract: ids[1],
            actor: 2,
            accept: false,
        },
    ];
    let expected = before.decide(&choices, 1).unwrap();
    draft.decide_in_place(&choices, 1).unwrap();
    assert_eq!(draft, expected);
    draft.validate(&people, &goods, 1).unwrap();
}
