use game_agreements::{Decision, Delivery, EventKind, Proposal, Status};
use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;

fn scenario(quantum: u64, offering: bool) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut source = r#"
seed = 3
tick_millis = QUANTUM
decision_period_millis = 20
stock_capacity = 100
food_relief_divisor = 20

[rules]
hunger_per_tick = 0
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 10
max_work_hunger = 20

[[regions]]
id = 1
stock = { bread = 0 }

[[people]]
id = 1
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false
goods = { bread = 1 }

[[people]]
id = 2
region = 1
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[agreements]
next_id = 1
time_unit_millis = 10
"#
    .replace("QUANTUM", &quantum.to_string());
    if offering {
        source.push_str(
            r#"
[[promise_offering]]
person = 1
recipient = 2
good = "bread"
quantity = 2
due_after = 5
retry_ticks = 10
willing = true
"#,
        );
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-elapsed-pledges-{}-{quantum}-{offering}.toml",
        std::process::id()
    ));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

fn restored(host: &CoreHost) -> CoreHost {
    let bytes = serde_json::to_vec(&host.snapshot()).unwrap();
    CoreHost::resume(serde_json::from_slice(&bytes).unwrap()).unwrap()
}

#[test]
fn elapsed_promises_expire_after_the_last_valid_delivery_and_conserve_stock() {
    let run = |workers, quantum, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scenario(quantum, false);
                let good = host
                    .snapshot()
                    .core
                    .physical
                    .goods
                    .iter()
                    .find(|good| good.name == "bread")
                    .unwrap()
                    .id;
                let promise = Proposal {
                    debtor: 1,
                    creditor: 2,
                    good,
                    quantity: 2,
                    due: 83,
                };
                let ids = host
                    .propose_agreements(&[promise.clone(), promise])
                    .unwrap();
                host.decide_agreements(&[Decision {
                    contract: ids[0],
                    actor: 2,
                    accept: true,
                }])
                .unwrap();
                for &millis in steps {
                    assert_eq!(host.advance_continuous(millis, &[]).unwrap().failure, None);
                    host = restored(&host);
                }
                let advance = host
                    .advance_continuous(83 - steps.iter().sum::<u64>(), &[])
                    .unwrap();
                assert_eq!(advance.failure, None);
                assert!(
                    advance
                        .events
                        .iter()
                        .all(|event| !matches!(event, CoreRunEvent::AgreementExpiry { .. }))
                );
                host = restored(&host);
                assert!(
                    host.decide_agreements(&[Decision {
                        contract: ids[1],
                        actor: 2,
                        accept: true
                    }])
                    .is_err()
                );
                let delivery = Delivery {
                    contract: ids[0],
                    actor: 1,
                    quantity: 1,
                };
                assert!(host.deliver_agreements(&[delivery]).unwrap()[0].accepted);
                assert!(!host.deliver_agreements(&[delivery]).unwrap()[0].accepted);
                let expired = host.advance_continuous(1, &[]).unwrap();
                assert_eq!(expired.failure, None);
                let receipts = expired
                    .events
                    .into_iter()
                    .find_map(|event| match event {
                        CoreRunEvent::AgreementExpiry { at_millis, events } => {
                            assert_eq!(at_millis, 84);
                            Some(
                                events
                                    .into_iter()
                                    .map(|event| (event.contract, event.kind, event.quantity))
                                    .collect::<Vec<_>>(),
                            )
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    receipts,
                    [
                        (ids[0], EventKind::Defaulted, 1),
                        (ids[1], EventKind::Withdrawn, 2)
                    ]
                );
                let state = host.snapshot();
                assert_eq!(
                    state.agreements.as_ref().unwrap().contracts[0].status,
                    Status::Defaulted
                );
                let amounts = state
                    .core
                    .physical
                    .custody
                    .containers
                    .iter()
                    .filter(|row| matches!(row.place, game_lots::Place::Person(_)))
                    .map(|row| row.used)
                    .collect::<Vec<_>>();
                assert_eq!(amounts, [0, 1]);
                assert_eq!(restored(&host).snapshot(), state);
                (receipts, amounts)
            })
    };
    assert_eq!(run(1, 1000, &[]), run(6, 40, &[31, 9, 1]));
}

#[test]
fn elapsed_offer_retry_does_not_compel_an_occupied_person() {
    let mut host = scenario(1000, true);
    assert_eq!(host.advance_continuous(1, &[]).unwrap().failure, None);
    let opening = host.snapshot().agreements.unwrap();
    assert_eq!(opening.contracts[0].due, 50);
    assert_eq!(opening.offering[0].next_review, 100);
    assert_eq!(host.advance_continuous(100, &[1]).unwrap().failure, None);
    let occupied = host.snapshot().agreements.unwrap();
    assert_eq!(occupied.offering[0].next_review, 100);
    assert_eq!(occupied.contracts.len(), 1);
    host = restored(&host);
    assert_eq!(host.advance_continuous(20, &[]).unwrap().failure, None);
    let retried = host.snapshot().agreements.unwrap();
    assert_eq!(retried.contracts[1].due, 170);
    assert_eq!(retried.offering[0].next_review, 220);
}

#[test]
fn a_controlled_person_can_offer_unheld_goods_and_choose_to_withdraw() {
    use game_economy::core_interval::CorePromiseReply;
    use game_economy_player::interval::Player;

    let mut opening = scenario(1250, false).snapshot();
    opening
        .core
        .people
        .iter_mut()
        .for_each(|person| person.controlled = true);
    let good = opening
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    let mut host = CoreHost::resume(opening).unwrap();
    let id = Player::new(&mut host, 1)
        .unwrap()
        .promise(2, good, 2, 60)
        .unwrap();
    let mut creditor = Player::new(&mut host, 2).unwrap();
    assert_eq!(creditor.view().unwrap().promises[0].due, 60);
    assert!(
        creditor
            .respond_to_promise(id, CorePromiseReply::Withdraw)
            .is_err()
    );
    creditor
        .respond_to_promise(id, CorePromiseReply::Accept)
        .unwrap();
    let mut debtor = Player::new(&mut host, 1).unwrap();
    assert!(!debtor.deliver_promise(id, 2).unwrap());
    assert!(debtor.deliver_promise(id, 1).unwrap());
    debtor
        .respond_to_promise(id, CorePromiseReply::Withdraw)
        .unwrap();
    let state = restored(&host).snapshot();
    let row = &state.agreements.unwrap().contracts[0];
    assert_eq!((row.status, row.delivered), (Status::Withdrawn, 1));
}
