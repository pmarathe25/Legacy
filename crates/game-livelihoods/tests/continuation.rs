mod support;
use game_core::*;
use game_livelihoods::*;
use game_sim::*;
use support::*;
fn ready() -> (Host, PersonId, InventoryId, u64) {
    let (mut h, p, inventory, area) = setup();
    h.dispatch(
        game_workshop::load(&mut game_content::Store::standard(), h.get().unwrap()).unwrap(),
    )
    .unwrap();
    h.dispatch(Install {
        rules: Rules {
            interval: HOUR,
            goals: vec![Goal {
                object: "charcoal".into(),
                quantity: 6,
                priority: 50,
                affinity: "discipline".into(),
                concept: "craft".into(),
            }],
        },
        stamps: vec![],
    })
    .unwrap();
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: game_cargo::TIMBER,
        stock: 12,
        capacity: 12,
        renew: 0,
    }))
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: p,
        inventory,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person: p })
        .unwrap();
    h.dispatch(Enable).unwrap();
    h.dispatch(game_choice::ConfigureExecution(true)).unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    let action = h.get::<Actions>().unwrap().0[&p].id;
    (h, p, inventory, action)
}
#[test]
fn continuation_advances_consumed_plan_steps_once_and_survives_reload() {
    let (mut h, p, inventory, gather) = ready();
    run(&mut h, 5 * MINUTE.0);
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        assert_eq!(
            host.dispatch(Continue {
                person: p,
                action: gather + 1
            })
            .unwrap(),
            game_choice::Outcome::Reconsider
        );
        assert_eq!(
            host.dispatch(Continue {
                person: p,
                action: gather
            })
            .unwrap(),
            game_choice::Outcome::Performed
        );
        let craft = host.get::<Actions>().unwrap().0[&p].id;
        assert_eq!(amount(host, inventory, "timber"), 0);
        assert_eq!(
            host.dispatch(Continue {
                person: p,
                action: gather
            })
            .unwrap(),
            game_choice::Outcome::Reconsider
        );
        assert_eq!(host.get::<Actions>().unwrap().0[&p].id, craft);
        run(host, 50 * MINUTE.0);
        assert_eq!(amount(host, inventory, "charcoal"), 6);
        assert_eq!(
            host.dispatch(Continue {
                person: p,
                action: craft
            })
            .unwrap(),
            game_choice::Outcome::Reconsider
        );
        assert_eq!(host.get::<Livelihoods>().unwrap().achieved[&p], 1);
        assert!(!host.get::<Livelihoods>().unwrap().has_plan(p));
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn cancelled_steps_do_not_authorize_goal_continuation() {
    let (mut h, p, inventory, action) = ready();
    h.dispatch(Cancel(p)).unwrap();
    run(&mut h, HOUR.0);
    assert_eq!(
        h.dispatch(Continue { person: p, action }).unwrap(),
        game_choice::Outcome::Reconsider
    );
    assert_eq!(amount(&h, inventory, "charcoal"), 0);
    assert!(h.get::<Actions>().unwrap().0.is_empty());
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Fail(Continue);
impl Request for Fail {
    const KEY: &'static str = "test.fail-continued-step";
    type Response = ();
}
#[test]
fn failed_transactions_restore_tokens_and_failed_steps_use_live_supplies() {
    let (mut h, p, inventory, action) = ready();
    run(&mut h, 5 * MINUTE.0);
    let before = h.save().unwrap();
    let mut b = builder();
    b.request::<Fail>(|tx, r| {
        tx.call(r.0)?;
        require(false, "reject continued step")
    })
    .unwrap();
    let mut loaded = b.load(&before, 4).unwrap();
    assert!(
        loaded
            .dispatch(Fail(Continue { person: p, action }))
            .is_err()
    );
    assert_eq!(loaded.save().unwrap(), before);
    h.dispatch(game_cargo::Consume {
        inventory,
        good: game_cargo::TIMBER,
        quantity: 12,
    })
    .unwrap();
    assert_eq!(
        h.dispatch(Continue { person: p, action }).unwrap(),
        game_choice::Outcome::Reconsider
    );
    assert_eq!(h.get::<Livelihoods>().unwrap().failures[&p], 1);
    assert_eq!(
        h.dispatch(Continue { person: p, action }).unwrap(),
        game_choice::Outcome::Reconsider
    );
    assert_eq!(h.get::<Livelihoods>().unwrap().failures[&p], 1);
    assert!(h.get::<Actions>().unwrap().0.is_empty());
}
