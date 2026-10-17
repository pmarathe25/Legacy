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
fn a_voluntary_visit_uses_real_travel_and_returns_after_a_layover() {
    let (mut h, p, home, away) = journey();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    assert!(game_transport::area(h.get().unwrap(), p).is_err());
    run(&mut h, MINUTE.0);
    assert_eq!(game_transport::area(h.get().unwrap(), p).unwrap(), away);
    assert!(
        h.get::<game_knowledge::Knowledge>().unwrap().0[&p]
            .contains_key(game_knowledge::roads_key(away).as_str())
    );
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, 4 * HOUR.0 + MINUTE.0);
        assert!(host.dispatch(Consider { person: p }).unwrap());
        run(host, 4 * HOUR.0 + 2 * MINUTE.0);
        assert_eq!(game_transport::area(host.get().unwrap(), p).unwrap(), home);
        host.dispatch(Consider { person: p }).unwrap();
        assert_eq!(host.get::<Wayfaring>().unwrap().completed, 1);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn cautious_people_and_player_control_do_not_start_implicit_trips() {
    let (mut h, p, _, _) = journey();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("risk-tolerance".into(), 0)].into(),
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
