use game_phonology::*;
#[test]
fn every_sound_pair_roundtrips_including_aspiration_boundaries() {
    for a in 0..PHONEMES.len() as u8 {
        assert_eq!(parse(&render(&[a])), Some(vec![a]));
        for b in 0..PHONEMES.len() as u8 {
            assert_eq!(parse(&render(&[a, b])), Some(vec![a, b]), "{a}, {b}");
        }
    }
    assert_eq!(render(&[11, 39]), "k·h");
    assert_eq!(render(&[12]), "kh");
    assert_eq!(parse("śānti"), Some(vec![36, 1, 26, 22, 2]));
    for bad in ["", "ḥ", "·a", "a·", "a··i", "a?", "a i"] {
        assert!(parse(bad).is_none());
    }
}
#[test]
fn display_does_not_silently_remove_unknown_sounds() {
    assert!(!valid(&[255]));
    assert_eq!(render(&[255]), "�");
    assert!(VOWELS.iter().all(|id| vowel(*id)));
    assert!(!vowel(26));
}
