use game_knowledge::{Knowledge, Mind, Observation, PersonReport, Reporting, SeenPerson};
use game_social::Encounter;
use std::collections::HashSet;

#[test]
fn encounter_reports_are_personal_bounded_and_keep_their_source() {
    let empty = |person| Mind {
        person,
        remember_people: false,
        people_reporting: None,
        people_reports: vec![],
        people_seen: vec![],
        reporting: None,
        misreport: None,
        records: vec![],
    };
    let mut speaker = empty(1);
    speaker.people_seen.push(SeenPerson {
        person: 3,
        region: 2,
        observed_at: 3,
    });
    let initial = Knowledge {
        retention: vec![],
        max_records: 1,
        minds: vec![speaker, empty(2), empty(3)],
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
        Observation {
            person: 3,
            health: 10,
            local: Some((2, 0)),
        },
    ];
    let encounter = [Encounter {
        tick: 5,
        region: 1,
        a: 1,
        b: 2,
        pressured: None,
        also_pressured: None,
    }];
    let regions = HashSet::from([1, 2]);
    let run = |workers, enabled, accepts, pressure| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = initial.clone();
                initial.minds[0].people_reporting = enabled;
                let next = initial
                    .prepare_validated_with_pressure(
                        &observations,
                        &regions,
                        &encounter,
                        4,
                        |receiver, sender| accepts && (receiver, sender) == (2, 1),
                        |speaker, listener| {
                            if (speaker, listener) == (1, 2) {
                                pressure
                            } else {
                                0
                            }
                        },
                    )
                    .unwrap();
                next.validate(&HashSet::from([1, 2, 3]), &regions, 5)
                    .unwrap();
                assert!(next.minds[1].people_seen.is_empty());
                next
            })
    };
    let willing = Some(Reporting {
        willing: true,
        pressure_threshold: None,
    });
    assert!(run(1, None, true, 0).minds[1].people_reports.is_empty());
    assert!(run(1, willing, false, 0).minds[1].people_reports.is_empty());
    let pressured = Some(Reporting {
        willing: false,
        pressure_threshold: Some(3),
    });
    assert!(
        run(1, pressured, true, 2).minds[1]
            .people_reports
            .is_empty()
    );
    let result = run(1, pressured, true, 3);
    assert_eq!(
        result.minds[1].people_reports,
        vec![PersonReport {
            person: 3,
            region: 2,
            observed_at: 3,
            speaker: 1
        }]
    );
    assert_eq!(result, run(4, pressured, true, 3));
}

#[test]
fn personal_encounter_memories_are_bounded_and_do_not_depend_on_report_acceptance() {
    let initial = Knowledge {
        retention: vec![],
        max_records: 1,
        minds: vec![
            Mind {
                person: 1,
                remember_people: true,
                people_reporting: None,
                people_reports: vec![],
                people_seen: vec![],
                reporting: None,
                misreport: None,
                records: vec![],
            },
            Mind {
                person: 2,
                remember_people: false,
                people_reporting: None,
                people_reports: vec![],
                people_seen: vec![],
                reporting: None,
                misreport: None,
                records: vec![],
            },
        ],
    };
    let regions = HashSet::from([1, 2]);
    let people = HashSet::from([1, 2, 3]);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let first = [
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
                    Observation {
                        person: 3,
                        health: 10,
                        local: Some((2, 0)),
                    },
                ];
                let meeting = [Encounter {
                    tick: 1,
                    region: 1,
                    a: 1,
                    b: 2,
                    pressured: Some(1),
                    also_pressured: None,
                }];
                let mut state = initial
                    .prepare_validated_with(&first, &regions, &meeting, 0, |_, _| false)
                    .unwrap();
                assert_eq!(
                    state.minds[0].people_seen,
                    vec![SeenPerson {
                        person: 2,
                        region: 1,
                        observed_at: 1
                    }]
                );
                assert!(state.minds[1].people_seen.is_empty());
                let second = [
                    Observation {
                        person: 1,
                        health: 10,
                        local: Some((2, 0)),
                    },
                    Observation {
                        person: 2,
                        health: 10,
                        local: Some((1, 0)),
                    },
                    Observation {
                        person: 3,
                        health: 10,
                        local: Some((2, 0)),
                    },
                ];
                let meeting = [Encounter {
                    tick: 2,
                    region: 2,
                    a: 1,
                    b: 3,
                    pressured: None,
                    also_pressured: None,
                }];
                state = state
                    .prepare_validated_with(&second, &regions, &meeting, 1, |_, _| false)
                    .unwrap();
                assert_eq!(
                    state.minds[0].people_seen,
                    vec![SeenPerson {
                        person: 3,
                        region: 2,
                        observed_at: 2
                    }]
                );
                state.minds[0].remember_people = false;
                let meeting = [Encounter {
                    tick: 3,
                    region: 1,
                    a: 1,
                    b: 2,
                    pressured: None,
                    also_pressured: None,
                }];
                state = state
                    .prepare_validated_with(&first, &regions, &meeting, 2, |_, _| false)
                    .unwrap();
                assert_eq!(
                    state.minds[0].people_seen,
                    vec![SeenPerson {
                        person: 3,
                        region: 2,
                        observed_at: 2
                    }]
                );
                state.validate(&people, &regions, 2).unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}
