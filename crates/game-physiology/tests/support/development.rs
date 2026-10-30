use game_core::*;
use game_physiology::*;
use game_sim::*;
use std::collections::BTreeMap;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    b
}
fn setup() -> Host {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.dispatch(Configure(Rules {
        tick: HOUR,
        hunger_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    h.advance(SimTime(24 * 365 * DAY.0), 1).unwrap();
    h
}
fn person(h: &mut Host, traits: &[(&str, i32)]) -> PersonId {
    let p = h
        .dispatch(Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: traits.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    })
    .unwrap();
    h.dispatch(Express { person: p }).unwrap();
    p
}
#[test]
fn constitution_longevity_and_robustness_change_real_bodies_without_repeat_expression_healing() {
    let mut h = setup();
    let ordinary = person(&mut h, &[]);
    let sturdy = person(
        &mut h,
        &[
            ("constitution", 3000),
            ("longevity", 2000),
            ("robustness", 3000),
        ],
    );
    let b = h.get::<Bodies>().unwrap();
    assert_eq!(b.people[&ordinary].health, 100);
    assert_eq!(b.people[&sturdy].health, 200);
    assert_eq!(
        b.people[&sturdy].life_limit,
        Some(SimTime(120 * 365 * DAY.0))
    );
    h.dispatch(Damage {
        person: sturdy,
        amount: 20,
    })
    .unwrap();
    assert_eq!(h.get::<Bodies>().unwrap().people[&sturdy].health, 190);
    h.dispatch(Express { person: sturdy }).unwrap();
    assert_eq!(h.get::<Bodies>().unwrap().people[&sturdy].health, 190);
    h.dispatch(Heal {
        person: sturdy,
        amount: 500,
    })
    .unwrap();
    assert_eq!(h.get::<Bodies>().unwrap().people[&sturdy].health, 200);
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
}
#[test]
fn expressed_speed_changes_arrival_and_endurance_retains_fractional_exertion() {
    let mut h = setup();
    let ordinary = person(&mut h, &[]);
    let swift = person(&mut h, &[("speed", 3000), ("endurance", 4000)]);
    let a = h
        .dispatch(game_land::CreateArea { name: "a".into() })
        .unwrap();
    let b = h
        .dispatch(game_land::CreateArea { name: "b".into() })
        .unwrap();
    h.dispatch(game_land::Link {
        from: a,
        to: b,
        duration: SimDuration(120_000),
    })
    .unwrap();
    for p in [ordinary, swift] {
        h.dispatch(game_transport::Place { person: p, area: a })
            .unwrap();
    }
    let now = h.now();
    assert_eq!(
        h.dispatch(game_transport::Move {
            person: ordinary,
            destination: b
        })
        .unwrap(),
        SimTime(now.0 + 120_000)
    );
    assert_eq!(
        h.dispatch(game_transport::Move {
            person: swift,
            destination: b
        })
        .unwrap(),
        SimTime(now.0 + 60_000)
    );
    h.dispatch(Start).unwrap();
    while h.advance(SimTime(now.0 + 4 * HOUR.0), 1000).unwrap().stop != Stop::Target {}
    let bodies = h.get::<Bodies>().unwrap();
    assert!(bodies.people[&swift].fatigue < bodies.people[&ordinary].fatigue);
    assert!(bodies.people[&swift].fatigue_fraction > 0);
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 2).unwrap().save().unwrap(), saved);
}
#[test]
fn aging_changes_expressed_capability_and_health_but_preserves_underlying_traits() {
    let mut h = setup();
    h.dispatch(Configure(Rules {
        tick: SimDuration(365 * DAY.0),
        hunger_per_tick: 0,
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let p = person(&mut h, &[("strength", 3000), ("intelligence", 3000)]);
    let profile = h.get::<game_traits::Traits>().unwrap().people[&p].clone();
    h.dispatch(Start).unwrap();
    while h.advance(SimTime(70 * 365 * DAY.0), 1000).unwrap().stop != Stop::Target {}
    let life = h.get::<game_vitality::Vitality>().unwrap().people[&p];
    assert!(life.ability(3000, "strength", h.now()) < life.ability(3000, "intelligence", h.now()));
    assert!(
        h.get::<Bodies>().unwrap().people[&p]
            .maximum_health
            .unwrap()
            < 100
    );
    assert_eq!(h.get::<game_traits::Traits>().unwrap().people[&p], profile);
    assert_eq!(
        profile,
        BTreeMap::from([("strength".into(), 3000), ("intelligence".into(), 3000)])
    );
}
#[test]
fn newborns_grow_into_adult_bodies_while_preserving_health_and_genetic_potential() {
    let mut h = setup();
    h.dispatch(Configure(Rules {
        tick: SimDuration(365 * DAY.0),
        hunger_per_tick: 0,
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let born = h.now();
    let infant = h
        .dispatch(Create {
            kind: Kind::Mortal,
            born,
        })
        .unwrap();
    h.dispatch(Express { person: infant }).unwrap();
    assert_eq!(h.get::<Bodies>().unwrap().people[&infant].health, 22);
    h.dispatch(Start).unwrap();
    while h
        .advance(SimTime(born.0 + 20 * 365 * DAY.0), 1000)
        .unwrap()
        .stop
        != Stop::Target
    {}
    let body = h.get::<Bodies>().unwrap().people[&infant];
    assert_eq!(body.health, 100);
    assert_eq!(body.maximum_health, Some(100));
    assert_eq!(
        h.get::<game_traits::Traits>()
            .unwrap()
            .ability(infant, "size"),
        1000
    );
}
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct CarePair(Option<(PersonId, PersonId)>);
impl Resource for CarePair {
    const KEY: &'static str = "test.care-pair";
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Pair(PersonId, PersonId);
impl Request for Pair {
    const KEY: &'static str = "test.pair";
    type Response = ();
}
fn carrying_builder() -> Builder {
    let mut b = builder();
    b.resource::<CarePair>().unwrap();
    b.request::<Pair>(|tx, r| {
        tx.edit::<CarePair>()?.0 = Some((r.0, r.1));
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_transport::Departed>("test.carry-dependent", |tx, e| {
        if let Some((carrier, passenger)) = tx.get::<CarePair>()?.0
            && carrier == e.person
        {
            tx.call(game_transport::Carry {
                carrier,
                passenger,
                destination: e.destination,
            })?;
        }
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn an_infant_is_carried_on_a_real_journey_and_stops_and_resumes_with_the_carrier() {
    let mut h = carrying_builder().build(1, 1).unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.advance(SimTime(24 * 365 * DAY.0), 1).unwrap();
    let carrier = h
        .dispatch(Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let infant = h
        .dispatch(Create {
            kind: Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    let a = h
        .dispatch(game_land::CreateArea { name: "a".into() })
        .unwrap();
    let b = h
        .dispatch(game_land::CreateArea { name: "b".into() })
        .unwrap();
    h.dispatch(game_land::Link {
        from: a,
        to: b,
        duration: HOUR,
    })
    .unwrap();
    for person in [carrier, infant] {
        h.dispatch(Express { person }).unwrap();
        h.dispatch(game_transport::Place { person, area: a })
            .unwrap();
    }
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_transport::Move {
            person: infant,
            destination: b
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(Pair(carrier, infant)).unwrap();
    let arrives = h
        .dispatch(game_transport::Move {
            person: carrier,
            destination: b,
        })
        .unwrap();
    h.advance(SimTime(h.now().0 + HOUR.0 / 2), 1000).unwrap();
    h.dispatch(game_transport::Cancel { person: carrier })
        .unwrap();
    assert_eq!(
        h.get::<game_transport::Positions>().unwrap().0[&carrier],
        h.get::<game_transport::Positions>().unwrap().0[&infant]
    );
    let save = h.save().unwrap();
    h = carrying_builder().load(&save, 4).unwrap();
    let resumed = h
        .dispatch(game_transport::Move {
            person: carrier,
            destination: b,
        })
        .unwrap();
    assert_eq!(resumed, arrives);
    h.advance(arrives, 1000).unwrap();
    assert_eq!(game_transport::area(h.get().unwrap(), infant).unwrap(), b);
    assert_eq!(game_transport::area(h.get().unwrap(), carrier).unwrap(), b);
    assert!(h.save().is_ok());
}
