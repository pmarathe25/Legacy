use crate::support;
use game_core::*;
use game_sim::*;
fn traveler() -> (Host, PersonId, InventoryId, AreaId, AreaId) {
    let mut h = support::builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        tick: HOUR,
        hunger_per_tick: 15,
        fatigue_per_tick: 0,
        deprivation: 1000,
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_characters::Configure(game_characters::Rules {
        review: SimDuration(12 * HOUR.0),
        ..Default::default()
    }))
    .unwrap();
    let home = h
        .dispatch(game_land::CreateArea {
            name: "Home".into(),
        })
        .unwrap();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Food".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: home,
        to: away,
        duration: SimDuration(2 * MINUTE.0),
    })
    .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(person),
            capacity: 1000,
        })
        .unwrap();
    (h, person, pack, home, away)
}
#[test]
fn remaining_hunger_can_prompt_another_portion_without_waiting_for_the_review_clock() {
    let (mut h, person, pack, home, _) = traveler();
    h.dispatch(game_transport::Place { person, area: home })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_characters::Start).unwrap();
    let at = SimTime(12 * HOUR.0);
    while support::food(&h, pack) == 100 {
        h.advance(at, 1).unwrap();
    }
    assert_eq!(support::food(&h, pack), 90);
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&person].hunger,
        120
    );
    let mut loaded = support::builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        support::run(host, at);
        assert_eq!(support::food(host, pack), 80);
        assert_eq!(
            host.get::<game_physiology::Bodies>().unwrap().people[&person].hunger,
            60
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    let saved: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    assert_eq!(
        saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|j| j["request"]["key"] == "characters.review")
            .count(),
        1
    );
}
#[test]
fn arriving_from_an_unrelated_journey_prompts_a_hungry_character_without_a_food_plan() {
    let (mut h, person, pack, home, away) = traveler();
    h.dispatch(game_transport::Place { person, area: home })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_characters::Start).unwrap();
    support::run(&mut h, SimTime(12 * HOUR.0 - MINUTE.0));
    let arrives = h
        .dispatch(game_transport::Move {
            person,
            destination: away,
        })
        .unwrap();
    loop {
        h.advance(arrives, 1).unwrap();
        if !h.get::<Actions>().unwrap().0.contains_key(&person) {
            break;
        }
    }
    assert!(
        h.get::<game_characters::Minds>().unwrap().people[&person]
            .plan
            .is_none()
    );
    let mut loaded = support::builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        support::run(host, SimTime(13 * HOUR.0));
        assert_eq!(support::food(host, pack), 0);
        assert!(
            host.notices()
                .any(|n| n.kind == "physiology.fed"
                    && n.data["patient"] == serde_json::json!(person))
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn a_chosen_food_journey_continues_on_completion_without_multiplying_review_clocks() {
    let (mut h, person, pack, home, away) = traveler();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area: away,
            good: game_cargo::FOOD,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    h.dispatch(game_transport::Place { person, area: away })
        .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person })
        .unwrap();
    h.dispatch(game_transport::Move {
        person,
        destination: home,
    })
    .unwrap();
    support::run(&mut h, SimTime(2 * MINUTE.0));
    h.dispatch(game_characters::Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_characters::Start).unwrap();
    let first = h.now().after(SimDuration(12 * HOUR.0)).unwrap();
    support::run(&mut h, first);
    let arrives = h.get::<Actions>().unwrap().0[&person].until.unwrap();
    loop {
        h.advance(arrives, 1).unwrap();
        if !h.get::<Actions>().unwrap().0.contains_key(&person) {
            break;
        }
    }
    let save = h.save().unwrap();
    let mut loaded = support::builder().load(&save, 4).unwrap();
    let mut interrupted = support::builder().load(&save, 1).unwrap();
    interrupted
        .dispatch(game_physiology::Strain {
            person,
            amount: 500,
        })
        .unwrap();
    interrupted
        .dispatch(game_physiology::Rest { person })
        .unwrap();
    let target = first.after(SimDuration(20 * MINUTE.0)).unwrap();
    support::run(&mut h, target);
    support::run(&mut loaded, target);
    support::run(&mut interrupted, target);
    assert!(
        interrupted.get::<game_characters::Minds>().unwrap().people[&person]
            .plan
            .is_none()
    );
    assert_eq!(
        interrupted.get::<game_physiology::Bodies>().unwrap().people[&person].hunger,
        120
    );
    assert_eq!(
        interrupted.get::<Actions>().unwrap().0[&person].kind,
        "rest"
    );
    assert!(
        h.get::<game_characters::Minds>().unwrap().people[&person]
            .plan
            .is_none()
    );
    assert_eq!(
        h.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        80
    );
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&person].hunger,
        60
    );
    assert!(
        h.notices()
            .any(|n| n.kind == "physiology.fed" && n.data["patient"] == serde_json::json!(person))
    );
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    let saved: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    assert_eq!(
        saved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|j| j["request"]["key"] == "characters.review")
            .count(),
        1
    );
    assert_eq!(support::food(&h, pack), 0);
}
