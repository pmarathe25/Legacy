use super::*;
use game_parallel_autowork::{Autowork, Policy};
use game_parallel_lot_runtime::{Runtime, Snapshot};
use game_parallel_lots::{Container, Lot, State};
use game_parallel_production::Production;
use game_parallel_production_model::{Good, Recipe, Start};

#[test]
fn personal_work_decisions_use_expiry_aware_projection_and_physical_admission() {
    let goods = vec![
        Good {
            id: 1,
            name: "material".into(),
            food: None,
        },
        Good {
            id: 2,
            name: "product".into(),
            food: None,
        },
    ];
    let recipes = vec![Recipe {
        id: 1,
        name: "make".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    }];
    let stock = |good, lots| Container::new(Place::Stock { region: 1, good }, 10, lots).unwrap();
    let mut runtime = Runtime::compile(Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![
                stock(
                    1,
                    vec![
                        Lot {
                            good: 1,
                            quantity: 1,
                            expires_at: Some(1),
                        },
                        Lot {
                            good: 1,
                            quantity: 1,
                            expires_at: Some(5),
                        },
                    ],
                ),
                stock(2, vec![]),
                Container::new(Place::Person(1), 10, vec![]).unwrap(),
                Container::new(Place::Person(2), 10, vec![]).unwrap(),
            ],
        },
        goods: goods.clone(),
        recipes: recipes.clone(),
        tick_millis: 86_400_000,
        work: vec![],
    })
    .unwrap();
    let view = runtime.view(1).unwrap();
    let inventory = inventories(&view);
    assert_eq!(inventory[0].stocks[0].quantity, 1);
    let projected = Production {
        max_work_hunger: 10,
        goods,
        recipes,
        recipe_categories: vec![],
        inventories: inventory,
        jobs: vec![],
        events: vec![],
    };
    let work = Autowork {
        next_id: 1,
        policies: vec![
            Policy {
                person: 1,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            },
            Policy {
                person: 2,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            },
        ],
        requests: None,
    };
    let choices = work
        .plan_validated(
            &[(1, Some(1), 10, 0), (2, Some(1), 10, 0)],
            &projected,
            &[],
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(choices.requests.len(), 1);
    assert!(cargo(&view, 10).unwrap().holdings.is_empty());
    assert!(cargo(&view, 9).is_err());
    let draft = runtime
        .prepare_with_starts(&[], &[], &choices.requests, 1)
        .unwrap();
    assert_eq!(draft.events().started, vec![1]);
    let events = runtime.commit_advance(draft).unwrap();
    assert_eq!(events.spoiled[0].quantity, 1);
    assert_eq!(runtime.custody(Place::JobInput(1)).unwrap().used, 1);
    assert_eq!(
        runtime
            .custody(Place::Stock { region: 1, good: 1 })
            .unwrap()
            .used,
        0
    );
    let _: Start = choices.requests[0];
}

