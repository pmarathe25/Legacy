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
#[test]
fn twenty_thousand_deaths_commit_in_stable_order_without_truncating_the_phase() {
    let mut host = builder().build(9, 4).unwrap();
    host.dispatch(Configure(Rules {
        deprivation: 0,
        harm_per_tick: 100,
        ..Default::default()
    }))
    .unwrap();
    for _ in 0..20_000 {
        host.dispatch(Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    }
    host.dispatch(Start).unwrap();
    run(&mut host, SimTime(MINUTE.0));
    assert!(
        host.get::<Bodies>()
            .unwrap()
            .people
            .values()
            .all(|p| !p.alive && p.health == 0)
    );
    let notices: Vec<_> = host.notices().collect();
    assert_eq!(notices.len(), 1024);
    assert_eq!(notices.last().unwrap().sequence, 20_000);
    assert!(
        notices
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    let saved = host.save().unwrap();
    let loaded = builder().load(&saved, 1).unwrap();
    assert_eq!(loaded.save().unwrap(), saved);
}
