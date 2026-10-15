mod support;
use game_core::*;
use game_missions::*;
use support::*;

#[test]
fn a_party_returns_to_a_world_that_continued_running() {
    let mut f = setup();
    f.host.dispatch(game_physiology::Start).unwrap();
    let id = f
        .host
        .dispatch(Prepare {
            leader: f.ruler,
            members: std::collections::BTreeSet::from([f.ruler, f.other]),
            destination: f.away,
        })
        .unwrap();
    assert!(
        f.host
            .dispatch(Depart {
                person: f.ruler,
                mission: id
            })
            .is_err()
    );
    f.host
        .dispatch(Join {
            person: f.other,
            mission: id,
        })
        .unwrap();
    f.host
        .dispatch(Depart {
            person: f.ruler,
            mission: id,
        })
        .unwrap();
    run(&mut f.host, SimTime(120_000));
    assert_eq!(
        f.host.get::<Missions>().unwrap().0[&id].phase,
        Phase::AtSite
    );
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.ruler].hunger,
        2
    );
    f.host
        .dispatch(Return {
            person: f.ruler,
            mission: id,
        })
        .unwrap();
    run(&mut f.host, SimTime(240_000));
    assert_eq!(
        f.host.get::<Missions>().unwrap().0[&id].phase,
        Phase::Complete
    );
    assert_eq!(
        game_transport::area(f.host.get().unwrap(), f.ruler).unwrap(),
        f.home
    );
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.ruler].hunger,
        4
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
