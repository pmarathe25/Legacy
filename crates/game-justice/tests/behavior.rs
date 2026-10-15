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
