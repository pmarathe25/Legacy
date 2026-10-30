use game_parallel::{Amount, GoodsHolding, TradeInfluence, TradeProposal, World};
use game_parallel_fused_physical::Kernel;
fn fear_opinions() -> game_parallel::Opinions {
    game_parallel::Opinions {
        sensitivities: Vec::new(),
        rules: game_parallel::OpinionRules {
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
        records: vec![],
        events: vec![],
    }
}

fn fixture(influence: Option<TradeInfluence>) -> Kernel {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    world
        .goods_cargo
        .as_mut()
        .unwrap()
        .holdings
        .push(GoodsHolding {
            person: 2,
            items: vec![Amount {
                good: 1,
                quantity: 2,
            }]
            .into(),
        });
    if let Some(influence) = influence {
        world.market.as_mut().unwrap().policies[0]
            .influences
            .push(influence);
    }
    let mut engine = Kernel::compile(world).unwrap();
    engine
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 2,
            give: vec![],
            receive: vec![Amount {
                good: 1,
                quantity: 2,
            }],
        })
        .unwrap();
    engine
}

#[test]
fn helping_can_follow_attachment_obligation_or_fear_without_an_agreement() {
    let mut indifferent = fixture(None);
    indifferent.step().unwrap();
    assert_eq!(indifferent.export().market.unwrap().offers.len(), 1);
    for motive in 0..3 {
        let mut influence = TradeInfluence {
            person: 1,
            attachment: 0,
            obligation: 0,
            fear: 0,
        };
        match motive {
            0 => influence.attachment = 4,
            1 => influence.obligation = 4,
            _ => influence.fear = 4,
        }
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut kernel = fixture(Some(influence.clone()));
                    kernel.step().unwrap();
                    let result = kernel.export();
                    assert!(result.agreements.is_none());
                    assert!(result.market.as_ref().unwrap().offers.is_empty());
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
                        2
                    );
                    assert_eq!(
                        result.market.as_ref().unwrap().policies[0].influences[0],
                        influence
                    );
                    let mut loaded = Kernel::compile(
                        World::from_json(&serde_json::to_string(&result).unwrap()).unwrap(),
                    )
                    .unwrap();
                    kernel.step().unwrap();
                    loaded.step().unwrap();
                    assert_eq!(kernel.export(), loaded.export());
                    result
                })
        };
        assert_eq!(run(1), run(4));
    }
}

#[test]
fn authored_pressure_does_not_imply_trust_or_create_missing_goods() {
    use game_parallel::{Opinion, OpinionRules, Opinions};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    world.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 1,
            default_loss: 1,
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
        records: vec![Opinion {
            observer: 2,
            target: 1,
            trust: -4,
            fear: 0,
            obligation: 0,
            affinity: 0,
        }],
        events: vec![],
    });
    let mut present = Kernel::compile(world.clone()).unwrap();
    present.step().unwrap();
    let after = present.export();
    assert_eq!(after.opinions.unwrap().records[0].trust, -4);
    let market = after.market.unwrap();
    assert!(market.offers.is_empty());
    assert_eq!(
        market
            .events
            .last()
            .unwrap()
            .influence
            .as_ref()
            .unwrap()
            .fear,
        4
    );
    world.goods_cargo.as_mut().unwrap().holdings.clear();
    let mut empty = Kernel::compile(world).unwrap();
    empty.step().unwrap();
    let after = empty.export();
    assert_eq!(after.market.unwrap().offers.len(), 1);
    assert!(after.goods_cargo.unwrap().holdings.is_empty());
}

