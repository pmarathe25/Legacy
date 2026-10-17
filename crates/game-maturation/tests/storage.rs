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
    game_environment::plugin(&mut b).unwrap();
    game_preservation::plugin(&mut b).unwrap();
    game_food_storage::plugin(&mut b).unwrap();
    game_maturation::plugin(&mut b).unwrap();
    b
}
fn run(h: &mut Host, at: SimTime) {
    while h.advance(at, 1000).unwrap().stop != Stop::Target {}
}
fn setup() -> (Host, AreaId, PersonId, InventoryId) {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let a = h
        .dispatch(game_land::CreateArea {
            name: "food yard".into(),
        })
        .unwrap();
    h.dispatch(game_environment::Configure(game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth: 20,
            rain: 2,
        }],
    }))
    .unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area: a })
        .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 10000,
        })
        .unwrap();
    h.dispatch(game_food_storage::Enable).unwrap();
    h.dispatch(game_preservation::Enable).unwrap();
    h.dispatch(game_maturation::Enable).unwrap();
    (h, a, p, pack)
}
#[test]
fn cold_storage_rescales_remaining_life_without_rejuvenating_old_food() {
    let (mut h, a, _, pack) = setup();
    let good = GoodId(410);
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good,
        quantity: 20,
    })
    .unwrap();
    let original =
        h.get::<game_preservation::Preservation>().unwrap().batches[&pack][&good][0].expires;
    run(&mut h, SimTime(DAY.0));
    h.dispatch(game_environment::ConfigureArea {
        area: a,
        climate: game_environment::Climate {
            seasons: vec![game_environment::Season {
                duration: DAY,
                warmth: 0,
                rain: 0,
            }],
        },
    })
    .unwrap();
    h.dispatch(game_preservation::Refresh { inventory: pack })
        .unwrap();
    let cold = h.get::<game_preservation::Preservation>().unwrap().batches[&pack][&good][0].expires;
    assert_eq!(cold.0, h.now().0 + 8 * (original.0 - h.now().0));
    h.dispatch(game_environment::ConfigureArea {
        area: a,
        climate: game_environment::Climate {
            seasons: vec![game_environment::Season {
                duration: DAY,
                warmth: 20,
                rain: 0,
            }],
        },
    })
    .unwrap();
    h.dispatch(game_preservation::Refresh { inventory: pack })
        .unwrap();
    assert_eq!(
        h.get::<game_preservation::Preservation>().unwrap().batches[&pack][&good][0].expires,
        original
    );
    run(&mut h, original);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(good),
        0
    );
    assert_eq!(
        h.get::<game_preservation::Preservation>().unwrap().spoiled,
        20
    );
}
#[test]
fn fermentation_continues_after_transfer_and_save_without_blocking_the_character() {
    let (mut h, a, p, pack) = setup();
    let prepared = GoodId(431);
    let food = GoodId(412);
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: prepared,
        quantity: 20,
    })
    .unwrap();
    assert!(!h.get::<Actions>().unwrap().0.contains_key(&p));
    run(&mut h, SimTime(5 * DAY.0));
    let bin = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(a),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_cargo::Transfer {
        from: pack,
        to: bin,
        good: prepared,
        quantity: 20,
    })
    .unwrap();
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    run(&mut h, SimTime(12 * DAY.0));
    run(&mut loaded, SimTime(12 * DAY.0));
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&bin].amount(prepared),
        0
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&bin].amount(food),
        20
    );
    assert_eq!(h.get::<game_maturation::Maturation>().unwrap().finished, 20);
}
#[test]
fn unsuitable_temperature_loses_the_prepared_batch() {
    let (mut h, a, _, pack) = setup();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(431),
        quantity: 20,
    })
    .unwrap();
    h.dispatch(game_environment::ConfigureArea {
        area: a,
        climate: game_environment::Climate {
            seasons: vec![game_environment::Season {
                duration: DAY,
                warmth: 45,
                rain: 0,
            }],
        },
    })
    .unwrap();
    run(&mut h, SimTime(12 * DAY.0));
    assert_eq!(h.get::<game_maturation::Maturation>().unwrap().failed, 20);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(GoodId(412)),
        0
    );
}
#[test]
fn building_and_handling_require_labor_and_failed_requests_roll_back() {
    let (mut h, _, p, pack) = setup();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(400),
        quantity: 1,
    })
    .unwrap();
    let done = h
        .dispatch(game_food_storage::Build {
            person: p,
            supplies: pack,
            kit: GoodId(400),
        })
        .unwrap();
    assert!(
        h.get::<game_food_storage::Stores>()
            .unwrap()
            .sites
            .is_empty()
    );
    run(&mut h, done);
    let bin = *h
        .get::<game_food_storage::Stores>()
        .unwrap()
        .sites
        .keys()
        .next()
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(410),
        quantity: 20,
    })
    .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_food_storage::PutAway {
            person: p,
            transfer: game_cargo::Transfer {
                from: pack,
                to: bin,
                good: GoodId(410),
                quantity: 100000
            }
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    let done = h
        .dispatch(game_food_storage::PutAway {
            person: p,
            transfer: game_cargo::Transfer {
                from: pack,
                to: bin,
                good: GoodId(410),
                quantity: 10,
            },
        })
        .unwrap();
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&bin].amount(GoodId(410)),
        0
    );
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 2).unwrap();
    run(&mut h, done);
    run(&mut loaded, done);
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&bin].amount(GoodId(410)),
        10
    );
}