#[test]
fn fear_can_motivate_cooperation_but_only_actual_transfer_resolves_the_offer() {
    use game_parallel_lots::{Movement, Transaction};
    use game_parallel_market::{Influence, Market, Offer, Outcome, Policy as TradePolicy, Value};
    let goods = vec![Good {
        id: 1,
        name: "food".into(),
        food: None,
    }];
    let snapshot = Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![
                Container::new(Place::Person(1), 2, vec![]).unwrap(),
                Container::new(
                    Place::Person(2),
                    2,
                    vec![Lot {
                        good: 1,
                        quantity: 1,
                        expires_at: Some(5),
                    }],
                )
                .unwrap(),
            ],
        },
        goods: goods.clone(),
        recipes: vec![],
        tick_millis: 86_400_000,
        work: vec![],
    };
    let mut runtime = Runtime::compile(snapshot.clone()).unwrap();
    let mut market = Market {
        next_id: 2,
        policies: vec![TradePolicy {
            person: 2,
            enabled: true,
            propose: false,
            request_help: false,
            min_gain: 0,
            help_value: 0,
            values: vec![Value { good: 1, value: 10 }],
            influences: vec![Influence {
                person: 1,
                attachment: 0,
                obligation: 0,
                fear: 10,
            }],
        }],
        offers: vec![Offer {
            id: 1,
            proposer: 1,
            recipient: 2,
            give: vec![].into(),
            receive: vec![Amount {
                good: 1,
                quantity: 1,
            }]
            .into(),
            proposed_at: 0,
        }],
        events: vec![],
    };
    let people = [(1, Some(1), 10, 0), (2, Some(1), 10, 0)];
    let view = runtime.view(1).unwrap();
    let context = market::Context {
        market: &market,
        view: &view,
        capacity: 2,
        people: &people,
        goods: &goods,
        busy: &[],
        opinions: None,
    };
    let plan = context.plan(100).unwrap();
    assert_eq!(plan.actions().len(), 1);
    let rival = Transaction {
        id: 200,
        movements: vec![Movement {
            id: 1,
            from: Place::Person(2),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let mut lost = false;
    for seed in 1..=100 {
        let actions = [plan.actions()[0].clone(), rival.clone()];
        let draft = runtime.prepare_advance(&[], &actions, seed).unwrap();
        if !draft
            .events()
            .physical_actions
            .iter()
            .find(|row| row.id == 100)
            .unwrap()
            .accepted
        {
            assert!(plan.decisions(&market, draft.events()).unwrap().is_none());
            assert_eq!(runtime.snapshot(), snapshot);
            lost = true;
            break;
        }
    }
    assert!(lost);
    let substituted = Transaction {
        id: 100,
        movements: vec![Movement {
            id: 1,
            from: Place::Person(2),
            to: None,
            good: 1,
            quantity: 1,
        }],
        creations: vec![],
    };
    let wrong = runtime.prepare_advance(&[], &[substituted], 1).unwrap();
    assert!(plan.decisions(&market, wrong.events()).is_err());
    let draft = runtime.prepare_advance(&[], plan.actions(), 1).unwrap();
    let mut changed = market.clone();
    changed.offers[0].receive[0].quantity = 2;
    assert!(plan.decisions(&changed, draft.events()).is_err());
    assert_eq!(runtime.snapshot(), snapshot);
    let decision = plan.decisions(&market, draft.events()).unwrap().unwrap();
    runtime.commit_advance(draft).unwrap();
    decision.commit(&mut market);
    assert!(market.offers.is_empty());
    assert_eq!(market.events[0].outcome, Outcome::Accepted);
    assert_eq!(
        runtime.custody(Place::Person(1)).unwrap().lots[0].expires_at,
        Some(5)
    );
    assert_eq!(runtime.custody(Place::Person(2)).unwrap().used, 0);
}

#[test]
fn promises_do_not_compel_delivery_but_fear_can_change_a_personal_choice() {
    use game_parallel_agreements::{Agreements, Contract, Policy as PromisePolicy, Status};
    use game_parallel_opinions::{Opinion, Opinions, Rules};
    let goods = vec![Good {
        id: 1,
        name: "food".into(),
        food: None,
    }];
    let snapshot = Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![
                Container::new(Place::Person(1), 2, vec![]).unwrap(),
                Container::new(
                    Place::Person(2),
                    2,
                    vec![Lot {
                        good: 1,
                        quantity: 1,
                        expires_at: Some(5),
                    }],
                )
                .unwrap(),
            ],
        },
        goods,
        recipes: vec![],
        tick_millis: 86_400_000,
        work: vec![],
    };
    let mut runtime = Runtime::compile(snapshot.clone()).unwrap();
    let mut agreements = Agreements {
        offering: vec![],
        next_id: 2,
        policies: vec![PromisePolicy {
            person: 2,
            honor: false,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        }],
        bulk_debtors: vec![],
        contracts: vec![Contract {
            id: 1,
            debtor: 2,
            creditor: 1,
            good: 1,
            quantity: 1,
            delivered: 0,
            due: 1,
            status: Status::Active,
        }],
        events: vec![],
    };
    let people = [(1, Some(1), 10, 0), (2, Some(1), 10, 0)];
    let unwilling = agreement::Context {
        agreements: &agreements,
        opinions: None,
        people: &people,
        locations: None,
        busy: &[],
        tick: 1,
    }
    .plan(100)
    .unwrap();
    assert!(unwilling.actions().is_empty());
    assert_eq!(agreements.advance(2).contracts[0].status, Status::Defaulted);
    assert_eq!(runtime.snapshot(), snapshot);
    agreements.policies[0].fear_honor_threshold = Some(3);
    let opinions = Opinions {
        rules: Rules {
            minimum: -10,
            maximum: 10,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
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
        records: vec![Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 5,
            obligation: 0,
            affinity: 0,
        }],
        events: vec![],
    };
    let pressured = agreement::Context {
        agreements: &agreements,
        opinions: Some(&opinions),
        people: &people,
        locations: None,
        busy: &[],
        tick: 1,
    }
    .plan(100)
    .unwrap();
    assert_eq!(pressured.actions().len(), 1);
    let draft = runtime
        .prepare_advance(&[], pressured.actions(), 1)
        .unwrap();
    let settlement = pressured
        .deliveries(&agreements, draft.events())
        .unwrap()
        .unwrap();
    assert_eq!(settlement.state.contracts[0].status, Status::Fulfilled);
    runtime.commit_advance(draft).unwrap();
    agreements = settlement.state;
    assert_eq!(agreements.contracts[0].delivered, 1);
    assert_eq!(
        runtime.custody(Place::Person(1)).unwrap().lots[0].expires_at,
        Some(5)
    );
}

