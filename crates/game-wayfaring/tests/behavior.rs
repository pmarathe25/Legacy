mod support;
use game_core::*;
use game_wayfaring::*;
use support::*;
fn journey() -> (game_sim::Host, PersonId, AreaId, AreaId) {
    let (mut h, p, _, home) = setup();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Unknown shore".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: home,
        to: away,
        duration: MINUTE,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("risk-tolerance".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person: p })
        .unwrap();
    h.dispatch(Enable).unwrap();
    (h, p, home, away)
}
#[test]
fn a_voluntary_visit_can_return_without_an_imposed_layover() {
    let (mut h, p, home, away) = journey();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    assert!(game_transport::area(h.get().unwrap(), p).is_err());
    run(&mut h, MINUTE.0);
    assert_eq!(game_transport::area(h.get().unwrap(), p).unwrap(), away);
    assert!(
        h.get::<game_knowledge::Knowledge>().unwrap().0[&p]
            .contains_key(game_knowledge::roads_key(away).as_str())
    );
    assert!(h.dispatch(Consider { person: p }).unwrap());
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, 2 * MINUTE.0);
        assert_eq!(game_transport::area(host.get().unwrap(), p).unwrap(), home);
        host.dispatch(Consider { person: p }).unwrap();
        assert_eq!(host.get::<Wayfaring>().unwrap().completed, 1);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn optional_completed_trip_cooldown_survives_reload_and_does_not_block_direct_travel() {
    let (mut h, p, home, away) = journey();
    h.dispatch(ConfigureTripCooldown(SimDuration(4 * 60 * MINUTE.0)))
        .unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, MINUTE.0);
    assert_eq!(game_transport::area(h.get().unwrap(), p).unwrap(), away);
    // The return leg is never delayed by the new-trip cooldown.
    assert!(h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, 2 * MINUTE.0);
    assert_eq!(game_transport::area(h.get().unwrap(), p).unwrap(), home);
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert!(!loaded.dispatch(Consider { person: p }).unwrap());
    let mut direct = builder().load(&saved, 1).unwrap();
    direct
        .dispatch(game_transport::Move {
            person: p,
            destination: away,
        })
        .unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, 2 * MINUTE.0 + 4 * 60 * MINUTE.0);
        assert!(host.dispatch(Consider { person: p }).unwrap());
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn experimental_journey_keeps_its_destination_or_returns_home() {
    let (mut h, p, home, away) = journey();
    let third = h
        .dispatch(game_land::CreateArea {
            name: "Third place".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: away,
        to: third,
        duration: MINUTE,
    })
    .unwrap();
    h.dispatch(ConfigureTripCooldown(SimDuration(4 * 60 * MINUTE.0)))
        .unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, MINUTE.0);
    assert!(h.dispatch(Consider { person: p }).unwrap());
    let location = &h.get::<game_transport::Positions>().unwrap().0[&p];
    assert!(matches!(location, game_transport::Location::Transit { to, .. } if *to == home));
}
#[test]
fn direct_return_home_closes_an_outbound_experimental_journey() {
    let (mut h, p, home, _) = journey();
    h.dispatch(ConfigureTripCooldown(SimDuration(4 * 60 * MINUTE.0)))
        .unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, MINUTE.0);
    h.dispatch(game_transport::Move {
        person: p,
        destination: home,
    })
    .unwrap();
    run(&mut h, 2 * MINUTE.0);
    assert!(!h.get::<Wayfaring>().unwrap().trips.contains_key(&p));
    assert!(!h.dispatch(Consider { person: p }).unwrap());
}
#[test]
fn stale_journey_offer_cannot_bypass_completed_trip_cooldown() {
    let (mut h, p, home, away) = journey();
    h.dispatch(ConfigureTripCooldown(SimDuration(4 * 60 * MINUTE.0)))
        .unwrap();
    let saved_offer = h
        .dispatch(game_choice::Collect { person: p })
        .unwrap()
        .into_iter()
        .find(|offer| offer.key == format!("journey:{}", away.0))
        .unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    run(&mut h, MINUTE.0);
    h.dispatch(game_transport::Move {
        person: p,
        destination: home,
    })
    .unwrap();
    run(&mut h, 2 * MINUTE.0);
    let before = h.save().unwrap();
    assert!(h.dispatch(game_choice::Run(saved_offer.request)).is_err());
    assert_eq!(h.save().unwrap(), before);
}
#[test]
fn personal_aversion_and_player_control_can_suppress_journeys() {
    let (mut h, p, _, _) = journey();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("risk-tolerance".into(), 0)].into(),
    })
    .unwrap();
    h.dispatch(game_relationships::Initialize {
        person: p,
        beliefs: Default::default(),
        values: [("discovery".into(), -100)].into(),
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("risk-tolerance".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_lineage::Begin { person: p }).unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
}
#[test]
fn death_clears_the_trip_and_keeps_actual_visited_places() {
    let (mut h, p, home, _) = journey();
    h.dispatch(Consider { person: p }).unwrap();
    h.dispatch(game_physiology::Damage {
        person: p,
        amount: 100,
    })
    .unwrap();
    assert!(h.get::<Wayfaring>().unwrap().trips.is_empty());
    assert!(h.get::<Wayfaring>().unwrap().visits[&p].contains_key(&home));
    builder().load(&h.save().unwrap(), 4).unwrap();
}
