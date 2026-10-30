use game_core::*;
use game_information::*;
#[path = "support/validation.rs"]
mod validation;
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
    setup_with(builder())
}
fn setup_with(builder: Builder) -> (Host, [PersonId; 3], [AreaId; 2]) {
    let mut h = builder.build(39, 1).unwrap();
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
fn outgoing_question_window_rotates_among_local_peers() {
    let (mut host, [asker, _, _], [area, _]) = setup();
    for index in 0..8 {
        let person = host
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        host.dispatch(game_transport::Place { person, area })
            .unwrap();
        host.dispatch(game_social_life::Register {
            person,
            name: format!("Additional person {index}"),
        })
        .unwrap();
    }
    host.dispatch(Want {
        person: asker,
        topic: Topic::Category("technique:".into()),
        importance: 80,
    })
    .unwrap();
    let collect = |host: &mut Host| -> std::collections::BTreeSet<_> {
        host.dispatch(game_choice::Collect { person: asker })
            .unwrap()
            .into_iter()
            .filter_map(|offer| {
                offer
                    .key
                    .starts_with("information:ask:")
                    .then_some(offer.key)
            })
            .collect()
    };
    let first = collect(&mut host);
    assert_eq!(first.len(), 4);
    run(&mut host, 60);
    let next = collect(&mut host);
    assert_eq!(next.len(), 4);
    assert_ne!(first, next);
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct DraftQuestions(PersonId);
impl Request for DraftQuestions {
    const KEY: &'static str = "test.information-draft-questions";
    type Response = Vec<game_choice::Proposal>;
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct ScopedQuestions {
    person: PersonId,
    fast: bool,
    fanout: bool,
}
impl Request for ScopedQuestions {
    const KEY: &'static str = "test.information-scoped-questions";
    type Response = Vec<game_choice::Proposal>;
}
fn draft_builder() -> Builder {
    let mut builder = builder();
    builder.plugin("information-draft-test", 1).unwrap();
    builder
        .request::<DraftQuestions>(|tx, request| {
            let mut offers = None;
            let draft = tx.draft(game_choice::Offers::planning(false), |child| {
                offers = Some(child.call(game_choice::Collect { person: request.0 })?);
                Ok(())
            });
            require(draft.accepted, "information draft rejected")?;
            offers.ok_or_else(|| Error::Missing("draft offers".into()))
        })
        .unwrap();
    builder
        .request::<ScopedQuestions>(|tx, request| {
            let mut offers = None;
            let draft = tx.draft(
                game_choice::Offers::planning_tier(request.fanout, request.fast),
                |child| {
                    offers = Some(child.call(game_choice::Collect {
                        person: request.person,
                    })?);
                    Ok(())
                },
            );
            require(draft.accepted, "scoped information draft rejected")?;
            offers.ok_or_else(|| Error::Missing("scoped information offers".into()))
        })
        .unwrap();
    builder
}

#[test]
fn fast_questions_answer_inbox_but_defer_outgoing_discovery() {
    let (mut host, [asker, respondent, _], _) = setup_with(draft_builder());
    let topic = Topic::Category("technique:".into());
    conclude(
        &mut host,
        respondent,
        "technique:oven",
        serde_json::json!("clay"),
    );
    host.dispatch(Want {
        person: respondent,
        topic: topic.clone(),
        importance: 80,
    })
    .unwrap();
    host.dispatch(Ask {
        person: asker,
        other: respondent,
        topic,
    })
    .unwrap();
    for fanout in [false, true] {
        let full = host
            .dispatch(ScopedQuestions {
                person: respondent,
                fast: false,
                fanout,
            })
            .unwrap();
        let fast = host
            .dispatch(ScopedQuestions {
                person: respondent,
                fast: true,
                fanout,
            })
            .unwrap();
        assert!(full.iter().any(|p| p.key.starts_with("information:ask:")));
        assert!(
            fast.iter().all(|p| !p.key.starts_with("information:ask:")
                && !p.key.starts_with("information:read:"))
        );
        for prefix in ["information:answer:", "information:decline:"] {
            let expected = full.iter().find(|p| p.key.starts_with(prefix)).unwrap();
            assert_eq!(
                fast.iter().find(|p| p.key.starts_with(prefix)),
                Some(expected)
            );
        }
        assert_eq!(
            full,
            host.dispatch(game_choice::Collect { person: respondent })
                .unwrap()
        );
    }
    let save = host.save().unwrap();
    let mut loaded = draft_builder().load(&save, 4).unwrap();
    let scoped = ScopedQuestions {
        person: respondent,
        fast: true,
        fanout: true,
    };
    assert_eq!(
        host.dispatch(scoped.clone()).unwrap(),
        loaded.dispatch(scoped).unwrap()
    );
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn drafted_questions_recompute_importance_and_match_after_reload() {
    let (mut host, [asker, _, _], _) = setup_with(draft_builder());
    let topic = Topic::Category("technique:".into());
    let want = |importance| Want {
        person: asker,
        topic: topic.clone(),
        importance,
    };
    host.dispatch(want(10)).unwrap();
    let cold = host.dispatch(DraftQuestions(asker)).unwrap();
    let warm = host.dispatch(DraftQuestions(asker)).unwrap();
    assert_eq!(cold, warm);
    assert_eq!(
        cold,
        host.dispatch(game_choice::Collect { person: asker })
            .unwrap()
    );
    host.dispatch(want(90)).unwrap();
    let updated = host.dispatch(DraftQuestions(asker)).unwrap();
    let question = |offers: &[game_choice::Proposal]| {
        offers
            .iter()
            .find(|p| p.key.starts_with("information:ask:"))
            .unwrap()
            .clone()
    };
    let old = question(&cold);
    let new = question(&updated);
    assert_eq!(old.key, new.key);
    assert_eq!(old.request, new.request);
    assert_eq!(old.motive.goal, 10);
    assert_eq!(new.motive.goal, 90);
    assert_eq!(new.motive.id, old.motive.id);
    let checkpoint = host.save().unwrap();
    let mut loaded = draft_builder().load(&checkpoint, 4).unwrap();
    assert_eq!(
        host.dispatch(DraftQuestions(asker)).unwrap(),
        loaded.dispatch(DraftQuestions(asker)).unwrap()
    );
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn answer_offer_reacts_to_metadata_only_personal_knowledge_updates() {
    let (mut host, [asker, respondent, _], _) = setup();
    for (key, confidence) in [("technique:a", 50), ("technique:b", 80)] {
        host.dispatch(game_knowledge::Conclude {
            person: respondent,
            key: key.into(),
            value: serde_json::json!("known"),
            confidence,
        })
        .unwrap();
    }
    host.dispatch(Ask {
        person: asker,
        other: respondent,
        topic: Topic::Category("technique:".into()),
    })
    .unwrap();
    let answer_key = |host: &mut Host| {
        let offers = host
            .dispatch(game_choice::Collect { person: respondent })
            .unwrap();
        let request = &offers
            .iter()
            .find(|offer| offer.key.starts_with("information:answer:"))
            .unwrap()
            .request;
        let undertaking =
            game_choice::inspect_frame_request::<game_choice::Undertake>(request).unwrap();
        game_choice::inspect_frame_request::<Answer>(&undertaking.request)
            .unwrap()
            .key
    };
    assert_eq!(answer_key(&mut host).as_deref(), Some("technique:b"));
    let before = host.get::<game_knowledge::Knowledge>().unwrap().0[&respondent].clone();
    let mut fact = before.get("technique:a").unwrap().to_owned();
    fact.confidence = 90;
    host.dispatch(game_knowledge::Receive {
        person: respondent,
        key: "technique:a".into(),
        fact,
    })
    .unwrap();
    let after = &host.get::<game_knowledge::Knowledge>().unwrap().0[&respondent];
    assert!(before.same_namespace(after, "technique"));
    assert_eq!(answer_key(&mut host).as_deref(), Some("technique:a"));
    let checkpoint = host.save().unwrap();
    let mut loaded = builder().load(&checkpoint, 4).unwrap();
    assert_eq!(answer_key(&mut host).as_deref(), Some("technique:a"));
    assert_eq!(answer_key(&mut loaded).as_deref(), Some("technique:a"));
    assert_eq!(host.save().unwrap(), loaded.save().unwrap());
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
    let (mut h, [reader, author, _], _) = setup_with(draft_builder());
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
    for fast in [false, true] {
        let scoped = h
            .dispatch(ScopedQuestions {
                person: reader,
                fast,
                fanout: true,
            })
            .unwrap();
        let reads = scoped
            .iter()
            .filter(|o| o.key.starts_with("information:read:"))
            .count();
        assert_eq!(reads, if fast { 0 } else { 3 });
    }
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
    let mut loaded = draft_builder().load(&h.save().unwrap(), 4).unwrap();
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

#[test]
fn answer_ranges_match_full_memory_search_after_evidence_changes_and_loading() {
    let (mut h, [asker, respondent, _], [area, _]) = setup();
    let good = game_cargo::FOOD;
    for i in 0..256 {
        conclude(
            &mut h,
            respondent,
            &format!("memory:{i:04}"),
            serde_json::json!({"history": i}),
        );
    }
    for (key, value, confidence) in [
        ("store:10", serde_json::json!({good.0.to_string(): 9}), 60),
        ("store:11", serde_json::json!({good.0.to_string(): 7}), 80),
        ("store:12", serde_json::json!({good.0.to_string(): 4}), 80),
        ("store:bad", serde_json::json!({"invalid": "stock"}), 100),
        (
            "store-location:10",
            serde_json::json!({good.0.to_string(): 99}),
            100,
        ),
        (
            "patch:10",
            serde_json::json!({"area": area, "good": good, "stock": 5, "capacity": 10, "renew": 1}),
            80,
        ),
        ("technique:bake", serde_json::json!("bread"), 70),
        (
            "technique:baker",
            serde_json::json!("unrelated exact fact"),
            90,
        ),
    ] {
        h.dispatch(game_knowledge::Conclude {
            person: respondent,
            key: key.into(),
            value,
            confidence,
        })
        .unwrap();
    }
    let topics = [
        Topic::Supplies(good),
        Topic::Supplies(GoodId(9999)),
        Topic::Category("store:".into()),
        Topic::Category("memory:".into()),
        Topic::Category("absent:".into()),
        Topic::Fact("technique:bake".into()),
    ];
    let questions: Vec<_> = topics
        .into_iter()
        .map(|topic| {
            let id = h
                .dispatch(Ask {
                    person: asker,
                    other: respondent,
                    topic: topic.clone(),
                })
                .unwrap();
            (id, topic)
        })
        .collect();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for round in 0..3 {
        for host in [&mut h, &mut loaded] {
            if round > 0 {
                run(host, round);
                conclude(
                    host,
                    respondent,
                    "store:10",
                    serde_json::json!({good.0.to_string(): if round == 1 { 3 } else { 0 }}),
                );
                if round == 2 {
                    conclude(
                        host,
                        respondent,
                        "patch:10",
                        serde_json::json!("forgotten location"),
                    );
                }
            }
            let offers = host
                .dispatch(game_choice::Collect { person: respondent })
                .unwrap();
            for (id, topic) in &questions {
                let facts = &host.get::<game_knowledge::Knowledge>().unwrap().0[&respondent];
                // Independent reference scans every remembered fact, including unrelated history.
                let expected = facts
                    .iter()
                    .filter(|(key, fact)| topic.matches(key, fact.value))
                    .max_by_key(|(key, fact)| {
                        (fact.observed, fact.confidence, std::cmp::Reverse(*key))
                    })
                    .map(|(key, _)| key.to_string());
                let offered = offers
                    .iter()
                    .find(|o| o.key == format!("information:answer:{id}"));
                let actual = offered.map(|o| serde_json::to_value(&o.request).unwrap()["payload"]["request"]["payload"]["key"].as_str().unwrap().to_owned());
                assert_eq!(actual, expected, "topic {topic:?}, round {round}");
            }
        }
        assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    }
}
