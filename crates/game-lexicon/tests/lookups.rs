use game_core::*;
use game_lexicon::*;
fn association(sounds: Vec<u8>, strength: u16) -> Association {
    Association {
        sounds,
        strength,
        source: PersonId(1),
        at: SimTime(0),
    }
}
fn reference(words: &Words, sounds: &[u8]) -> (Option<String>, u16) {
    words
        .iter()
        .flat_map(|(k, v)| v.iter().map(move |w| (k, w)))
        .filter(|(_, w)| w.sounds == sounds)
        .max_by_key(|(k, w)| (w.strength, std::cmp::Reverse(*k)))
        .map_or((None, 0), |(k, w)| (Some(k.clone()), w.strength))
}
#[test]
fn packed_sound_keys_preserve_prefixes_zero_bytes_and_long_form_fallbacks() {
    let mut decoder = Decoder::default();
    for long in [false, true] {
        let mut words = Words::default();
        let sounds: Vec<_> = (0..128)
            .map(|n| {
                let mut s = vec![0; n % 8];
                if let Some(first) = s.first_mut() {
                    *first = (n / 8) as u8;
                }
                if long && n == 127 {
                    s.extend([255; 20]);
                }
                s
            })
            .collect();
        for (i, sounds) in sounds.iter().enumerate() {
            words.insert(
                format!("meaning-{i:03}"),
                vec![association(sounds.clone(), (i % 7) as u16)],
            );
        }
        for _ in 0..3 {
            for s in sounds.iter().chain([vec![255; 7], vec![255; 25]].iter()) {
                assert_eq!(decoder.decode(PersonId(1), &words, s), reference(&words, s));
            }
        }
    }
}
#[test]
fn bounded_many_person_cache_survives_eviction_updates_rollback_and_world_switches() {
    let mut decoder = Decoder::default();
    let mut people: Vec<Words> = (0..160)
        .map(|p| {
            (0..220)
                .map(|i| {
                    (
                        format!("person-{p:03}-meaning-{i:03}"),
                        vec![association(
                            vec![(i / 16) as u8, (i % 16) as u8],
                            (p % 100) as u16,
                        )],
                    )
                })
                .collect()
        })
        .collect();
    let before = people[0].clone();
    for round in 0..3 {
        for (p, words) in people.iter().enumerate() {
            for s in [&[0, 0][..], &[1, 1], &[30, 30]] {
                for _ in 0..2 {
                    assert_eq!(
                        decoder.decode(PersonId(p as u64), words, s),
                        reference(words, s)
                    );
                }
            }
        }
        people[0].insert(
            format!("new-{round}"),
            vec![association(vec![0, 0], 101 + round)],
        );
        for words in [&people[0], &before, &people[1], &before, &people[0]] {
            for _ in 0..3 {
                assert_eq!(
                    decoder.decode(PersonId(0), words, &[0, 0]),
                    reference(words, &[0, 0])
                );
            }
        }
    }
}

#[test]
fn total_form_pressure_never_discards_personal_associations() {
    let people: Vec<Words> = (0..8)
        .map(|p| {
            (0..50)
                .map(|i| {
                    (
                        format!("person-{p}-meaning-{i:03}"),
                        (0..100)
                            .map(|j| association(vec![i % 4, j % 4], j as u16))
                            .collect(),
                    )
                })
                .collect()
        })
        .collect();
    let mut decoder = Decoder::default();
    for _ in 0..3 {
        for (p, words) in people.iter().enumerate() {
            for _ in 0..3 {
                assert_eq!(
                    decoder.decode(PersonId(p as u64), words, &[0, 0]),
                    reference(words, &[0, 0])
                );
            }
        }
    }
}

#[test]
fn shared_unicode_meanings_outlive_cache_eviction_and_vocabulary_revisions() {
    let mut words: Words = (0..32)
        .map(|i| (format!("word-{i}"), vec![association(vec![i], 1)]))
        .collect();
    words.insert("雪の記憶".into(), vec![association(vec![255], 100)]);
    let mut decoder = Decoder::default();
    decoder.meaning(PersonId(1), &words, &[255]);
    let original = decoder.meaning(PersonId(1), &words, &[255]).0.unwrap();
    words.get_mut("雪の記憶").unwrap()[0].sounds = vec![254];
    assert_eq!(decoder.meaning(PersonId(1), &words, &[255]).0, None);
    drop(decoder);
    drop(words);
    assert_eq!(original.as_ref(), "雪の記憶");
}

#[test]
fn oversized_text_pools_keep_complete_lookup_without_retaining_an_index() {
    let mut words: Words = (0..17)
        .map(|i| {
            (
                format!("{i}-{}", "x".repeat(300_000)),
                vec![association(vec![1, i], 1)],
            )
        })
        .collect();
    words.insert("needle".into(), vec![association(vec![255], 97)]);
    let mut decoder = Decoder::default();
    for _ in 0..3 {
        assert_eq!(
            decoder.decode(PersonId(1), &words, &[255]),
            (Some("needle".into()), 97)
        );
    }
}
