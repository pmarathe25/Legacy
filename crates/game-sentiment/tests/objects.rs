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
    game_relationships::plugin(&mut b).unwrap();
    game_items::plugin(&mut b).unwrap();
    game_sentiment::plugin(&mut b).unwrap();
    b
}
fn setup() -> (Host, PersonId, PersonId, InventoryId, InventoryId) {
    let mut h = builder().build(1, 1).unwrap();
    h.advance(SimTime(24 * 365 * DAY.0), 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let mut people = vec![];
    for _ in 0..2 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        let inv = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(p),
                capacity: 1000,
            })
            .unwrap();
        h.dispatch(game_physiology::Express { person: p }).unwrap();
        people.push((p, inv));
    }
    (h, people[0].0, people[1].0, people[0].1, people[1].1)
}
#[test]
fn identity_follows_the_specific_unit_and_memories_survive_transfer_and_destruction() {
    let (mut h, a, b, first, second) = setup();
    let good = GoodId(442);
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good,
        quantity: 2,
    })
    .unwrap();
    let item = h
        .dispatch(game_items::Mark {
            person: a,
            inventory: first,
            good,
            label: "Mother's coat".into(),
        })
        .unwrap();
    h.dispatch(game_sentiment::Experience {
        person: a,
        target: game_sentiment::Target::Object(item),
        intensity: 200,
        reason: "a family memory".into(),
    })
    .unwrap();
    // Generic stock operations take the plain unit first.
    h.dispatch(game_cargo::Transfer {
        from: first,
        to: second,
        good,
        quantity: 1,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_items::Items>().unwrap().objects[&item].inventory,
        Some(first)
    );
    h.dispatch(game_items::Move {
        person: a,
        item,
        destination: second,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_items::Items>().unwrap().objects[&item].inventory,
        Some(second)
    );
    let key = game_sentiment::Target::Object(item).key();
    assert_eq!(
        h.get::<game_sentiment::Sentiment>().unwrap().people[&a][&key].value,
        200
    );
    assert!(h.get::<game_sentiment::Sentiment>().unwrap().people[&b][&key].value > 0);
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        host.dispatch(game_cargo::Consume {
            inventory: second,
            good,
            quantity: 2,
        })
        .unwrap();
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        h.get::<game_items::Items>().unwrap().objects[&item].inventory,
        None
    );
    assert_eq!(
        h.get::<game_sentiment::Sentiment>().unwrap().people[&a][&key].value,
        200
    );
    assert!(h.get::<game_vitality::Vitality>().unwrap().people[&b].stress > 0);
}
#[test]
fn a_failed_specific_transfer_cannot_duplicate_the_object_or_its_stock() {
    let (mut h, a, _, first, second) = setup();
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: GoodId(442),
        quantity: 1,
    })
    .unwrap();
    let item = h
        .dispatch(game_items::Mark {
            person: a,
            inventory: first,
            good: GoodId(442),
            label: "keepsake".into(),
        })
        .unwrap();
    let other = h
        .dispatch(game_land::CreateArea {
            name: "distant".into(),
        })
        .unwrap();
    let remote = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(other),
            capacity: 1000,
        })
        .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_items::Move {
            person: a,
            item,
            destination: remote
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(game_items::Move {
        person: a,
        item,
        destination: second,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&second].amount(GoodId(442)),
        1
    );
}
#[test]
fn distant_destruction_does_not_inform_an_absent_attached_character() {
    let (mut h, a, _, first, _) = setup();
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: GoodId(442),
        quantity: 1,
    })
    .unwrap();
    let item = h
        .dispatch(game_items::Mark {
            person: a,
            inventory: first,
            good: GoodId(442),
            label: "keepsake".into(),
        })
        .unwrap();
    h.dispatch(game_sentiment::Experience {
        person: a,
        target: game_sentiment::Target::Object(item),
        intensity: 200,
        reason: "cherished".into(),
    })
    .unwrap();
    let area = match h.get::<game_transport::Positions>().unwrap().0[&a] {
        game_transport::Location::At(a) => a,
        _ => unreachable!(),
    };
    let bin = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_items::Move {
        person: a,
        item,
        destination: bin,
    })
    .unwrap();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "away".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: away,
        duration: MINUTE,
    })
    .unwrap();
    let arrive = h
        .dispatch(game_transport::Move {
            person: a,
            destination: away,
        })
        .unwrap();
    while h.advance(arrive, 1000).unwrap().stop != Stop::Target {}
    h.dispatch(game_cargo::Consume {
        inventory: bin,
        good: GoodId(442),
        quantity: 1,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_sentiment::Sentiment>()
            .unwrap()
            .witnessed_losses,
        0
    );
}
