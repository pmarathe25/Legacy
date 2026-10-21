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
            .dispatch(Decide {
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

#[test]
fn a_recorded_practice_does_not_prevent_a_custodian_withdrawing_consent() {
    let mut f = setup();
    let institution = f
        .host
        .dispatch(Found(Institution {
            name: "neighbors".into(),
            jurisdiction: f.home,
            members: [f.ruler, f.other].into(),
            procedure: Procedure::Personal,
            offices: Default::default(),
            rules: vec![],
        }))
        .unwrap();
    f.host
        .dispatch(game_property::Permit {
            issuer: f.ruler,
            person: f.other,
            inventory: f.store,
            allow: true,
        })
        .unwrap();
    let terms = AccessRule {
        custodian: f.ruler,
        inventory: f.store,
        users: [f.other].into(),
    };
    f.host
        .dispatch(RecordPractice {
            institution,
            terms: terms.clone(),
        })
        .unwrap();
    f.host
        .dispatch(game_property::Permit {
            issuer: f.ruler,
            person: f.other,
            inventory: f.store,
            allow: false,
        })
        .unwrap();
    assert!(
        !f.host
            .get::<game_property::Property>()
            .unwrap()
            .allowed(f.other, f.store)
    );
    assert_eq!(
        f.host.get::<Institutions>().unwrap().groups[&institution].rules,
        vec![terms]
    );
    let save = f.host.save().unwrap();
    assert_eq!(builder().load(&save, 2).unwrap().save().unwrap(), save);
}

#[test]
fn negotiated_unequal_influence_controls_real_access_and_survives_loading() {
    let mut f = setup();
    let institution = f
        .host
        .dispatch(Found(Institution {
            name: "shared store".into(),
            jurisdiction: f.home,
            members: [f.ruler, f.other, f.guardian].into(),
            procedure: Procedure::Consultation(game_deliberation::DecisionRule {
                influence: [(f.ruler, 2), (f.other, 7), (f.guardian, 1)].into(),
                required_support: 8,
            }),
            offices: Default::default(),
            rules: vec![],
        }))
        .unwrap();
    let proposal = f
        .host
        .dispatch(Propose {
            person: f.other,
            institution,
            terms: AccessRule {
                custodian: f.ruler,
                inventory: f.store,
                users: [f.other].into(),
            },
        })
        .unwrap();
    let mut loaded = builder().load(&f.host.save().unwrap(), 4).unwrap();
    for h in [&mut f.host, &mut loaded] {
        assert!(
            !h.dispatch(Vote {
                person: f.other,
                proposal,
                support: true
            })
            .unwrap()
        );
        assert!(
            !h.get::<game_property::Property>()
                .unwrap()
                .allowed(f.other, f.store)
        );
        assert!(
            h.dispatch(Vote {
                person: f.guardian,
                proposal,
                support: true
            })
            .unwrap()
        );
        assert!(
            h.get::<game_property::Property>()
                .unwrap()
                .allowed(f.other, f.store)
        );
        // Losing a contributor does not silently lower the negotiated threshold.
        h.dispatch(Leave {
            institution,
            person: f.other,
        })
        .unwrap();
        let Procedure::Consultation(rule) =
            &h.get::<Institutions>().unwrap().groups[&institution].procedure
        else {
            panic!()
        };
        assert_eq!(rule.required_support, 8);
        assert_eq!(rule.total(), 3);
        assert!(!passed(
            &h.get::<Institutions>().unwrap().groups[&institution].procedure,
            &[f.ruler, f.guardian].into(),
            &[(f.ruler, true), (f.guardian, true)].into()
        ));
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}
