mod support;
use game_core::*;
use game_logistics::*;
use support::*;

#[test]
fn delivery_moves_real_cargo_only_on_arrival() {
    let mut f = setup();
    let dest = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(f.away),
            capacity: 100,
        })
        .unwrap();
    f.host
        .dispatch(Haul {
            carrier: f.ruler,
            from: f.store,
            to: dest,
            pack: f.pack,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.store), 990);
    assert_eq!(food(&f.host, f.pack), 10);
    assert_eq!(food(&f.host, dest), 0);
    run(&mut f.host, SimTime(119_999));
    assert_eq!(food(&f.host, dest), 0);
    run(&mut f.host, SimTime(120_000));
    assert_eq!(food(&f.host, dest), 10);
    assert_eq!(food(&f.host, f.pack), 0);
    assert!(f.host.get::<Logistics>().unwrap().0.is_empty());
}
#[test]
fn interruption_keeps_loaded_goods() {
    let mut f = setup();
    let dest = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(f.away),
            capacity: 100,
        })
        .unwrap();
    f.host
        .dispatch(Haul {
            carrier: f.ruler,
            from: f.store,
            to: dest,
            pack: f.pack,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
    run(&mut f.host, SimTime(30_000));
    f.host.dispatch(Cancel { carrier: f.ruler }).unwrap();
    run(&mut f.host, SimTime(120_000));
    assert_eq!(food(&f.host, f.pack), 10);
    assert_eq!(food(&f.host, dest), 0);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
