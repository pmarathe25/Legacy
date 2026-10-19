mod support;
use game_core::*;
use game_justice::*;
use support::*;

#[test]
fn a_judgment_waits_for_a_willing_local_enforcer() {
    let mut f = setup();
    let case = f
        .host
        .dispatch(File(Case {
            claimant: f.ruler,
            defendant: f.other,
            evidence: Evidence {
                taking: true,
                consent: false,
                necessity: false,
                credible: true,
            },
            transfer: game_cargo::Transfer {
                from: f.store,
                to: f.pack,
                good: game_cargo::FOOD,
                quantity: 10,
            },
            remedy: None,
            enforced: false,
        }))
        .unwrap();
    f.host
        .dispatch(KnowPrecedent {
            person: f.ruler,
            precedent: Precedent {
                id: CaseId(99),
                evidence: Evidence {
                    taking: true,
                    consent: false,
                    necessity: false,
                    credible: true,
                },
                remedy: Remedy::Return,
            },
        })
        .unwrap();
    assert_eq!(
        f.host
            .dispatch(Decide {
                judge: f.ruler,
                case
            })
            .unwrap(),
        Remedy::Return
    );
    assert_eq!(food(&f.host, f.pack), 0);
    assert!(
        f.host
            .dispatch(Enforce {
                person: f.ruler,
                case,
                willing: false
            })
            .is_err()
    );
    f.host
        .dispatch(Enforce {
            person: f.ruler,
            case,
            willing: true,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.pack), 10);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}

#[test]
fn local_witnesses_remember_an_incident_but_filing_is_a_separate_choice() {
    let base = setup();
    let mut h = builder().load(&base.host.save().unwrap(), 1).unwrap();
    let arrives = h
        .dispatch(game_transport::Move {
            person: base.guardian,
            destination: base.away,
        })
        .unwrap();
    run(&mut h, arrives);
    let destination = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(base.other),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_property::Take {
        actor: base.other,
        unauthorized: true,
        transfer: game_cargo::Transfer {
            from: base.store,
            to: destination,
            good: game_cargo::FOOD,
            quantity: 10,
        },
    })
    .unwrap();
    assert!(h.get::<Justice>().unwrap().cases.is_empty());
    assert!(
        h.get::<Justice>()
            .unwrap()
            .incidents
            .get(&base.ruler)
            .unwrap()
            .contains_key(&base.other)
    );
    assert!(
        !h.get::<Justice>()
            .unwrap()
            .incidents
            .contains_key(&base.guardian)
    );
    let case = h
        .dispatch(Prosecute {
            claimant: base.ruler,
            defendant: base.other,
        })
        .unwrap();
    assert_eq!(
        h.get::<Justice>().unwrap().cases[&case].transfer.from,
        destination
    );
}

#[test]
fn different_personal_standards_can_produce_opposite_judgments_of_identical_evidence() {
    let mut b = builder();
    game_traits::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    let mut f = setup_with(b);
    f.host
        .dispatch(game_traits::Install(
            game_traits::load(&mut game_content::Store::standard()).unwrap(),
        ))
        .unwrap();
    let case = Case {
        claimant: f.ruler,
        defendant: f.other,
        evidence: Evidence {
            taking: true,
            consent: false,
            necessity: false,
            credible: true,
        },
        transfer: game_cargo::Transfer {
            from: f.store,
            to: f.pack,
            good: game_cargo::FOOD,
            quantity: 10,
        },
        remedy: None,
        enforced: false,
    };
    for (honesty, altruism, expected) in [(0, 1000, Remedy::None), (1000, 0, Remedy::Return)] {
        f.host
            .dispatch(game_traits::Set {
                person: f.ruler,
                profile: [("honesty".into(), honesty), ("altruism".into(), altruism)].into(),
            })
            .unwrap();
        let id = f.host.dispatch(File(case.clone())).unwrap();
        assert_eq!(
            f.host
                .dispatch(Decide {
                    judge: f.ruler,
                    case: id
                })
                .unwrap(),
            expected
        );
    }
}
