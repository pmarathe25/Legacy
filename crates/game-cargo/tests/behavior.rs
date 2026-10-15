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
#[test]
fn a_large_reservation_book_keeps_cached_capacity_atomic_and_validated_on_load() {
    let mut f = setup();
    let inventory = f
        .host
        .dispatch(Create {
            custody: Custody::Area(f.home),
            capacity: 4096,
        })
        .unwrap();
    let tokens: Vec<_> = (0..4096)
        .map(|_| {
            f.host
                .dispatch(ReserveSpace {
                    inventory,
                    good: FOOD,
                    quantity: 1,
                })
                .unwrap()
        })
        .collect();
    let saved = f.host.save().unwrap();
    assert_eq!(
        f.host.get::<Cargo>().unwrap().0[&inventory].used().unwrap(),
        4096
    );
    assert!(
        f.host
            .dispatch(ReserveSpace {
                inventory,
                good: FOOD,
                quantity: 1
            })
            .is_err()
    );
    assert!(
        f.host
            .dispatch(DepositReserved {
                inventory,
                reservation: tokens[0],
                good: FOOD,
                quantity: 2
            })
            .is_err()
    );
    assert_eq!(f.host.save().unwrap(), saved);
    let mut loaded = builder().load(&saved, 4).unwrap();
    for (index, reservation) in tokens.into_iter().enumerate() {
        if index % 2 == 0 {
            loaded
                .dispatch(ReleaseSpace {
                    inventory,
                    reservation,
                })
                .unwrap();
        } else {
            loaded
                .dispatch(DepositReserved {
                    inventory,
                    reservation,
                    good: FOOD,
                    quantity: 1,
                })
                .unwrap();
        }
    }
    let inv = &loaded.get::<Cargo>().unwrap().0[&inventory];
    assert_eq!(inv.amount(FOOD), 2048);
    assert_eq!(inv.used().unwrap(), 2048);
    assert_eq!(inv.reserved_mass, 0);
    assert!(inv.reserved.is_empty());
    let mut bad: serde_json::Value = serde_json::from_str(&saved).unwrap();
    bad["resources"]["cargo.inventories"][inventory.0.to_string()]["reserved_mass"] = 1.into();
    assert!(builder().load(&bad.to_string(), 1).is_err());
}

#[test]
fn carrier_lookup_and_drop_ignore_unrelated_inventories_and_rebuild_on_loading() {
    let mut f = setup();
    for _ in 0..1000 {
        f.host
            .dispatch(Create {
                custody: Custody::Area(f.home),
                capacity: 100,
            })
            .unwrap();
    }
    let mut carried = vec![];
    for _ in 0..100 {
        let id = f
            .host
            .dispatch(Create {
                custody: Custody::Carried(f.other),
                capacity: 100,
            })
            .unwrap();
        f.host
            .dispatch(Deposit {
                inventory: id,
                good: FOOD,
                quantity: 10,
            })
            .unwrap();
        carried.push(id);
    }
    assert_eq!(
        f.host
            .get::<Cargo>()
            .unwrap()
            .carried(f.other)
            .collect::<Vec<_>>(),
        carried
    );
    let mut loaded = builder().load(&f.host.save().unwrap(), 4).unwrap();
    for host in [&mut f.host, &mut loaded] {
        host.dispatch(DropCarried { person: f.other }).unwrap();
        assert!(
            host.get::<Cargo>()
                .unwrap()
                .carried(f.other)
                .next()
                .is_none()
        );
        for id in &carried {
            let inventory = &host.get::<Cargo>().unwrap().0[id];
            assert_eq!(inventory.custody, Custody::Area(f.home));
            assert_eq!(inventory.amount(FOOD), 10);
        }
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}
