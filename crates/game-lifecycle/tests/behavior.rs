mod support;
use game_core::*;
use game_lifecycle::*;
use support::*;

#[test]
fn birth_is_actual_and_death_conserves_carried_possessions() {
    let mut f = setup();
    f.host
        .dispatch(game_cargo::Transfer {
            from: f.store,
            to: f.pack,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
    f.host
        .dispatch(game_transport::Move {
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(30_000));
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.pack), 10);
    assert!(matches!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.pack].custody,
        game_cargo::Custody::Road {
            elapsed: SimDuration(30_000),
            ..
        }
    ));
    let child = f
        .host
        .dispatch(Birth {
            mother: f.other,
            father: f.guardian,
            caregiver: f.other,
            area: f.home,
        })
        .unwrap();
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&child].born,
        SimTime(30_000)
    );
    assert_eq!(
        f.host.get::<Population>().unwrap().families[&child].caregiver,
        f.other
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
fn a_fathers_death_does_not_cancel_an_existing_pregnancy() {
    let mut f = setup();
    run(&mut f.host, SimTime(DAY.0 * 365 * 18));
    let id = f
        .host
        .dispatch(Conceive {
            mother: f.other,
            father: f.ruler,
            caregiver: f.other,
        })
        .unwrap();
    let due = f.host.get::<Population>().unwrap().pregnancies[&id].due;
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    run(&mut f.host, due);
    let state = f.host.get::<Population>().unwrap();
    assert!(state.pregnancies.is_empty());
    assert_eq!(state.families.len(), 1);
    assert_eq!(state.families.values().next().unwrap().father, f.ruler);
}
