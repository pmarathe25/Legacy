use game_language::writing::{Association, Mind, Observation, SharingPolicy, Writing};

#[test]
fn marks_remain_personal_ambiguous_and_replaceable() {
    let mut writing = Writing {
        capacity: 2,
        sharing: vec![],
        explanations: vec![],
        observations: vec![],
        minds: vec![
            Mind {
                person: 1,
                associations: vec![],
            },
            Mind {
                person: 2,
                associations: vec![],
            },
        ],
    };
    let form = |sounds: &[u8], marks: &[u32]| Association {
        sounds: sounds.to_vec(),
        marks: marks.to_vec(),
    };
    writing.assign(1, form(&[0], &[7]), 3).unwrap();
    writing.assign(1, form(&[1], &[7]), 3).unwrap();
    writing.assign(2, form(&[2], &[7]), 3).unwrap();
    assert_eq!(writing.interpretations(1, &[7]), vec![vec![0], vec![1]]);
    assert_eq!(writing.interpretations(2, &[7]), vec![vec![2]]);
    let before = writing.clone();
    assert!(writing.assign(1, form(&[2], &[8]), 3).is_err());
    assert!(writing.assign(2, form(&[3], &[8]), 3).is_err());
    assert!(writing.assign(2, form(&[1], &[0]), 3).is_err());
    assert_eq!(writing, before);
    writing.assign(1, form(&[0], &[9]), 3).unwrap();
    assert_eq!(writing.interpretations(1, &[7]), vec![vec![1]]);
    assert_eq!(writing.interpretations(2, &[7]), vec![vec![2]]);
    writing.validate(3, &[1, 2]).unwrap();
    let saved = serde_json::to_string(&writing).unwrap();
    let restored: Writing = serde_json::from_str(&saved).unwrap();
    restored.validate(3, &[1, 2]).unwrap();
    assert_eq!(writing, restored);
}

#[test]
fn disjoint_mark_choices_are_atomic_and_worker_invariant() {
    fn run(workers: usize) -> Writing {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let form = |sound, mark| Association {
                    sounds: vec![sound],
                    marks: vec![mark],
                };
                let mut writing = Writing {
                    capacity: 1,
                    sharing: vec![],
                    explanations: vec![],
                    observations: vec![],
                    minds: vec![Mind {
                        person: 2,
                        associations: vec![form(0, 5)],
                    }],
                };
                let before = writing.clone();
                assert!(
                    writing
                        .assign_batch(&[(1, form(1, 7)), (2, form(1, 8))], 3)
                        .is_err()
                );
                assert_eq!(writing, before);
                writing
                    .assign_batch(&[(1, form(1, 7)), (2, form(0, 8)), (3, form(2, 7))], 3)
                    .unwrap();
                writing.validate(3, &[1, 2, 3]).unwrap();
                assert_eq!(writing.interpretations(1, &[7]), vec![vec![1]]);
                assert_eq!(writing.interpretations(3, &[7]), vec![vec![2]]);
                writing
            })
    }
    assert_eq!(run(1), run(4));
}

#[test]
fn personal_observation_retention_evicts_only_the_owners_oldest_records() {
    fn run(workers: usize) -> Writing {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let policy: SharingPolicy =
                    serde_json::from_value(serde_json::json!({"person":2,"remember_capacity":2}))
                        .unwrap();
                let mut writing = Writing {
                    capacity: 2,
                    minds: vec![],
                    sharing: vec![policy],
                    explanations: vec![],
                    observations: vec![],
                };
                let observation = |page, tick, heard| Observation {
                    person: 2,
                    page,
                    speaker: 1,
                    marks: vec![page as u32],
                    heard,
                    first_seen_at: tick,
                    seen_at: tick,
                };
                writing
                    .remember(
                        &[observation(1, 1, None), observation(2, 2, Some(vec![0]))],
                        2,
                    )
                    .unwrap();
                let before = writing.clone();
                assert!(writing.remember(&[observation(0, 3, None)], 2).is_err());
                assert_eq!(writing, before);
                writing
                    .remember(&[observation(1, 3, None), observation(3, 4, None)], 2)
                    .unwrap();
                assert_eq!(
                    writing
                        .observations
                        .iter()
                        .map(|row| row.page)
                        .collect::<Vec<_>>(),
                    vec![1, 3]
                );
                assert_eq!(
                    (
                        writing.observations[0].first_seen_at,
                        writing.observations[0].seen_at
                    ),
                    (1, 3)
                );
                let first = observation(1, 5, None);
                let mut second = first.clone();
                second.marks = vec![9];
                let mut reversed = writing.clone();
                writing
                    .remember(&[first.clone(), second.clone()], 2)
                    .unwrap();
                reversed.remember(&[second, first], 2).unwrap();
                assert_eq!(writing, reversed);
                writing.validate(2, &[1, 2]).unwrap();
                writing
            })
    }
    assert_eq!(run(1), run(4));
}
