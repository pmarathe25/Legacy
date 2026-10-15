mod support;
use game_armies::*;
use game_core::*;
use support::*;

#[test]
fn recruitment_can_be_refused_and_deployment_consumes_supplies() {
    let mut f = setup();
    let id = f
        .host
        .dispatch(Create {
            commander: f.ruler,
            supply: f.store,
        })
        .unwrap();
    assert!(
        f.host
            .dispatch(Recruit {
                army: id,
                person: f.other,
                willing: false
            })
            .is_err()
    );
    f.host
        .dispatch(Recruit {
            army: id,
            person: f.other,
            willing: true,
        })
        .unwrap();
    let mission = f
        .host
        .dispatch(Deploy {
            army: id,
            person: f.ruler,
            destination: f.away,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.store), 980);
    run(&mut f.host, SimTime(120_000));
    assert_eq!(
        f.host.get::<game_missions::Missions>().unwrap().0[&mission].phase,
        game_missions::Phase::AtSite
    );
    assert_eq!(f.host.get::<Armies>().unwrap().0[&id].members.len(), 2);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
