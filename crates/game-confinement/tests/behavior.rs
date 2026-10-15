mod support;
use game_confinement::*;
use game_core::*;
use support::*;

#[test]
fn a_destroyed_guardian_recovers_in_the_same_prison() {
    let mut f = setup();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.guardian,
            amount: 100,
        })
        .unwrap();
    f.host
        .dispatch(Confine {
            custodian: f.ruler,
            person: f.guardian,
            consent: false,
            resistance: 50,
        })
        .unwrap();
    f.host.dispatch(game_physiology::Start).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 10));
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.guardian].health,
        20
    );
    assert!(
        f.host
            .dispatch(game_transport::Move {
                person: f.guardian,
                destination: f.away
            })
            .is_err()
    );
    assert_eq!(
        game_transport::area(f.host.get().unwrap(), f.guardian).unwrap(),
        f.home
    );
    assert!(
        f.host
            .get::<Confinement>()
            .unwrap()
            .0
            .contains_key(&f.guardian)
    );
    assert!(
        !f.host
            .dispatch(Escape {
                person: f.guardian,
                capability: 10
            })
            .unwrap()
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
