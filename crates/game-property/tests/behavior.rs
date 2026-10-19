mod support;
use game_core::*;
use game_property::*;
use support::*;

#[test]
fn permission_is_explicit_and_theft_is_not_globally_known() {
    let mut f = setup();
    for person in [f.ruler, f.guardian] {
        f.host
            .dispatch(game_transport::Move {
                person,
                destination: f.away,
            })
            .unwrap();
    }
    run(&mut f.host, SimTime(2 * MINUTE.0));
    let dest = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(f.other),
            capacity: 100,
        })
        .unwrap();
    let transfer = game_cargo::Transfer {
        from: f.store,
        to: dest,
        good: game_cargo::FOOD,
        quantity: 10,
    };
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Take {
                actor: f.other,
                transfer: transfer.clone(),
                unauthorized: false
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
    f.host
        .dispatch(Take {
            actor: f.other,
            transfer,
            unauthorized: true,
        })
        .unwrap();
    assert_eq!(food(&f.host, dest), 10);
    let notice = f.host.notices().last().unwrap();
    assert_eq!(notice.witnesses, vec![f.other]);
    assert!(!notice.witnesses.contains(&f.ruler));
}
#[test]
fn conflicting_claims_coexist() {
    let mut f = setup();
    f.host
        .dispatch(ClaimInventory {
            claimant: f.other,
            inventory: f.store,
            recognized_by: std::collections::BTreeSet::from([f.other]),
        })
        .unwrap();
    assert_eq!(f.host.get::<Property>().unwrap().claims[&f.store].len(), 2);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
