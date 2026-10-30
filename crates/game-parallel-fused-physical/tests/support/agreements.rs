use game_parallel::{
    AgreementDecision, AgreementDelivery, AgreementProposal, AgreementStatus, World,
};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/agreements.toml")).unwrap()
}
#[test]
fn fused_promise_custody_and_default_match_the_editable_host() {
    let mut reference = world();
    let mut kernel = Kernel::compile(reference.clone()).unwrap();
    let proposal = AgreementProposal {
        debtor: 1,
        creditor: 2,
        good: 1,
        quantity: 4,
        due: 2,
    };
    let ids = reference
        .propose_agreements(std::slice::from_ref(&proposal))
        .unwrap();
    assert_eq!(ids, kernel.propose_agreements(&[proposal]).unwrap());
    let decision = AgreementDecision {
        contract: ids[0],
        actor: 2,
        accept: true,
    };
    reference.decide_agreements(&[decision]).unwrap();
    kernel.decide_agreements(&[decision]).unwrap();
    let delivery = AgreementDelivery {
        contract: ids[0],
        actor: 1,
        quantity: 2,
    };
    reference.deliver_agreements(&[delivery]).unwrap();
    kernel.deliver_agreements(&[delivery]).unwrap();
    assert_eq!(reference, kernel.export());
    for _ in 0..3 {
        reference.step().unwrap();
        kernel.step().unwrap();
    }
    assert_eq!(reference, kernel.export());
    assert_eq!(
        kernel.export().agreements.unwrap().contracts[0].status,
        AgreementStatus::Defaulted
    );
}
#[test]
fn failed_delivery_and_failed_tick_leave_agreements_and_custody_unchanged() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let content = world();
    state.production = content.production;
    state.goods_cargo = content.goods_cargo;
    state.agreements = content.agreements;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let id = kernel
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 6,
            due: 1,
        }])
        .unwrap()[0];
    kernel
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let before = kernel.export();
    assert!(
        kernel
            .deliver_agreements(&[AgreementDelivery {
                contract: id,
                actor: 1,
                quantity: 6
            }])
            .is_err()
    );
    assert_eq!(kernel.export(), before);
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn opted_in_debtors_fulfill_one_competing_promise_per_tick_with_real_goods() {
    use game_parallel::{AgreementPolicy, Person};
    let mut state = world();
    state.regions[0].people.push(Person {
        id: 4,
        health: 100,
        hunger: 0,
    });
    state
        .agreements
        .as_mut()
        .unwrap()
        .policies
        .push(AgreementPolicy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        });
    let ids = state
        .propose_agreements(&[
            AgreementProposal {
                debtor: 1,
                creditor: 2,
                good: 1,
                quantity: 2,
                due: 3,
            },
            AgreementProposal {
                debtor: 1,
                creditor: 4,
                good: 1,
                quantity: 2,
                due: 3,
            },
        ])
        .unwrap();
    state
        .decide_agreements(&[
            AgreementDecision {
                contract: ids[0],
                actor: 2,
                accept: true,
            },
            AgreementDecision {
                contract: ids[1],
                actor: 4,
                accept: true,
            },
        ])
        .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    assert_eq!(
        first.agreements.as_ref().unwrap().contracts[0].status,
        AgreementStatus::Fulfilled
    );
    assert_eq!(
        first.agreements.as_ref().unwrap().contracts[1].status,
        AgreementStatus::Active
    );
    assert_eq!(
        first
            .goods_cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .find(|h| h.person == 2)
            .unwrap()
            .items[0]
            .quantity,
        2
    );
    kernel.step().unwrap();
    let second = kernel.export();
    assert_eq!(
        second.agreements.as_ref().unwrap().contracts[1].status,
        AgreementStatus::Fulfilled
    );
    let save = serde_json::to_string(&second).unwrap();
    let mut reload = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
    for _ in 0..4 {
        kernel.step().unwrap();
        reload.step().unwrap();
    }
    assert_eq!(kernel.export(), reload.export());
    assert!(
        kernel
            .export()
            .agreements
            .unwrap()
            .contracts
            .iter()
            .all(|c| c.status == AgreementStatus::Fulfilled)
    );
}
#[test]
fn automatic_fulfillment_is_not_published_when_a_later_owner_fails() {
    use game_parallel::AgreementPolicy;
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let content = world();
    state.production = content.production;
    state.goods_cargo = content.goods_cargo;
    state.agreements = content.agreements;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    state
        .agreements
        .as_mut()
        .unwrap()
        .policies
        .push(AgreementPolicy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        });
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 2,
            due: 3,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn automatic_promises_and_disjoint_trades_both_preserve_their_custody_effects() {
    use game_parallel::{
        AgreementPolicy, Amount, GoodsHolding, Person, TradePolicy, TradeProposal, TradeValue,
    };
    let mut state = world();
    let person3 = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(person3);
    state.regions[0].people.push(Person {
        id: 4,
        health: 100,
        hunger: 0,
    });
    state
        .goods_cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(GoodsHolding {
            person: 3,
            items: vec![Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
        });
    state
        .agreements
        .as_mut()
        .unwrap()
        .policies
        .push(AgreementPolicy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        });
    state.market.as_mut().unwrap().policies.push(TradePolicy {
        person: 4,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        influences: Vec::new(),
        values: vec![TradeValue { good: 1, value: 1 }],
    });
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 2,
            due: 3,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    state
        .propose_trade(TradeProposal {
            proposer: 3,
            recipient: 4,
            give: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            receive: vec![],
        })
        .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    let cargo = result.goods_cargo.as_ref().unwrap();
    let held = |id| {
        cargo
            .holdings
            .iter()
            .find(|h| h.person == id)
            .unwrap()
            .items[0]
            .quantity
    };
    assert_eq!((held(1), held(2), held(3), held(4)), (3, 2, 2, 1));
    assert_eq!(
        result.agreements.unwrap().contracts[0].status,
        AgreementStatus::Fulfilled
    );
    assert!(result.market.unwrap().offers.is_empty());
}

