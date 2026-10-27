mod support;
use game_cargo::{Cargo, Consume, FOOD, TIMBER};
use game_cargo_planning::{StockDemand, StockIntent, plan_stock};
use game_drafting::arbitration::Decision;

#[test]
fn provisional_stock_planning_is_fair_deterministic_and_does_not_consume() {
    let f = support::setup();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let demand = StockDemand {
        inventory: f.store,
        good: FOOD,
        quantity: 700,
    };
    let intents = vec![
        StockIntent {
            id: 2,
            actor: f.other,
            participants: vec![],
            priority: 1,
            demands: vec![demand.clone()],
        },
        StockIntent {
            id: 1,
            actor: f.ruler,
            participants: vec![],
            priority: 1,
            demands: vec![demand],
        },
    ];
    let before = f.host.save().unwrap();
    let cargo = f.host.get::<Cargo>().unwrap();
    let first = plan_stock(cargo, &pool, 0, &intents).unwrap();
    let second = plan_stock(cargo, &pool, 1, &intents).unwrap();
    assert_eq!(first.resolution.decisions[&1], Decision::Granted);
    assert_eq!(second.resolution.decisions[&2], Decision::Granted);
    assert!(first.stock_unchanged(cargo));
    assert_eq!(before, f.host.save().unwrap());
    assert_eq!(support::food(&f.host, f.store), 1000);
}

#[test]
fn provisional_plans_detect_changed_stock_and_keep_multi_good_claims_atomic() {
    let mut f = support::setup();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let intents = vec![
        StockIntent {
            id: 1,
            actor: f.ruler,
            participants: vec![],
            priority: 0,
            demands: vec![
                StockDemand {
                    inventory: f.store,
                    good: FOOD,
                    quantity: 1,
                },
                StockDemand {
                    inventory: f.store,
                    good: TIMBER,
                    quantity: 101,
                },
            ],
        },
        StockIntent {
            id: 2,
            actor: f.other,
            participants: vec![],
            priority: 1,
            demands: vec![StockDemand {
                inventory: f.store,
                good: FOOD,
                quantity: 1000,
            }],
        },
    ];
    let plan = plan_stock(f.host.get::<Cargo>().unwrap(), &pool, 0, &intents).unwrap();
    assert!(matches!(
        plan.resolution.decisions[&1],
        Decision::Insufficient(_)
    ));
    assert_eq!(plan.resolution.decisions[&2], Decision::Granted);
    f.host
        .dispatch(Consume {
            inventory: f.store,
            good: FOOD,
            quantity: 1,
        })
        .unwrap();
    assert!(!plan.stock_unchanged(f.host.get::<Cargo>().unwrap()));
}

#[test]
fn joint_participant_conflicts_with_their_own_selected_action() {
    let f = support::setup();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let intents = vec![
        StockIntent {
            id: 1,
            actor: f.ruler,
            participants: vec![f.other],
            priority: 0,
            demands: vec![StockDemand {
                inventory: f.store,
                good: FOOD,
                quantity: 1,
            }],
        },
        StockIntent {
            id: 2,
            actor: f.other,
            participants: vec![],
            priority: 1,
            demands: vec![],
        },
    ];
    let before = f.host.save().unwrap();
    let plan = plan_stock(f.host.get::<Cargo>().unwrap(), &pool, 0, &intents).unwrap();
    assert_eq!(plan.resolution.decisions[&1], Decision::Granted);
    assert_eq!(
        plan.resolution.decisions[&2],
        Decision::ParticipantAlreadyGranted(f.other.0)
    );
    assert_eq!(before, f.host.save().unwrap());
}
