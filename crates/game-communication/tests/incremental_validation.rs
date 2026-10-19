use game_communication::{Association, Communication};
use game_core::*;
use game_sim::Resource;
fn evidence() -> Vec<Association> {
    vec![Association {
        sounds: vec![0, 1],
        strength: 10,
        source: PersonId(1),
        at: SimTime(0),
    }]
}
#[test]
fn changed_lexical_rows_reject_corruptions_without_changing_the_original() {
    let mut old = Communication::default();
    for person in 1..=80 {
        for word in 0..100 {
            old.lexicons
                .entry(PersonId(person))
                .or_default()
                .insert(format!("word-{word}"), evidence());
        }
    }
    old.validate().unwrap();
    for (person, key) in [
        (PersonId(1), "word-0"),
        (PersonId(80), "word-99"),
        (PersonId(100), "new"),
    ] {
        let mut changed = old.clone();
        changed.lexicons.entry(person).or_default().insert(
            key.to_string(),
            vec![Association {
                sounds: vec![255],
                ..evidence().remove(0)
            }],
        );
        assert!(changed.validate_changes(&old).is_err());
        old.validate().unwrap();
    }
    let mut changed = old.clone();
    changed
        .lexicons
        .get_mut(&PersonId(1))
        .unwrap()
        .get_mut("word-0")
        .unwrap()[0]
        .strength += 1;
    changed.validate_changes(&old).unwrap();
}
