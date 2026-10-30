use game_parallel::{
    Agreement, AgreementPolicy, AgreementStatus, Amount, GoodsHolding, Person, World,
};
use game_parallel_fused_physical::Kernel;

fn fixture(count: u64, funds: u64) -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/agreements.toml")).unwrap();
    world.regions[0].people = (1..=count + 1)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.rules.food_capacity = u32::try_from(count * 3 + 10).unwrap();
    world.regions[0].food = u32::try_from(count * 3).unwrap();
    world.market = None;
    let cargo = world.goods_cargo.as_mut().unwrap();
    cargo.capacity = count * 2 + 10;
    cargo.holdings = vec![GoodsHolding {
        person: 1,
        items: vec![Amount {
            good: 1,
            quantity: funds,
        }]
        .into(),
    }];
    let agreements = world.agreements.as_mut().unwrap();
    agreements.next_id = count + 1;
    agreements.policies = vec![AgreementPolicy {
        person: 1,
        honor: true,
        accept: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    }];
    agreements.bulk_debtors = vec![1];
    agreements.contracts = (1..=count)
        .map(|id| Agreement {
            id,
            debtor: 1,
            creditor: id + 1,
            good: 1,
            quantity: 1,
            delivered: 0,
            due: 10,
            status: AgreementStatus::Active,
        })
        .collect();
    world.validate().unwrap();
    world
}

#[test]
fn authored_bulk_policy_fulfills_distinct_creditors() {
    let world = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_bulk.toml"
    ))
    .unwrap();
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    let world = kernel.export();
    assert!(
        world
            .agreements
            .unwrap()
            .contracts
            .iter()
            .all(|row| row.status == AgreementStatus::Fulfilled)
    );
    let holdings = world.goods_cargo.unwrap().holdings;
    assert_eq!(
        holdings.iter().find(|row| row.person == 1).unwrap().items[0].quantity,
        1
    );
    for person in [2, 4] {
        assert_eq!(
            holdings
                .iter()
                .find(|row| row.person == person)
                .unwrap()
                .items[0]
                .quantity,
            2
        );
    }
}

#[test]
fn bulk_and_ordinary_obligations_can_settle_in_the_same_tick() {
    let mut world = fixture(2, 2);
    world.regions[0].people.extend((4..=5).map(|id| Person {
        id,
        health: 100,
        hunger: 0,
    }));
    world
        .goods_cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(GoodsHolding {
            person: 4,
            items: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
        });
    let agreements = world.agreements.as_mut().unwrap();
    agreements.next_id = 4;
    agreements.policies.push(AgreementPolicy {
        person: 4,
        honor: true,
        accept: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    });
    agreements.contracts.push(Agreement {
        id: 3,
        debtor: 4,
        creditor: 5,
        good: 1,
        quantity: 1,
        delivered: 0,
        due: 10,
        status: AgreementStatus::Active,
    });
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert!(
        result
            .agreements
            .unwrap()
            .contracts
            .iter()
            .all(|row| row.status == AgreementStatus::Fulfilled)
    );
    let holdings = result.goods_cargo.unwrap().holdings;
    assert_eq!(
        holdings
            .iter()
            .map(|holding| holding.person)
            .collect::<Vec<_>>(),
        vec![2, 3, 5]
    );
    assert_eq!(
        holdings
            .iter()
            .flat_map(|holding| &holding.items)
            .map(|item| item.quantity)
            .sum::<u64>(),
        3
    );
}

#[test]
fn a_person_can_change_their_mind_and_break_a_promise() {
    let world = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_bulk.toml"
    ))
    .unwrap();
    let mut kernel = Kernel::compile(world).unwrap();
    kernel.set_agreement_honor(1, false, false).unwrap();
    let state = kernel.export();
    assert!(state.agreements.as_ref().unwrap().bulk_debtors.is_empty());
    let save = serde_json::to_string(&state).unwrap();
    let mut loaded = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
    for _ in 0..4 {
        kernel.step().unwrap();
        loaded.step().unwrap();
    }
    assert_eq!(kernel.export(), loaded.export());
    let state = kernel.export();
    assert!(
        state
            .agreements
            .unwrap()
            .contracts
            .iter()
            .all(|row| row.status == AgreementStatus::Defaulted)
    );
    assert_eq!(state.goods_cargo.unwrap().holdings[0].items[0].quantity, 5);
}