#[test]
fn authored_automatic_agreement_scenario_is_playable() {
    let state = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_auto.toml"
    ))
    .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.agreements.unwrap().contracts[0].status,
        AgreementStatus::Fulfilled
    );
    assert_eq!(
        result
            .goods_cargo
            .unwrap()
            .holdings
            .iter()
            .find(|h| h.person == 2)
            .unwrap()
            .items[0]
            .quantity,
        2
    );
}

#[test]
fn thousands_of_automatic_deliveries_are_worker_invariant_and_survive_reload() {
    use game_parallel::{Agreement, AgreementPolicy, Amount, GoodsHolding, Person};
    let mut state = world();
    let count = 3000u64;
    state.regions[0].people = (1..=count * 2)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.market = None;
    state.goods_cargo.as_mut().unwrap().holdings = (1..=count)
        .map(|person| GoodsHolding {
            person,
            items: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let owner = state.agreements.as_mut().unwrap();
    owner.next_id = count + 1;
    owner.policies = (1..=count)
        .map(|person| AgreementPolicy {
            person,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        })
        .collect();
    owner.contracts = (1..=count)
        .map(|id| Agreement {
            id,
            debtor: id,
            creditor: id + count,
            good: 1,
            quantity: 1,
            delivered: 0,
            due: 2,
            status: AgreementStatus::Active,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let save = serde_json::to_string(&kernel.export()).unwrap();
                let mut reloaded = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
                kernel.step().unwrap();
                reloaded.step().unwrap();
                assert_eq!(kernel.export(), reloaded.export());
                kernel.export()
            })
    };
    let serial = run(1);
    let parallel = run(4);
    assert_eq!(serial, parallel);
    assert!(
        parallel
            .agreements
            .unwrap()
            .contracts
            .iter()
            .all(|c| c.status == AgreementStatus::Fulfilled)
    );
}

#[test]
fn automatic_custody_matches_checked_exchange_and_retries_when_capacity_frees() {
    use game_parallel::{AgreementPolicy, Amount, GoodsDirection, GoodsHolding, GoodsTransfer};
    let mut state = world();
    state.goods_cargo.as_mut().unwrap().holdings[0]
        .items
        .push(Amount {
            good: 2,
            quantity: 2,
        });
    state
        .goods_cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(GoodsHolding {
            person: 2,
            items: vec![
                Amount {
                    good: 1,
                    quantity: 4,
                },
                Amount {
                    good: 2,
                    quantity: 4,
                },
            ]
            .into(),
        });
    state
        .agreements
        .as_mut()
        .unwrap()
        .policies
        .push(AgreementPolicy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        });
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 3,
            due: 4,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().agreements.unwrap().contracts[0].status,
        AgreementStatus::Active
    );
    kernel
        .transfer_goods(&[GoodsTransfer {
            person: 2,
            good: 2,
            quantity: 2,
            direction: GoodsDirection::Deposit,
        }])
        .unwrap();
    let mut expected = kernel.export();
    expected
        .deliver_agreements(&[AgreementDelivery {
            contract: id,
            actor: 1,
            quantity: 3,
        }])
        .unwrap();
    kernel.step().unwrap();
    let actual = kernel.export();
    assert_eq!(actual.goods_cargo, expected.goods_cargo);
    assert_eq!(
        actual.agreements.unwrap().contracts[0].status,
        AgreementStatus::Fulfilled
    );
}

