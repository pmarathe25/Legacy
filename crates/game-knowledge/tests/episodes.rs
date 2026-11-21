use game_knowledge::episodes::{Attention, Consolidation, Cue, Kind, Memory, Mind, Observation};
use std::collections::HashSet;

fn mind(person: u64, value: u32) -> Mind {
    Mind {
        person,
        capacity: 3,
        attention: vec![Attention {
            kind: Kind::Meeting,
            value,
            retention_millis: 100,
        }],
        consolidation: Some(Consolidation {
            rest_millis: 20,
            extension_millis: 200,
        }),
        reflection_millis: 50,
        resting_since: None,
        memories: Vec::new(),
    }
}

const MEETING: Cue = Cue::Meeting {
    other: 2,
    region: 7,
};

fn observation(person: u64, at_millis: u128, cue: Cue) -> Observation {
    Observation {
        person,
        at_millis,
        cue,
    }
}

#[test]
fn subjective_attention_fading_reflection_and_actual_rest_are_independent() {
    let mut memory = Memory {
        minds: vec![mind(1, 10), mind(2, 0)],
    };
    let mut observations = [
        observation(1, 10, MEETING),
        observation(
            2,
            10,
            Cue::Meeting {
                other: 1,
                region: 7,
            },
        ),
    ];
    memory.remember(&mut observations, 10);
    assert!(memory.recall(2, None, 10).is_empty());
    assert!(memory.recall(1, Some(3), 10).is_empty());
    assert_eq!(memory.recall(1, Some(2), 10)[0].precise_time, Some(10));
    let snapshot = memory.clone();
    let fading = memory.recall(1, None, 90)[0];
    assert_eq!(fading.strength, 2);
    assert_eq!(fading.precise_time, None);
    assert_eq!(memory, snapshot);
    assert!(memory.recall(1, None, 110).is_empty());

    memory.rest(1, true, 60);
    memory.rest(1, false, 79);
    assert_eq!(memory.minds[0].memories[0].accessible_until, 110);
    assert!(memory.reflect(1, MEETING, 90));
    assert_eq!(memory.minds[0].memories[0].accessible_until, 160);
    memory.rest(1, true, 140);
    memory.rest(1, true, 148);
    memory.rest(1, false, 180);
    assert_eq!(memory.minds[0].memories[0].accessible_until, 360);
    assert_eq!(memory.minds[0].memories[0].observed_at, 10);
    assert!(!memory.reflect(1, MEETING, 360));
    memory
        .validate(&HashSet::from([1, 2]), &HashSet::from([7]))
        .unwrap();
    assert!(memory.validate_at(9).is_err());
    memory.validate_at(180).unwrap();
}

#[test]
fn concentrated_and_independent_observations_keep_bounded_identical_memory() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut memory = Memory {
                    minds: (1..=4096).map(|person| mind(person, 10)).collect(),
                };
                let mut evidence: Vec<_> = (2..=4096)
                    .map(|other| {
                        observation(1, u128::from(other % 9), Cue::Meeting { other, region: 7 })
                    })
                    .chain((2..=4096).map(|person| {
                        observation(
                            person,
                            10,
                            Cue::Meeting {
                                other: 1,
                                region: 7,
                            },
                        )
                    }))
                    .collect();
                evidence.reverse();
                memory.remember(&mut evidence, 10);
                assert_eq!(memory.minds[0].memories.len(), 3);
                assert!(
                    memory.minds[1..]
                        .iter()
                        .all(|mind| mind.memories.len() == 1)
                );
                memory
            })
    };
    assert_eq!(run(1), run(6));
}