#[test]
fn witnessed_theft_can_make_a_debtor_withhold_one_promise_but_honor_another() {
    use game_parallel::{OpinionRules, Opinions};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_bulk.toml"
    ))
    .unwrap();
    world.agreements.as_mut().unwrap().policies[0].minimum_honor_trust = Some(0);
    world.property = World::from_toml(include_str!("../../../../content/parallel/property.toml"))
        .unwrap()
        .property;
    world.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 2,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 3,
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
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(world.clone()).unwrap();
                engine.steal_goods(2, 1, 1).unwrap();
                for _ in 0..4 {
                    engine.step().unwrap();
                }
                let result = engine.export();
                let contracts = &result.agreements.as_ref().unwrap().contracts;
                assert_eq!(contracts[0].status, AgreementStatus::Defaulted);
                assert_eq!(contracts[1].status, AgreementStatus::Fulfilled);
                assert_eq!(
                    result
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .find(|row| row.observer == 1 && row.target == 2)
                        .unwrap()
                        .trust,
                    -1
                );
                assert_eq!(
                    result
                        .goods_cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .find(|row| row.person == 1)
                        .unwrap()
                        .items[0]
                        .quantity,
                    3
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn witnessed_unpermitted_work_can_change_a_later_promise_decision() {
    use game_parallel::{OpinionRules, Opinions, StartWork};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_bulk.toml"
    ))
    .unwrap();
    world.agreements.as_mut().unwrap().policies[0].minimum_honor_trust = Some(0);
    world.property = World::from_toml(include_str!("../../../../content/parallel/property.toml"))
        .unwrap()
        .property;
    world.opinions = Some(Opinions {
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 2,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 3,
            encounter_gain: 0,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        },
        sensitivities: vec![],
        records: vec![],
        events: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                kernel
                    .work_without_permission(StartWork {
                        id: 100,
                        worker: 2,
                        region: 1,
                        recipe: 1,
                        units: 1,
                    })
                    .unwrap();
                let witnessed = kernel.export();
                assert_eq!(witnessed.opinions.as_ref().unwrap().index().trust(1, 2), -1);
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&witnessed).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..4 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                let contracts = &result.agreements.as_ref().unwrap().contracts;
                assert_eq!(contracts[0].status, AgreementStatus::Defaulted);
                assert_eq!(contracts[1].status, AgreementStatus::Fulfilled);
                assert_eq!(
                    result
                        .goods_cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .find(|row| row.person == 1)
                        .unwrap()
                        .items[0]
                        .quantity,
                    3
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn fear_of_actual_harm_can_motivate_fulfillment_despite_distrust() {
    use game_parallel::{
        CombatEncounter, CombatFighter, CombatOrder, Conflict, Opinion, OpinionRules, Opinions,
    };
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_bulk.toml"
    ))
    .unwrap();
    let policy = &mut world.agreements.as_mut().unwrap().policies[0];
    policy.minimum_honor_trust = Some(0);
    policy.fear_honor_threshold = Some(4);
    world.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            fear_per_damage: 1,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        },
        records: vec![Opinion {
            observer: 1,
            target: 2,
            trust: -1,
            fear: 0,
            obligation: 0,
            affinity: 0,
        }],
        events: vec![],
    });
    world.conflict = Some(Conflict {
        damage: 4,
        next_id: 2,
        events: vec![],
        encounters: vec![CombatEncounter {
            id: 1,
            region: 1,
            fighters: vec![
                CombatFighter {
                    person: 1,
                    side: 1,
                    order: CombatOrder::Retreat,
                },
                CombatFighter {
                    person: 2,
                    side: 2,
                    order: CombatOrder::Attack { target: 1 },
                },
            ],
        }],
    });
    let mut engine = Kernel::compile(world).unwrap();
    engine.step().unwrap();
    let fear = engine.export().opinions.unwrap().records[0].clone();
    assert_eq!((fear.trust, fear.fear), (-1, 4));
    engine.step().unwrap();
    let result = engine.export();
    assert!(
        result
            .agreements
            .unwrap()
            .contracts
            .iter()
            .all(|contract| contract.status == AgreementStatus::Fulfilled)
    );
    assert_eq!(result.opinions.unwrap().records[0].trust, -1);
}

