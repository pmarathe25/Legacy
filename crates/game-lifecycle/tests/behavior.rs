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
            attendant: f.other,
            area: f.home,
        })
        .unwrap();
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&child].born,
        SimTime(30_000)
    );
    assert_eq!(
        f.host.get::<Population>().unwrap().families[&child].attendant,
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
            attendant: f.other,
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

#[test]
fn reproductive_aging_follows_inherited_lifespan_without_forbidding_late_partnerships() {
    let f = setup();
    let mut body = f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other];
    let adult = SimDuration(18 * 365 * DAY.0);
    assert!(!fertile(&body, adult, SimTime(17 * 365 * DAY.0)));
    assert!(fertile(&body, adult, SimTime(30 * 365 * DAY.0)));
    assert!(!fertile(&body, adult, SimTime(60 * 365 * DAY.0)));
    body.life_limit = Some(SimTime(160 * 365 * DAY.0));
    assert!(fertile(&body, adult, SimTime(60 * 365 * DAY.0)));
    body.alive = false;
    assert!(!fertile(&body, adult, SimTime(60 * 365 * DAY.0)));
}

#[test]
fn travelling_at_the_due_time_does_not_erase_a_pregnancy() {
    let mut f = setup();
    run(&mut f.host, SimTime(18 * 365 * DAY.0));
    let id = f
        .host
        .dispatch(Conceive {
            mother: f.other,
            father: f.ruler,
            attendant: f.other,
        })
        .unwrap();
    let due = f.host.get::<Population>().unwrap().pregnancies[&id].due;
    run(&mut f.host, SimTime(due.0 - MINUTE.0));
    f.host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, due);
    assert_eq!(f.host.get::<Population>().unwrap().pregnancies.len(), 1);
    assert!(f.host.get::<Population>().unwrap().families.is_empty());
    let save = f.host.save().unwrap();
    f.host = builder().load(&save, 2).unwrap();
    run(&mut f.host, due.after(SimDuration(3 * MINUTE.0)).unwrap());
    assert!(f.host.get::<Population>().unwrap().pregnancies.is_empty());
    assert_eq!(f.host.get::<Population>().unwrap().families.len(), 1);
}

#[test]
fn birth_provenance_records_a_present_attendant_when_a_parent_has_died() {
    let mut f = setup();
    run(&mut f.host, SimTime(DAY.0 * 365 * 18));
    let id = f
        .host
        .dispatch(Conceive {
            mother: f.other,
            father: f.ruler,
            attendant: f.ruler,
        })
        .unwrap();
    let due = f.host.get::<Population>().unwrap().pregnancies[&id].due;
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    let mut loaded = builder().load(&f.host.save().unwrap(), 4).unwrap();
    for host in [&mut f.host, &mut loaded] {
        run(host, due);
        let family = host
            .get::<Population>()
            .unwrap()
            .families
            .values()
            .next()
            .unwrap();
        assert_eq!(family.father, f.ruler);
        assert_eq!(family.attendant, f.other);
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}
