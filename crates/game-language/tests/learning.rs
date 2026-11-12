use game_language::{
    GroundedMeeting, Language, Meaning, Meeting, Mind, Rules, TopicPolicy, TopicPreference, Word,
};
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
fn people_can_choose_and_share_personal_sound_forms_for_a_person() {
    let mut state = language();
    state
        .configure_topics(&[
            (
                1,
                TopicPolicy {
                    idle: 0,
                    act_on_tie: false,
                    preferences: vec![TopicPreference {
                        meaning: Meaning::Person(2),
                        enabled: true,
                        value: 10,
                    }],
                },
            ),
            (
                2,
                TopicPolicy {
                    idle: 0,
                    act_on_tie: false,
                    preferences: vec![],
                },
            ),
        ])
        .unwrap();
    for mind in &mut state.minds {
        mind.initiative = 1000;
        mind.adoption = 1000;
    }
    let meeting = GroundedMeeting {
        a: 1,
        b: 2,
        a_topics: vec![Meaning::Place(7), Meaning::Person(2)],
        b_topics: vec![Meaning::Place(7)],
    };
    let spoken = state
        .prepare_grounded(std::slice::from_ref(&meeting), 7, 1)
        .unwrap();
    assert_eq!(
        (spoken.events[0].speaker, spoken.events[0].meaning),
        (1, Meaning::Person(2))
    );
    assert!(spoken.events[0].learned);
    assert_eq!(spoken.minds[0].words[0].meaning, Meaning::Person(2));
    assert_eq!(spoken.minds[1].words[0].meaning, Meaning::Person(2));
    let mut refuses = state.clone();
    refuses.minds[1].learn = false;
    assert!(!refuses.prepare_grounded(&[meeting], 7, 1).unwrap().events[0].learned);
    assert!(spoken.validate(&HashSet::from([1, 2]), 1).is_ok());
    assert!(spoken.validate(&HashSet::from([1]), 1).is_err());
    assert_eq!(
        serde_json::from_str::<Language>(&serde_json::to_string(&spoken).unwrap()).unwrap(),
        spoken
    );
}

#[test]
fn grounded_topic_choices_preserve_the_actual_speakers_preferences() {
    let mut state = language();
    let policy = |meaning| TopicPolicy {
        idle: 0,
        act_on_tie: false,
        preferences: vec![TopicPreference {
            meaning,
            enabled: true,
            value: 10,
        }],
    };
    state
        .configure_topics(&[
            (1, policy(Meaning::Object(10))),
            (2, policy(Meaning::Object(20))),
        ])
        .unwrap();
    for mind in &mut state.minds {
        mind.initiative = 1000;
        mind.adoption = 0;
    }
    let meeting = GroundedMeeting {
        a: 1,
        b: 2,
        a_topics: vec![Meaning::Place(7), Meaning::Object(10)],
        b_topics: vec![Meaning::Place(7), Meaning::Object(20)],
    };
    for seed in 0..20 {
        let next = state
            .prepare_grounded(std::slice::from_ref(&meeting), seed, 1)
            .unwrap();
        let event = &next.events[0];
        assert_eq!(
            event.meaning,
            if event.speaker == 1 {
                Meaning::Object(10)
            } else {
                Meaning::Object(20)
            }
        );
        assert!(!event.learned);
        assert_eq!(
            next.minds
                .iter()
                .find(|mind| mind.person == event.listener)
                .unwrap()
                .words
                .len(),
            0
        );
    }
    let saved = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<Language>(&saved).unwrap(), state);
}

#[test]
fn grounded_silence_missing_objects_and_legacy_behavior_are_personal() {
    let initial = language();
    let original = Meeting {
        a: 1,
        b: 2,
        meaning: Meaning::Place(7),
    };
    let grounded = GroundedMeeting {
        a: 1,
        b: 2,
        a_topics: vec![Meaning::Place(7), Meaning::Object(10)],
        b_topics: vec![Meaning::Place(7)],
    };
    for seed in 0..30 {
        assert_eq!(
            initial.prepare(&[original], seed, 1).unwrap(),
            initial
                .prepare_grounded(std::slice::from_ref(&grounded), seed, 1)
                .unwrap()
        );
    }
    let mut state = initial;
    let silent = TopicPolicy {
        idle: 0,
        act_on_tie: false,
        preferences: vec![],
    };
    state
        .configure_topics(&[
            (1, silent.clone()),
            (
                2,
                TopicPolicy {
                    idle: 0,
                    act_on_tie: false,
                    preferences: vec![TopicPreference {
                        meaning: Meaning::Object(10),
                        enabled: true,
                        value: 10,
                    }],
                },
            ),
        ])
        .unwrap();
    for mind in &mut state.minds {
        mind.initiative = 1000;
    }
    assert!(state.prepare(&[original], 1, 1).unwrap().events.is_empty());
    assert!(
        state
            .prepare_grounded(std::slice::from_ref(&grounded), 1, 1)
            .unwrap()
            .events
            .is_empty()
    );
    let before = state.clone();
    assert!(
        state
            .configure_topics(&[(1, silent.clone()), (1, silent)])
            .is_err()
    );
    assert_eq!(state, before);
    let invalid = GroundedMeeting {
        a: 1,
        b: 2,
        a_topics: vec![Meaning::Place(7), Meaning::Object(10), Meaning::Object(10)],
        b_topics: vec![Meaning::Place(7)],
    };
    assert!(state.prepare_grounded(&[invalid], 1, 1).is_err());
}

