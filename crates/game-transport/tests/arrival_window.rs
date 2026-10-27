mod support;
use game_core::*;
use game_sim::*;
use game_transport::*;

#[test]
fn optional_window_roundtrips_without_rewriting_existing_journeys() {
    let mut f = support::setup();
    let original = f.host.save().unwrap();
    assert_eq!(arrival_window(&f.host).unwrap(), SimDuration(0));
    assert!(!original.contains("arrival_window"));
    let exact = f
        .host
        .dispatch(Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    f.host
        .dispatch(ConfigureArrivalWindow(SimDuration(15 * MINUTE.0)))
        .unwrap();
    let rounded = f
        .host
        .dispatch(Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    assert_eq!(exact, SimTime(2 * MINUTE.0));
    assert_eq!(rounded, SimTime(15 * MINUTE.0));
    assert_eq!(
        f.host.get::<Actions>().unwrap().0[&f.other].until,
        Some(rounded)
    );
    assert!(
        f.host
            .scheduled()
            .any(|j| j.at == exact && j.priority == 10)
    );
    assert!(
        f.host
            .scheduled()
            .any(|j| j.at == rounded && j.priority == 9)
    );
    let saved = f.host.save().unwrap();
    let mut loaded = support::builder().load(&saved, 4).unwrap();
    assert_eq!(arrival_window(&loaded).unwrap(), SimDuration(15 * MINUTE.0));
    loaded
        .dispatch(ConfigureArrivalWindow(SimDuration(0)))
        .unwrap();
    assert!(loaded.scheduled().any(|j| j.at == rounded));
    let before = loaded.save().unwrap();
    assert!(
        loaded
            .dispatch(ConfigureArrivalWindow(SimDuration(DAY.0 + 1)))
            .is_err()
    );
    assert_eq!(loaded.save().unwrap(), before);
}

#[test]
fn rounded_travel_preserves_interruption_and_zero_length_return() {
    let mut f = support::setup();
    f.host
        .dispatch(ConfigureArrivalWindow(SimDuration(15 * MINUTE.0)))
        .unwrap();
    f.host
        .dispatch(Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    f.host.dispatch(Cancel { person: f.ruler }).unwrap();
    assert_eq!(
        f.host
            .dispatch(Move {
                person: f.ruler,
                destination: f.home
            })
            .unwrap(),
        SimTime(0)
    );
    support::run(&mut f.host, SimTime(0));
    assert_eq!(area(f.host.get().unwrap(), f.ruler).unwrap(), f.home);
    f.host
        .dispatch(Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    support::run(&mut f.host, SimTime(5 * MINUTE.0));
    f.host.dispatch(Cancel { person: f.ruler }).unwrap();
    assert!(matches!(
        f.host.get::<Positions>().unwrap().0[&f.ruler],
        Location::Road {
            elapsed: SimDuration(300_000),
            duration: SimDuration(900_000),
            ..
        }
    ));
    assert_eq!(
        f.host
            .dispatch(Move {
                person: f.ruler,
                destination: f.away
            })
            .unwrap(),
        SimTime(15 * MINUTE.0)
    );
    support::run(&mut f.host, SimTime(15 * MINUTE.0));
    assert_eq!(area(f.host.get().unwrap(), f.ruler).unwrap(), f.away);
}

#[test]
fn overflow_at_a_boundary_rolls_back_departure() {
    let mut f = support::setup();
    f.host
        .dispatch(ConfigureArrivalWindow(SimDuration(15 * MINUTE.0)))
        .unwrap();
    support::run(&mut f.host, SimTime(u64::MAX - 120_000));
    let before = f.host.save().unwrap();
    assert_eq!(
        f.host.dispatch(Move {
            person: f.ruler,
            destination: f.away
        }),
        Err(Error::Overflow)
    );
    assert_eq!(before, f.host.save().unwrap());
}
