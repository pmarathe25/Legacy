use game_core::*;
use game_information::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_choice::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_inquiry::plugin(&mut b).unwrap();
    game_items::plugin(&mut b).unwrap();
    game_literacy::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b
}
fn setup() -> (Host, [PersonId; 3], [AreaId; 2]) {
    let mut h = builder().build(39, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let a = h
        .dispatch(game_land::CreateArea {
            name: "Village".into(),
        })
        .unwrap();
    let b = h
        .dispatch(game_land::CreateArea {
            name: "Market".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: a,
        to: b,
        duration: MINUTE,
    })
    .unwrap();
    let people = [0, 1, 2].map(|i| {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area: a })
            .unwrap();
        h.dispatch(game_social_life::Register {
            person: p,
            name: format!("Person {i}"),
        })
        .unwrap();
        p
    });
    (h, people, [a, b])
}
fn run(h: &mut Host, minutes: u64) {
    while h.advance(SimTime(minutes * MINUTE.0), 512).unwrap().stop == Stop::Budget {}
}
fn conclude(h: &mut Host, p: PersonId, key: &str, value: serde_json::Value) {
    h.dispatch(game_knowledge::Conclude {
        person: p,
        key: key.into(),
        value,
        confidence: 80,
    })
    .unwrap();
}
fn choose(h: &mut Host, person: PersonId, prefix: &str) {
    let offers = h.dispatch(game_choice::Collect { person }).unwrap();
    let proposal = offers
        .into_iter()
        .find(|o| o.key.starts_with(prefix))
        .unwrap();
    h.dispatch(game_choice::Run(proposal.request)).unwrap();
}
#[test]
fn information_needs_offer_questions_without_inspecting_a_prospective_respondents_mind() {
    let (mut h, [asker, teacher, stranger], _) = setup();
    conclude(
        &mut h,
        teacher,
        "technique:baking",
        serde_json::json!({"oven":"warm"}),
    );
    let topic = Topic::Category("technique:".into());
    h.dispatch(Want {
        person: asker,
        topic: topic.clone(),
        importance: 80,
    })
    .unwrap();
    let offers = h.dispatch(game_choice::Collect { person: asker }).unwrap();
    assert!(offers.iter().any(|o| {
        o.key
            .starts_with(&format!("information:ask:{}:", teacher.0))
    }));
    assert!(offers.iter().any(|o| {
        o.key
            .starts_with(&format!("information:ask:{}:", stranger.0))
    }));
    choose(&mut h, asker, &format!("information:ask:{}:", teacher.0));
    run(&mut h, 2);
    assert!(
        h.get::<game_knowledge::Knowledge>()
            .unwrap()
            .0
            .get(&asker)
            .is_none_or(|f| !f.contains_key("technique:baking"))
    );
    assert_eq!(
        h.get::<Information>().unwrap().inboxes[&teacher][0].topic,
        topic
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        choose(host, teacher, "information:answer:");
        run(host, 4);
        let fact = host.get::<game_knowledge::Knowledge>().unwrap().0[&asker]
            .get("technique:baking")
            .unwrap();
        assert_eq!(fact.source, teacher);
        assert_eq!(fact.observed, SimTime(0));
        assert_eq!(fact.confidence, 80);
        assert!(host.get::<Information>().unwrap().inboxes[&teacher].is_empty());
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn supply_answers_include_remembered_routes_and_locations_and_can_be_out_of_date() {
    let (mut h, [asker, witness, _], [_, market]) = setup();
    let food = game_cargo::FOOD;
    let stock = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(market),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: stock,
        good: food,
        quantity: 10,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: witness,
        destination: market,
    })
    .unwrap();
    run(&mut h, 1);
    h.dispatch(game_knowledge::ObserveSurroundings { person: witness })
        .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: witness,
        inventory: stock,
    })
    .unwrap();
    let village = game_transport::area(h.get().unwrap(), asker).unwrap();
    h.dispatch(game_transport::Move {
        person: witness,
        destination: village,
    })
    .unwrap();
    run(&mut h, 2);
    h.dispatch(game_knowledge::ObserveSurroundings { person: witness })
        .unwrap();
    run(&mut h, 10);
    h.dispatch(game_cargo::Consume {
        inventory: stock,
        good: food,
        quantity: 10,
    })
    .unwrap();
    let id = h
        .dispatch(Ask {
            person: asker,
            other: witness,
            topic: Topic::Supplies(food),
        })
        .unwrap();
    let key = game_knowledge::store_key(stock);
    assert_eq!(
        h.dispatch(Answer {
            person: witness,
            question: id,
            key: Some(key.clone())
        })
        .unwrap(),
        game_choice::Outcome::Performed
    );
    run(&mut h, 10);
    let facts = &h.get::<game_knowledge::Knowledge>().unwrap().0[&asker];
    assert_eq!(
        game_knowledge::known::<game_knowledge::Stock>(facts, &key).unwrap()[&food],
        10
    );
    assert_eq!(
        game_knowledge::known::<game_knowledge::StoreLocation>(
            facts,
            &game_knowledge::location_key(stock)
        )
        .unwrap()
        .area,
        market
    );
    assert!(game_knowledge::route(facts, village, market).is_ok());
    assert_eq!(facts.get(key.as_str()).unwrap().observed, SimTime(MINUTE.0));
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&stock].amount(food),
        0
    );
}
#[test]
fn personality_and_relationships_can_favor_declining_and_unrelated_answers_are_rejected() {
    let (mut h, [asker, reluctant, _], _) = setup();
    h.dispatch(game_traits::Set {
        person: reluctant,
        profile: [("altruism".into(), 0), ("sociability".into(), 0)].into(),
    })
    .unwrap();
    h.dispatch(game_relationships::LearnValue {
        person: reluctant,
        concept: "privacy".into(),
        outcome: 100,
    })
    .unwrap();
    conclude(&mut h, reluctant, "name:child", serde_json::json!("Ada"));
    conclude(
        &mut h,
        reluctant,
        "technique:oven",
        serde_json::json!("clay"),
    );
    let id = h
        .dispatch(Ask {
            person: asker,
            other: reluctant,
            topic: Topic::Fact("name:child".into()),
        })
        .unwrap();
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(Answer {
            person: reluctant,
            question: id,
            key: Some("technique:oven".into())
        })
        .is_err()
    );
    assert_eq!(saved, h.save().unwrap());
    let offers = h
        .dispatch(game_choice::Collect { person: reluctant })
        .unwrap();
    let response = offers
        .iter()
        .find(|o| o.key.starts_with("information:answer:"))
        .unwrap();
    let refusal = offers
        .iter()
        .find(|o| o.key.starts_with("information:decline:"))
        .unwrap();
    assert!(refusal.motive.score() > response.motive.score());
    choose(&mut h, reluctant, "information:decline:");
    run(&mut h, 2);
    assert!(
        h.get::<game_knowledge::Knowledge>()
            .unwrap()
            .0
            .get(&asker)
            .is_none_or(|f| !f.contains_key("name:child"))
    );
    assert!(h.get::<Information>().unwrap().inboxes[&reluctant].is_empty());
}
#[test]
fn questions_require_contact_and_an_understood_language_and_never_force_answers() {
    let (mut h, [asker, respondent, _], [_, market]) = setup();
    h.dispatch(game_communication::Enable).unwrap();
    let r = Ask {
        person: asker,
        other: respondent,
        topic: Topic::Fact("unfamiliar-mechanism".into()),
    };
    let saved = h.save().unwrap();
    assert!(h.dispatch(r.clone()).is_err());
    assert_eq!(saved, h.save().unwrap());
    h.dispatch(game_transport::Move {
        person: respondent,
        destination: market,
    })
    .unwrap();
    assert!(h.dispatch(r).is_err());
    assert!(h.get::<Information>().unwrap().inboxes.is_empty());
}
#[test]
fn a_departed_listener_cannot_receive_a_timed_reply() {
    let (mut h, [asker, respondent, _], [_, market]) = setup();
    conclude(&mut h, respondent, "roads:story", serde_json::json!("east"));
    let id = h
        .dispatch(Ask {
            person: asker,
            other: respondent,
            topic: Topic::Fact("roads:story".into()),
        })
        .unwrap();
    choose(&mut h, respondent, "information:answer:");
    h.dispatch(game_transport::Move {
        person: asker,
        destination: market,
    })
    .unwrap();
    run(&mut h, 2);
    assert!(
        h.get::<game_knowledge::Knowledge>()
            .unwrap()
            .0
            .get(&asker)
            .is_none_or(|f| !f.contains_key("roads:story"))
    );
    assert_eq!(
        h.get::<Information>().unwrap().inboxes[&respondent][0].id,
        id
    );
}

