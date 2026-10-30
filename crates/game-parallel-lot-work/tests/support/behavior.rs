use super::*;
use game_parallel_lot_model::{Lot, State};
use game_parallel_lots::Store;
use game_parallel_production_model::{FoodProfile, Good};
use std::collections::BTreeMap;

#[test]
fn physical_job_reserves_then_produces_and_delivers_without_refreshing_output() {
    let catalog = Catalog::compile(
        &[
            Good {
                id: 1,
                name: "grain".into(),
                food: None,
            },
            Good {
                id: 2,
                name: "bread".into(),
                food: Some(FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: BTreeMap::new(),
                    health_effect: 0,
                    shelf_days: Some(2),
                }),
            },
        ],
        2,
    )
    .unwrap();
    let recipe = Recipe {
        id: 1,
        name: "bake".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 2,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    };
    let stock = |good, quantity| {
        Container::new(
            Place::Stock { region: 1, good },
            10,
            if quantity == 0 {
                vec![]
            } else {
                vec![Lot {
                    good,
                    quantity,
                    expires_at: None,
                }]
            },
        )
        .unwrap()
    };
    let mut store = Store::compile(State {
        tick: 0,
        revision: 0,
        containers: vec![stock(1, 4), stock(2, 0)],
    })
    .unwrap();
    let plan = start(
        &[Start {
            id: 5,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        }],
        &[recipe],
    )
    .unwrap();
    store.add_empty(plan.containers).unwrap();
    let draft = store.prepare_transactions(1, &plan.transactions).unwrap();
    assert!(draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    let mut jobs = plan.jobs;
    jobs[0].remaining = 0;
    let actions = finish(
        &[(&jobs[0], store.container(Place::JobInput(5)).unwrap())],
        &catalog,
        2,
    )
    .unwrap();
    let draft = store
        .prepare_transactions(1, &actions.transactions)
        .unwrap();
    assert!(draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    jobs[0].phase = JobPhase::Waiting;
    jobs[0].reserved.clear();
    let actions = deliver(
        &[(&jobs[0], store.container(Place::JobOutput(5)).unwrap())],
        3,
    )
    .unwrap();
    let draft = store.prepare_transactions(1, &actions.returns).unwrap();
    assert!(draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    let state = store.export();
    let bread = state
        .containers
        .iter()
        .find(|row| row.place == Place::Stock { region: 1, good: 2 })
        .unwrap();
    assert_eq!(bread.lots[0].expires_at, Some(6));
    assert_eq!(bread.used, 1);
    store
        .remove_empty(vec![Place::JobInput(5), Place::JobOutput(5)])
        .unwrap();
}

#[test]
fn blocked_waiting_output_spoils_in_its_job_and_cannot_be_recreated_by_delivery() {
    let catalog = Catalog::compile(
        &[
            Good {
                id: 1,
                name: "wood".into(),
                food: None,
            },
            Good {
                id: 2,
                name: "food".into(),
                food: Some(FoodProfile {
                    portion: 1,
                    hunger_relief: 1,
                    nutrients: BTreeMap::new(),
                    health_effect: 0,
                    shelf_days: Some(2),
                }),
            },
        ],
        1,
    )
    .unwrap();
    let recipe = Recipe {
        id: 1,
        name: "cook".into(),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    };
    let mut store = Store::compile(State {
        tick: 0,
        revision: 0,
        containers: vec![
            Container::new(
                Place::Stock { region: 1, good: 1 },
                1,
                vec![Lot {
                    good: 1,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap(),
            Container::new(
                Place::Stock { region: 1, good: 2 },
                1,
                vec![Lot {
                    good: 2,
                    quantity: 1,
                    expires_at: None,
                }],
            )
            .unwrap(),
        ],
    })
    .unwrap();
    let plan = start(
        &[Start {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }],
        &[recipe],
    )
    .unwrap();
    store.add_empty(plan.containers).unwrap();
    assert!(
        finish(
            &[(&plan.jobs[0], store.container(Place::JobInput(1)).unwrap())],
            &catalog,
            2
        )
        .is_err()
    );
    store
        .commit(store.prepare_transactions(1, &plan.transactions).unwrap())
        .unwrap();
    let mut jobs = plan.jobs;
    jobs[0].remaining = 0;
    let completed = finish(
        &[(&jobs[0], store.container(Place::JobInput(1)).unwrap())],
        &catalog,
        2,
    )
    .unwrap();
    store
        .commit(
            store
                .prepare_transactions(1, &completed.transactions)
                .unwrap(),
        )
        .unwrap();
    jobs[0].phase = JobPhase::Waiting;
    jobs[0].reserved.clear();
    let delivery = deliver(
        &[(&jobs[0], store.container(Place::JobOutput(1)).unwrap())],
        3,
    )
    .unwrap();
    let draft = store.prepare_transactions(1, &delivery.returns).unwrap();
    assert!(!draft.transactions()[0].accepted);
    store.commit(draft).unwrap();
    let delivery = deliver(
        &[(&jobs[0], store.container(Place::JobOutput(1)).unwrap())],
        4,
    )
    .unwrap();
    assert_eq!(delivery.empty, vec![1]);
    let draft = store.prepare_transactions(1, &delivery.returns).unwrap();
    let (_, spoiled) = store.commit(draft).unwrap();
    assert_eq!(
        (spoiled[0].place, spoiled[0].quantity, spoiled[0].expires_at),
        (Place::JobOutput(1), 1, 4)
    );
    assert_eq!(
        store
            .export()
            .containers
            .iter()
            .find(|row| row.place == Place::JobOutput(1))
            .unwrap()
            .used,
        0
    );
}

#[test]
fn interruption_returns_surviving_input_without_refreshing_it() {
    let source = Place::Stock { region: 1, good: 1 };
    let input = Place::JobInput(5);
    let mut store = Store::compile(State {
        tick: 1,
        revision: 0,
        containers: vec![
            Container::new(source, 10, vec![]).unwrap(),
            Container::new(
                input,
                10,
                vec![
                    Lot {
                        good: 1,
                        quantity: 2,
                        expires_at: Some(2),
                    },
                    Lot {
                        good: 1,
                        quantity: 3,
                        expires_at: Some(8),
                    },
                ],
            )
            .unwrap(),
        ],
    })
    .unwrap();
    let job = Job {
        id: 5,
        worker: 1,
        region: 1,
        recipe: 1,
        remaining: 2,
        phase: JobPhase::Working,
        reserved: vec![Amount {
            good: 1,
            quantity: 5,
        }]
        .into(),
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }]
        .into(),
    };
    let plan = cancel(&[(&job, store.container(input).unwrap())], 2).unwrap();
    assert!(plan.empty.is_empty());
    let draft = store.prepare_transactions(1, &plan.returns).unwrap();
    assert!(draft.transactions()[0].accepted);
    let (_, spoiled) = store.commit(draft).unwrap();
    assert_eq!(spoiled[0].quantity, 2);
    assert_eq!(
        store.container(source).unwrap().lots,
        vec![Lot {
            good: 1,
            quantity: 3,
            expires_at: Some(8)
        }]
    );
}

#[test]
fn delivery_moves_only_unexpired_output_and_reports_the_loss() {
    let stock = Place::Stock { region: 1, good: 2 };
    let output = Place::JobOutput(1);
    let mut store = Store::compile(State {
        tick: 1,
        revision: 0,
        containers: vec![
            Container::new(stock, 10, vec![]).unwrap(),
            Container::new(
                output,
                10,
                vec![
                    Lot {
                        good: 2,
                        quantity: 1,
                        expires_at: Some(2),
                    },
                    Lot {
                        good: 2,
                        quantity: 2,
                        expires_at: Some(5),
                    },
                ],
            )
            .unwrap(),
        ],
    })
    .unwrap();
    let job = Job {
        id: 1,
        worker: 1,
        region: 1,
        recipe: 1,
        remaining: 0,
        phase: JobPhase::Waiting,
        reserved: vec![].into(),
        outputs: vec![Amount {
            good: 2,
            quantity: 3,
        }]
        .into(),
    };
    let plan = deliver(&[(&job, store.container(output).unwrap())], 2).unwrap();
    assert!(plan.empty.is_empty());
    let mut draft = store.prepare_transactions(1, &plan.returns).unwrap();
    assert!(draft.transactions()[0].accepted);
    store.retire_in(&mut draft, vec![output]).unwrap();
    let (_, spoiled) = store.commit(draft).unwrap();
    assert_eq!(spoiled[0].quantity, 1);
    assert_eq!(
        store.container(stock).unwrap().lots,
        vec![Lot {
            good: 2,
            quantity: 2,
            expires_at: Some(5)
        }]
    );
    assert!(store.container(output).is_none());
}

#[test]
fn missing_or_expiring_inputs_interrupt_work_and_do_not_make_output() {
    let catalog = Catalog::compile(
        &[
            Good {
                id: 1,
                name: "ingredient".into(),
                food: None,
            },
            Good {
                id: 2,
                name: "product".into(),
                food: None,
            },
        ],
        1,
    )
    .unwrap();
    let input = Container::new(
        Place::JobInput(1),
        5,
        vec![
            Lot {
                good: 1,
                quantity: 1,
                expires_at: Some(2),
            },
            Lot {
                good: 1,
                quantity: 1,
                expires_at: Some(4),
            },
        ],
    )
    .unwrap();
    let job = Job {
        id: 1,
        worker: 1,
        region: 1,
        recipe: 1,
        remaining: 0,
        phase: JobPhase::Working,
        reserved: vec![Amount {
            good: 1,
            quantity: 2,
        }]
        .into(),
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }]
        .into(),
    };
    let plan = finish(&[(&job, &input)], &catalog, 2).unwrap();
    assert_eq!(plan.interrupted, vec![1]);
    assert!(plan.transactions.is_empty());
    let returning = cancel(&[(&job, &input)], 2).unwrap();
    assert_eq!(returning.returns[0].movements[0].quantity, 1);
}
