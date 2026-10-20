use game_communication::*;
use game_core::*;

fn word(sounds: Vec<u8>, strength: u16) -> Association {
    Association {
        sounds,
        strength,
        source: PersonId(1),
        at: SimTime(0),
    }
}
fn reference(c: &Communication, p: PersonId, sounds: &[u8]) -> (Option<String>, u16) {
    let mut best: Option<(String, u16)> = None;
    for (key, variants) in c.lexicons.get(&p).into_iter().flat_map(|w| w.iter()) {
        for w in variants {
            if w.sounds == sounds
                && best.as_ref().is_none_or(|(old, strength)| {
                    w.strength > *strength || w.strength == *strength && key < old
                })
            {
                best = Some((key.clone(), w.strength));
            }
        }
    }
    best.map_or((None, 0), |(key, strength)| (Some(key), strength))
}
fn check(c: &Communication, p: PersonId, sounds: &[u8]) {
    let expected = reference(c, p, sounds);
    let decoded = decode(c, p, sounds);
    assert_eq!((decoded.meaning, decoded.confidence), expected);
    assert_eq!(decoded.surface, render(sounds));
}
#[test]
fn reverse_sound_lookup_tracks_homophones_new_evidence_rollback_worlds_and_loading() {
    let p = PersonId(1);
    let mut c = Communication::default();
    for i in 0..32 {
        c.lexicons
            .entry(p)
            .or_default()
            .insert(format!("concept-{i:03}"), vec![word(vec![i % 22, 1], 10)]);
    }
    let before = c.clone();
    for _ in 0..3 {
        for sounds in [&[0, 1][..], &[4, 1], &[21, 21]] {
            check(&c, p, sounds);
        }
    }
    assert_eq!(
        decode(&c, p, &[0, 1]).meaning.as_deref(),
        Some("concept-000")
    );
    c.lexicons
        .get_mut(&p)
        .unwrap()
        .get_mut("concept-022")
        .unwrap()[0]
        .strength = 20;
    for _ in 0..3 {
        check(&c, p, &[0, 1]);
    }
    assert_eq!(
        decode(&c, p, &[0, 1]).meaning.as_deref(),
        Some("concept-022")
    );
    for source in [&before, &c, &before] {
        for _ in 0..3 {
            check(source, p, &[0, 1]);
        }
    }
    let saved = serde_json::to_string(&c).unwrap();
    let loaded: Communication = serde_json::from_str(&saved).unwrap();
    assert_eq!(serde_json::to_string(&loaded).unwrap(), saved);
    for _ in 0..3 {
        check(&loaded, p, &[0, 1]);
    }
    let mut other = Communication::default();
    other.lexicons.insert(p, c.lexicons[&p].clone());
    other
        .lexicons
        .get_mut(&p)
        .unwrap()
        .insert("aaa".into(), vec![word(vec![0, 1], 100)]);
    for source in [&other, &c, &loaded] {
        for _ in 0..3 {
            check(source, p, &[0, 1]);
        }
    }
    check(&c, PersonId(99), &[0, 1]);
}
#[test]
fn changing_personal_lexicons_match_a_brute_force_reference_across_many_aliases() {
    let p = PersonId(1);
    for seed in 0..8 {
        let mut c = Communication::default();
        for i in 0..128 {
            let key = format!("concept-{}", random(seed, i, 1) % 64);
            let sounds = vec![
                (random(seed, i, 2) % 22) as u8,
                (random(seed, i, 3) % 22) as u8,
            ];
            let strength = (random(seed, i, 4) % 100 + 1) as u16;
            c.lexicons.entry(p).or_default().insert(
                key,
                vec![word(sounds.clone(), strength), word(vec![1, 1], strength)],
            );
            for _ in 0..3 {
                check(&c, p, &sounds);
                check(&c, p, &[1, 1]);
            }
        }
    }
}
#[test]
fn vocabularies_larger_than_the_cache_budget_keep_every_learned_meaning() {
    let p = PersonId(1);
    let mut c = Communication::default();
    for i in 0..4097 {
        c.lexicons.entry(p).or_default().insert(
            format!("concept-{i:05}"),
            (1..=8).map(|s| word(vec![1, 2, 3], s)).collect(),
        );
    }
    for _ in 0..3 {
        check(&c, p, &[1, 2, 3]);
        check(&c, p, &[4, 5]);
    }
    assert_eq!(
        decode(&c, p, &[1, 2, 3]).meaning.as_deref(),
        Some("concept-00000")
    );
}