#[test]
fn witnessed_help_can_create_personal_obligation_and_future_reciprocity() {
    use game_parallel::{TradePolicy, TradeValue};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    let mut opinions = fear_opinions();
    opinions.rules.gratitude_gain = 4;
    world.opinions = Some(opinions);
    world.market.as_mut().unwrap().policies.push(TradePolicy {
        person: 1,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        values: vec![TradeValue { good: 1, value: 2 }],
        influences: vec![],
    });
    let mut engine = Kernel::compile(world).unwrap();
    engine.step().unwrap();
    let first = engine.export();
    let opinion = first
        .opinions
        .as_ref()
        .unwrap()
        .records
        .iter()
        .find(|row| row.observer == 1 && row.target == 2)
        .unwrap();
    assert_eq!(opinion.obligation, 4);
    let mut engine =
        Kernel::compile(World::from_json(&serde_json::to_string(&first).unwrap()).unwrap())
            .unwrap();
    engine
        .propose_trade(TradeProposal {
            proposer: 2,
            recipient: 1,
            give: vec![],
            receive: vec![Amount {
                good: 1,
                quantity: 2,
            }],
        })
        .unwrap();
    engine.step().unwrap();
    let after = engine.export();
    assert!(after.market.as_ref().unwrap().offers.is_empty());
    assert_eq!(
        after
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
    assert_eq!(
        after
            .market
            .unwrap()
            .events
            .last()
            .unwrap()
            .influence
            .as_ref()
            .unwrap()
            .obligation,
        4
    );
}

#[test]
fn failed_physical_exchange_does_not_create_an_obligation() {
    use game_parallel::{TradeOutcome, TradePolicy, TradeValue};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    let mut opinions = fear_opinions();
    opinions.rules.gratitude_gain = 4;
    world.opinions = Some(opinions);
    world.goods_cargo.as_mut().unwrap().holdings.clear();
    world.market.as_mut().unwrap().policies.push(TradePolicy {
        person: 1,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        values: vec![TradeValue { good: 1, value: 2 }],
        influences: vec![],
    });
    let mut engine = Kernel::compile(world).unwrap();
    let before = engine.export();
    assert!(engine.decide_trade(1, 2, TradeOutcome::Accepted).is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn later_tick_failure_rolls_back_automatic_gifts_and_memories() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut content = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    world.production = content.production.take();
    world.goods_cargo = content.goods_cargo.take();
    world.market = content.market.take();
    let mut opinions = fear_opinions();
    opinions.rules.gratitude_gain = 4;
    world.opinions = Some(opinions);
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut engine = Kernel::compile(world).unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn a_direct_gift_creates_a_personal_memory_without_a_market() {
    use game_parallel::{GoodsExchange, OpinionSensitivity};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    world.market = None;
    let mut opinions = fear_opinions();
    opinions.rules.gratitude_gain = 0;
    opinions.sensitivities.push(OpinionSensitivity {
        person: 1,
        gratitude_gain: Some(3),
        obligation_relief: None,
        fear_per_damage: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    });
    world.opinions = Some(opinions);
    let exchange = GoodsExchange {
        a: 2,
        b: 1,
        offer_a: vec![Amount {
            good: 1,
            quantity: 1,
        }]
        .into(),
        offer_b: Default::default(),
    };
    let mut reference = world.clone();
    let mut engine = Kernel::compile(world).unwrap();
    reference
        .exchange_goods(std::slice::from_ref(&exchange))
        .unwrap();
    engine.exchange_goods(&[exchange]).unwrap();
    let result = engine.export();
    assert_eq!(result, reference);
    let memory = &result.opinions.unwrap().records[0];
    assert_eq!(
        (memory.observer, memory.target, memory.obligation),
        (1, 2, 3)
    );
    assert_eq!(
        result
            .goods_cargo
            .unwrap()
            .holdings
            .iter()
            .find(|row| row.person == 1)
            .unwrap()
            .items[0]
            .quantity,
        1
    );
}

#[test]
fn real_reciprocal_gifts_can_relieve_felt_obligation_without_a_contract() {
    use game_parallel::GoodsExchange;
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    world.market = None;
    let mut opinions = fear_opinions();
    opinions.rules.gratitude_gain = 3;
    opinions.rules.obligation_relief = 2;
    world.opinions = Some(opinions);
    let gift = |a, b| GoodsExchange {
        a,
        b,
        offer_a: vec![Amount {
            good: 1,
            quantity: 1,
        }]
        .into(),
        offer_b: Default::default(),
    };
    let mut engine = Kernel::compile(world).unwrap();
    engine.exchange_goods(&[gift(2, 1)]).unwrap();
    let first = engine.export();
    assert_eq!(first.opinions.as_ref().unwrap().index().obligation(1, 2), 3);
    engine = Kernel::compile(World::from_json(&serde_json::to_string(&first).unwrap()).unwrap())
        .unwrap();
    engine.exchange_goods(&[gift(1, 2)]).unwrap();
    let result = engine.export();
    let feelings = result.opinions.as_ref().unwrap().index();
    assert_eq!(feelings.obligation(1, 2), 1);
    assert_eq!(feelings.obligation(2, 1), 3);
    assert!(result.agreements.is_none());
    assert_eq!(
        result
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
fn an_autonomous_person_can_request_help_and_another_can_comply_from_obligation() {
    use game_parallel::{TradePolicy, TradeValue};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    let market = world.market.as_mut().unwrap();
    market.offers.clear();
    market.next_id = 1;
    market.policies[0].influences[0].fear = 0;
    market.policies[0].influences[0].obligation = 2;
    market.policies.push(TradePolicy {
        person: 1,
        enabled: true,
        propose: false,
        request_help: true,
        min_gain: 0,
        help_value: 0,
        values: vec![TradeValue { good: 1, value: 2 }],
        influences: vec![],
    });
    let mut engine = Kernel::compile(world).unwrap();
    engine.step().unwrap();
    let offer = &engine.export().market.unwrap().offers[0];
    assert_eq!((offer.proposer, offer.recipient), (1, 2));
    assert!(offer.give.is_empty());
    assert_eq!(offer.receive[0].quantity, 1);
    engine.step().unwrap();
    let result = engine.export();
    assert!(result.agreements.is_none());
    assert_eq!(
        result
            .goods_cargo
            .unwrap()
            .holdings
            .iter()
            .find(|row| row.person == 1)
            .unwrap()
            .items[0]
            .quantity,
        1
    );
}

#[test]
fn concentrated_empty_handed_people_are_matched_with_holders_in_parallel() {
    use game_parallel::{Person, TradePolicy, TradeValue};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    let count = 5000u64;
    world.rules.food_capacity = 25_000;
    world.regions[0].food = 22_000;
    world.regions[0].people = (1..=count * 2)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.goods_cargo.as_mut().unwrap().holdings = (count + 1..=count * 2)
        .map(|person| GoodsHolding {
            person,
            items: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let market = world.market.as_mut().unwrap();
    market.policies = (1..=count * 2)
        .map(|person| TradePolicy {
            person,
            enabled: true,
            propose: false,
            request_help: person <= count,
            min_gain: 0,
            help_value: 0,
            values: vec![TradeValue { good: 1, value: 1 }],
            influences: if person > count {
                vec![TradeInfluence {
                    person: person - count,
                    attachment: 0,
                    obligation: 1,
                    fear: 0,
                }]
            } else {
                vec![]
            },
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(world.clone()).unwrap();
                engine.step().unwrap();
                let state = engine.export();
                assert_eq!(state.market.as_ref().unwrap().offers.len(), count as usize);
                engine.step().unwrap();
                let state = engine.export();
                assert!(state.market.as_ref().unwrap().offers.is_empty());
                assert_eq!(
                    state.goods_cargo.as_ref().unwrap().holdings.len(),
                    count as usize
                );
                assert!(
                    state
                        .goods_cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .all(|row| row.person <= count && row.items[0].quantity == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn thousands_of_personal_responses_and_learned_obligations_are_worker_invariant() {
    use game_parallel::{Person, TradeOffer, TradePolicy, TradeValue};
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    let mut opinions = fear_opinions();
    opinions.rules.gratitude_gain = 1;
    world.opinions = Some(opinions);
    let count = 5000u64;
    world.rules.food_capacity = 25000;
    world.regions[0].food = 22000;
    world.regions[0].people = (1..=count * 2)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    world.regions[1].people.clear();
    world.goods_cargo.as_mut().unwrap().holdings = (1..=count)
        .map(|id| GoodsHolding {
            person: id * 2,
            items: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let market = world.market.as_mut().unwrap();
    market.next_id = count + 1;
    market.offers = (1..=count)
        .map(|id| TradeOffer {
            id,
            proposer: id * 2 - 1,
            recipient: id * 2,
            give: Default::default(),
            receive: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
            proposed_at: 0,
        })
        .collect();
    market.policies = (1..=count * 2)
        .map(|person| TradePolicy {
            person,
            enabled: person % 2 == 0,
            propose: false,
            request_help: false,
            min_gain: 0,
            help_value: 0,
            values: vec![TradeValue { good: 1, value: 1 }],
            influences: if person % 2 == 0 {
                vec![TradeInfluence {
                    person: person - 1,
                    attachment: 0,
                    obligation: 0,
                    fear: 1,
                }]
            } else {
                vec![]
            },
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(world.clone()).unwrap();
                engine.step().unwrap();
                let result = engine.export();
                let market = result.market.as_ref().unwrap();
                assert!(market.offers.is_empty());
                assert_eq!(market.events.len(), count as usize);
                assert!(
                    result
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .filter(|row| row.observer % 2 == 1)
                        .count()
                        == count as usize
                );
                assert!(
                    result
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .all(|row| row.obligation == 1)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn suffered_harm_can_create_fear_and_subsequent_compliance_without_affecting_bystanders() {
    use game_parallel::{
        CombatEncounter, CombatFighter, CombatOrder, Conflict, TradePolicy, TradeValue,
    };
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    world.market.as_mut().unwrap().policies[0]
        .influences
        .clear();
    world.opinions = Some(fear_opinions());
    world.market.as_mut().unwrap().policies.push(TradePolicy {
        person: 3,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        values: vec![TradeValue { good: 1, value: 2 }],
        influences: vec![],
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
                    order: CombatOrder::Attack { target: 2 },
                },
                CombatFighter {
                    person: 2,
                    side: 2,
                    order: CombatOrder::Retreat,
                },
            ],
        }],
    });
    let mut engine = Kernel::compile(world).unwrap();
    engine.step().unwrap();
    let first = engine.export();
    let market = first.market.as_ref().unwrap();
    assert_eq!(
        first
            .opinions
            .as_ref()
            .unwrap()
            .records
            .iter()
            .find(|row| row.observer == 2 && row.target == 1)
            .unwrap()
            .fear,
        4
    );
    assert!(
        !first
            .opinions
            .as_ref()
            .unwrap()
            .records
            .iter()
            .any(|row| row.observer == 3)
    );
    assert!(market.policies[1].influences.is_empty());
    assert_eq!(market.offers.len(), 1);
    engine.step().unwrap();
    let result = engine.export();
    let market = result.market.unwrap();
    assert!(market.offers.is_empty());
    assert_eq!(
        market
            .events
            .last()
            .unwrap()
            .influence
            .as_ref()
            .unwrap()
            .fear,
        4
    );
    assert_eq!(
        result
            .goods_cargo
            .unwrap()
            .holdings
            .iter()
            .find(|row| row.person == 1)
            .unwrap()
            .items[0]
            .quantity,
        2
    );
}

#[test]
fn a_failed_tick_does_not_publish_harm_or_learned_fear() {
    use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, Conflict};
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut content = World::from_toml(include_str!(
        "../../../../content/parallel/cooperation.toml"
    ))
    .unwrap();
    content.market.as_mut().unwrap().policies[0]
        .influences
        .clear();
    world.opinions = Some(fear_opinions());
    world.production = content.production;
    world.goods_cargo = content.goods_cargo;
    world.market = content.market;
    world.families.as_mut().unwrap().members[0].age = u64::MAX;
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
                    order: CombatOrder::Attack { target: 2 },
                },
                CombatFighter {
                    person: 2,
                    side: 2,
                    order: CombatOrder::Retreat,
                },
            ],
        }],
    });
    let mut engine = Kernel::compile(world).unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn harm_changes_directed_fear_even_without_trade() {
    use game_parallel::{
        CombatEncounter, CombatFighter, CombatOrder, Conflict, OpinionSensitivity,
    };
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/auto_market.toml"
    ))
    .unwrap();
    world.market = None;
    let mut opinions = fear_opinions();
    opinions.rules.fear_per_damage = 0;
    opinions.sensitivities.push(OpinionSensitivity {
        person: 2,
        fear_per_damage: Some(2),
        gratitude_gain: None,
        obligation_relief: None,
        theft_loss: None,
        confrontation: None,
        affinity_help: None,
        affinity_harm_per_damage: None,
        affinity_encounter: None,
        affinity_pressured_encounter: None,
        relationship_cases: vec![],
        obligation_development: None,
        promise_obligation_gain: None,
    });
    world.opinions = Some(opinions);
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
                    order: CombatOrder::Attack { target: 2 },
                },
                CombatFighter {
                    person: 2,
                    side: 2,
                    order: CombatOrder::Retreat,
                },
            ],
        }],
    });
    let mut engine = Kernel::compile(world).unwrap();
    engine.step().unwrap();
    let result = engine.export();
    let opinion = &result.opinions.unwrap().records[0];
    assert_eq!(
        (
            opinion.observer,
            opinion.target,
            opinion.trust,
            opinion.fear
        ),
        (2, 1, 0, 8)
    );
}
