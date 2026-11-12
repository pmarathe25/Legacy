use game_personal_choice::{BirthPreference, BirthProfile, Newborn};

#[test]
fn newborns_receive_individual_inclinations_without_targets_or_decisions() {
    let profile = BirthProfile {
        idle: 0,
        act_on_tie: false,
        preferences: vec![BirthPreference {
            domain: 7,
            enabled: true,
            bias: 100,
            weight: 1000,
            locus: Some(2),
            genetic_bias: 1000,
        }],
    };
    profile.validate().unwrap();
    let births = [
        Newborn {
            id: 4,
            genes: [500, 500, 100, 500],
        },
        Newborn {
            id: 5,
            genes: [500, 500, 900, 500],
        },
    ];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| profile.enroll(&births))
    };
    let people = run(1);
    assert_eq!(people, run(4));
    assert_eq!(people[0].preferences[0].bias, -300);
    assert_eq!(people[1].preferences[0].bias, 500);
    assert!(people.iter().all(|person| person.actions.is_empty()));
    let mut invalid = profile;
    invalid.preferences[0].locus = Some(4);
    assert!(invalid.validate().is_err());
}

#[test]
#[ignore = "newborn preference scaling measurement"]
fn newborn_preference_scaling() {
    let profile = BirthProfile {
        idle: 0,
        act_on_tie: false,
        preferences: (1..=8)
            .map(|domain| BirthPreference {
                domain,
                enabled: true,
                bias: 0,
                weight: 1000,
                locus: Some((domain % 4) as u8),
                genetic_bias: 1000,
            })
            .collect(),
    };
    let births: Vec<_> = (1..=100_000)
        .map(|id| Newborn {
            id,
            genes: [100, 300, 700, 900],
        })
        .collect();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let result = pool.install(|| profile.enroll(&births));
            times.push(start.elapsed());
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {} births",
            times[1],
            births.len()
        );
    }
}
