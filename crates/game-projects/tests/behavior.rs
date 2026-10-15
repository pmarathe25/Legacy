mod support;
use game_core::*;
use game_projects::*;
use support::*;

#[test]
fn construction_needs_local_inputs_and_time() {
    let mut f = setup();
    let id = f
        .host
        .dispatch(Propose(Project {
            sponsor: f.ruler,
            area: f.home,
            store: f.store,
            materials: std::collections::BTreeMap::from([(game_cargo::TIMBER, 10)]),
            duration: HOUR,
            contributions: std::collections::BTreeMap::new(),
            status: Status::Proposed,
        }))
        .unwrap();
    f.host
        .dispatch(Build {
            person: f.ruler,
            project: id,
        })
        .unwrap();
    assert!(matches!(
        f.host.get::<Projects>().unwrap().0[&id].status,
        Status::Building { .. }
    ));
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.store].amount(game_cargo::TIMBER),
        90
    );
    run(&mut f.host, SimTime(HOUR.0));
    assert_eq!(
        f.host.get::<Projects>().unwrap().0[&id].status,
        Status::Operating
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
