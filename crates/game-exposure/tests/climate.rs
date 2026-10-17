use game_core::*;
use game_sim::*;
const EPOCH: u64 = 24 * 365 * DAY.0;
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
    game_environment::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_equipment::plugin(&mut b).unwrap();
    game_shelter::plugin(&mut b).unwrap();
    game_exposure::plugin(&mut b).unwrap();
    b
}
fn setup(warmth: i32) -> (Host, AreaId) {
    let mut h = builder().build(1, 1).unwrap();
    h.advance(SimTime(EPOCH), 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.dispatch(game_environment::Configure(game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth,
            rain: 0,
        }],
    }))
    .unwrap();
    let a = h
        .dispatch(game_land::CreateArea {
            name: "outside".into(),
        })
        .unwrap();
    h.dispatch(game_exposure::Enable).unwrap();
    (h, a)
}
fn person(h: &mut Host, a: AreaId) -> (PersonId, InventoryId) {
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_physiology::Express { person: p }).unwrap();
    h.dispatch(game_transport::Place { person: p, area: a })
        .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 10000,
        })
        .unwrap();
    (p, pack)
}
fn run(h: &mut Host, at: SimTime) {
    while h.advance(at, 1000).unwrap().stop != Stop::Target {}
}
#[test]
fn clothes_reduce_cold_injury_and_removal_prevents_heat_stress() {
    let (mut h, a) = setup(-35);
    let (bare, _) = person(&mut h, a);
    let (clothed, pack) = person(&mut h, a);
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(442),
        quantity: 1,
    })
    .unwrap();
    h.dispatch(game_equipment::Equip {
        person: clothed,
        inventory: pack,
        good: GoodId(442),
    })
    .unwrap();
    run(&mut h, SimTime(EPOCH + 2 * DAY.0));
    assert!(
        h.get::<game_physiology::Bodies>().unwrap().people[&bare].health
            < h.get::<game_physiology::Bodies>().unwrap().people[&clothed].health
    );
    let (mut hot, a) = setup(35);
    let (p, pack) = person(&mut hot, a);
    hot.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(442),
        quantity: 1,
    })
    .unwrap();
    hot.dispatch(game_equipment::Equip {
        person: p,
        inventory: pack,
        good: GoodId(442),
    })
    .unwrap();
    run(&mut hot, SimTime(EPOCH + DAY.0));
    let before = hot.get::<game_exposure::Exposure>().unwrap().people[&p].core;
    hot.dispatch(game_equipment::Remove { person: p }).unwrap();
    run(&mut hot, SimTime(EPOCH + 2 * DAY.0));
    assert!(hot.get::<game_exposure::Exposure>().unwrap().people[&p].core < before);
}
#[test]
fn road_travel_has_thermal_exposure_and_roundtrips() {
    let (mut h, a) = setup(-40);
    let (p, _) = person(&mut h, a);
    let b = h
        .dispatch(game_land::CreateArea { name: "far".into() })
        .unwrap();
    h.dispatch(game_land::Link {
        from: a,
        to: b,
        duration: SimDuration(10 * DAY.0),
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: p,
        destination: b,
    })
    .unwrap();
    run(&mut h, SimTime(EPOCH + DAY.0));
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&p].health < 100);
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    assert_eq!(loaded.save().unwrap(), saved);
    run(&mut h, SimTime(EPOCH + 2 * DAY.0));
    run(&mut loaded, SimTime(EPOCH + 2 * DAY.0));
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn real_room_blocks_rain_and_fire_consumes_fuel_then_goes_out() {
    let (mut h, a) = setup(-20);
    let (p, pack) = person(&mut h, a);
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(440),
        quantity: 1,
    })
    .unwrap();
    let done = h
        .dispatch(game_shelter::Build {
            person: p,
            inventory: pack,
            kit: GoodId(440),
        })
        .unwrap();
    run(&mut h, done);
    let room = *h
        .get::<game_shelter::Shelters>()
        .unwrap()
        .rooms
        .keys()
        .next()
        .unwrap();
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(GoodId(440)),
        0
    );
    let arrive = h
        .dispatch(game_transport::Move {
            person: p,
            destination: room,
        })
        .unwrap();
    run(&mut h, arrive);
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(2),
        quantity: 2,
    })
    .unwrap();
    let until = h
        .dispatch(game_shelter::Stoke {
            person: p,
            room,
            inventory: pack,
            fuel: GoodId(2),
            quantity: 2,
        })
        .unwrap();
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(GoodId(2)),
        0
    );
    let climate = &h.get::<game_environment::Regions>().unwrap().0[&room];
    assert_eq!(climate.seasons[0].rain, 0);
    assert_eq!(climate.seasons[0].warmth, -5);
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 2).unwrap();
    run(&mut h, until);
    run(&mut loaded, until);
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert_eq!(
        h.get::<game_environment::Regions>().unwrap().0[&room].seasons[0].warmth,
        -20
    );
}

#[test]
fn an_authored_boreal_winter_costs_fatigue_and_comfort_before_severe_injury() {
    let climate = game_environment::load(&mut game_content::Store::standard(), "boreal").unwrap();
    let winter = climate
        .seasons
        .iter()
        .min_by_key(|s| s.warmth)
        .unwrap()
        .warmth;
    let (mut h, a) = setup(winter);
    let (bare, _) = person(&mut h, a);
    let (clothed, pack) = person(&mut h, a);
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: GoodId(442),
        quantity: 1,
    })
    .unwrap();
    h.dispatch(game_equipment::Equip {
        person: clothed,
        inventory: pack,
        good: GoodId(442),
    })
    .unwrap();
    run(&mut h, SimTime(EPOCH + DAY.0));
    let bodies = h.get::<game_physiology::Bodies>().unwrap();
    assert!(bodies.people[&bare].fatigue > bodies.people[&clothed].fatigue);
    assert!(
        h.get::<game_vitality::Vitality>().unwrap().people[&bare].stress
            > h.get::<game_vitality::Vitality>().unwrap().people[&clothed].stress
    );
    assert!(
        h.get::<game_exposure::Exposure>().unwrap().people[&bare].core
            < h.get::<game_exposure::Exposure>().unwrap().people[&clothed].core
    );
}
