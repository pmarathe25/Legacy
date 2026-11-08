use game_knowledge::{Knowledge, Mind, Misreport, Observation, Record, Reporting};
use game_social::Encounter;
use std::collections::HashSet;

#[test]
fn speaker_can_withhold_or_report_under_their_own_pressure() {
    let initial = Knowledge {
        retention: vec![],
        max_records: 3,
        minds: vec![
            Mind {
                person: 1,
                reporting: Some(Reporting {
                    willing: false,
                    pressure_threshold: Some(4),
                }),
                remember_people: false,
                people_reporting: None,
                people_reports: vec![],
                people_seen: vec![],
                misreport: Some(Misreport {
                    target: Some(2),
                    food_delta: 5,
                }),
                records: vec![Record {
                    region: 2,
                    food: 10,
                    observed_at: 0,
                    witness: 1,
                    learned_from: None,
                }],
            },
            Mind {
                person: 2,
                reporting: None,
                remember_people: false,
                people_reporting: None,
                people_reports: vec![],
                people_seen: vec![],
                misreport: None,
                records: vec![],
            },
        ],
    };
    let observations = [
        Observation {
            person: 1,
            health: 10,
            local: Some((1, 0)),
        },
        Observation {
            person: 2,
            health: 10,
            local: Some((1, 0)),
        },
    ];
    let regions = HashSet::from([1, 2]);
    let encounters = [Encounter {
        tick: 1,
        region: 1,
        a: 1,
        b: 2,
        pressured: None,
        also_pressured: None,
    }];
    let run = |workers, pressure, accept| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                initial
                    .prepare_validated_with_pressure(
                        &observations,
                        &regions,
                        &encounters,
                        0,
                        |_, _| accept,
                        |speaker, listener| {
                            if (speaker, listener) == (1, 2) {
                                pressure
                            } else {
                                0
                            }
                        },
                    )
                    .unwrap()
            })
    };
    let silent = run(1, 3, true);
    assert!(!silent.minds[1].records.iter().any(|r| r.region == 2));
    let pressured = run(1, 4, true);
    let report = pressured.minds[1]
        .records
        .iter()
        .find(|r| r.region == 2)
        .unwrap();
    assert_eq!((report.food, report.learned_from), (15, Some(1)));
    assert_eq!(
        pressured.minds[0]
            .records
            .iter()
            .find(|r| r.region == 2)
            .unwrap()
            .food,
        10
    );
    assert_eq!(pressured, run(4, 4, true));
    assert!(
        !run(1, 4, false).minds[1]
            .records
            .iter()
            .any(|r| r.region == 2)
    );
}
