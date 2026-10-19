mod support;
use game_core::*;
use game_sim::*;
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Idle(u64);
impl Resource for Idle {
    const KEY: &'static str = "test.idle";
}
fn review(fatigue: u64) -> (Host, PersonId, InventoryId, GoodId) {
    let mut b = support::builder();
    game_nutrition::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.resource::<Idle>().unwrap();
    b.subscribe::<game_characters::IdleReview>("test.idle", |tx, _| {
        tx.edit::<Idle>()?.0 += 1;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(3, 1).unwrap();
    let objects = game_objects::standard().unwrap();
    let honey = objects.resolve("honey").unwrap();
    h.dispatch(game_objects::Install(objects)).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Clearing".into(),
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
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: honey,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_nutrition::Enable).unwrap();
    h.dispatch(game_nutrition::Register { person }).unwrap();
    support::run(&mut h, SimTime(31 * DAY.0));
    // Recall enough honey to replenish its energy, but none of the missing protein or fat.
    h.dispatch(game_nutrition::Meal {
        person,
        good: honey,
        quantity: 1000,
    })
    .unwrap();
    h.dispatch(game_physiology::Strain {
        person,
        amount: fatigue,
    })
    .unwrap();
    h.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore { person, inventory })
        .unwrap();
    h.dispatch(game_characters::Start).unwrap();
    let at = h.now().after(MINUTE).unwrap();
    support::run(&mut h, at);
    (h, person, inventory, honey)
}
#[test]
fn a_deficiency_does_not_lock_a_satiated_person_into_food_that_cannot_help() {
    let (h, person, inventory, honey) = review(0);
    assert!(h.get::<Idle>().unwrap().0 >= 1);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(honey),
        100 - h
            .get::<game_objects::Objects>()
            .unwrap()
            .by_id(honey)
            .unwrap()
            .food
            .as_ref()
            .unwrap()
            .portion
    );
    assert!(
        h.get::<game_characters::Minds>().unwrap().people[&person]
            .plan
            .is_none()
    );
    h.save().unwrap();
}
#[test]
fn a_tired_satiated_person_can_still_choose_comfort_food_over_rest() {
    let (h, person, inventory, honey) = review(11);
    assert!(h.get::<Idle>().unwrap().0 >= 1);
    assert_eq!(h.get::<Actions>().unwrap().0[&person].kind, "rest");
    let portion = h
        .get::<game_objects::Objects>()
        .unwrap()
        .by_id(honey)
        .unwrap()
        .food
        .as_ref()
        .unwrap()
        .portion;
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(honey),
        100 - portion
    );
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&person].fatigue,
        11
    );
    h.save().unwrap();
}
