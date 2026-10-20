mod support;
use game_core::*;
use game_guardians::*;
use support::*;

#[test]
fn guardians_travel_to_actual_successor_and_can_refuse() {
    let mut f = setup();
    f.host
        .dispatch(game_lineage::Begin { person: f.ruler })
        .unwrap();
    f.host.dispatch(Register { person: f.guardian }).unwrap();
    f.host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(120_000));
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    run(&mut f.host, SimTime(120_001));
    let child = f
        .host
        .dispatch(game_lifecycle::Birth {
            mother: f.other,
            father: f.guardian,
            attendant: f.other,
            area: f.away,
        })
        .unwrap();
    assert!(matches!(
        f.host.get::<game_transport::Positions>().unwrap().0[&f.guardian],
        game_transport::Location::Transit { .. }
    ));
    assert_eq!(
        f.host.get::<game_lineage::Lineage>().unwrap().current,
        Some(child)
    );
    run(&mut f.host, SimTime(240_001));
    assert_eq!(
        game_transport::area(f.host.get().unwrap(), f.guardian).unwrap(),
        f.away
    );
    assert!(
        !f.host
            .dispatch(Order {
                guardian: f.guardian,
                destination: f.home,
                danger: 100
            })
            .unwrap()
    );
    assert!(f.host.get::<Guardians>().unwrap().0[&f.guardian].mentoring);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