#[test]
fn contested_creditor_grants_one_promise_per_tick_without_losing_goods() {
    use game_parallel::{Agreement, AgreementPolicy, Amount, GoodsHolding, Person};
    let mut state = world();
    let count = 6000u64;
    state.regions[0].people = (1..=count + 1)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.market = None;
    state.goods_cargo.as_mut().unwrap().holdings = (1..=count)
        .map(|person| GoodsHolding {
            person,
            items: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let owner = state.agreements.as_mut().unwrap();
    owner.next_id = count + 1;
    owner.policies = (1..=count)
        .map(|person| AgreementPolicy {
            person,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        })
        .collect();
    owner.contracts = (1..=count)
        .map(|id| Agreement {
            id,
            debtor: id,
            creditor: count + 1,
            good: 1,
            quantity: 1,
            delivered: 0,
            due: 10,
            status: AgreementStatus::Active,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for _ in 0..3 {
                    kernel.step().unwrap();
                }
                kernel.export()
            })
    };
    let one = run(1);
    let four = run(4);
    assert_eq!(one, four);
    let fulfilled: Vec<_> = one
        .agreements
        .unwrap()
        .contracts
        .into_iter()
        .filter(|c| c.status == AgreementStatus::Fulfilled)
        .map(|c| c.id)
        .collect();
    assert_eq!(fulfilled, vec![1, 2, 3]);
    let cargo = one.goods_cargo.unwrap();
    assert_eq!(
        cargo
            .holdings
            .iter()
            .flat_map(|h| h.items.iter())
            .map(|item| item.quantity)
            .sum::<u64>(),
        count
    );
    assert_eq!(
        cargo
            .holdings
            .iter()
            .find(|h| h.person == count + 1)
            .unwrap()
            .items[0]
            .quantity,
        3
    );
}

#[test]
fn sparse_shuffled_custody_patches_match_checked_batch_exchange() {
    use game_parallel::{Agreement, AgreementPolicy, Amount, GoodsHolding, Person};
    let mut state = world();
    let pairs = 2000u64;
    state.regions[0].people = (1..=pairs * 2)
        .rev()
        .map(|n| Person {
            id: n * 3,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.market = None;
    state.goods_cargo.as_mut().unwrap().holdings = (1..=pairs)
        .rev()
        .map(|n| GoodsHolding {
            person: n * 3,
            items: vec![
                Amount {
                    good: 1,
                    quantity: 3,
                },
                Amount {
                    good: 2,
                    quantity: 1,
                },
            ]
            .into(),
        })
        .collect();
    let owner = state.agreements.as_mut().unwrap();
    owner.next_id = pairs + 1;
    owner.policies = (1..=pairs)
        .rev()
        .map(|n| AgreementPolicy {
            person: n * 3,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        })
        .collect();
    owner.contracts = (1..=pairs)
        .rev()
        .map(|n| Agreement {
            id: n,
            debtor: n * 3,
            creditor: (n + pairs) * 3,
            good: 1,
            quantity: 2,
            delivered: 0,
            due: 5,
            status: AgreementStatus::Active,
        })
        .collect();
    let mut expected = state.clone();
    let deliveries: Vec<_> = (1..=pairs)
        .map(|id| AgreementDelivery {
            contract: id,
            actor: id * 3,
            quantity: 2,
        })
        .collect();
    expected.deliver_agreements(&deliveries).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().goods_cargo, expected.goods_cargo);
}

#[test]
fn opted_in_recipient_accepts_before_debtor_automatically_delivers() {
    use game_parallel::{AgreementEventKind, AgreementPolicy};
    let mut state = world();
    state.agreements.as_mut().unwrap().policies = vec![
        AgreementPolicy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        },
        AgreementPolicy {
            person: 2,
            honor: false,
            accept: true,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        },
    ];
    let id = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 2,
            due: 4,
        }])
        .unwrap()[0];
    let original_cargo = state.goods_cargo.clone();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let accepted = kernel.export();
    assert_eq!(
        accepted.agreements.as_ref().unwrap().contracts[0].status,
        AgreementStatus::Active
    );
    assert!(
        accepted
            .agreements
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.contract == id && event.kind == AgreementEventKind::Accepted)
    );
    assert_eq!(accepted.goods_cargo, original_cargo);
    kernel.step().unwrap();
    let delivered = kernel.export();
    assert_eq!(
        delivered.agreements.unwrap().contracts[0].status,
        AgreementStatus::Fulfilled
    );
    assert_eq!(
        delivered
            .goods_cargo
            .unwrap()
            .holdings
            .iter()
            .find(|row| row.person == 2)
            .unwrap()
            .items[0]
            .quantity,
        2
    );
}

