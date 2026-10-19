mod support;
use game_core::*;
use game_observations::*;
#[test]
fn consuming_one_sample_does_not_change_another_and_missing_stock_is_not_known_decay() {
    let (mut h, p, first) = support::people(support::builder());
    let second = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p[0]),
            capacity: 1000,
        })
        .unwrap();
    for inventory in [first, second] {
        h.dispatch(game_cargo::Deposit {
            inventory,
            good: game_cargo::FOOD,
            quantity: 100,
        })
        .unwrap();
        h.dispatch(Inspect {
            person: p[0],
            inventory,
        })
        .unwrap();
    }
    support::run(&mut h, SimTime(HOUR.0));
    h.dispatch(game_physiology::ConsumeEdible {
        person: p[0],
        inventory: first,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let value = serde_json::to_value(h.get::<Samples>().unwrap()).unwrap();
    let row = &value["probes"][p[0].0.to_string()];
    assert_eq!(
        row[format!("{}:{}", first.0, game_cargo::FOOD.0)]["initial"],
        90
    );
    assert_eq!(
        row[format!("{}:{}", second.0, game_cargo::FOOD.0)]["initial"],
        100
    );
    h.dispatch(game_cargo::Transfer {
        from: second,
        to: first,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(Inspect {
        person: p[0],
        inventory: second,
    })
    .unwrap();
    let memory = &h.get::<game_inquiry::Inquiry>().unwrap().people[&p[0]];
    assert!(
        memory
            .observations
            .iter()
            .any(|o| o.outcome == "stock-loss" && o.value > 0)
    );
    assert!(memory.observations.iter().all(|o| o.outcome != "spoilage"));
    let saved = h.save().unwrap();
    assert_eq!(
        saved,
        support::builder().load(&saved, 4).unwrap().save().unwrap()
    );
}

#[test]
fn self_eating_and_being_served_both_create_personal_effect_observations() {
    let (mut h, p, inventory) = support::people(support::builder());
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    support::run(&mut h, SimTime(35 * MINUTE.0));
    h.dispatch(game_physiology::ConsumeEdible {
        person: p[0],
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    h.dispatch(game_physiology::Serve {
        caregiver: p[0],
        patient: p[2],
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let inquiry = h.get::<game_inquiry::Inquiry>().unwrap();
    assert_eq!(
        inquiry.people[&p[0]]
            .observations
            .iter()
            .filter(|o| o.subject.starts_with("good:"))
            .collect::<Vec<_>>(),
        inquiry.people[&p[2]]
            .observations
            .iter()
            .filter(|o| o.subject.starts_with("good:"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        inquiry.people[&p[2]]
            .observations
            .iter()
            .filter(|o| o.subject.starts_with("good:"))
            .count(),
        3
    );
}

#[test]
fn departure_records_origin_weather_and_unseen_decay_on_a_road_is_not_a_revelation() {
    let (mut h, p, inventory) = support::people(support::builder());
    let origin = game_transport::area(h.get().unwrap(), p[0]).unwrap();
    let destination = h
        .dispatch(game_land::CreateArea {
            name: "Far away".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: origin,
        to: destination,
        duration: SimDuration(100 * DAY.0),
    })
    .unwrap();
    let good = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("raw-meat")
        .unwrap();
    h.dispatch(game_preservation::Enable).unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(Inspect {
        person: p[0],
        inventory,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: p[0],
        destination,
    })
    .unwrap();
    let value = serde_json::to_value(h.get::<Observations>().unwrap()).unwrap();
    assert_eq!(
        value["trips"][p[0].0.to_string()]["context"]["temperature"],
        20
    );
    let saved = h.save().unwrap();
    let mut loaded = support::builder().load(&saved, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        support::run(host, SimTime(10 * DAY.0));
        assert!(
            host.get::<game_preservation::Preservation>()
                .unwrap()
                .spoiled
                > 0
        );
        assert!(
            !host
                .get::<game_inquiry::Inquiry>()
                .unwrap()
                .people
                .contains_key(&p[0])
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn inspections_ignore_zero_nonfood_entries_and_have_no_information_gain_after_finishing() {
    let (mut h, p, inventory) = support::people(support::builder());
    let objects = h.get::<game_objects::Objects>().unwrap();
    let food = objects.food_rules().keys().copied().max().unwrap();
    let mut materials: Vec<_> = objects
        .items
        .values()
        .filter(|o| o.food.is_none() && o.id < food && o.mass <= 20)
        .map(|o| o.id)
        .collect();
    materials.sort_unstable();
    for good in materials.into_iter().take(20) {
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 1,
        })
        .unwrap();
        h.dispatch(game_cargo::Consume {
            inventory,
            good,
            quantity: 1,
        })
        .unwrap();
    }
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: food,
        quantity: 2,
    })
    .unwrap();
    h.dispatch(Inspect {
        person: p[0],
        inventory,
    })
    .unwrap();
    assert!(
        h.get::<Samples>().unwrap().probes[&p[0]]
            .values()
            .any(|s| serde_json::to_value(s).unwrap()["good"] == serde_json::json!(food))
    );
    let offers = h.dispatch(game_choice::Collect { person: p[0] }).unwrap();
    assert!(
        offers
            .iter()
            .filter(|o| o.key.starts_with("inquiry:inspect:"))
            .all(|o| o.motive.score() <= 0)
    );
    h.dispatch(Inspect {
        person: p[0],
        inventory,
    })
    .unwrap(); // Repetition remains physically available.
    h.dispatch(game_cargo::Consume {
        inventory,
        good: food,
        quantity: 2,
    })
    .unwrap();
    h.dispatch(Inspect {
        person: p[0],
        inventory,
    })
    .unwrap();
    assert!(
        !h.dispatch(game_choice::Collect { person: p[0] })
            .unwrap()
            .iter()
            .any(|o| o.key.starts_with("inquiry:inspect:"))
    );
}

#[test]
fn bounded_inspections_rotate_until_every_present_food_has_been_examined() {
    let (mut h, p, inventory) = support::people(support::builder());
    let foods: Vec<_> = h
        .get::<game_objects::Objects>()
        .unwrap()
        .items
        .values()
        .filter(|o| o.food.is_some() && o.mass <= 10)
        .map(|o| o.id)
        .take(40)
        .collect();
    assert!(foods.len() > 20);
    for good in &foods {
        h.dispatch(game_cargo::Deposit {
            inventory,
            good: *good,
            quantity: 1,
        })
        .unwrap();
    }
    h.dispatch(Inspect {
        person: p[0],
        inventory,
    })
    .unwrap();
    assert!(h.get::<Samples>().unwrap().probes[&p[0]].len() < foods.len());
    for _ in 0..4 {
        h.dispatch(Inspect {
            person: p[0],
            inventory,
        })
        .unwrap();
    }
    assert_eq!(h.get::<Samples>().unwrap().probes[&p[0]].len(), foods.len());
}

#[test]
fn searching_for_a_moved_sample_records_absence_without_learning_its_new_location() {
    let (mut h, p, _) = support::people(support::builder());
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p[1]),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(Inspect {
        person: p[0],
        inventory,
    })
    .unwrap();
    let origin = game_transport::area(h.get().unwrap(), p[0]).unwrap();
    let far = h
        .dispatch(game_land::CreateArea {
            name: "Remote".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: origin,
        to: far,
        duration: MINUTE,
    })
    .unwrap();
    let until = h
        .dispatch(game_transport::Move {
            person: p[1],
            destination: far,
        })
        .unwrap();
    support::run(&mut h, until);
    h.dispatch(Inspect {
        person: p[0],
        inventory,
    })
    .unwrap();
    let facts = &h.get::<game_knowledge::Knowledge>().unwrap().0[&p[0]];
    assert_eq!(
        game_knowledge::known::<game_knowledge::StoreLocation>(
            facts,
            &game_knowledge::location_key(inventory)
        )
        .unwrap()
        .area,
        origin
    );
    let absent =
        game_knowledge::known::<serde_json::Value>(facts, &game_knowledge::store_key(inventory))
            .unwrap();
    assert_eq!(absent["absent_from"], serde_json::json!(origin));
}