#[test]
fn a_requested_causal_explanation_can_be_simplified_for_a_less_capable_listener() {
    let (mut h, [listener, teacher, _], _) = setup();
    for (person, intelligence) in [(listener, 200), (teacher, 4000)] {
        h.dispatch(game_traits::Set {
            person,
            profile: [("intelligence".into(), intelligence)].into(),
        })
        .unwrap();
    }
    let value = serde_json::json!({"subject":"iron","factor":"heat","outcome":"hardness","complexity":3,
        "context":{"water":1},"effect":100,"confidence":80,"comparisons":5});
    let key = "theory:iron:heat:hardness";
    conclude(&mut h, teacher, key, value);
    let id = h
        .dispatch(Ask {
            person: listener,
            other: teacher,
            topic: Topic::Category("theory:iron:".into()),
        })
        .unwrap();
    assert!(
        !h.dispatch(game_inquiry::Explain {
            teacher,
            listener,
            key: key.into(),
            form: game_inquiry::Explanation::Detailed
        })
        .unwrap()
    );
    assert_eq!(
        h.dispatch(Answer {
            person: teacher,
            question: id,
            key: Some(key.into())
        })
        .unwrap(),
        game_choice::Outcome::Performed
    );
    run(&mut h, 0);
    assert!(h.get::<game_knowledge::Knowledge>().unwrap().0[&listener].contains_key(key));
}
#[test]
fn consulting_a_registry_requires_seen_text_and_a_decoded_title_and_survives_reload() {
    let (mut h, [reader, author, _], _) = setup();
    h.dispatch(game_traits::Set {
        person: author,
        profile: [("intelligence".into(), 4000)].into(),
    })
    .unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(author),
            capacity: 1000,
        })
        .unwrap();
    for key in ["paper", "ink"] {
        let good = h
            .get::<game_objects::Objects>()
            .unwrap()
            .resolve(key)
            .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 5,
        })
        .unwrap();
    }
    let until = h
        .dispatch(game_literacy::Develop {
            person: author,
            inventory,
        })
        .unwrap();
    while h.advance(until, 512).unwrap().stop == Stop::Budget {}
    let key = game_knowledge::store_key(inventory);
    h.dispatch(game_knowledge::ObserveStore {
        person: author,
        inventory,
    })
    .unwrap();
    let script = *h
        .get::<game_literacy::Literacy>()
        .unwrap()
        .scripts
        .keys()
        .next()
        .unwrap();
    let until = h
        .dispatch(game_literacy::Inscribe {
            person: author,
            inventory,
            key: key.clone(),
            script,
        })
        .unwrap();
    while h.advance(until, 512).unwrap().stop == Stop::Budget {}
    let document = *h
        .get::<game_literacy::Literacy>()
        .unwrap()
        .documents
        .keys()
        .next()
        .unwrap();
    let topic = Topic::Supplies(
        h.get::<game_objects::Objects>()
            .unwrap()
            .resolve("paper")
            .unwrap(),
    );
    h.dispatch(Want {
        person: reader,
        topic: topic.clone(),
        importance: 70,
    })
    .unwrap();
    assert!(
        !h.dispatch(game_choice::Collect { person: reader })
            .unwrap()
            .iter()
            .any(|o| o.key.starts_with("information:read:"))
    );
    h.dispatch(game_literacy::Show {
        person: author,
        listener: reader,
        document,
    })
    .unwrap();
    // Seeing a page doesn't reveal its private semantic key.
    assert!(
        !h.dispatch(game_choice::Collect { person: reader })
            .unwrap()
            .iter()
            .any(|o| o.key.starts_with("information:read:"))
    );
    let concepts = [
        format!("person:{}", author.0),
        format!("person:{}", reader.0),
    ];
    for _ in 0..4 {
        for concept in &concepts {
            h.dispatch(game_communication::Convey {
                speaker: author,
                listener: reader,
                concept: concept.clone(),
                demonstrate: true,
                explanation: vec![],
            })
            .unwrap();
        }
    }
    for _ in 0..8 {
        h.dispatch(game_communication::Convey {
            speaker: author,
            listener: reader,
            concept: key.clone(),
            demonstrate: false,
            explanation: concepts.to_vec(),
        })
        .unwrap();
    }
    for topic in [Topic::Fact(key.clone()), Topic::Category("store:".into())] {
        h.dispatch(Want {
            person: reader,
            topic,
            importance: 70,
        })
        .unwrap();
    }
    let options = h.dispatch(game_choice::Collect { person: reader }).unwrap();
    assert_eq!(
        options
            .iter()
            .filter(|o| o.key.starts_with("information:read:"))
            .count(),
        3
    );
    for topic in [Topic::Fact(key.clone()), Topic::Category("store:".into())] {
        h.dispatch(Withdraw {
            person: reader,
            topic,
        })
        .unwrap();
    }
    choose(&mut h, reader, "information:read:");
    let failed_at = h.now().after(SimDuration(2 * MINUTE.0)).unwrap();
    while h.advance(failed_at, 512).unwrap().stop == Stop::Budget {}
    assert!(
        !h.dispatch(game_choice::Collect { person: reader })
            .unwrap()
            .iter()
            .any(|o| o.key.starts_with("information:read:"))
    );
    // Learning the script provides a new reason to retry, rather than an imposed wait.
    for _ in 0..2 {
        h.dispatch(game_literacy::Teach {
            teacher: author,
            student: reader,
            script,
        })
        .unwrap();
    }
    let village = game_transport::area(h.get().unwrap(), author).unwrap();
    let market = h.get::<game_land::Land>().unwrap().0[&village]
        .links
        .keys()
        .next()
        .copied()
        .unwrap();
    h.dispatch(game_transport::Move {
        person: author,
        destination: market,
    })
    .unwrap();
    choose(&mut h, reader, "information:read:");
    let unavailable_at = h.now().after(SimDuration(2 * MINUTE.0)).unwrap();
    while h.advance(unavailable_at, 512).unwrap().stop == Stop::Budget {}
    h.dispatch(game_transport::Move {
        person: author,
        destination: village,
    })
    .unwrap();
    let available_at = h.now().after(MINUTE).unwrap();
    while h.advance(available_at, 512).unwrap().stop == Stop::Budget {}
    choose(&mut h, reader, "information:read:");
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    let end = h.now().after(SimDuration(2 * MINUTE.0)).unwrap();
    for host in [&mut h, &mut loaded] {
        while host.advance(end, 512).unwrap().stop == Stop::Budget {}
        assert!(
            host.get::<game_knowledge::Knowledge>().unwrap().0[&reader].contains_key(key.as_str())
        );
        assert!(
            !host
                .dispatch(game_choice::Collect { person: reader })
                .unwrap()
                .iter()
                .any(|o| o.key.starts_with("information:read:"))
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn unexplained_contrasts_create_voluntary_information_interests_without_revealing_answers() {
    let (mut h, [observer, _, _], _) = setup();
    for (temperature, value) in [(5, 90), (30, 10)] {
        h.dispatch(game_inquiry::Witness { observer, observation: serde_json::from_value(serde_json::json!({
            "subject":"fruit","context":{"temperature":temperature},"outcome":"freshness","value":value,"trial":1
        })).unwrap() }).unwrap();
    }
    let interest = &h.get::<Information>().unwrap().interests[&observer][0];
    assert_eq!(interest.topic, Topic::Category("theory:fruit:".into()));
    assert!(
        h.dispatch(game_choice::Collect { person: observer })
            .unwrap()
            .iter()
            .any(|o| o.key.starts_with("information:ask:"))
    );
    assert!(
        game_knowledge::with_prefix(
            &h.get::<game_knowledge::Knowledge>().unwrap().0[&observer],
            "theory:"
        )
        .next()
        .is_none()
    );
    h.dispatch(Withdraw {
        person: observer,
        topic: Topic::Category("theory:fruit:".into()),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: observer,
        profile: [("openness".into(), 0), ("contemplation".into(), 0)].into(),
    })
    .unwrap();
    h.dispatch(game_inquiry::Witness { observer, observation: serde_json::from_value(serde_json::json!({
        "subject":"fruit","context":{"temperature":15},"outcome":"freshness","value":40,"trial":2
    })).unwrap() }).unwrap();
    assert!(h.get::<Information>().unwrap().interests[&observer].is_empty());
}

#[test]
fn changing_one_question_history_preserves_other_people_and_older_revisions() {
    let (mut h, people, _) = setup();
    for person in people {
        h.dispatch(Want {
            person,
            topic: Topic::Fact("weather".into()),
            importance: 50,
        })
        .unwrap();
    }
    let previous = h.get::<Information>().unwrap().clone();
    let previous_wire = serde_json::to_string(&previous).unwrap();
    let other = previous.interests.get(&people[1]).unwrap().clone();
    h.dispatch(Ask {
        person: people[0],
        other: people[1],
        topic: Topic::Fact("weather".into()),
    })
    .unwrap();
    assert_eq!(serde_json::to_string(&previous).unwrap(), previous_wire);
    assert_eq!(
        h.get::<Information>().unwrap().interests.get(&people[1]),
        Some(&other)
    );
    assert!(
        !h.get::<Information>()
            .unwrap()
            .inboxes
            .get(&people[1])
            .unwrap()
            .is_empty()
    );
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Ask {
            person: people[0],
            other: people[0],
            topic: Topic::Fact("weather".into())
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    assert_eq!(builder().load(&before, 4).unwrap().save().unwrap(), before);
}