#[test]
fn authored_negotiation_completes_with_consent_and_physical_delivery() {
    let state = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_negotiation.toml"
    ))
    .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().agreements.unwrap().contracts[0].status,
        AgreementStatus::Active
    );
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().agreements.unwrap().contracts[0].status,
        AgreementStatus::Fulfilled
    );
}

#[test]
fn fear_can_motivate_delivery_without_an_ordinary_willingness_to_honor() {
    let run = |workers, fear| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = World::from_toml(include_str!(
                    "../../../../content/parallel/agreements_auto.toml"
                ))
                .unwrap();
                let policy = &mut state.agreements.as_mut().unwrap().policies[0];
                policy.honor = false;
                policy.minimum_honor_trust = None;
                policy.fear_honor_threshold = Some(2);
                assert!(state.validate().is_err());
                let mut opinions =
                    World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
                        .unwrap()
                        .opinions
                        .unwrap();
                opinions.rules.fear_decay_per_tick = 0;
                opinions.records = vec![game_parallel::Opinion {
                    observer: 1,
                    target: 2,
                    trust: -1,
                    fear,
                    obligation: 0,
                    affinity: 0,
                }];
                state.opinions = Some(opinions);
                let mut kernel = Kernel::compile(state).unwrap();
                kernel.step().unwrap();
                let first = kernel.export();
                assert_eq!(
                    first.agreements.as_ref().unwrap().contracts[0].status,
                    if fear >= 2 {
                        AgreementStatus::Fulfilled
                    } else {
                        AgreementStatus::Active
                    }
                );
                let mut reloaded = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..3 {
                    kernel.step().unwrap();
                    reloaded.step().unwrap();
                }
                let result = kernel.export();
                assert_eq!(result, reloaded.export());
                assert_eq!(
                    result.agreements.as_ref().unwrap().contracts[0].status,
                    if fear >= 2 {
                        AgreementStatus::Fulfilled
                    } else {
                        AgreementStatus::Defaulted
                    }
                );
                assert_eq!(
                    result
                        .goods_cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .find(|h| h.person == 1)
                        .unwrap()
                        .items[0]
                        .quantity,
                    if fear >= 2 { 3 } else { 5 }
                );
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 2), -1);
                result
            })
    };
    for fear in [0, 2] {
        assert_eq!(run(1, fear), run(4, fear));
    }
}
#[test]
fn proposer_can_withdraw_while_recipient_is_engaged_elsewhere() {
    use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, Conflict};
    let mut state = world();
    let person = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(person);
    state.conflict = Some(Conflict {
        damage: 1,
        next_id: 1,
        encounters: Vec::new(),
        events: Vec::new(),
    });
    let mut kernel = Kernel::compile(state).unwrap();
    let id = kernel
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 1,
            due: 3,
        }])
        .unwrap()[0];
    kernel
        .begin_combat(CombatEncounter {
            id: 1,
            region: 1,
            fighters: vec![
                CombatFighter {
                    person: 2,
                    side: 1,
                    order: CombatOrder::Attack { target: 3 },
                },
                CombatFighter {
                    person: 3,
                    side: 2,
                    order: CombatOrder::Attack { target: 2 },
                },
            ],
        })
        .unwrap();
    kernel
        .decide_agreements(&[AgreementDecision {
            contract: id,
            actor: 1,
            accept: false,
        }])
        .unwrap();
    assert_eq!(
        kernel.export().agreements.unwrap().contracts[0].status,
        AgreementStatus::Withdrawn
    );
}

