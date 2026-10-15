mod support;
use game_institutions::*;
use support::*;

#[test]
fn adopted_proposal_changes_access_and_an_office_can_be_removed() {
    let mut f = setup();
    let institution = f
        .host
        .dispatch(Found(Institution {
            name: "cooperative".into(),
            jurisdiction: f.home,
            members: std::collections::BTreeSet::from([f.ruler, f.other]),
            procedure: Procedure::Leader(f.ruler),
            offices: std::collections::BTreeMap::new(),
            rules: vec![],
        }))
        .unwrap();
    assert!(
        !f.host
            .get::<game_property::Property>()
            .unwrap()
            .allowed(f.other, f.store)
    );
    let proposal = f
        .host
        .dispatch(Propose {
            person: f.other,
            institution,
            terms: AccessRule {
                custodian: f.ruler,
                inventory: f.store,
                users: std::collections::BTreeSet::from([f.other]),
            },
        })
        .unwrap();
    assert!(
        f.host
            .dispatch(Vote {
                person: f.ruler,
                proposal,
                support: true
            })
            .unwrap()
    );
    assert!(
        f.host
            .get::<game_property::Property>()
            .unwrap()
            .allowed(f.other, f.store)
    );
    f.host
        .dispatch(Appoint {
            issuer: f.ruler,
            institution,
            office: "ruler".into(),
            person: Some(f.ruler),
        })
        .unwrap();
    assert!(holds(f.host.get().unwrap(), f.ruler, institution, "ruler"));
    f.host
        .dispatch(Appoint {
            issuer: f.ruler,
            institution,
            office: "ruler".into(),
            person: None,
        })
        .unwrap();
    assert!(!holds(f.host.get().unwrap(), f.ruler, institution, "ruler"));
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
