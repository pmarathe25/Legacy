mod support;
use game_core::*;
use game_ecology::*;
use support::*;

#[test]
fn gathering_reserves_stock_and_credits_work_on_completion() {
    let mut f = setup();
    let patch = f
        .host
        .dispatch(Create(Patch {
            area: f.home,
            good: game_cargo::FOOD,
            stock: 10,
            capacity: 20,
            renew: 5,
        }))
        .unwrap();
    f.host
        .dispatch(Gather {
            person: f.ruler,
            patch,
            quantity: 10,
            destination: f.pack,
        })
        .unwrap();
    assert_eq!(
        f.host.get::<Ecology>().unwrap().available(patch).unwrap(),
        0
    );
    assert_eq!(f.host.get::<Ecology>().unwrap().patches[&patch].stock, 10);
    assert!(
        f.host
            .dispatch(Gather {
                person: f.other,
                patch,
                quantity: 1,
                destination: f.store
            })
            .is_err()
    );
    assert_eq!(food(&f.host, f.pack), 0);
    run(&mut f.host, SimTime(300_000));
    assert_eq!(food(&f.host, f.pack), 10);
    assert_eq!(
        f.host.get::<game_skills::Skills>().unwrap().levels[&f.ruler]["forage"],
        5
    );
}
#[test]
fn mortality_releases_gathering_reservations() {
    let mut f = setup();
    let patch = f
        .host
        .dispatch(Create(Patch {
            area: f.home,
            good: game_cargo::FOOD,
            stock: 10,
            capacity: 10,
            renew: 0,
        }))
        .unwrap();
    f.host
        .dispatch(Gather {
            person: f.ruler,
            patch,
            quantity: 10,
            destination: f.pack,
        })
        .unwrap();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    run(&mut f.host, SimTime(300_000));
    assert_eq!(f.host.get::<Ecology>().unwrap().patches[&patch].stock, 10);
    assert_eq!(food(&f.host, f.pack), 0);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}

#[test]
fn local_indexes_and_contested_reservations_rebuild_after_load_and_release_on_death() {
    let mut f = setup();
    let patch = f
        .host
        .dispatch(Create(Patch {
            area: f.home,
            good: game_cargo::FOOD,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    for _ in 0..1000 {
        f.host
            .dispatch(Create(Patch {
                area: f.away,
                good: game_cargo::FOOD,
                stock: 1,
                capacity: 1,
                renew: 0,
            }))
            .unwrap();
    }
    assert_eq!(
        f.host
            .get::<Ecology>()
            .unwrap()
            .in_area(f.home)
            .collect::<Vec<_>>(),
        [patch]
    );
    let mut people = vec![];
    for _ in 0..100 {
        let person = f
            .host
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        f.host
            .dispatch(game_transport::Place {
                person,
                area: f.home,
            })
            .unwrap();
        f.host
            .dispatch(Gather {
                person,
                patch,
                quantity: 1,
                destination: f.pack,
            })
            .unwrap();
        people.push(person);
    }
    assert_eq!(
        f.host.get::<Ecology>().unwrap().available(patch).unwrap(),
        0
    );
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Gather {
                person: f.other,
                patch,
                quantity: 1,
                destination: f.store
            })
            .is_err()
    );
    assert_eq!(f.host.save().unwrap(), before);
    let mut loaded = builder().load(&before, 4).unwrap();
    for host in [&mut f.host, &mut loaded] {
        assert_eq!(
            host.get::<Ecology>()
                .unwrap()
                .in_area(f.home)
                .collect::<Vec<_>>(),
            [patch]
        );
        for person in people.iter().step_by(2) {
            host.dispatch(game_physiology::Damage {
                person: *person,
                amount: 100,
            })
            .unwrap();
        }
        assert_eq!(host.get::<Ecology>().unwrap().available(patch).unwrap(), 50);
        run(host, SimTime(5 * MINUTE.0));
        assert_eq!(host.get::<Ecology>().unwrap().patches[&patch].stock, 50);
        assert_eq!(host.get::<Ecology>().unwrap().available(patch).unwrap(), 50);
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
    let mut corrupt: serde_json::Value = serde_json::from_str(&before).unwrap();
    let key = people[0].0.to_string();
    corrupt["resources"]["ecology.patches"]["gathering"][key]["quantity"] = u64::MAX.into();
    assert!(builder().load(&corrupt.to_string(), 1).is_err());
}