#[test]
fn grounded_topics_respect_coining_and_are_worker_invariant() {
    let mut state = language();
    let entries: Vec<_> = (1..=2_000)
        .map(|person| {
            (
                person,
                TopicPolicy {
                    idle: 0,
                    act_on_tie: false,
                    preferences: vec![TopicPreference {
                        meaning: Meaning::Object(10),
                        enabled: true,
                        value: 5,
                    }],
                },
            )
        })
        .collect();
    state.configure_topics(&entries).unwrap();
    for mind in &mut state.minds {
        mind.initiative = 1000;
        mind.adoption = 1000;
    }
    let meetings: Vec<_> = (0..1_000)
        .map(|index| GroundedMeeting {
            a: index * 2 + 1,
            b: index * 2 + 2,
            a_topics: vec![Meaning::Place(7), Meaning::Object(10)],
            b_topics: vec![Meaning::Place(7), Meaning::Object(10)],
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| state.prepare_grounded(&meetings, 7, 1).unwrap())
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one.events.len(), 1_000);
    let mut unwilling_to_coin = state;
    for mind in &mut unwilling_to_coin.minds {
        mind.coin = false;
    }
    assert!(
        unwilling_to_coin
            .prepare_grounded(&meetings, 7, 1)
            .unwrap()
            .events
            .is_empty()
    );
}

#[test]
#[ignore = "personal topic scaling benchmark"]
fn parallel_personally_chosen_topics_benchmark() {
    let person_topics = std::env::var_os("LEGACY_LANGUAGE_PERSON_TOPICS").is_some();
    let people: u64 = std::env::var("LEGACY_LANGUAGE_PEOPLE")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(50_000);
    assert!(people > 0 && people.is_multiple_of(2));
    let mut state = language();
    let policies: Vec<_> = (1..=people)
        .map(|person| {
            (
                person,
                TopicPolicy {
                    idle: 0,
                    act_on_tie: false,
                    preferences: vec![TopicPreference {
                        meaning: if person_topics {
                            Meaning::Person(person)
                        } else {
                            Meaning::Object(10)
                        },
                        enabled: true,
                        value: 10,
                    }],
                },
            )
        })
        .collect();
    state.configure_topics(&policies).unwrap();
    for mind in &mut state.minds {
        mind.initiative = 1000;
        mind.adoption = 1000;
    }
    let meetings: Vec<_> = (0..people / 2)
        .map(|index| GroundedMeeting {
            a: index * 2 + 1,
            b: index * 2 + 2,
            a_topics: vec![
                Meaning::Place(1),
                if person_topics {
                    Meaning::Person(index * 2 + 1)
                } else {
                    Meaning::Object(10)
                },
            ],
            b_topics: vec![
                Meaning::Place(1),
                if person_topics {
                    Meaning::Person(index * 2 + 2)
                } else {
                    Meaning::Object(10)
                },
            ],
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
            let next = pool.install(|| state.prepare_grounded(&meetings, 7, 1).unwrap());
            times.push(start.elapsed());
            assert_eq!(next.events.len() as u64, people / 2);
            if let Some(expected) = &reference {
                assert_eq!(&next, expected);
            } else {
                reference = Some(next);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {people} people",
            times[1]
        );
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
            topic_policy: None,
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
            topic_policy: None,
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
            topic_policy: None,
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
fn observer_transcription_preserves_phoneme_boundaries() {
    let state = language();
    assert_eq!(state.rules.render(&[1, 0, 2]).unwrap(), "k·a·m");
    assert!(state.rules.render(&[]).is_err());
    assert!(state.rules.render(&[255]).is_err());
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
            topic_policy: None,
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
            topic_policy: None,
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
