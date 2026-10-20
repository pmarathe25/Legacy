mod support;
use game_core::*;
use game_lineage::*;
use support::*;

#[test]
fn no_birth_means_no_successor_and_no_skill_inheritance() {
    let mut f = setup();
    f.host.dispatch(Begin { person: f.ruler }).unwrap();
    f.host
        .dispatch(game_skills::Practice {
            person: f.ruler,
            skill: "farm".into(),
            effort: 100,
            attribution: 1000,
        })
        .unwrap();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    run(&mut f.host, SimTime(DAY.0));
    assert_eq!(current(&f.host).unwrap(), None);
    let child = f
        .host
        .dispatch(game_lifecycle::Birth {
            mother: f.other,
            father: f.guardian,
            attendant: f.other,
            area: f.home,
        })
        .unwrap();
    assert_eq!(current(&f.host).unwrap(), Some(child));
    assert!(
        !f.host
            .get::<game_skills::Skills>()
            .unwrap()
            .levels
            .contains_key(&child)
    );
    assert_eq!(
        f.host
            .get::<Lineage>()
            .unwrap()
            .history
            .values()
            .next()
            .unwrap()
            .skills["farm"],
        100
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
