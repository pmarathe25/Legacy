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
