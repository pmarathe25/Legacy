use game_core::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_agreements::plugin(&mut b).unwrap();
    game_demand::plugin(&mut b).unwrap();
    b.resource::<game_lineage::Lineage>().unwrap();
    game_items::plugin(&mut b).unwrap();
    game_sentiment::plugin(&mut b).unwrap();
    game_exchange::plugin(&mut b).unwrap();
    b
}
fn setup() -> (Host, PersonId, PersonId, InventoryId, InventoryId) {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "meeting place".into(),
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
                capacity: 10000,
            })
            .unwrap();
        people.push((p, inv));
    }
    (h, people[0].0, people[1].0, people[0].1, people[1].1)
}
#[test]
fn unequal_terms_transfer_atomically_and_teach_only_personal_acceptance() {
    let (mut h, a, b, first, second) = setup();
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: game_cargo::TIMBER,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: second,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    let id = h
        .dispatch(game_exchange::Trade {
            first: a,
            second: b,
            offered: game_cargo::TIMBER,
            requested: game_cargo::FOOD,
            offered_quantity: 4,
            requested_quantity: 2,
        })
        .unwrap();
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&first].amount(game_cargo::TIMBER),
        96
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&first].amount(game_cargo::FOOD),
        2
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&second].amount(game_cargo::TIMBER),
        4
    );
    let demand = h.get::<game_demand::Demand>().unwrap();
    assert_eq!(demand.people[&a].goods[&game_cargo::TIMBER][&b].count, 1);
    assert!(!demand.people[&a].goods.contains_key(&game_cargo::FOOD));
    assert!(
        game_demand::confidence(&demand.people[&a], game_cargo::TIMBER, SimTime(0))
            > game_demand::confidence(&demand.people[&a], game_cargo::TIMBER, SimTime(365 * DAY.0))
    );
    let before = h.save().unwrap();
    h.dispatch(game_demand::ObserveSettlement {
        observer: a,
        agreement: id,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_demand::Demand>().unwrap().people[&a].goods[&game_cargo::TIMBER][&b].count,
        1
    );
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
    assert!(!before.is_empty());
}
#[test]
fn unacceptable_terms_leave_both_sides_unchanged() {
    let (mut h, a, b, first, second) = setup();
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: game_cargo::TIMBER,
        quantity: 1,
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: second,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_exchange::Trade {
            first: a,
            second: b,
            offered: game_cargo::TIMBER,
            requested: game_cargo::FOOD,
            offered_quantity: 1,
            requested_quantity: 100
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    assert!(h.get::<game_demand::Demand>().unwrap().people.is_empty());
}
#[test]
fn outsiders_cannot_acquire_private_acceptance_evidence() {
    let (mut h, a, b, first, second) = setup();
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: game_cargo::TIMBER,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: second,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    let id = h
        .dispatch(game_exchange::Trade {
            first: a,
            second: b,
            offered: game_cargo::TIMBER,
            requested: game_cargo::FOOD,
            offered_quantity: 4,
            requested_quantity: 2,
        })
        .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_demand::ObserveSettlement {
            observer: PersonId(99999),
            agreement: id
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
}
#[test]
fn learned_acceptance_can_motivate_an_intermediate_good_for_another_exchange() {
    fn trial(learn: bool) -> bool {
        let (mut h, a, b, first, second) = setup();
        h.dispatch(game_cargo::Deposit {
            inventory: first,
            good: game_cargo::FOOD,
            quantity: 100,
        })
        .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory: second,
            good: game_cargo::TIMBER,
            quantity: 100,
        })
        .unwrap();
        if learn {
            h.dispatch(game_exchange::Trade {
                first: a,
                second: b,
                offered: game_cargo::FOOD,
                requested: game_cargo::TIMBER,
                offered_quantity: 4,
                requested_quantity: 2,
            })
            .unwrap();
        } else {
            // Matched possessions without ever observing somebody accept this good.
            h.dispatch(game_cargo::Consume {
                inventory: first,
                good: game_cargo::FOOD,
                quantity: 4,
            })
            .unwrap();
            h.dispatch(game_cargo::Deposit {
                inventory: first,
                good: game_cargo::TIMBER,
                quantity: 2,
            })
            .unwrap();
        }
        let area = match h.get::<game_transport::Positions>().unwrap().0[&a] {
            game_transport::Location::At(a) => a,
            _ => unreachable!(),
        };
        let c = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: c, area })
            .unwrap();
        let third = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(c),
                capacity: 10000,
            })
            .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory: third,
            good: game_cargo::FOOD,
            quantity: 100,
        })
        .unwrap();
        h.dispatch(game_exchange::Trade {
            first: a,
            second: c,
            offered: game_cargo::TIMBER,
            requested: game_cargo::FOOD,
            offered_quantity: 1,
            requested_quantity: 1,
        })
        .is_ok()
    }
    assert!(!trial(false));
    assert!(trial(true));
}

