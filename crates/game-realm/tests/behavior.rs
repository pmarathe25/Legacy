mod support;
use game_realm::*;
use support::*;

#[test]
fn delegation_has_budget_and_revocation_without_remote_transfers() {
    let mut f = setup();
    let institution = f
        .host
        .dispatch(game_institutions::Found(game_institutions::Institution {
            name: "realm".into(),
            jurisdiction: f.home,
            members: std::collections::BTreeSet::from([f.ruler, f.other]),
            procedure: game_institutions::Procedure::Leader(f.ruler),
            offices: std::collections::BTreeMap::from([("ruler".into(), f.ruler)]),
            rules: vec![],
        }))
        .unwrap();
    let mandate = f
        .host
        .dispatch(Delegate {
            issuer: f.ruler,
            institution,
            person: f.other,
            jurisdiction: f.home,
            treasury: f.store,
            budget: 10,
        })
        .unwrap();
    f.host
        .dispatch(Spend {
            person: f.other,
            mandate,
            destination: f.pack,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Spend {
                person: f.other,
                mandate,
                destination: f.pack,
                good: game_cargo::FOOD,
                quantity: 1
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
    f.host
        .dispatch(Revoke {
            issuer: f.ruler,
            mandate,
        })
        .unwrap();
    assert!(!f.host.get::<Realm>().unwrap().0[&mandate].active);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
