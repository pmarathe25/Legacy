use game_language::{Language, Meaning, Meeting, Mind, Rules, Word};
use std::collections::HashSet;

fn language() -> Language {
    Language {
        rules: Rules {
            sounds: vec!["a".into(), "k".into(), "m".into()],
            word_length: 3,
            capacity: 4,
        },
        minds: vec![],
        events: vec![],
    }
}

#[test]
fn a_physical_meeting_coins_and_grounds_a_personal_place_word() {
    let initial = language();
    let next = (0..100)
        .map(|seed| {
            initial
                .prepare(
                    &[Meeting {
                        a: 1,
                        b: 2,
                        meaning: Meaning::Place(7),
                    }],
                    seed,
                    1,
                )
                .unwrap()
        })
        .find(|next| next.events.first().is_some_and(|event| event.learned))
        .unwrap();
    assert!(initial.minds.is_empty());
    assert_eq!(next.events.len(), 1);
    assert_eq!(next.events[0].interpreted, None);
    assert!(next.events[0].learned);
    assert_eq!(next.minds[0].words, next.minds[1].words);
    assert_eq!(next.minds[0].words[0].meaning, Meaning::Place(7));
    next.validate(&HashSet::from([1, 2]), 1).unwrap();
    let saved = serde_json::to_string(&next).unwrap();
    let restored: Language = serde_json::from_str(&saved).unwrap();
    assert_eq!(next, restored);
}

#[test]
fn a_listener_can_have_a_different_interpretation_and_decline_learning() {
    let mut state = language();
    state.minds = vec![
        Mind {
            person: 1,
            speak: true,
            hear: true,
            learn: false,
            coin: false,
            initiative: 1000,
            adoption: 0,
            modify: false,
            phonemes: vec![],
            words: vec![Word {
                meaning: Meaning::Place(8),
                sounds: vec![0],
            }],
        },
        Mind {
            person: 2,
            speak: true,
            hear: true,
            learn: false,
            coin: false,
            initiative: 1000,
            adoption: 0,
            modify: false,
            phonemes: vec![],
            words: vec![Word {
                meaning: Meaning::Place(7),
                sounds: vec![0],
            }],
        },
    ];
    let next = (0..100)
        .map(|seed| {
            state
                .prepare(
                    &[Meeting {
                        a: 1,
                        b: 2,
                        meaning: Meaning::Place(7),
                    }],
                    seed,
                    1,
                )
                .unwrap()
        })
        .find(|next| !next.events.is_empty())
        .unwrap();
    let event = &next.events[0];
    assert_eq!(event.speaker, 2);
    assert_eq!(event.interpreted, Some(Meaning::Place(8)));
    assert!(!event.learned);
    assert_eq!(next.minds, state.minds);
}

#[test]
fn people_can_decline_to_speak_or_choose_to_change_a_form() {
    let mut state = language();
    state.minds = (1..=2)
        .map(|person| Mind {
            person,
            speak: true,
            hear: true,
            learn: true,
            coin: true,
            initiative: 0,
            adoption: 1000,
            modify: true,
            phonemes: vec![0, 1],
            words: vec![],
        })
        .collect();
    let meeting = [Meeting {
        a: 1,
        b: 2,
        meaning: Meaning::Place(4),
    }];
    assert!(state.prepare(&meeting, 1, 1).unwrap().events.is_empty());
    for mind in &mut state.minds {
        mind.initiative = 1000;
    }
    let next = state.prepare(&meeting, 1, 1).unwrap();
    let event = &next.events[0];
    assert!(event.learned);
    let speaker = next
        .minds
        .iter()
        .find(|mind| mind.person == event.speaker)
        .unwrap();
    let listener = next
        .minds
        .iter()
        .find(|mind| mind.person == event.listener)
        .unwrap();
    assert_eq!(speaker.words[0].sounds, event.sounds);
    assert_ne!(speaker.words[0].sounds, listener.words[0].sounds);
    assert!(listener.words[0].sounds.iter().all(|sound| *sound < 2));
}

