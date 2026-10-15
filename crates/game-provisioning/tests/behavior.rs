mod support;
use game_core::*;
use game_provisioning::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use support::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Choices(PersonId);
impl Request for Choices {
    const KEY: &'static str = "test.choices";
    type Response = Vec<Plan>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Step {
    person: PersonId,
    plan: Plan,
}
impl Request for Step {
    const KEY: &'static str = "test.step";
    type Response = (bool, Plan);
}
#[test]
fn stock_and_location_must_both_be_known_and_inspection_is_passive() {
    let mut f = setup();
    f.host
        .dispatch(game_knowledge::ObserveSurroundings { person: f.other })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(f.host.query(Choices(f.other)).unwrap().is_empty());
    assert_eq!(f.host.save().unwrap(), before);
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    f.host
        .dispatch(game_knowledge::Tell {
            sender: f.ruler,
            recipient: f.other,
            key: game_knowledge::store_key(f.store),
        })
        .unwrap();
    run(&mut f.host, SimTime(0));
    assert!(f.host.query(Choices(f.other)).unwrap().is_empty());
    f.host
        .dispatch(game_knowledge::Tell {
            sender: f.ruler,
            recipient: f.other,
            key: game_knowledge::location_key(f.store),
        })
        .unwrap();
    run(&mut f.host, SimTime(0));
    assert_eq!(
        f.host.query(Choices(f.other)).unwrap(),
        vec![Plan::Meal { inventory: f.store }]
    );
    let before = f.host.save().unwrap();
    // Believing a meal exists does not grant permission or consume anything on refusal.
    assert!(
        f.host
            .dispatch(Step {
                person: f.other,
                plan: Plan::Meal { inventory: f.store }
            })
            .is_err()
    );
    assert_eq!(f.host.save().unwrap(), before);
}
#[test]
fn gathering_completion_is_required_before_a_plan_can_consume_the_output() {
    let mut f = setup();
    let patch = f
        .host
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area: f.home,
            good: game_cargo::FOOD,
            stock: 20,
            capacity: 20,
            renew: 0,
        }))
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveSurroundings { person: f.ruler })
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.ruler,
            inventory: f.pack,
        })
        .unwrap();
    let plan = f
        .host
        .query(Choices(f.ruler))
        .unwrap()
        .into_iter()
        .find(|p| matches!(p, Plan::Forage { .. }))
        .unwrap();
    let (done, plan) = f
        .host
        .dispatch(Step {
            person: f.ruler,
            plan,
        })
        .unwrap();
    assert!(!done);
    assert_eq!(food(&f.host, f.pack), 0);
    assert_eq!(
        f.host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        20
    );
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Step {
                person: f.ruler,
                plan: plan.clone()
            })
            .is_err()
    );
    assert_eq!(f.host.save().unwrap(), before);
    run(&mut f.host, SimTime(5 * MINUTE.0));
    assert_eq!(food(&f.host, f.pack), 10);
    assert_eq!(
        f.host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        10
    );
    let (done, _) = f
        .host
        .dispatch(Step {
            person: f.ruler,
            plan,
        })
        .unwrap();
    assert!(done);
    assert_eq!(food(&f.host, f.pack), 0);
    assert_eq!(
        f.host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        10
    );
}
