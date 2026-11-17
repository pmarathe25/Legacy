use game_language::writing::{Association, Mind, Writing};

#[test]
fn marks_remain_personal_ambiguous_and_replaceable() {
    let mut writing = Writing {
        capacity: 2,
        sharing: vec![],
        explanations: vec![],
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
