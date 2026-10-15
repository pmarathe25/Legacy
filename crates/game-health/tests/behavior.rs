mod support;
use game_core::*;
use game_health::*;
use support::*;

#[test]
fn treatment_uses_medicine_and_local_caregiver_time() {
    let mut f = setup();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.other,
            amount: 20,
        })
        .unwrap();
    f.host
        .dispatch(Treat {
            caregiver: f.ruler,
            patient: f.other,
            supplies: f.store,
        })
        .unwrap();
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.store].amount(game_cargo::MEDICINE),
        9
    );
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].health,
        80
    );
    run(&mut f.host, SimTime(HOUR.0));
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].health,
        90
    );
}
#[test]
fn a_patient_leaving_does_not_receive_remote_healing() {
    let mut f = setup();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.other,
            amount: 20,
        })
        .unwrap();
    f.host
        .dispatch(Treat {
            caregiver: f.ruler,
            patient: f.other,
            supplies: f.store,
        })
        .unwrap();
    f.host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(HOUR.0));
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].health,
        80
    );
    assert_eq!(f.host.notices().last().unwrap().data["successful"], false);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