#[test]
fn grounded_meanings_are_not_limited_to_places() {
    let state = language();
    let meeting = [Meeting {
        a: 1,
        b: 2,
        meaning: Meaning::Object(42),
    }];
    let next = (0..100)
        .map(|seed| state.prepare(&meeting, seed, 1).unwrap())
        .find(|next| next.events.first().is_some_and(|event| event.learned))
        .unwrap();
    assert_eq!(next.minds[0].words[0].meaning, Meaning::Object(42));
    assert_eq!(next.minds[1].words[0].meaning, Meaning::Object(42));
}

#[test]
fn sparse_person_ids_do_not_require_a_dense_allocation() {
    let state = language();
    let meeting = [Meeting {
        a: 1,
        b: u64::MAX,
        meaning: Meaning::Place(9),
    }];
    let next = (0..100)
        .map(|seed| state.prepare(&meeting, seed, 1).unwrap())
        .find(|next| next.events.first().is_some_and(|event| event.learned))
        .unwrap();
    assert_eq!(next.minds.len(), 2);
    next.validate(&HashSet::from([1, u64::MAX]), 1).unwrap();
}

#[test]
fn a_person_can_speak_even_when_the_other_person_cannot_hear() {
    let mut state = language();
    state.minds = (1..=2)
        .map(|person| Mind {
            person,
            speak: true,
            hear: false,
            learn: true,
            coin: true,
            initiative: 1000,
            adoption: 1000,
            modify: false,
            phonemes: vec![],
            words: vec![],
        })
        .collect();
    let next = state
        .prepare(
            &[Meeting {
                a: 1,
                b: 2,
                meaning: Meaning::Need(1),
            }],
            1,
            1,
        )
        .unwrap();
    assert_eq!(next.events.len(), 1);
    assert!(!next.events[0].heard);
    assert!(!next.events[0].learned);
    assert_eq!(
        next.minds
            .iter()
            .filter(|mind| !mind.words.is_empty())
            .count(),
        1
    );
}

#[test]
fn a_chosen_form_can_spread_through_separate_grounded_encounters() {
    let mut state = language();
    state.minds = (1..=3)
        .map(|person| Mind {
            person,
            speak: person == 1,
            hear: true,
            learn: true,
            coin: true,
            initiative: 1000,
            adoption: 1000,
            modify: false,
            phonemes: vec![],
            words: vec![],
        })
        .collect();
    let first = state
        .prepare(
            &[Meeting {
                a: 1,
                b: 2,
                meaning: Meaning::Place(12),
            }],
            3,
            1,
        )
        .unwrap();
    assert_eq!(first.minds[0].words, first.minds[1].words);
    let mut next = first;
    next.minds[1].speak = true;
    let second = next
        .prepare(
            &[Meeting {
                a: 2,
                b: 3,
                meaning: Meaning::Place(12),
            }],
            3,
            2,
        )
        .unwrap();
    assert_eq!(second.minds[1].words, second.minds[2].words);
    assert!(second.events[0].learned);
}

#[test]
fn disjoint_conversations_are_deterministic_across_worker_counts() {
    let meetings: Vec<_> = (0..10_000)
        .map(|n| Meeting {
            a: 2 * n + 1,
            b: 2 * n + 2,
            meaning: Meaning::Place(n % 4 + 1),
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| language().prepare(&meetings, 27, 1).unwrap())
    };
    assert_eq!(run(1), run(4));
    assert!(
        language()
            .prepare(
                &[
                    Meeting {
                        a: 1,
                        b: 2,
                        meaning: Meaning::Place(1)
                    },
                    Meeting {
                        a: 2,
                        b: 3,
                        meaning: Meaning::Place(1)
                    }
                ],
                1,
                1
            )
            .is_err()
    );
}

#[test]
#[ignore = "performance measurement"]
fn parallel_grounded_conversation_benchmark() {
    let meetings: Vec<_> = (0..100_000)
        .map(|n| Meeting {
            a: 2 * n + 1,
            b: 2 * n + 2,
            meaning: Meaning::Place(n % 64 + 1),
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
            let result = pool.install(|| language().prepare(&meetings, 27, 1).unwrap());
            times.push(start.elapsed());
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {} encounters",
            times[1],
            meetings.len()
        );
    }
}
