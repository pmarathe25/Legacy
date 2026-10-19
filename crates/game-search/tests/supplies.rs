use game_core::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_choice::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_information::plugin(&mut b).unwrap();
    game_search::plugin(&mut b).unwrap();
    b
}
fn run(h: &mut Host, minutes: u64) {
    while h.advance(SimTime(minutes * MINUTE.0), 512).unwrap().stop == Stop::Budget {}
}
#[test]
fn a_supply_search_can_ask_for_directions_travel_inspect_and_take_finite_goods() {
    let mut h = builder().build(39, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let a = h
        .dispatch(game_land::CreateArea {
            name: "Crossroads".into(),
        })
        .unwrap();
    let b = h
        .dispatch(game_land::CreateArea {
            name: "Store".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: a,
        to: b,
        duration: MINUTE,
    })
    .unwrap();
    let [asker, witness] = [a, b].map(|area| {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(game_social_life::Register {
            person: p,
            name: format!("P{}", p.0),
        })
        .unwrap();
        p
    });
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(asker),
            capacity: 100,
        })
        .unwrap();
    let store = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(b),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::MEDICINE,
        quantity: 7,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person: witness })
        .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: witness,
        inventory: store,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: witness,
        destination: a,
    })
    .unwrap();
    run(&mut h, 1);
    h.dispatch(game_knowledge::ObserveSurroundings { person: witness })
        .unwrap();
    let search = game_search::FindGoods {
        person: asker,
        good: game_cargo::MEDICINE,
        quantity: 3,
    };
    assert_eq!(
        h.dispatch(search.clone()).unwrap(),
        game_choice::Outcome::Performed
    );
    assert!(
        h.get::<game_knowledge::Knowledge>()
            .unwrap()
            .0
            .get(&asker)
            .is_none_or(|facts| !facts.contains_key(game_knowledge::store_key(store).as_str()))
    );
    run(&mut h, 3);
    let question = h.get::<game_information::Information>().unwrap().inboxes[&witness][0].id;
    h.dispatch(game_information::Answer {
        person: witness,
        question,
        key: Some(game_knowledge::store_key(store)),
    })
    .unwrap();
    run(&mut h, 3);
    assert_eq!(
        h.dispatch(search.clone()).unwrap(),
        game_choice::Outcome::Performed
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, 4);
        assert_eq!(game_transport::area(host.get().unwrap(), asker).unwrap(), b);
        assert_eq!(
            host.dispatch(search.clone()).unwrap(),
            game_choice::Outcome::Performed
        );
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(game_cargo::MEDICINE),
            3
        );
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(game_cargo::MEDICINE),
            4
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn unexplored_supplies_use_only_known_roads_and_retain_a_finite_search_trail() {
    let mut h = builder().build(39, 1).unwrap();
    let a = h
        .dispatch(game_land::CreateArea {
            name: "Known".into(),
        })
        .unwrap();
    let b = h
        .dispatch(game_land::CreateArea {
            name: "Frontier".into(),
        })
        .unwrap();
    let hidden = h
        .dispatch(game_land::CreateArea {
            name: "Hidden".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: a,
        to: b,
        duration: MINUTE,
    })
    .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area: a })
        .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person })
        .unwrap();
    let r = game_search::FindGoods {
        person,
        good: game_cargo::MEDICINE,
        quantity: 1,
    };
    assert_eq!(
        h.dispatch(r.clone()).unwrap(),
        game_choice::Outcome::Performed
    );
    run(&mut h, 1);
    h.dispatch(game_knowledge::ObserveSurroundings { person })
        .unwrap();
    assert_eq!(h.dispatch(r).unwrap(), game_choice::Outcome::Reconsider);
    let visited = &h.get::<game_search::GoodsTrails>().unwrap().0[&person][&game_cargo::MEDICINE];
    assert_eq!(visited.len(), 2);
    assert!(!visited.contains(&hidden));
    assert_eq!(game_transport::area(h.get().unwrap(), person).unwrap(), b);
}

#[test]
fn failed_supply_searches_retain_negative_stock_evidence_through_the_common_chooser() {
    let mut h = builder().build(39, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Empty clinic".into(),
        })
        .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(game_cargo::Create {
        custody: game_cargo::Custody::Carried(person),
        capacity: 100,
    })
    .unwrap();
    let store = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::MEDICINE,
        quantity: 3,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person,
        inventory: store,
    })
    .unwrap();
    h.dispatch(game_cargo::Consume {
        inventory: store,
        good: game_cargo::MEDICINE,
        quantity: 3,
    })
    .unwrap();
    h.dispatch(game_information::Want {
        person,
        topic: game_information::Topic::Supplies(game_cargo::MEDICINE),
        importance: 60,
    })
    .unwrap();
    let outcome = h
        .dispatch(game_choice::Run(
            Pending::new(game_search::FindGoods {
                person,
                good: game_cargo::MEDICINE,
                quantity: 3,
            })
            .unwrap(),
        ))
        .unwrap();
    assert_eq!(outcome, game_choice::Outcome::Reconsider);
    let facts = &h.get::<game_knowledge::Knowledge>().unwrap().0[&person];
    let known: game_knowledge::Stock =
        game_knowledge::known(facts, &game_knowledge::store_key(store)).unwrap();
    assert_eq!(known[&game_cargo::MEDICINE], 0);
}
