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
#[derive(Clone, Serialize, Deserialize)]
struct Inspect(Step);
impl Request for Inspect {
    const KEY: &'static str = "test.local-meal";
    type Response = Option<LocalMeal>;
}
fn fixture() -> Fixture {
    fixture_with_objects(game_objects::standard().unwrap())
}
fn fixture_with_objects(objects: game_objects::Objects) -> Fixture {
    setup_with_objects(descriptor_builder(), objects)
}
fn descriptor_builder() -> Builder {
    let mut b = builder();
    b.request::<Inspect>(|tx, r| local_meal(tx, r.0.person, &r.0.plan))
        .unwrap();
    b
}
fn meal(person: PersonId, inventory: InventoryId) -> Step {
    Step {
        person,
        plan: Plan::Meal {
            inventory,
            good: Some(game_cargo::FOOD),
        },
    }
}
fn observe(f: &mut Fixture, inventory: InventoryId) {
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.ruler,
            inventory,
        })
        .unwrap();
}
#[test]
fn description_is_passive_and_uses_the_actual_authored_portion() {
    let mut objects = game_objects::standard().unwrap();
    objects
        .items
        .values_mut()
        .find(|o| o.id == game_cargo::FOOD)
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .portion = 7;
    let mut f = fixture_with_objects(objects);
    let request = meal(f.ruler, f.store);
    assert_eq!(f.host.query(Inspect(request.clone())).unwrap(), None);
    observe(&mut f, request.plan.inventory().unwrap());
    let before = f.host.save().unwrap();
    assert_eq!(
        f.host.query(Inspect(request.clone())).unwrap(),
        Some(LocalMeal {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 7,
        })
    );
    assert_eq!(before, f.host.save().unwrap());
    for workers in [1, 4] {
        let loaded = descriptor_builder().load(&before, workers).unwrap();
        assert_eq!(
            loaded.query(Inspect(request.clone())).unwrap(),
            f.host.query(Inspect(request.clone())).unwrap()
        );
        assert_eq!(before, loaded.save().unwrap());
    }
    let stock = food(&f.host, f.store);
    assert!(f.host.dispatch(request).unwrap().0);
    assert_eq!(food(&f.host, f.store), stock - 7);
}

#[test]
fn remembered_and_actual_locality_must_both_match() {
    let mut f = fixture();
    let inventory = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(f.other),
            capacity: 100,
        })
        .unwrap();
    observe(&mut f, inventory);
    let request = Inspect(meal(f.ruler, inventory));
    // Empty stock is still a demand: admission and execution check availability.
    assert!(f.host.query(request.clone()).unwrap().is_some());
    f.host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(120_000));
    assert_eq!(f.host.query(request.clone()).unwrap(), None);
    f.host
        .dispatch(game_transport::Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(240_000));
    // The stock is now physically local, but its remembered destination is not.
    assert_eq!(f.host.query(request.clone()).unwrap(), None);
    observe(&mut f, inventory);
    assert!(f.host.query(request).unwrap().is_some());
}

#[test]
fn own_carried_store_follows_actor_but_travelling_and_busy_actors_are_excluded() {
    let mut f = fixture();
    let pack = f.pack;
    observe(&mut f, pack);
    let request = Inspect(meal(f.ruler, pack));
    f.host
        .dispatch(game_transport::Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(120_000));
    assert!(f.host.query(request.clone()).unwrap().is_some());
    f.host
        .dispatch(game_physiology::Rest { person: f.ruler })
        .unwrap();
    assert_eq!(f.host.query(request).unwrap(), None);

    let mut f = fixture();
    let pack = f.pack;
    observe(&mut f, pack);
    f.host
        .dispatch(game_transport::Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    assert_eq!(f.host.query(Inspect(meal(f.ruler, pack))).unwrap(), None);
}

#[test]
fn only_explicit_edible_meals_are_described() {
    let mut f = fixture();
    let store = f.store;
    observe(&mut f, store);
    for plan in [
        Plan::Meal {
            inventory: store,
            good: None,
        },
        Plan::Meal {
            inventory: store,
            good: Some(game_cargo::TIMBER),
        },
        Plan::Meal {
            inventory: store,
            good: Some(GoodId(u64::MAX)),
        },
        Plan::Meal {
            inventory: InventoryId(u64::MAX),
            good: Some(game_cargo::FOOD),
        },
        Plan::Explore { area: f.home },
        Plan::Forage {
            patch: 1,
            inventory: store,
            quantity: 7,
            gathering: false,
        },
        Plan::Forage {
            patch: 1,
            inventory: store,
            quantity: 7,
            gathering: true,
        },
    ] {
        assert_eq!(
            f.host
                .query(Inspect(Step {
                    person: f.ruler,
                    plan
                }))
                .unwrap(),
            None
        );
    }
}
