mod support;
use game_core::*;
#[test]
fn unfamiliar_food_already_in_ones_pack_can_be_tried_without_gathering_more() {
    let mut b = support::builder();
    game_nutrition::plugin(&mut b).unwrap();
    let mut h = b.build(7, 1).unwrap();
    let objects = game_objects::standard().unwrap();
    let honey = objects.resolve("honey").unwrap();
    let beet = objects.resolve("beet").unwrap();
    let portion = objects.by_id(beet).unwrap().food.as_ref().unwrap().portion;
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        tick: DAY,
        hunger_per_tick: 3,
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Garden".into(),
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
    for good in [honey, beet] {
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 100,
        })
        .unwrap();
    }
    h.dispatch(game_relationships::Initialize {
        person,
        beliefs: Default::default(),
        values: [("independence".into(), 100)].into(),
    })
    .unwrap();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good: beet,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    h.dispatch(game_nutrition::Enable).unwrap();
    h.dispatch(game_nutrition::Register { person }).unwrap();
    support::run(&mut h, SimTime(31 * DAY.0));
    h.dispatch(game_nutrition::Meal {
        person,
        good: honey,
        quantity: 1000,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    support::run(&mut h, SimTime(32 * DAY.0));
    h.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_characters::Start).unwrap();
    let at = h.now().after(MINUTE).unwrap();
    support::run(&mut h, at);
    let cargo = h.get::<game_cargo::Cargo>().unwrap();
    assert_eq!(cargo.0[&inventory].amount(beet), 100 - 2 * portion);
    assert_eq!(cargo.0[&inventory].amount(honey), 100);
    assert_eq!(
        h.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        100
    );
    assert!(
        h.get::<game_nutrition::Nutrition>().unwrap().people[&person]
            .learned
            .contains_key(&beet)
    );
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&person].hunger < 30);
    assert!(
        h.get::<game_characters::Minds>().unwrap().people[&person]
            .plan
            .is_none()
    );
}
