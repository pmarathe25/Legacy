mod support;
use game_core::*;
use game_workshop::*;
use support::*;

fn recipe() -> Recipe {
    Recipe {
        inputs: std::collections::BTreeMap::from([(game_cargo::TIMBER, 5)]),
        output: GoodId(8),
        quantity: 1,
        duration: HOUR,
        skill: "craft".into(),
    }
}
#[test]
fn manufacturing_consumes_real_time_and_excludes_travel() {
    let mut f = setup();
    f.host
        .dispatch(Define {
            name: "tool".into(),
            recipe: recipe(),
        })
        .unwrap();
    f.host
        .dispatch(Craft {
            person: f.ruler,
            recipe: "tool".into(),
            inventory: f.store,
        })
        .unwrap();
    assert!(
        f.host
            .dispatch(game_transport::Move {
                person: f.ruler,
                destination: f.away
            })
            .is_err()
    );
    run(&mut f.host, SimTime(HOUR.0 - 1));
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.store].amount(GoodId(8)),
        0
    );
    run(&mut f.host, SimTime(HOUR.0));
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.store].amount(GoodId(8)),
        1
    );
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.store].amount(game_cargo::TIMBER),
        95
    );
}
#[test]
fn canceled_work_records_waste_and_never_finishes() {
    let mut f = setup();
    f.host
        .dispatch(Define {
            name: "tool".into(),
            recipe: recipe(),
        })
        .unwrap();
    f.host
        .dispatch(Craft {
            person: f.ruler,
            recipe: "tool".into(),
            inventory: f.store,
        })
        .unwrap();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    run(&mut f.host, SimTime(HOUR.0));
    assert_eq!(
        f.host.get::<Workshop>().unwrap().waste[&game_cargo::TIMBER],
        5
    );
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&f.store].amount(GoodId(8)),
        0
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
fn unrelated_deposit_cannot_steal_output_capacity() {
    let mut f = setup();
    let store = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(f.home),
            capacity: 5,
        })
        .unwrap();
    f.host
        .dispatch(game_property::ClaimInventory {
            claimant: f.ruler,
            inventory: store,
            recognized_by: std::collections::BTreeSet::from([f.ruler]),
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Deposit {
            inventory: store,
            good: game_cargo::TIMBER,
            quantity: 5,
        })
        .unwrap();
    f.host
        .dispatch(Define {
            name: "tool".into(),
            recipe: recipe(),
        })
        .unwrap();
    f.host
        .dispatch(Craft {
            person: f.ruler,
            recipe: "tool".into(),
            inventory: store,
        })
        .unwrap();
    assert!(
        f.host
            .dispatch(game_cargo::Deposit {
                inventory: store,
                good: game_cargo::FOOD,
                quantity: 5
            })
            .is_err()
    );
    run(&mut f.host, SimTime(HOUR.0));
    assert_eq!(
        f.host.get::<game_cargo::Cargo>().unwrap().0[&store].amount(GoodId(8)),
        1
    );
}
