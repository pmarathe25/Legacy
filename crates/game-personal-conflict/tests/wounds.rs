use game_personal_conflict::wounds::{Injury, Remedy, Rules, Wounds};

#[test]
fn wounds_accumulate_capped_harm_and_recover_without_assigning_blame() {
    let initial = Wounds {
        rules: Rules {
            minimum_harm: 2,
            recovery_ticks: 3,
            force_loss_per_severity: 1,
        },
        cases: vec![],
        remedies: vec![],
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let first = initial
                    .injured(
                        &[Injury { person: 1, harm: 4 }, Injury { person: 2, harm: 1 }],
                        2,
                    )
                    .unwrap();
                assert_eq!(first.cases.len(), 1);
                assert_eq!(first.cases[0].heals_at, 5);
                let second = first.injured(&[Injury { person: 1, harm: 3 }], 3).unwrap();
                assert_eq!(second.cases[0].severity, 7);
                assert_eq!(second.available_force(1, 10), 3);
                assert_eq!(second.available_force(2, 10), 10);
                assert_eq!(second.cases[0].heals_at, 6);
                assert!(second.validate(3, &|id| id <= 2).is_ok());
                assert!(second.injured(&[], 6).unwrap().cases.is_empty());
                assert!(
                    second
                        .injured(
                            &[Injury { person: 1, harm: 2 }, Injury { person: 1, harm: 2 }],
                            4
                        )
                        .is_err()
                );
                second
            })
    };
    assert_eq!(run(1), run(4));
    assert!(initial.cases.is_empty());
    let mut overflow = initial.clone();
    overflow.rules.recovery_ticks = u64::MAX;
    assert!(
        overflow
            .injured(&[Injury { person: 1, harm: 2 }], 1)
            .is_err()
    );
    assert!(overflow.cases.is_empty());
    assert!(overflow.injured(&[], u64::MAX).is_ok());
}

#[test]
fn only_accepted_personal_treatments_change_wounds() {
    let initial = Wounds {
        rules: Rules {
            minimum_harm: 1,
            recovery_ticks: 3,
            force_loss_per_severity: 1,
        },
        cases: vec![],
        remedies: vec![Remedy {
            good: 1,
            quantity: 2,
            severity_relief: 3,
        }],
    };
    let wounded = initial
        .injured(
            &[Injury { person: 1, harm: 5 }, Injury { person: 2, harm: 2 }],
            0,
        )
        .unwrap();
    let treated = wounded.treated(&[(1, 1)]).unwrap();
    assert_eq!(treated.cases[0].severity, 2);
    assert_eq!(treated.cases[1].severity, 2);
    assert!(wounded.treated(&[(1, 2)]).is_err());
    assert!(wounded.treated(&[(1, 1), (1, 1)]).is_err());
    assert!(
        treated
            .treated(&[(1, 1)])
            .unwrap()
            .cases
            .iter()
            .all(|case| case.person != 1)
    );
    assert_eq!(wounded.cases[0].severity, 5);
}
