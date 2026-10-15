mod support;
use game_cargo::*;
use support::*;

#[test]
fn contention_and_failed_capacity_checks_conserve_stock() {
    let mut f = setup();
    let tiny = f
        .host
        .dispatch(Create {
            custody: Custody::Area(f.home),
            capacity: 1,
        })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Transfer {
                from: f.store,
                to: tiny,
                good: FOOD,
                quantity: 10
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
    f.host
        .dispatch(Transfer {
            from: f.store,
            to: f.pack,
            good: FOOD,
            quantity: 1000,
        })
        .unwrap();
    assert!(
        f.host
            .dispatch(Transfer {
                from: f.store,
                to: tiny,
                good: FOOD,
                quantity: 1
            })
            .is_err()
    );
    assert_eq!(food(&f.host, f.store) + food(&f.host, f.pack), 1000);
}
#[test]
fn distant_transfer_is_inert() {
    let mut f = setup();
    let distant = f
        .host
        .dispatch(Create {
            custody: Custody::Area(f.away),
            capacity: 1000,
        })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Transfer {
                from: f.store,
                to: distant,
                good: FOOD,
                quantity: 100
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
#[test]
fn output_space_is_reserved_and_cancellation_releases_it() {
    let mut f = setup();
    let reservation = f
        .host
        .dispatch(ReserveSpace {
            good: FOOD,
            inventory: f.pack,
            quantity: 1000,
        })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Transfer {
                from: f.store,
                to: f.pack,
                good: FOOD,
                quantity: 1
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
    f.host
        .dispatch(DepositReserved {
            inventory: f.pack,
            reservation,
            good: FOOD,
            quantity: 1000,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.pack), 1000);
    assert!(
        f.host.get::<Cargo>().unwrap().0[&f.pack]
            .reserved
            .is_empty()
    );
}
