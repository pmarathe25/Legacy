mod support;
use game_core::*;
use game_transport::*;
use support::*;

#[test]
fn arrival_is_exact_and_invalid_input_is_inert() {
    let mut f = setup();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Move {
                person: f.ruler,
                destination: AreaId(999)
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
    f.host
        .dispatch(Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(119_999));
    assert!(matches!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::Transit { .. }
    ));
    run(&mut f.host, SimTime(120_000));
    assert_eq!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::At(f.away)
    );
    assert!(
        !f.host
            .get::<game_sim::Actions>()
            .unwrap()
            .0
            .contains_key(&f.ruler)
    );
}
#[test]
fn interrupted_travel_resumes_without_teleporting() {
    let mut f = setup();
    f.host
        .dispatch(Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(30_000));
    f.host.dispatch(Cancel { person: f.ruler }).unwrap();
    assert!(matches!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::Road {
            elapsed: SimDuration(30_000),
            ..
        }
    ));
    f.host
        .dispatch(Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(119_999));
    assert!(matches!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::Transit { .. }
    ));
    run(&mut f.host, SimTime(120_000));
    assert_eq!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::At(f.away)
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}

#[test]
fn an_existing_direct_road_remains_usable_when_an_indirect_route_is_faster() {
    let mut f = setup();
    let shortcut = f
        .host
        .dispatch(game_land::CreateArea {
            name: "shortcut".into(),
        })
        .unwrap();
    for (from, to) in [(f.home, shortcut), (shortcut, f.away)] {
        f.host
            .dispatch(game_land::Link {
                from,
                to,
                duration: SimDuration(1000),
            })
            .unwrap();
    }
    // A one-road movement request traverses the requested road, without choosing
    // another route from world information that the actor may not know.
    let arrives = f
        .host
        .dispatch(Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    assert_eq!(arrives, SimTime(120_000));
    run(&mut f.host, SimTime(2000));
    assert!(matches!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::Transit { .. }
    ));
    run(&mut f.host, arrives);
    assert_eq!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::At(f.away)
    );
}

#[path = "support/arrival_wave.rs"]
mod arrival_wave;

#[path = "support/arrival_window.rs"]
mod arrival_window;
#[path = "support/passengers.rs"]
mod passengers;