#[test]
fn bulk_fulfillment_uses_each_creditors_actual_fear_without_general_willingness() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = fixture(2, 2);
                let policy = &mut state.agreements.as_mut().unwrap().policies[0];
                policy.honor = false;
                policy.fear_honor_threshold = Some(2);
                let mut opinions =
                    World::from_toml(include_str!("../../../../content/parallel/campaign.toml"))
                        .unwrap()
                        .opinions
                        .unwrap();
                opinions.rules.fear_decay_per_tick = 0;
                opinions.records = vec![
                    game_parallel::Opinion {
                        observer: 1,
                        target: 2,
                        trust: -1,
                        fear: 2,
                        obligation: 0,
                        affinity: 0,
                    },
                    game_parallel::Opinion {
                        observer: 1,
                        target: 3,
                        trust: -1,
                        fear: 0,
                        obligation: 0,
                        affinity: 0,
                    },
                ];
                state.opinions = Some(opinions);
                let mut kernel = Kernel::compile(state).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let contracts = &result.agreements.as_ref().unwrap().contracts;
                assert_eq!(contracts[0].status, AgreementStatus::Fulfilled);
                assert_eq!(contracts[1].status, AgreementStatus::Active);
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
                    1
                );
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 2), -1);
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_can_withhold_a_later_promise_after_fear_subsides() {
    use game_parallel::{Opinion, OpinionRules, Opinions};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/agreements_bulk.toml"
    ))
    .unwrap();
    let agreements = world.agreements.as_mut().unwrap();
    agreements.contracts[1].creditor = 2;
    agreements.policies[0].minimum_honor_trust = Some(0);
    agreements.policies[0].fear_honor_threshold = Some(4);
    world.opinions = Some(Opinions {
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            fear_per_damage: 0,
            fear_decay_per_tick: 2,
            gratitude_gain: 0,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        },
        sensitivities: vec![],
        events: vec![],
        records: vec![Opinion {
            observer: 1,
            target: 2,
            trust: -4,
            fear: 4,
            obligation: 0,
            affinity: 0,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
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
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..3 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                assert_eq!(
                    result.agreements.as_ref().unwrap().contracts[1].status,
                    AgreementStatus::Defaulted
                );
                assert_eq!(
                    result
                        .goods_cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .find(|row| row.person == 1)
                        .unwrap()
                        .items[0]
                        .quantity,
                    3
                );
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 2), -4);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn concentrated_bulk_delivery_is_atomic_conserved_and_worker_invariant() {
    let original = fixture(5000, 2500);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(original.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let agreements = result.agreements.as_ref().unwrap();
                assert_eq!(
                    agreements
                        .contracts
                        .iter()
                        .filter(|row| row.status == AgreementStatus::Fulfilled)
                        .count(),
                    2500
                );
                assert!(
                    agreements.contracts[..2500]
                        .iter()
                        .all(|row| row.status == AgreementStatus::Fulfilled)
                );
                assert_eq!(
                    result
                        .goods_cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .flat_map(|row| &row.items)
                        .map(|item| item.quantity)
                        .sum::<u64>(),
                    2500
                );
                let encoded = serde_json::to_string(&result).unwrap();
                let mut reloaded = Kernel::compile(World::from_json(&encoded).unwrap()).unwrap();
                kernel.step().unwrap();
                reloaded.step().unwrap();
                assert_eq!(kernel.export(), reloaded.export());
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn bulk_transfer_rolls_back_when_another_owner_fails() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut content = fixture(2, 2);
    world.production = content.production.take();
    world.goods_cargo = content.goods_cargo.take();
    world.agreements = content.agreements.take();
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