#[test]
fn default_changes_only_the_creditors_trust_and_influences_later_choices() {
    use game_parallel::{AgreementPolicy, OpinionRules, Opinions};
    let mut state = world();
    state.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 2,
            default_loss: 2,
            theft_loss: 0,
            encounter_gain: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
        },
        records: vec![],
        events: vec![],
    });
    state.agreements.as_mut().unwrap().policies = vec![
        AgreementPolicy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        },
        AgreementPolicy {
            person: 2,
            honor: false,
            accept: true,
            minimum_trust: Some(0),
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        },
    ];
    let first = state
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 6,
            due: 1,
        }])
        .unwrap()[0];
    state
        .decide_agreements(&[AgreementDecision {
            contract: first,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    let missed = kernel.export();
    assert_eq!(
        missed.agreements.as_ref().unwrap().contracts[0].status,
        AgreementStatus::Defaulted
    );
    assert_eq!(missed.opinions.as_ref().unwrap().index().trust(2, 1), -2);
    assert_eq!(missed.opinions.as_ref().unwrap().index().trust(1, 2), 0);
    let second = kernel
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 1,
            due: 6,
        }])
        .unwrap()[0];
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().agreements.as_ref().unwrap().contracts[1].status,
        AgreementStatus::Proposed
    );
    kernel
        .decide_agreements(&[AgreementDecision {
            contract: second,
            actor: 2,
            accept: true,
        }])
        .unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().opinions.unwrap().index().trust(2, 1), 0);
    let third = kernel
        .propose_agreements(&[AgreementProposal {
            debtor: 1,
            creditor: 2,
            good: 1,
            quantity: 1,
            due: 8,
        }])
        .unwrap()[0];
    kernel.step().unwrap();
    assert_eq!(
        kernel
            .export()
            .agreements
            .as_ref()
            .unwrap()
            .contracts
            .iter()
            .find(|c| c.id == third)
            .unwrap()
            .status,
        AgreementStatus::Active
    );
    kernel.step().unwrap();
    let after = kernel.export();
    assert_eq!(after.opinions.as_ref().unwrap().index().trust(2, 1), 2);
    after.validate().unwrap();
    let saved = serde_json::to_string(&after).unwrap();
    assert_eq!(after, World::from_json(&saved).unwrap());
}

#[test]
fn authored_distrust_prevents_automatic_consent() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/distrust.toml")).unwrap();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(
        result.agreements.unwrap().contracts[0].status,
        AgreementStatus::Proposed
    );
    assert_eq!(result.opinions.unwrap().index().trust(2, 1), -2);
}

#[test]
fn late_lifecycle_failure_does_not_publish_default_or_trust_changes() {
    use game_parallel::{Agreement, OpinionRules, Opinions};
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let content = world();
    state.production = content.production;
    state.goods_cargo = content.goods_cargo;
    state.agreements = content.agreements;
    state.agreements.as_mut().unwrap().next_id = 2;
    state.agreements.as_mut().unwrap().contracts = vec![Agreement {
        id: 1,
        debtor: 1,
        creditor: 2,
        good: 1,
        quantity: 1,
        delivered: 0,
        due: 0,
        status: AgreementStatus::Active,
    }];
    state.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 2,
            default_loss: 2,
            theft_loss: 0,
            encounter_gain: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
        },
        records: vec![],
        events: vec![],
    });
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
