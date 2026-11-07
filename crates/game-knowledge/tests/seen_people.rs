use game_knowledge::{Knowledge, Mind, Observation, SeenPerson};
use game_social::Encounter;
use std::collections::HashSet;

#[test]
fn personal_encounter_memories_are_bounded_and_do_not_depend_on_report_acceptance() {
    let initial = Knowledge {
        max_records: 1,
        minds: vec![
            Mind {
                person: 1,
                remember_people: true,
                people_seen: vec![],
                reporting: None,
                misreport: None,
                records: vec![],
            },
            Mind {
                person: 2,
                remember_people: false,
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