#[test]
fn many_individual_promises_can_deliver_to_one_busy_recipient() {
    use game_parallel_agreements::{Agreements, Contract, Policy as PromisePolicy, Status};
    let count = 1000u64;
    let goods = vec![Good {
        id: 1,
        name: "food".into(),
        food: None,
    }];
    let mut containers = vec![Container::new(Place::Person(1), count / 2, vec![]).unwrap()];
    containers.extend((2..=count + 1).map(|person| {
        Container::new(
            Place::Person(person),
            1,
            vec![Lot {
                good: 1,
                quantity: 1,
                expires_at: Some(5),
            }],
        )
        .unwrap()
    }));
    let snapshot = Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers,
        },
        goods,
        recipes: vec![],
        tick_millis: 86_400_000,
        work: vec![],
    };
    let agreements = Agreements {
        offering: vec![],
        next_id: count + 1,
        policies: (2..=count + 1)
            .map(|person| PromisePolicy {
                person,
                honor: true,
                accept: false,
                minimum_trust: None,
                minimum_honor_trust: None,
                fear_honor_threshold: None,
                obligation_honor_threshold: None,
                accept_pressure_threshold: None,
            })
            .collect(),
        bulk_debtors: vec![],
        contracts: (1..=count)
            .map(|id| Contract {
                id,
                debtor: id + 1,
                creditor: 1,
                good: 1,
                quantity: 1,
                delivered: 0,
                due: 1,
                status: Status::Active,
            })
            .collect(),
        events: vec![],
    };
    let people: Vec<_> = (1..=count + 1)
        .map(|person| (person, Some(1), 10, 0))
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut runtime = Runtime::compile(snapshot.clone()).unwrap();
                let plan = agreement::Context {
                    agreements: &agreements,
                    opinions: None,
                    people: &people,
                    locations: None,
                    busy: &[1],
                    tick: 1,
                }
                .plan(1)
                .unwrap();
                assert_eq!(plan.actions().len(), count as usize);
                let draft = runtime.prepare_advance(&[], plan.actions(), 17).unwrap();
                let settled = plan
                    .deliveries(&agreements, draft.events())
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    settled
                        .state
                        .contracts
                        .iter()
                        .filter(|row| row.status == Status::Fulfilled)
                        .count(),
                    count as usize / 2
                );
                runtime.commit_advance(draft).unwrap();
                assert_eq!(runtime.custody(Place::Person(1)).unwrap().used, count / 2);
                (runtime.snapshot(), settled.state)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore]
fn concentrated_promise_delivery_benchmark() {
    use game_parallel_agreements::{Agreements, Contract, Policy as PromisePolicy, Status};
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PROMISES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000u64);
    let mut containers = vec![Container::new(Place::Person(1), count / 2, vec![]).unwrap()];
    containers.extend((2..=count + 1).map(|person| {
        Container::new(
            Place::Person(person),
            1,
            vec![Lot {
                good: 1,
                quantity: 1,
                expires_at: Some(5),
            }],
        )
        .unwrap()
    }));
    let snapshot = Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers,
        },
        goods: vec![Good {
            id: 1,
            name: "food".into(),
            food: None,
        }],
        recipes: vec![],
        tick_millis: 86_400_000,
        work: vec![],
    };
    let agreements = Agreements {
        offering: vec![],
        next_id: count + 1,
        policies: (2..=count + 1)
            .map(|person| PromisePolicy {
                person,
                honor: true,
                accept: false,
                minimum_trust: None,
                minimum_honor_trust: None,
                fear_honor_threshold: None,
                obligation_honor_threshold: None,
                accept_pressure_threshold: None,
            })
            .collect(),
        bulk_debtors: vec![],
        contracts: (1..=count)
            .map(|id| Contract {
                id,
                debtor: id + 1,
                creditor: 1,
                good: 1,
                quantity: 1,
                delivered: 0,
                due: 1,
                status: Status::Active,
            })
            .collect(),
        events: vec![],
    };
    let people: Vec<_> = (1..=count + 1)
        .map(|person| (person, Some(1), 10, 0))
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut runtime = Runtime::compile(snapshot.clone()).unwrap();
                let started = std::time::Instant::now();
                let plan = agreement::Context {
                    agreements: &agreements,
                    opinions: None,
                    people: &people,
                    locations: None,
                    busy: &[],
                    tick: 1,
                }
                .plan(1)
                .unwrap();
                let draft = runtime.prepare_advance(&[], plan.actions(), 17).unwrap();
                let settled = plan
                    .deliveries(&agreements, draft.events())
                    .unwrap()
                    .unwrap();
                runtime.commit_advance(draft).unwrap();
                let elapsed = started.elapsed().as_secs_f64();
                assert_eq!(
                    settled
                        .state
                        .contracts
                        .iter()
                        .filter(|row| row.status == Status::Fulfilled)
                        .count(),
                    count as usize / 2
                );
                assert_eq!(runtime.custody(Place::Person(1)).unwrap().used, count / 2);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} promises={count} fulfilled={} elapsed={:.6}",
                count / 2,
                samples[1]
            );
        });
}