#[test]
fn attachment_changes_a_trade_choice_but_a_plain_substitute_can_still_be_sold() {
    let (mut h, a, b, first, second) = setup();
    let coat = GoodId(442);
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: coat,
        quantity: 1,
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: second,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    let offer = game_exchange::Trade {
        first: a,
        second: b,
        offered: coat,
        requested: game_cargo::FOOD,
        offered_quantity: 1,
        requested_quantity: 2,
    };
    let saved = h.save().unwrap();
    let mut indifferent = builder().load(&saved, 1).unwrap();
    assert!(indifferent.dispatch(offer.clone()).is_ok());
    let item = h
        .dispatch(game_items::Mark {
            person: a,
            inventory: first,
            good: coat,
            label: "The coat I kept".into(),
        })
        .unwrap();
    h.dispatch(game_sentiment::Experience {
        person: a,
        target: game_sentiment::Target::Object(item),
        intensity: 200,
        reason: "a reminder of a loved one".into(),
    })
    .unwrap();
    assert!(h.dispatch(offer.clone()).is_err());
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: coat,
        quantity: 1,
    })
    .unwrap();
    assert!(h.dispatch(offer).is_ok());
    assert_eq!(
        h.get::<game_items::Items>().unwrap().objects[&item].inventory,
        Some(first)
    );
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Meet(PersonId, PersonId);
impl Request for Meet {
    const KEY: &'static str = "test.meet";
    type Response = ();
}
#[test]
fn autonomous_offers_revalue_stock_and_personal_acceptance_at_the_next_encounter() {
    let mut b = builder();
    b.request::<Meet>(|tx, r| {
        tx.emit(
            game_social_life::Encounter {
                first: r.0,
                second: r.1,
            },
            vec![r.0, r.1],
        )
    })
    .unwrap();
    let (base, a, other, first, second) = setup();
    let mut h = b.load(&base.save().unwrap(), 1).unwrap();
    for (inventory, good) in [(first, game_cargo::TIMBER), (second, game_cargo::FOOD)] {
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 100,
        })
        .unwrap();
    }
    h.dispatch(game_exchange::Enable).unwrap();
    h.dispatch(Meet(a, other)).unwrap();
    let cargo = h.get::<game_cargo::Cargo>().unwrap();
    assert_eq!(cargo.0[&first].amount(game_cargo::TIMBER), 98);
    assert_eq!(cargo.0[&first].amount(game_cargo::FOOD), 8);
    h.advance(SimTime(DAY.0), 1000).unwrap();
    h.dispatch(Meet(a, other)).unwrap();
    // The first exchange supplied new stock and acceptance evidence. The same
    // person now buys back timber rather than repeating the old offer.
    assert_eq!(h.get::<game_exchange::Exchange>().unwrap().completed, 2);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&first].amount(game_cargo::TIMBER),
        100
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&first].amount(game_cargo::FOOD),
        6
    );
}

#[test]
fn offer_shortcuts_preserve_refusals_and_allow_custom_trade_checks() {
    let (base, a, b, first, second) = setup();
    let saved = base.save().unwrap();
    for custom in [false, true] {
        let mut build = builder();
        build
            .request::<Meet>(|tx, r| {
                tx.emit(
                    game_social_life::Encounter {
                        first: r.0,
                        second: r.1,
                    },
                    vec![r.0, r.1],
                )
            })
            .unwrap();
        if custom {
            build
                .before::<game_exchange::Trade>("test.reconsider", |tx, r| {
                    tx.call(game_relationships::SetRelationship {
                        observer: r.first,
                        other: r.second,
                        relationship: Default::default(),
                    })
                })
                .unwrap();
        }
        let mut h = build.load(&saved, 1).unwrap();
        for (inventory, good) in [(first, game_cargo::TIMBER), (second, game_cargo::FOOD)] {
            h.dispatch(game_cargo::Deposit {
                inventory,
                good,
                quantity: 100,
            })
            .unwrap();
        }
        h.dispatch(game_relationships::SetRelationship {
            observer: a,
            other: b,
            relationship: game_relationships::Relationship {
                fear: 100,
                ..Default::default()
            },
        })
        .unwrap();
        h.dispatch(game_exchange::Enable).unwrap();
        h.dispatch(Meet(a, b)).unwrap();
        assert_eq!(
            h.get::<game_exchange::Exchange>().unwrap().completed,
            u64::from(custom)
        );
        let held = h.get::<game_cargo::Cargo>().unwrap().0[&first].amount(game_cargo::FOOD);
        assert_eq!(held, if custom { 8 } else { 0 });
    }
}
