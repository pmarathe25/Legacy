use game_core::*;
use game_credit::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_lineage::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_agreements::plugin(&mut b).unwrap();
    game_exchange::plugin(&mut b).unwrap();
    game_choice::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b
}
fn setup() -> (Host, Vec<PersonId>, Vec<InventoryId>, AreaId) {
    let mut h = builder().build(9, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Market".into(),
        })
        .unwrap();
    let mut people = vec![];
    let mut packs = vec![];
    for _ in 0..3 {
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
            good: game_cargo::FOOD,
            quantity: 100,
        })
        .unwrap();
        people.push(person);
        packs.push(inventory);
    }
    for observer in &people {
        for other in &people {
            if observer != other {
                h.dispatch(game_relationships::SetRelationship {
                    observer: *observer,
                    other: *other,
                    relationship: game_relationships::Relationship {
                        trust: 100,
                        ..Default::default()
                    },
                })
                .unwrap();
            }
        }
    }
    (h, people, packs, area)
}
#[test]
fn an_accepted_promise_can_circulate_without_revealing_it_to_an_absent_debtor() {
    let (mut h, p, _, area) = setup();
    let agreement = h
        .dispatch(Borrow {
            borrower: p[0],
            lender: p[1],
            good: game_cargo::FOOD,
            quantity: 20,
            repayment: 40,
            due: SimTime(7 * DAY.0),
        })
        .unwrap();
    let far = h
        .dispatch(game_land::CreateArea {
            name: "Distant farm".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: far,
        duration: MINUTE,
    })
    .unwrap();
    let at = h
        .dispatch(game_transport::Move {
            person: p[0],
            destination: far,
        })
        .unwrap();
    while h.advance(at, 32).unwrap().stop == Stop::Budget {}
    assert!(
        h.dispatch(Endorse {
            holder: p[1],
            recipient: p[2],
            agreement,
            term: 1,
            good: game_cargo::FOOD,
            quantity: 10
        })
        .unwrap()
    );
    assert_eq!(
        h.get::<game_agreements::Agreements>().unwrap().0[&agreement].terms[1].creditor,
        p[2]
    );
    assert_eq!(
        h.get::<Credit>().unwrap().promises[&p[0]][&agreement][1].creditor,
        p[1]
    );
    let at = h
        .dispatch(game_transport::Move {
            person: p[0],
            destination: area,
        })
        .unwrap();
    while h.advance(at, 32).unwrap().stop == Stop::Budget {}
    assert!(
        h.dispatch(TellPromise {
            speaker: p[2],
            listener: p[0],
            agreement
        })
        .unwrap()
    );
    assert_eq!(
        h.get::<Credit>().unwrap().promises[&p[0]][&agreement][1].creditor,
        p[2]
    );
    let saved = h.save().unwrap();
    assert_eq!(saved, builder().load(&saved, 4).unwrap().save().unwrap());
}
#[test]
fn extreme_terms_and_unacceptable_prices_roll_back_without_creating_debts() {
    let (mut h, p, _, _) = setup();
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(Borrow {
            borrower: p[0],
            lender: p[1],
            good: game_cargo::FOOD,
            quantity: u64::MAX,
            repayment: u64::MAX,
            due: SimTime(DAY.0)
        })
        .is_err()
    );
    assert_eq!(saved, h.save().unwrap());
    let agreement = h
        .dispatch(Borrow {
            borrower: p[0],
            lender: p[1],
            good: game_cargo::FOOD,
            quantity: 20,
            repayment: 40,
            due: SimTime(7 * DAY.0),
        })
        .unwrap();
    let saved = h.save().unwrap();
    assert!(
        !h.dispatch(Endorse {
            holder: p[1],
            recipient: p[2],
            agreement,
            term: 1,
            good: game_cargo::FOOD,
            quantity: 100
        })
        .unwrap()
    );
    let before: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let after: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    assert!(before["resources"] == after["resources"]);
}
