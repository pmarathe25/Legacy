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
    assert_eq!(
        f.host.query(Choices(f.other)).unwrap(),
        vec![Plan::Explore { area: f.away }]
    );
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
    assert_eq!(
        f.host.query(Choices(f.other)).unwrap(),
        vec![Plan::Explore { area: f.away }]
    );
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
        vec![Plan::Meal {
            inventory: f.store,
            good: Some(game_cargo::FOOD)
        }]
    );
    let before = f.host.save().unwrap();
    // Deciding to eat can defy the claim; a query alone changes no world state.
    assert_eq!(f.host.save().unwrap(), before);
    f.host
        .dispatch(Step {
            person: f.other,
            plan: Plan::Meal {
                inventory: f.store,
                good: None,
            },
        })
        .unwrap();
    assert!(
        !f.host
            .get::<game_property::Property>()
            .unwrap()
            .allowed(f.other, f.store)
    );
    assert!(f.host.notices().any(|n| n.kind == "property.interfered"));
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

#[test]
fn a_food_search_follows_known_roads_before_discovering_unknown_supplies() {
    let mut f = setup();
    f.host
        .dispatch(game_cargo::Consume {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1000,
        })
        .unwrap();
    let patch = f
        .host
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area: f.away,
            good: game_cargo::FOOD,
            stock: 20,
            capacity: 20,
            renew: 0,
        }))
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveSurroundings { person: f.ruler })
        .unwrap();
    let plans = f.host.query(Choices(f.ruler)).unwrap();
    assert_eq!(plans, vec![Plan::Explore { area: f.away }]);
    assert!(
        !f.host.get::<game_knowledge::Knowledge>().unwrap().0[&f.ruler]
            .contains_key(game_knowledge::patch_key(patch).as_str())
    );
    let (done, plan) = f
        .host
        .dispatch(Step {
            person: f.ruler,
            plan: plans[0].clone(),
        })
        .unwrap();
    assert!(!done);
    run(&mut f.host, SimTime(120_000));
    assert_eq!(
        game_transport::area(f.host.get().unwrap(), f.ruler).unwrap(),
        f.away
    );
    let (done, _) = f
        .host
        .dispatch(Step {
            person: f.ruler,
            plan,
        })
        .unwrap();
    assert!(done);
    assert!(
        f.host
            .query(Choices(f.ruler))
            .unwrap()
            .iter()
            .any(|p| matches!(p,Plan::Forage {patch:id,..} if *id==patch))
    );
}

#[test]
fn a_full_pack_can_store_surplus_and_gather_a_meal_without_losing_goods() {
    let mut b = builder();
    game_stockpiles::plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Overgrown cotton field".into(),
        })
        .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(person),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: person,
        inventory,
        recognized_by: [person].into(),
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::TIMBER,
        quantity: 1000,
    })
    .unwrap();
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: game_cargo::FOOD,
        stock: 20,
        capacity: 20,
        renew: 0,
    }))
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person })
        .unwrap();
    let plan = h
        .query(Choices(person))
        .unwrap()
        .into_iter()
        .find(|p| matches!(p, Plan::Forage { .. }))
        .unwrap();
    let (_, plan) = h.dispatch(Step { person, plan }).unwrap();
    run(&mut h, SimTime(5 * MINUTE.0));
    let cache = h.get::<game_stockpiles::Stockpiles>().unwrap().stores[&person][&area];
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&cache].amount(game_cargo::TIMBER),
        10
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::TIMBER),
        990
    );
    assert_eq!(food(&h, inventory), 10);
    let (done, _) = h.dispatch(Step { person, plan }).unwrap();
    assert!(done);
    assert_eq!(food(&h, inventory), 0);
}

#[test]
fn a_foraging_intention_consumes_the_food_it_selected_instead_of_another_available_food() {
    let mut f = setup();
    let objects = f.host.get::<game_objects::Objects>().unwrap();
    let mint = objects.resolve("dreamleaf").unwrap();
    let honey = objects.resolve("honey").unwrap();
    let portion = objects.by_id(mint).unwrap().food.as_ref().unwrap().portion;
    f.host
        .dispatch(game_cargo::Deposit {
            inventory: f.pack,
            good: honey,
            quantity: 100,
        })
        .unwrap();
    let patch = f
        .host
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area: f.home,
            good: mint,
            stock: portion * 2,
            capacity: portion * 2,
            renew: 0,
        }))
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveSurroundings { person: f.ruler })
        .unwrap();
    let (_, plan) = f
        .host
        .dispatch(Step {
            person: f.ruler,
            plan: Plan::Forage {
                patch,
                inventory: f.pack,
                quantity: portion,
                gathering: false,
            },
        })
        .unwrap();
    run(&mut f.host, SimTime(5 * MINUTE.0));
    assert!(
        f.host
            .dispatch(Step {
                person: f.ruler,
                plan
            })
            .unwrap()
            .0
    );
    let cargo = f.host.get::<game_cargo::Cargo>().unwrap();
    assert_eq!(cargo.0[&f.pack].amount(mint), 0);
    assert_eq!(cargo.0[&f.pack].amount(honey), 100);
}

#[test]
fn older_unspecified_meals_still_decode_and_round_trip() {
    let value = serde_json::json!({"Meal":{"inventory":12}});
    let plan: Plan = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        plan,
        Plan::Meal {
            inventory: InventoryId(12),
            good: None
        }
    );
    assert_eq!(serde_json::to_value(plan).unwrap(), value);
}

#[test]
fn source_recall_expands_with_intelligence_while_personal_supplies_remain_available() {
    let mut b = builder();
    game_traits::plugin(&mut b).unwrap();
    let mut f = setup_with(b);
    f.host
        .dispatch(game_traits::Install(
            game_traits::load(&mut game_content::Store::standard()).unwrap(),
        ))
        .unwrap();
    for _ in 0..20 {
        let inventory = f
            .host
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Area(f.home),
                capacity: 100,
            })
            .unwrap();
        f.host
            .dispatch(game_property::ClaimInventory {
                claimant: f.ruler,
                inventory,
                recognized_by: [f.ruler].into(),
            })
            .unwrap();
        f.host
            .dispatch(game_cargo::Deposit {
                inventory,
                good: game_cargo::FOOD,
                quantity: 20,
            })
            .unwrap();
        f.host
            .dispatch(game_knowledge::ObserveStore {
                person: f.other,
                inventory,
            })
            .unwrap();
    }
    let own = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(f.other),
            capacity: 100,
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Deposit {
            inventory: own,
            good: game_cargo::FOOD,
            quantity: 20,
        })
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.other,
            inventory: own,
        })
        .unwrap();
    f.host
        .dispatch(game_traits::Set {
            person: f.other,
            profile: [("intelligence".into(), 0)].into(),
        })
        .unwrap();
    let before = f.host.save().unwrap();
    let limited = f.host.query(Choices(f.other)).unwrap();
    assert_eq!(limited.len(), 2);
    assert!(
        limited
            .iter()
            .any(|p| matches!(p,Plan::Meal {inventory,..} if *inventory==own))
    );
    assert_eq!(f.host.save().unwrap(), before);
    f.host
        .dispatch(game_traits::Set {
            person: f.other,
            profile: [("intelligence".into(), 16000)].into(),
        })
        .unwrap();
    assert_eq!(f.host.query(Choices(f.other)).unwrap().len(), 21);
}
