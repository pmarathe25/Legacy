mod support;
use game_core::*;
use game_physiology::*;
use support::*;

#[test]
fn actual_food_relief_and_inaccessible_abundance() {
    let mut f = setup();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 35));
    let before = f.host.get::<Bodies>().unwrap().people[&f.other].hunger;
    assert!(
        f.host
            .dispatch(Eat {
                person: f.other,
                inventory: f.store
            })
            .is_err()
    );
    assert_eq!(
        f.host.get::<Bodies>().unwrap().people[&f.other].hunger,
        before
    );
    f.host
        .dispatch(Eat {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.store), 990);
    assert_eq!(f.host.get::<Bodies>().unwrap().people[&f.ruler].hunger, 0);
    run(&mut f.host, SimTime(MINUTE.0 * 220));
    assert!(!f.host.get::<Bodies>().unwrap().people[&f.other].alive);
    assert!(f.host.get::<Bodies>().unwrap().people[&f.guardian].alive);
}
#[test]
fn rest_takes_time_and_a_dead_person_cannot_travel() {
    let mut f = setup();
    f.host.dispatch(Start).unwrap();
    f.host
        .dispatch(Strain {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    f.host.dispatch(Rest { person: f.ruler }).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 2));
    assert_eq!(f.host.get::<Bodies>().unwrap().people[&f.ruler].fatigue, 92);
    f.host
        .dispatch(Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    assert!(f.host.get::<game_sim::Actions>().unwrap().0.is_empty());
    assert!(
        f.host
            .dispatch(game_transport::Move {
                person: f.ruler,
                destination: f.away
            })
            .is_err()
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
