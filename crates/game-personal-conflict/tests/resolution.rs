use game_personal_conflict::{Defense, Strike, resolve};

#[test]
fn strikes_are_simultaneous_and_defense_is_individual() {
    let strikes = [
        Strike {
            actor: 1,
            target: 2,
            force: 9,
        },
        Strike {
            actor: 2,
            target: 1,
            force: 7,
        },
        Strike {
            actor: 3,
            target: 2,
            force: 4,
        },
    ];
    let defense = [Defense {
        person: 2,
        reduction: 3,
    }];
    let result = resolve(&strikes, &defense).unwrap();
    assert_eq!(
        result
            .impacts
            .iter()
            .map(|row| (row.target, row.harm))
            .collect::<Vec<_>>(),
        vec![(1, 7), (2, 7)]
    );
    assert_eq!(
        result
            .events
            .iter()
            .find(|row| row.actor == 1)
            .unwrap()
            .defended,
        3
    );
    assert_eq!(
        resolve(&[strikes[0], strikes[0]], &defense).unwrap_err(),
        "a person cannot strike twice in one batch"
    );
}

#[test]
fn independent_and_concentrated_strikes_are_worker_deterministic() {
    for shared in [false, true] {
        let strikes: Vec<_> = (1..=10_000)
            .map(|actor| Strike {
                actor,
                target: if shared { 20_001 } else { actor + 20_000 },
                force: 5,
            })
            .collect();
        let mut reference = None;
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let result = pool.install(|| resolve(&strikes, &[]).unwrap());
            assert_eq!(result.events.len(), strikes.len());
            if shared {
                assert_eq!(result.impacts[0].harm, 50_000);
            }
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
    }
}
